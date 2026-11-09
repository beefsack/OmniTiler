# KDE float/native divergence

## Goal and scope

- Diagnose the first divergence in the post-namespace-fix physical float-toggle
  capture and correct it offline with regression coverage. Check whether the
  earlier sticky tile/float round trips share the cause.
- Preserve existing state-target, preimage, executor and correlation contracts;
  exclude live testing and runtime-store changes.

## Acceptance and approach

1. Compare the new capture with production source and earlier sticky captures;
   distinguish proven divergence from hypotheses.
2. Reproduce the cause offline before fixing, or add bounded diagnostics if
   the evidence cannot locate it.
3. Verify core, protocol, service, KDE adapter and scenario-runner suites at
   current baselines, with immutable fixtures unchanged; independently review.
4. Record source/log evidence and exact traced physical re-run instructions.

## Evidence and outcome (2026-11-10)

- Diagnostic fallback complete; behavioral cause NOT LOCATED. No behavior,
  policy, wire or persistence contract changed. Physical ST and the macOS
  start gate remain open. The user reports rapid maximize/fullscreen PASS,
  rapid float FAIL; sticky after the namespace correction is unverified.
- Capture: `omnitiler-dev.lI1Pr8.log`, 716 lines, SHA-256
  `b911e52ee5c004bb78abb8b210d0c4f8d3554239cfa688aadaf220973b51eb3d`.
  Line 6 proves `preimage-read outcome=ok stored=1 returned=1`.
- Proven: 22 queued/dispatched float commands and 22 `above` pending lines;
  the pending lines name the same native window. No native-state dropped/refused lines.
  `world-native-state.ts:759-818` emits pending after an accepted setter,
  not after core/native convergence. Thus setters were accepted, but their
  boolean targets and post-call facts are absent from this capture. This
  disfavors a universal pre-arm/storage failure, not every above-lane race.
- First log-visible cadence change: lines 515 and 526 show adjacent revisions
  31 and 32 with `geom=1`, without the preceding pattern's intervening
  `geom=0` observe apply. This is NOT a proven first behavioral divergence:
  batching can change cadence, and the count cannot name the window.
  `world-apply.ts:1232-1243` counts converged writes; the original command
  and applied logs in the captured build contain no per-window
  frame attribution. The user's subject-stuck/sibling-moving report cannot
  be reconstructed from those aggregate lines.
- Lines 106-107 and 480-481 show two explicitly fenced shortcut failures;
  they do not explain continued layout toggles for dispatched commands.
  No maximize-clear unfloat diagnostic appears. Earlier sticky captures
  `omnitiler-dev.HOFkEy.log` and `omnitiler-dev.dVXwsP.log` used degraded
  namespace seeds; they cannot establish post-correction sticky behavior.
- Hypotheses still open: core intent/authority versus frame application or
  correlation, native acquisition/release settlement, and subject/focus
  attribution. No specific race or first incorrect setter is proven.
- Focus-path inspection found no offline behavioral proof: ordinary commands
  submit `currentFocus` (`world-entry.ts:2092-2104`); execution reads the core
  slot (`world_commands.rs:68-74`), updated by new native focus/loss/departure
  (`world_run_v4.rs:360-390`). `world-native.ts:383-461` does not activate a
  window on float/unfloat. The capture lacks slot, focus and take-window facts.
  A different execution subject remains a hypothesis, not a diagnosis.

## Diagnostic implementation and limits

- Trace-only `ev=cmd` records clientCmd, `press` id, actual submitted `win`
  (including `currentFocus`), press focus and optional frame. `ev=subject`
  explicitly labels `basis=take|submitted|press|affected` and records accepted
  core float/frame/allocation, frame/member authority and hold reasons,
  native target value/authority/cmdSeq,
  transfers and the exactly associated take result (including refusal facts).
- `obs` and `fact` are the flight's captured input snapshot, NOT post-apply
  native reads. Frame attempts provide immediate pre/readback; state attempts
  provide pre/post, target, authorizing revision, setter and original stamp.
  Compare the next capture to establish later convergence.
- `applySrc` identifies newly observed synchronous apply rows; `flushSrc`
  identifies the boundary delivering late persist-ack rows, NOT their origin.
  State `rev`, frame `op` and `attemptMs` retain in-row provenance. Implicit
  subjects without an exact clientCmd result naming a window remain
  `marker-unassigned`; any retained press-window row is only `basis=press`,
  never evidence of broker resolution. Sticky results need not name a window.
  Repeated resolved subjects retain one row per command within the row cap.
- Caps: 64 remembered commands, 16 capture batches, 16 commands inspected per
  batch, 8 subject/affected-window rows and 32 new attempt rows per boundary.
  Latest-wins attempt retention can coalesce rows before tracing; truncation
  or missing rows are not proof of no write. Use two disposable windows for
  this rerun. Trace changes timing/log volume and is not a race-free observer.
- Non-trace bundles strip the diagnostics. No titles, application payloads or
  runtime-store contents are logged. A directional-only outcome longer than
  32 characters renders `-`; requested float/sticky refusal fields fit.

## Offline verification

- The original eight new diagnostic tests against `16d25836` sources
  in an isolated archive: 0 passed, 8 failed on missing diagnostics (earlier
  verification). The final diagnostic file has 12 passed, 0 failed. This proves
  observability, NOT a regression reproducing or fixing the unknown live defect.
- Coverage: six accepted float presses; deliberately frozen subject versus
  converged sibling; sticky from tile and float; independent `noSpace` and
  rejection kind; legal long/dotted identifiers; late-ack once-only delivery;
  exact clientCmd association without guessed implicit subjects; ordinary
  float press A/submitted marker/take B; unresolved sticky press basis;
  eight-row combined cap and repeated same-window resolved command rows.
  Replies are synthetic core plans through the real adapter/executor, not a live core/WM.
- Earlier review found identifier loss, repeated/misattributed async rows
  and order-guessed implicit subjects; those were corrected. Later inspection
  found the subject-basis defect below. Fresh independent review accepted
  its correction, then found a ninth affected row beyond the eight-row cap
  and resolved same-window duplicates labeled unassigned. Both were corrected
  and re-reviewed with no unresolved material finding.
  A pre-existing new-window-probe silence test
  initially failed (1213/1214); its check now isolates the original probe
  vocabulary and separately expects the new command diagnostics. Original
  detach assertions remain intact. Final full suite passes.

| Gate | Result |
| --- | --- |
| `cargo test -p tiler-core -- --skip workspace_unsafe_posture` | 2551 passed, 0 ignored, 1 filtered |
| `cargo test -p tiler-protocol -- --skip workspace_unsafe_posture` | 288 passed, 0 ignored |
| `cargo test -p omnitiler -- --skip workspace_unsafe_posture` | 341 passed, 0 ignored |
| KDE `npm run typecheck` | passed |
| KDE focused `world-trace-diagnostics.test.ts` | 12/12, zero skipped |
| KDE `npm test` (includes non-trace build/strip guards) | 1219/1219, zero skipped |
| `node --test scripts/kde-scenarios.test.mjs` | 130/130 |
| `cargo fmt --all -- --check`; `git diff --check` | passed |

- Immutable world-wire/stage5 fixture bytes unchanged. No live test or
  runtime-store mutation ran. No product decision requires promotion.

## Attribution defect and regression evidence

- Proven diagnostic defect in the initial patch: ordinary commands converted
  to `win:{currentFocus:true}`, but the diagnostic recorded `originalTarget`
  and treated that press id as an explicit resolved subject. A differing core
  focus could therefore make `ev=subject` name the wrong acting window. This
  was not present in the captured non-trace log and is not the proven live bug.
- Corrected at `world-entry.ts:2155-2163` (separate press/submitted values)
  and `:696-740` (exact take, explicit submitted or honest unresolved press
  basis). Core marker semantics are unchanged.
- Two new production-conversion tests before correction: 8 passed, 2 failed
  at missing `press=` distinction. After correction: 10/10. The float test
  also asserts exact take B, never resolved/submitted press A; sticky asserts
  unassigned marker plus press-basis core/obs/fact rows.
- Two display-cap regressions before the review correction failed on nine
  rows and missing second resolved command; after correction: 12/12. Tests
  use parked sticky batches with synthetic window-bearing take results to
  exercise shared display logic, not sticky's actual result shape or core
  behavior. Float itself is native-fenced while a flight is outstanding.
- Remaining bounded limits: repeated unresolved press windows can share a
  press-basis row but retain per-command unassigned evidence; explicit
  `basis=submitted` continuation has no dedicated new test. Neither permits
  guessing a resolved ordinary subject. Physical verification remains open.

## Exact traced user rerun

- Follow the [live guide](../../live-kwin-testing.md). Append a test workspace
  beyond the whole existing roster; use two disposable ordinary resizable
  windows. Establish a known tiled baseline and keep the tested subject
  focused. Use fresh known-baseline subjects for each row below. Do not reset
  or delete any runtime store. These are user-run diagnostics, not acceptance
  of remaining storage/recovery/menu or composed state-target journeys.
- Before startup, retain the unfiltered journal cursor and start tracing:

  ```sh
  CURSOR=$(journalctl --user --show-cursor -n 0 --no-pager | sed -n 's/^-- cursor: //p')
  just dev trace
  ```

- Retain the printed combined/Planner log locations and verified KWin PID;
  assign that printed number to `KWIN_PID` in the capture shell. Before keys,
  require cursor-bounded `world:ready`, `world:discovered`, matched accepted
  dispatch/applied, settings accepted and preimage-seed `outcome=ok`. Require
  recovery completion if same-namespace records are pending. Missing readiness
  or corrupt/unreadable seed means stop and retain evidence, not restart/reset.

| Separate baseline | Physical keys | Evidence and expected working outcome |
| --- | --- | --- |
| Ordinary tile | `Meta+Shift+G`, `Meta+Shift+G`, releasing and waiting after each | All-desktop/above acquisition, then single home/tiled release; retain both subject and sibling frame evidence. |
| Ordinary tile | `Meta+G`, `Meta+Shift+G`, `Meta+Shift+G`, `Meta+G`, releasing and waiting after each | Float, sticky, prior float, tiled. At any rejection retain exact UI text/time and whether command, dispatch, take refusal, clear, stale or shortcut-failed exists. |
| Ordinary tile, two windows | Up to 30 `Meta+G` presses at the previously failing rapid cadence; fully release each chord, no wait between presses within a burst | Stop at first subject/sibling divergence, otherwise at the bound. Retain actual accepted clientCmds, not parity inferred from raw presses: busy-fenced presses may be refused. Record subject, focus, visible frame and sibling response. |

- Maximize/fullscreen spam already passed after the earlier fix; these rows
  target only the remaining float/sticky discriminator.
- For each row note start/end time and subject identity. Outside the spam row,
  wait for the result/follow-up capture before the next press. Verify the first
  accepted key produces `world:trace ev=cmd` with `press`/submitted `win` and
  `ev=subject` with explicit `basis`; absent new trace
  rows means stop and retain the build/capture evidence. After a stall, do not
  keep pressing to force convergence; retain the final follow-up and UI state.
- Stop with Ctrl-C, then save both journal streams using the retained cursor
  and verified PID (the second command keeps script errors outside the prefix):

  ```sh
  journalctl --user --after-cursor "$CURSOR" _PID="$KWIN_PID" -o cat --no-pager
  journalctl --user --after-cursor "$CURSOR" _PID="$KWIN_PID" QT_CATEGORY=kwin_scripting -o cat --no-pager
  ```

- Preserve the printed combined and Planner logs plus these cursor-bounded
  outputs before temporary logs disappear. Return the first mismatch's
  clientCmd/src, press/submitted subject and take/press basis, `core` and
  authority fields, captured `obs`/`fact`, frame
  attempt target/pre/readback, state attempt target/rev/pre/post, and exact take
  refusal if any. Native setter accepted alone is not convergence. No bus
  monitor or runtime-store reset is part of this procedure.

# Windows wake-attribution trace volume

## Goal and scope

- Reduce repetitive trace-only message-wake rows without changing owner work,
  scheduling, input handling or safety gates.
- Preserve event/deadline attribution used to diagnose foreground starvation,
  aggregate wait accounting and paired pager/world-state stress evidence.
- Investigate the wake source first; a demonstrated wasted-wake or idle-cost
  defect is a separate behavioral question, not a logging fix.

## Acceptance and approach

1. Explain the repeated empty message-wake shape from the wait/message path
   and measure a fixture-only rapid-spam baseline.
2. Apply the smallest observational filter or aggregation with regression
   coverage for retained meaningful causes. Add no aggregation timer.
3. Pass locked core/Windows tests, strict all-target package clippy, relevant
   formatting and diff checks, plus Windows stress parser/mock checks.
4. Compare before/after live log volume using the same fixture-only rapid
   sequence, retain pager/world-state checks and verify exact cleanup.

## Cause and correction

- `wait_source` selects the next deadline, not the unique waker. A pager
  deadline can coexist with message-queue readiness and an empty WinEvent drain.
- Before correction, the emitter logged every traced wait return before idle early-outs;
  idle returns need not advance the world tick. Aggregate `pump-waits` counting
  is separate from these detail rows.
- The native wait observes all owner-thread message readiness, while the hook
  drain records effective WinEvents after filtering, including own-overlay
  exclusion. Non-WinEvent messages and fully filtered callbacks can therefore
  leave an empty drain. Exact native message identities are not captured; the
  trace does not establish that the pager generated those messages.
- The zero-delay home/away fixture baseline passed 102 commands and all
  checkpoints: 577 wake-detail rows, including 51 empty message-only rows;
  139 pager payloads retained paired world proof. It did not reproduce the
  physical report's same-tick clustering or demonstrate idle spin.
- Detail rows incorrectly reported zero elapsed wait: accounting stamped a copied
  diagnostic while the emitter read the original. The loop now stamps the
  shared diagnostic in place, with the existing single elapsed sample.
- Suppress only unstaged message-only detail with an empty effective hook
  drain. Keep staged work, all other wait flags, effective hook/gesture evidence,
  foreground joins/omissions and due deadlines. Count filtered detail in the
  existing naturally emitted/final `pump-waits` summary; add no timer. A
  filtered row means no retained wake detail, not proof of no owner work.

## Accepted evidence (2026-10-27)

- Two native fixture-only normal-owner legs used the unchanged home/away
  stress definition, 102 exact-owner commands and zero added delay. Both
  passed all 18 checkpoints, including setup/final. Publication spans were
  34.9 and 36.4 seconds; CLI transport rates are not physical-keyboard rates.

| Measure | Before | After |
| --- | ---: | ---: |
| Total log lines | 2,497 | 2,436 |
| Total log bytes | 1,898,223 | 1,871,778 |
| Wake-detail rows | 577 | 520 |
| Wake-detail JSON bytes, excluding newlines | 261,319 | 236,695 |
| Filter-eligible empty-message rows still logged | 51 | 0 |
| Suppressed detail, final summary | Not present | 55 |
| Retained rows with nonzero elapsed wait | 0 | 173 |
| Paired pager payloads | 139 | 139 |
| Accepted world-apply rows | 354 | 354 |

- After correction, 520 emitted details plus 55 suppressed details exactly
  equal the 575 final owner waits. The five retained unstaged message-only
  rows each carry real foreground evidence. Staged, timeout and other-flag
  rows remain; counts naturally vary between runs, so row deltas alone are
  not a performance claim.
- Stress parsing still passes exact native/world/pager checks; paired proof
  and its parser were not changed. Exact-owner stop/restore, fixture shutdown,
  unchanged settings/SPI preimages and absence of process, recovery and control
  residue were verified. Source/dirty provenance, artifact hashes and detailed
  run evidence are retained externally.
- Regression coverage retains each wait-flag/deadline/effective-hook/foreground
  guard, verifies raw filtered-batch and elapsed-independent suppression,
  saturating summary accounting and nonzero elapsed serialization. The actual
  loop's elapsed correction is confirmed by the native comparison, not a
  standalone assignment test.
- Final gates pass: locked core/protocol/Windows tests (2,932 passed, one
  existing ignored test), strict all-target package clippy, workspace fmt,
  diff check, Windows stress parser and fault-injection mocks.

## Outcome and limits

- Empty-message wake detail is filtered with honest aggregate counts. Useful
  starvation attribution, pager/world-state proof and product behavior remain
  unchanged; no timer, wait, retry or new wake source is introduced.
- Neither leg demonstrated idle spin or reproduced physical same-tick spam
  clustering. Exact native message types, physical-keyboard density and game
  cost remain unmeasured; no behavioral/performance fix is claimed.
- No further action is required for this diagnostic correction.

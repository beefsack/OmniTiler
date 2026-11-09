# KDE latest float convergence

## Goal and scope

- Locate and fix the first native/core divergence during overlapping float and
  unfloat commands, restoring latest-desired-state convergence offline.
- Check whether the earlier sticky failures share the cause. Preserve existing
  state-target, preimage, executor, correlation and subject-resolution contracts.
- Exclude live testing, runtime-store changes and unrelated architecture work.

## Acceptance and approach

1. Establish subject identity and first divergence from `omnitiler-dev.JOkEjv.log`;
   test overlapping native float/unfloat application before other hypotheses.
2. Add a core-producer regression with odd/even bursts ending float/tile;
   demonstrate red before the smallest causal fix and green after it. Attempt
   additional real adapter/core executor coverage without claiming a passing
   pre-fix journey reproduces the defect.
3. Verify core, protocol, service, KDE adapter and scenario-runner suites at or
   above current baselines; preserve immutable fixture bytes.
4. Independently review the diff, record proven versus hypothetical findings,
   and retain precise traced physical re-run instructions.

## Outcome

- Implemented and verified offline. The core-producer regression is red before
  the fix and green after it; all scoped suites pass and independent review
  has no unresolved findings. Physical float convergence and the separate
  sticky outcomes remain unverified.
- Baselines: core 2562, protocol 288, service 341, KDE 1224, runner 130.

## Accepted evidence

- `omnitiler-dev.JOkEjv.log`: the exact take subject remains the press/focus
  window (for example lines 307 and 345). Its third float frame attempt is
  accepted with asynchronous old readback at line 311. The fourth command's
  unfloat publishes `hold/constraint-clamp` at lines 336 and 345, revision 10;
  `geom=1` at line 329 reflects the sibling write, not subject convergence.
  The captured above targets alternate and apply successfully. Wrong-subject
  resolution and an above acquisition/release latch do not explain this run.
- `omnitiler-dev.8tHpKS.log`: revision 7 already shows a temporary clamp hold
  (line 195) after an asynchronous unfloat write. Revision 10 recovers to
  controlled authority (line 306), and revision 11 remains controlled
  (line 339). The persistent failure begins on command 6, revision 13:
  lines 432, 440 and 450 show sibling-only geometry, a subject frame hold,
  and tiled intent against the floating native rectangle. Above release
  succeeds at line 462; frame holds persist, including line 478 and the final
  observation. This run also contains stale native-state drops (lines 595
  and 758); they do not establish the frame hold's cause.
- Pre-fix producer path: `world_frames.rs:562-587` breaks burst continuity for an
  ineligible floating row by zeroing `last_change_ms` without clearing budget;
  `:673-695` counts completed tiled attempts too; `:599-632` holds at two
  attempts for the old unchanged tiled target and cannot prove quiet from a
  zero continuity clock. `world_publish.rs:996-997` publishes that veto as
  `constraint-clamp`; `kwin/src/world-apply.ts:721,747` correctly skips it.
- Before the fix, the new six-toggle producer replay reaches `attempts=2`,
  `last_change_ms=0`, `frames=None`, then publishes a hold instead of the
  latest tiled frame. This proves the burst-memory wedge independently of
  clamp memory or keep-above state. The three-toggle float-ending replay
  retains the correct desired float and placement token. Neither test drives
  a native executor. The exact earlier onset in `JOkEjv` cannot be reconstructed
  from its captured staged-payload detail; the specific internal lane in that
  capture remains unproven beyond the published constraint-clamp hold.
- The additional owner-PID journal has no captured differing PID, but unchanged
  ownership throughout the run was not independently verified. Its category-
  filtered scripting capture contains no entries. Neither establishes native
  frame convergence.
- Earlier sticky captures used degraded namespace seeds. This frame-publication
  veto cannot itself explain an all-desktop acquisition/release failure or a
  command rejection. Those sticky outcomes remain independently unresolved.

## Correction and review

- Accepted float/unfloat and sticky entry/exit commits drop the tiled burst
  budget because they change the core placement target. The reset is attached
  to applied command paths in `world_operations.rs` and `world_sticky.rs`, not
  derived native classification. Refusals, same-state no-ops and no-space
  retries do not acquire a new budget. Clamp, transient and float-token gates
  are unchanged; exact-echo spending and observation-gap retention are unchanged.
- The helper is `world_frames.rs:560-566`; applied float/unfloat resets are
  `world_operations.rs:1915,1932`; direct sticky transition commits reset at
  `world_sticky.rs:887,1227`. Native sticky-off defers placement until proof;
  its followup at `:1571-1602` uses those same commit paths, not the unproven
  binding-mint path.
- A trial excluded exact-match completions from burst spending. Compatibility
  review rejected it: it contradicts `world_frames.rs:26-33,113-118`, the
  accepted policy in `kde-client-frame-drift.md:15-21`, and the existing
  `burst_exact_echo_then_redrift_preserves_budget` regression. The trial also
  failed its own replay's intermediate second-spend assertion. The production
  edit was removed.
- An initial mode-based correction used merged floating/sticky classification.
  Independent review found that native transient/sticky flaps could then re-arm
  the budget without a core-requested transition, and that its gap/hold comments
  overstated the gating. It was replaced with accepted-transition-only resets.
  New tests pin native-only flaps retaining the budget, native sticky entry,
  composed sticky round-trip and rejected float commands over observation gaps.
- Final independent review confirmed both findings resolved and verified native
  sticky-off resets at the dependent placement commit. Refused/ambiguous native
  operations do not reset the budget. Seven additive core regressions cover
  six-toggle tile ending, three-toggle float ending, odd/exhausted budgets with
  floating placement settlement, native-only flaps and gaps, composed sticky
  round-trip, and rejected float commands with minimized/unreadable facts.
- The rule is clarified in the original
  [frame-drift record](kde-client-frame-drift.md), not a new product decision.
- Two attempted genuine broker/adapter burst regressions (ordinary sequential
  bursts and asynchronous setter readbacks with interleaved stale captures)
  passed both before and after the fix. They were removed rather than claimed
  as red-to-green evidence. These attempts did not reproduce the budget wedge;
  they do not prove it is unreachable. The retained regression proves the core
  publish producer, not end-to-end native execution.

## Verification

| Check | Result |
| --- | --- |
| `cargo test -p tiler-core --lib world_frame_policy_tests::rapid_toggle -- --skip workspace_unsafe_posture` against original production code | 1 passed, 1 failed: six-toggle tile ending holds with budget 2 and clock 0; three-toggle float ending passes |
| Existing exact-echo budget test against trial fix | 0 passed, 1 failed: missing spent budget |
| Tile-ending replay against trial fix | 0 passed, 1 failed: expected second spend 2, observed 1 |
| Existing exact-echo budget test after production restore | 1 passed, 0 failed |
| Corrected core frame-policy suite | 66 passed, 0 failed |
| `cargo test -p tiler-core -- --skip workspace_unsafe_posture` | 2569 passed, 0 failed, 0 ignored, 1 filtered |
| `cargo test -p tiler-protocol -- --skip workspace_unsafe_posture` | 288 passed, 0 failed, 0 ignored, 0 filtered |
| `cargo test -p omnitiler -- --skip workspace_unsafe_posture` | 341 passed, 0 failed, 0 ignored, 0 filtered |
| KDE `npm test`; `npm run typecheck` | 1224/1224, zero skipped; typecheck passed |
| `node --test scripts/kde-scenarios.test.mjs` | 130/130 |
| `cargo fmt --all -- --check`; `git diff --check` | passed |

- Verification covers the final production correction. Existing immutable
  fixture bytes are unchanged. No live testing or runtime-store changes were
  performed. No end-to-end executor regression or physical convergence proof
  is claimed; native acceptance remains the next check.
- Traced physical re-run instructions remain those in
  [the diagnostic record](kde-float-native-divergence.md#exact-traced-user-rerun).
  Re-run rapid float bursts with odd and even accepted-command counts, ending
  both floating and tiled. After each burst, wait for follow-up observations:
  the final native frame and above state must match the latest core intent;
  the subject must not remain `hold/constraint-clamp` while its sibling reflows.
  Preserve exact take-subject identity, attempts and refusal/supersession facts;
  parity is based on accepted commands, not raw physical key counts.

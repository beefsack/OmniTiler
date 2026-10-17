# B9 / M09: unmaximize before intentional unfloat admission

## Goal and scope

- Deliver User 2026-10-08 M09=A: an explicit unfloat of a maximized floating
  client clears maximize, observes settlement, then fresh-admits to tiling.
- Shared core plus KDE; Windows runtime remains pending with an updated handoff.
- Preserve normal-to-float overlay refusals, native identity/settlement fences,
  fullscreen behavior and one native attempt per explicit activation.
- Offline only. Initial HEAD `8a33163`, clean; COSMIC confirmation is not a gate.

## Acceptance and approach

- Reuse observation-driven native-state settlement; no new timers or retries.
- Refused/unobserved/raced clears degrade narrowly, log honestly, leave no
  stuck operation and allow later explicit activation.
- Existing D3/D7 fixed-window explicit tile overrides apply to this unfloat;
  automatic fixed classification must not defeat the user's tile command.
- Regression coverage: maximize-clear before admission geometry, ordinary
  unfloat unchanged, refused/unobserved clear, fixed-window override and fences.

## Bounded units and verification

1. Implementation, targeted regression evidence and
   source handover; stop on a material product ambiguity.
2. Inspect diff and update decisions/spec/reference outcomes, Windows
   handoff and backlog (explicitly authorized by user).
3. Full offline gates with exact commands and counts.
4. Fresh independent review, then any causal corrections.
- Gates: KWin tests/typecheck/bundle, Rust workspace tests/clippy/fmt, native
  CTest, `just check-portable`, offline shell suites and `git diff --check`.
- Baseline: KWin 1225, Rust 1269, native CTest 33/33.
- Archive this note after accepted evidence; publish intended files in
  repository style without force/amend/hook skips.

## Accepted evidence and choices

- KDE reuses the native maximize echo fence and fresh observation, then falls
  through to ordinary unfloat dispatch with the restored frame. Shared core
  requires no production change: pre-clear observation refuses, post-clear
  ordinary/fixed clients fresh-admit under the existing lifecycle transaction.
- Interpretations: fixed automatic/intentional unfloat is the D3/D7 explicit
  tile override; ordinary R-FLT-06 does not change sticky maximize refusals.
  Unconfirmed clear preserves float intent and ends that toggle attempt;
  a later press can retry. Minimal discriminating rows recorded in the
  reference matrix; unsupported reference/native outcomes stay TBD.
- Red/green in a separate HEAD source copy `b9-red`: baseline
  new mock suite 4 pass/6 fail, current 10/10 pass. Targeted KWin 249/249 and
  core fixed-size suite 36/36 pass, including real Planner admission/write
  ordering, narrow refusal and subsequent fixed tile-override observation.
- An old stash accidentally popped during an attempted
  reproduction; only unrelated tracked paths restored to the verified
  clean HEAD and only stash-added obsolete files removed. Intended changes
  and all three pre-existing stashes preserved; all gates rerun after recovery.
  One fixture sequencing correction kept native float state unchanged until
  reply application. No unresolved semantic failed approach.
- Full offline gates: KWin 1238/1238 (173 suites), Rust workspace 1270,
  Linux Windows allowlist 1118, native CTest 33/33; typecheck/bundle,
  clippy/fmt, `just check-portable`, nine shell suites and diff whitespace pass.
  Evidence: `01-diff-check.log` through
  `12-shell-suites.log`. Native gate's generated result symlink removed before
  shell gates; no live testing or Windows runtime edits.
- Decisions/REQ-FLT-06/06b and Ours KDE cell updated. B9/M09 P0 removed;
  Windows handoff item 15 and named KDE native check added; COSMIC remains
  confirmation only.
- Fresh independent review accepted all behavior/scope conditions
  and substantiated full-gate logs, with no source blockers. Its only procedural
  finding was the pending archive move, completed here. Targeted/red counts
  above are implementation reports, not archived gate logs; acceptance
  rests on the independently inspected final full-gate evidence.
- Three sequential units (implementation, full verification, fresh
  independent review). Offline delivery complete;
  native acceptance remains user-owned.

## Full-state native follow-up, 2026-10-18

- Live callback testing found that the full-state native route read maximize
  synchronously after clearing it. Native settlement was asynchronous: the first
  activation cleared maximize but refused unfloat; the second activation tiled.
- The correction issues one clear and decides on the first fresh accepted world
  report: observed clear admits once, unconfirmed/raced clear ends the attempt.
  Newer input, scope/lifetime/gesture races and terminal transport or generation
  failures cancel pending intent. No new timers or setter retries were added.
- Producer-shaped red/green regressions and independent safety review accepted
  the corrected bounded continuation. KWin 1794 tests and Rust workspace 3017
  tests pass. [Current acceptance and remaining checks](../kde-live-acceptance.md).

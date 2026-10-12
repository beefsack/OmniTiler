# KDE Shortcut Apply And Result Dialogs

## Goal And Scope

- Replace inline shortcut refusal/Force controls with one complete modal conflict
  preview offering Force Apply and Cancel. Apply without conflicts acts directly.
- Show modal success/error results for Apply, Force and Revert; successful Apply
  is active immediately, independent of the settings window's Apply/OK.
- Keep Revert's custom-binding-loss confirmation and existing Force revalidation,
  exemptions, persistence and mutation boundaries.
- Normalize conflict completeness/order below the UI and retain reconstructible
  `omnitiler.shortcut` diagnostics using chords and holder IDs only.
- KDE implementation only; matching Windows dialogs are a backlog handoff.

## Acceptance And Approach

1. Inspect current shortcut flow, decision constraints and offline test harness.
2. Implement modal flow and reconciler-owned complete, stable conflict data;
   add hermetic completeness/order and dialog-decision regression coverage.
3. Review the mutation boundary, run settings-only and full native CTest, and
   check the diff. Update decisions and user-owned live verification steps.
4. Archive the outcome and evidence. Physical delivery and Settings-process
   loading remain user-owned live checks, with unsupported observations TBD.

## Material Decisions

- The conflict preview is the single Force confirmation. Ordinary Apply has no
  extra confirmation. Revert retains one confirmation because restoring defaults
  loses custom bindings, then shows its result. No-stacked-confirmations is
  tentative, pending user review, as requested 2026-10-12.
- Source investigation found a five-holder truncation in the per-binding row
  summary, not a six-row cap in the old Force preview. The old status and refusal
  also collected their first conflict differently. The reported live six-versus-26
  discrepancy is not reproduced offline and remains a user-owned check.
- Reconciler snapshots now drive status and refusal; blockers precede clearable
  holders, each sorted by component/action (blocker chord breaks ties). The
  modal consumes that order without UI-side sorting, including actual Lock
  relocation and retained-but-displaced authorized target binding.
- Per-holder indexed, bounded/chunked diagnostics include every chord and ID,
  never display labels or user content. Operational/partial Apply failures are
  distinguished from zero-write conflict refusal and never offer Force.

## Evidence And Outcome

- Delivered offline 2026-10-12: direct Apply, one scrollable Force/Cancel modal,
  success/error results for Apply/Force/Revert, retained Revert loss confirmation,
  complete conflict data and consistent first-holder selection. No inline Force
  controls or callable unconfirmed Force entry point remain. Cancel drops its
  local preview, preserves draft intent, writes nothing and opens no second dialog.
- Independent mutation-boundary review identified mixed blocker/clear-row order,
  joined-list log truncation, Cancel's extra dialog and hidden preview, a direct
  Force entry point, missing exemption details, offscreen dialog suppression and
  unscrollable many-holder presentation. Corrections accepted by source review
  and regression coverage. Initial test assumptions about Apply confirmation
  required updates; final gates below pass after the latest code changes.
- Native builds used separate out-of-tree build directories against matching
  KWin development headers, with effect OFF/settings ON for the settings gate
  and effect ON/settings ON for the full gate, both with BUILD_TESTING=ON.
  Settings-only CTest: **26/26 passed**. Full native CTest: **32/32 passed**.
  Focused shortcut suites: **17/17 passed**. `git diff --check`: passed.
- Hermetic coverage includes six-holder multi-chord dialog completeness, 30-holder
  indexed-log completeness, mixed blocker/clear ordering, keyed-only friendly
  names, unrelated blocker-key retention, Lock exemptions, real offscreen
  Force/Cancel/disabled-Force widgets, immediate-result wording, stale owner/live
  image refusal, partial failure reporting and Revert results. Existing Save,
  selection, disabled-binding and recovery suites remain green.
- No KWin JavaScript or shell source changed; those suites were not required.
- Decisions, functional-contract wording, discriminating outcomes and
  [live procedure](../../live-shortcut-override-verification.md) updated;
  matching Windows settings dialogs remain a [backlog handoff](../../backlog.md).

## User-Owned Live Checks

- Follow the live procedure for conflict-free immediate Apply, complete many-holder
  modal/list/journal agreement, Cancel with no writes or second dialog, sole Force
  confirmation, stale snapshot refusal, physical chords and Revert-to-defaults.
  Native results remain TBD; offline checks do not establish physical delivery.
- With an already-discovered development stage, `just build-native-effect` plus
  fully quitting/reopening Settings or kcmshell6 loads this KCM change without
  logout. A new delivery path/initial setup or loading a rebuilt effect still
  requires logout/login. Settings-process loading itself remains a live check.

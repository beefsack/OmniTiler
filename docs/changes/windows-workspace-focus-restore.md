# Windows workspace focus restore

## Goal and scope

- Restore remembered focus on workspace selection under R-WS-09 / REQ-WS-09,
  including truthful workspace-overview focus presentation.
- Diagnose the reported switch failures from existing traces, then reproduce
  with fixture-only native Windows windows before correcting production code.
- Preserve legitimate no-focus states and core/platform policy separation.
- Gather pinned Tier 1 source evidence for unremembered arrival, directional
  focus from no focus, and reachable no-focus states. These are evidence-only;
  no new focus policy is selected here.

## Acceptance

- Explain core versus adapter responsibility using trace and source evidence.
- Regression tests cover realistic host observation ordering and remembered
  focus in both switch directions, including a second destination window.
- Native fixture checks corroborate foreground, visibility and overview data,
  with verified cleanup; physical shortcut acceptance remains distinct.
- Minimal reference-outcome rows cite supported Tier 1 source findings and
  leave unsupported outcomes TBD.

## Approach and bounded units

1. Diagnose trace, current focus ownership and native test entry points.
2. Reproduce fixture-only, implement the smallest existing-policy correction,
   and run targeted regressions plus native checks.
3. Investigate the three Tier 1 questions at documented source pins.
4. Independently review live/focus behavior, integrate evidence, and archive
   this note when acceptance is complete.

## Decisions and evidence

- Existing approved remembered-focus policy is the repair boundary. A new
  fallback for a workspace genuinely lacking remembered focus requires a
  separate product decision.
- Diagnosis, verification and source evidence pending.

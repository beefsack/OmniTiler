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
- The reported trace uses one output. Numbered selection publishes empty focus
  effects while visibility changes succeed; the Windows effect dispatcher
  therefore never attempts foreground activation. The core selection route
  returns `NoEffect` after retaining its view target in the incident revision.
- Existing retained-session tests prove remembered tiled focus survives a view
  roundtrip, but do not assert a return-focus effect. Actual remembered leaves
  and pager highlight payloads are not recorded in the supplied trace.
- Draft correction emits core remembered-focus intent for explicit selection;
  Windows re-reads a baseline-hidden target after the same-pass view reveal.
  No first-window fallback is added. These changes are not accepted as a
  complete fix: native focus landing remains unproven.
- Incident-shaped core regression proves that a focused newly admitted
  singleton records remembered focus; a second focused arrival updates it.
  Occupied-workspace returns both ways emit the remembered target. The user's
  ordinary focused-window scenario therefore does not require a new fallback.
- Pager diagnosis: `world_presentation.rs` retains report-time foreground but
  builds pager rows over post-command views. A departing WS2 foreground and
  a newly current WS1 can coexist in one reply, showing WS2 focused until a
  fresh desktop/destination observation clears or replaces it. The regression
  exposes this mixed-epoch feed; no pager correction or live highlight proof
  is claimed. Existing live logs do not carry pager presentation payloads.

## Verification and blocker (2026-10-25)

- The incident trace proves the original route emits no selection focus.
  Fixture-only native reproduction with the core-only correction emits focus
  but the adapter holds the target against its pre-reveal hidden baseline.
- With the baseline re-read correction, two native checks reveal the workspace
  then stop focus dispatch as stale. Independent native reads confirm visible
  fixture windows without foreground landing. Cleanup released the owner,
  restored the recovery ledger and shut down fixture windows/processes.
- Source inspection identifies the ordering conflict: the view's own show/hide
  WinEvents queue wakes, and the trailing focus freshness probe treats pending
  hook edges as invalidation. The adapter diagnostic test reproduces this
  view-write-then-stale-focus sequence without consuming the effect.
- No further native retry, event exemption, safety-fence relaxation or delayed
  focus replay has been introduced. Classifying every pending edge as self
  generated would also risk overlooking genuine intervening input/window state.
- Latest offline gates: `mise exec -- cargo test -p tiler-core --locked`
  (843 library tests plus integration suites), Windows equivalent (959 library
  tests, one existing ignored test, integration suites and four doc tests),
  strict all-target clippy for both crates and crate fmt checks passed. These
  green checks include a diagnostic of the unresolved stale hold, not native
  focus acceptance.
- Tier 1 source findings and unsupported cells are recorded in the
  [minimal no-focus discriminators](../spec/reference-outcomes.md#workspace-no-focus-discriminators).
  No new product policy is selected.

## Remaining work

- Resolve same-pass view-event/focus freshness ordering without losing genuine
  intervening-change protection or violating at-most-once focus. Add a desired-
  behavior regression before another native attempt; the current diagnostic
  only proves the blocker.
- Correct or settle the mixed-epoch pager presentation, then verify foreground,
  visibility and actual pager data on occupied workspace switches both ways.
- Shared/global-swap variants and floating remembered-focus coverage remain
  outside the current draft's demonstrated acceptance.

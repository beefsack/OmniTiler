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
- Self-generated view notifications must not invalidate the same reply's
  trailing remembered-focus dispatch; genuinely intervening user/app changes
  must still win. Per-HWND wake suppression and cross-reply focus replay are
  not selected. Any mechanism must preserve core policy ownership and the
  existing at-most-once effect contract.
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
  fresh desktop/destination observation clears or replaces it. The draft now
  marks focus only on a current workspace. Regression covers stale departing
  focus, native destination catch-up and foreign focus; it never substitutes
  desired remembered focus for observed foreground. Live pager-cell payloads
  remain unverified; existing logs carry pager lifecycle, not cell data.

## Verification and blocker (2026-10-25)

- The incident trace proves the original route emits no selection focus.
  Fixture-only native reproduction with the core-only correction emits focus
  but the adapter holds the target against its pre-reveal hidden baseline.
- With the baseline re-read correction, two native checks reveal the workspace
  then stop focus dispatch as stale. Independent native reads confirm visible
  fixture windows without foreground landing. Cleanup released the owner,
  restored the recovery ledger and shut down fixture windows/processes.
- The ordering conflict is inside production, not a missing harness drain:
  `product_hide/sys/mod.rs::post_visibility` pumps messages while confirming
  each visibility write. Hook deliveries during that nested pump populate the
  queue after the view's freshness check and before trailing focus. A normal
  loop-top drain cannot prevent this. The adapter diagnostic reproduces the
  view-write-then-stale-focus sequence without consuming the effect.
- A poll-only exemption based on proven view state and managed foreground was
  rejected and removed. Those reads do not establish event causality, miss
  foreign foreground and gesture supersession, and leave the native focus
  setter's non-consuming pending-event veto intact. A consuming fake had hidden
  that veto. The added production-contract regression proves pending edges keep
  stopping until drained; no exemption, watermark reset, queue filtering or
  delayed focus replay remains in the draft.
- A subsequent fixture-only diagnostic used the real owner and exact-owner
  numbered selection, with no synthetic input. Both fixture windows occupied
  WS1; selecting empty WS2 hid them, then returning revealed both without
  foreground landing. The return reply emitted remembered focus but held it
  `stale`, with two visibility and five other pending wakes. The departure also
  had one foreground wake. Counts distinguish causes without draining or
  weakening gates, but do not prove individual events are self-generated.
  This is not occupied-workspace acceptance in both directions. Stop/restore
  released the owner, cleaned the ledger and shut down the fixture; no owned
  processes remained.
- Latest offline gates: `mise exec -- cargo test -p tiler-core --locked`
  (844 library tests plus integration suites), Windows equivalent (963 passing
  library tests, one existing ignored test, integration suites and doc tests),
  strict all-target clippy for both crates, workspace fmt and
  `git diff --check` passed after integrating the native-settlement changes.
  These green checks include diagnostics of the unresolved stale hold, not
  native focus acceptance.
- Tier 1 source findings and unsupported cells are recorded in the
  [minimal no-focus discriminators](../spec/reference-outcomes.md#workspace-no-focus-discriminators).
  No new product policy is selected.

## Offline-proven portions (2026-10-25)

- Core selection emits the existing remembered tiled-focus intent and pager
  marks only the current workspace. Independent review found two global-swap
  routes dropping the new effect; regressions failed before correction and
  pass after capturing the destination's remembered window before session
  rekey. Empty destinations remain quiet. No new option, public contract,
  unremembered-focus fallback, or adapter-owned focus policy is added.
- Full core tests pass: 847 library tests and 436 integration tests. Full
  Windows tests pass: 1,385 tests, one existing ignored test. Strict all-target
  clippy passes for both crates. Native focus acceptance remains blocked.
- Hook wakes now preserve exact foreground/create/destroy/show/hide/location
  kinds, native thread/time fields, and a delivery-time PID/member-tag read.
  Destroy stays unknown. A bounded, non-consuming diagnostic joins a token
  only against a single matching PID/tag candidate and reports unknown or
  mismatched generations explicitly. Delivery-time reads do not establish the
  original event generation or its initiating operation. All existing
  pending-event vetoes remain intact.
- A matching HWND, event kind, timing interval and final state are not causal
  proof: an application's own change can produce the same notification.
  `ShowWindowAsync` and the delivered WinEvent carry no shared operation token.
  No freshness exemption based only on those matches is accepted. In
  particular, a foreground transition after hiding the active window cannot
  simply be assumed to be a self-generated fallback.

## Remaining work

- Resolve same-pass view-event/focus freshness ordering without losing genuine
  intervening-change protection or violating at-most-once focus. Preserve event
  kind and window-lifetime attribution before attempting any self-event
  classification; per-HWND wake removal alone is insufficient. Add a desired-
  behavior regression against the non-consuming native contract before another
  fix attempt. Cross-reply focus replay would change the one-shot contract and
  is not selected here.
- Verify foreground, visibility and actual pager-cell data on occupied workspace
  switches both ways, including the second destination arrival. Pager lifecycle
  traces and the current empty-workspace diagnostic do not meet that acceptance.
- Global-swap focus intent has offline regressions; shared variants and
  floating remembered-focus coverage remain outside demonstrated acceptance.

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
- The correction emits core remembered-focus intent for explicit selection;
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
  fresh desktop/destination observation clears or replaces it. The core now
  marks focus only on a current workspace. Regression covers stale departing
  focus, native destination catch-up and foreign focus; it never substitutes
  desired remembered focus for observed foreground. The Windows mapper also
  needed correction; actual renderer-payload evidence is recorded below.

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
  delayed focus replay is implemented.
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
- Earlier offline gates: `mise exec -- cargo test -p tiler-core --locked`
  (844 library tests plus integration suites), Windows equivalent (963 passing
  library tests, one existing ignored test, integration suites and doc tests),
  strict all-target clippy for both crates, workspace fmt and
  `git diff --check` passed after integrating the native-settlement changes.
  These green checks include diagnostics of the unresolved stale hold, not
  native focus acceptance.
- Tier 1 source findings and unsupported cells are recorded in the
   [minimal no-focus discriminators](../../spec/reference-outcomes.md#workspace-no-focus-discriminators).
  No new product policy is selected.

## Offline-proven portions (2026-10-25)

- Core selection emits the existing remembered tiled-focus intent and pager
  marks only the current workspace. Independent review found two global-swap
  routes dropping the new effect; regressions failed before correction and
  pass after capturing the destination's remembered window before session
  rekey. Empty destinations remain quiet. No new option, public contract,
  unremembered-focus fallback, or adapter-owned focus policy is added.
- Windows same-pass reveal eligibility re-reads visibility but retains the
  original identity baseline at the final setter gate. A regression exposed
  that replacing the baseline could accept a recycled HWND with a matching
  tag; it fails before correction and now stops without spending focus.
- Actual Windows pager miniatures previously recomputed foreground equality
  without the current-workspace guard, independently of the core's workspace
  focus mark. The shared production mapper now rejects that departing-view
  highlight too. Regression covers stale foreground, observed catch-up and
  no focus. Trace-only `pager-cells` records the actual renderer payload,
  bounded with honest omission counts and change-only emission.
- Full core tests pass: 847 library tests and 436 integration tests. Full
  Windows tests pass: 973 library tests, one existing ignored test, and all
  integration suites. Strict package-scoped all-target clippy passes for core,
  Windows and the fixture; workspace fmt and `git diff --check` pass. Native
  focus acceptance remains blocked.
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

## Occupied native diagnostic (2026-10-25)

- At `c6d7dd38`, a fixture-only, single-output run used exact-owner selection
  controls and fixture focus requests, without synthetic input. A was observed
  focused on WS1; B then C were observed focused on WS2. Returning to WS1
  emitted A; selecting WS2 emitted C. Visibility switched correctly both ways,
  but both trailing effects stopped `stale` without dispatch. Foreground moved
  to an unrelated pre-existing application during hiding and stayed there.
- Actual `pager-cells` payloads corroborated the view switches: departed cells
  retained their miniature windows but no focus marks. Current cells also
  carried no marks, truthfully matching native foreground. This verifies the
  departing-highlight repair, not successful remembered-focus landing.
- Exact pending rows identify fixture hide/show notifications and unmanaged
  taskbar location notifications; one switch also delivered the unrelated-app
  foreground notification. Row caps honestly omitted seven and five edges,
  respectively, so not every show notification is individually attested.
  A matching delivery-time PID/tag still does not identify the initiating
  operation. The foreground notification does not distinguish hide-induced
  host fallback from an independently initiated user/app focus change.
- Graceful stop and independent restore completed; fixture readbacks showed
  every owned window visible before shutdown. Settings bytes and sampled
  taskbar/Mouse Snap state matched their preimages. No owned process, recovery
  ledger or pending request remained. Raw restore reported zero windows;
  visibility readbacks, not that count, establish the final visible state.
- Genuine competing focus during the switch was not deterministically staged;
  supersession acceptance remains unverified. The universal pending-event veto
  is unchanged and is not evidence of a selective self-event mechanism.
- CI [40187337758](https://github.com/beefsack/OmniTiler/actions/runs/40187337758)
  is not green. Linux Rust repeats the prior KDE settings catalog partition
  failure (19 mapped fields versus 23). Windows repeats four prior macOS
  dock-swipe tests refusing an unreadable macOS version, plus one additional
  scripted-topology exhaustion in `wrong_sid_arrival_is_unproven_without_replay`.
  The latter is unclassified, not a proven new regression or proven flake;
  macOS source is unchanged by these repairs. The native macOS, KWin, native,
  shell, licenses and notices jobs pass. These failures remain outside this
  Windows focus repair; package-scoped local gates are not full-CI acceptance.

## Alternative native mechanism (2026-10-25)

- Standalone fixture comparison found that `ShowWindowAsync(SW_HIDE)` moved
  foreground from the departing fixture window to another window, whereas
  `SetWindowPos` with `SWP_HIDEWINDOW | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS`
  kept its foreground HWND unchanged after visibility became false. Matching
  nonactivating reveal restored both owned windows; fixture shutdown and
  native process absence verified cleanup. This is a primitive experiment,
  not integrated workspace acceptance.
- **Tentative, pending user review:** use nonactivating async visibility
  writes with non-pumping confirmation only for a pure view matching the
  focus target's live homing. Retain full destination-view proof before
  focus, existing visibility deadlines, ledger/recovery fencing and the
  non-consuming pending-event veto. Unrelated views, migration/refill and
  switches without focus intent retain their existing path; minimized
  reveal retains its existing pumped restore command and fails closed if
  that produces pending events.
- **Tentative, pending user review:** compare full native foreground identity
  before/after the switch; changed or unreadable foreground prevents focus
  dispatch. Re-prove native move/size state independently of delayed hooks.
  The existing fullscreen/game, lifetime and at-most-once gates remain.
- **Tentative, pending user review:** for the departing foreground hidden by
  this same nonactivating switch only, read fresh hidden-window facts under
  the exact committed claim and live identity/member-tag fences. Apply the
  fullscreen and eligibility predicates to these facts rather than treating
  expected invisibility as unreadable. Old hidden claims, unknown facts,
  fullscreen, recycled lifetimes, tag drift or foreground drift refuse;
  frame writes retain their original unreadable-foreground veto.
- Activating before departure hides was not selected: it would require
  narrowing the existing full-view-before-focus gate. Native nonactivating
  visibility instead offers a way to retain that gate without classifying
  queued events as self-generated.

## Accepted outcome (2026-10-26)

- The alternative removes the stale-event blocker without classifying,
  suppressing or exempting queued events. No new core change, option,
  public contract, no-memory fallback or cross-reply replay was needed.
- Desired-behavior regressions failed before the repair: self fallout held
  focus stale; foreign foreground with an empty delivered queue incorrectly
  allowed dispatch. Both pass, alongside matching-view routing, unchanged
  unrelated-view routing, failed reveal, duplicate effects, lifetime drift,
  minimized restoration and native move/size refusal tests.
- Initial integrated checks exposed a second causal blocker: the fullscreen
  guard's visible-only reader rejected the intentionally hidden departure,
  refusing before the native focus setter. A foreground-rights positive
  control proved real movement between visible windows, and an explicit
  rights grant did not repair the return. The same-pass hidden-facts repair
  above resolves that blocker; production-seam tests now model the reader's
  actual hidden-window refusal instead of returning fabricated visible facts.
- Final fixture-only native run: A observed focused on WS1; B then C observed
  focused on WS2. Selecting WS1 moved native foreground C to visible A;
  selecting WS2 moved it back to visible C. Fixture visibility/focus agreed.
  Both setters accepted. Immediate foreground proof was still false during
  the native transition; subsequent independent native reads and fresh
  observations, not setter acceptance, establish landing. The forward
  pre-switch field was omitted by the harness; bracketing native A/C reads,
  unchanged intervening focus and dispatch-time false proof establish movement.
- Actual renderer `pager-cells` payloads caught up from no highlight to A on
  current WS1 and C on current WS2; departed cells retained their miniatures
  without focus marks. WS2's second miniature, not its first, was highlighted.
- A separate unmanaged fixture window received genuine foreground from an
  exact-departure hide-event watcher during the final switch. The owner held
  `foreground-changed` without dispatch; independent native reads confirmed
  the competitor retained foreground and the pager showed no managed focus.
  Earlier checks also demonstrated this guard twice before the final repair.
- Test-only foreground rights used an explicit owner grant; guarded input,
  where needed to establish grant rights, targeted an identity-verified owned
  fixture. No product input filter or focus privilege mechanism was changed.
  Graceful stop, independent restore, all fixture shutdowns and native process
  absence passed. Settings bytes and sampled taskbar/Mouse Snap state matched
  preimages; no recovery ledger or owned control residue remained. Raw restore
  count zero is not used as visibility evidence.
- Final gates: core 847 library plus 436 integration tests; Windows 1001
  library tests, one existing ignored test, 413 integration and four doc tests.
  All pass. Strict all-target clippy for core, Windows and fixture, workspace
  fmt and `git diff --check` pass. Final native owner artifact SHA-256:
  `1d60cae8fe4fb0c2c535a4917d0d4675520771742d7b1aabc0530a69d3c0461f`.

## CI classification (2026-10-26)

- Prior CI [40188690771](https://github.com/beefsack/OmniTiler/actions/runs/40188690771)
  repeats the same KDE catalog partition failure and four macOS-version
  refusals on Windows as 40187337758. Its extra
  `wrong_sid_arrival_is_unproven_without_replay` failure is the same scripted
  topology-row exhaustion, not a Windows focus-effect regression.
- The macOS source and that test's execution cone are unchanged across these
  focus repairs. The exact test passes at pre-focus parent `a081a7f1` and in
  15 consecutive isolated current runs. Its fixed four-row script is sensitive
  to extra deadline-loop reads; an early timeout is a plausible trigger, not
  a reproduced causal diagnosis. Classify it as pre-existing test fragility,
  not proven new focus behavior. No unrelated macOS correction is included.
- Package gates are green; full CI is not claimed green from these results.

## Remaining verification and decisions

- Physical workspace chords, ordinary applications, multiple-output/shared
  variants, floating remembered windows and game behavior were not established
  by this single-output fixture run. Global-swap intent has offline coverage.
- A workspace genuinely lacking remembered focus still has no selected fallback
  policy; the pinned Tier 1 discriminator rows remain the decision evidence.

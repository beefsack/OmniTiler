# V4 KDE focus feedback and anchored border

## Goal and scope

- Port causal one-shot focus feedback and the native anchored-border consumer
  using the approved v4 contract. Restore the OmniTiler active border.
- Close populated-selection completion through truthful native feedback, never
  weakened core completion, fabricated acknowledgement or replayed effects.
- Exclude state targets, scaled metrics and reconnect/rehome. Keep the active
  phase-1 migration and resize-diagnostics records unchanged.

## Acceptance

- Authorized focus and mechanical refusal observations retain exact effect,
  target, issuing revision, lifetime and original attempt correlation.
- Production entry-to-real-decoder regressions prove feedback and populated
  selection completion without reissuing spent one-shot work.
- Native evidence joins correlated wire to authoritative focus and border state,
  including witness/lifetime vetoes and frame-event anchor re-resolution.
- Full adapter and scoped Rust gates pass without new skips; immutable fixtures
  and enabled stage5 production-route proofs remain unchanged.
- Independent integrated review has no unresolved in-scope finding. Leave a
  usable unscoped non-trace build and state exact limitations.

## Approach and bounded units

1. Audit existing focus application, capture/retention and native presentation
   consumers against the approved contract; identify only missing mechanics.
2. Port causal feedback and native anchored descriptors with genuine decoder
   regressions, preserving freshness, authorization and at-most-once fences.
3. Run offline gates and small private nested fixture proofs, then integrated
   review. Remove touched obsolete paths separately when evidence permits.

## Current state

- Focus feedback now rides retained, generation-bound evidence with exact
  effect, target, issuing revision, broker clock and live foreground reads.
  Arrival brackets use the narrow shown witness and preserve the broker-owned
  observation separately from the local invalidation fence. Anchored borders
  remain intentionally hidden pending the native consumer port.
- Populated-selection completion requires truthful shown evidence:
  `world-report.ts` truthfully reports shown state as unknown unless hidden or
  minimized. Synchronous `windowAt` membership supplies a positive shown witness
  without reading a private show-desktop flag or extending a native bridge.
- Core arrival proof requires visible, non-minimized, non-hidden matched-lifetime
  samples around native home/frame reads. Setter acceptance or foreground
  equality cannot replace that proof. No completion rule or fact has been
  weakened.
- Broker observation, script native invalidation and native effect coverage are
  distinct counters. Witness and arrival sampling must preserve their causal
  joins rather than equating them.
- Host shown-witness acceptance and offline focus integration are complete.
  Native focus/terminal acceptance, the anchored consumer and final integrated
  review remain pending.

## Shown evidence

- The rejected global derivation was
  `visible = !minimized && !hidden && !showingDesktop`, reading the public
  `/KWin` `org.kde.KWin` D-Bus property.
- Source verification at both KWin pins found a coverage exception:
  `Workspace::breaksShowingDesktop` excludes `belongsToDesktop()` windows
  (`src/workspace.cpp:1766` at `1b798c05c19b1124edb1e16623191fcd2c128fd0`,
  `:1679` at `e53bb3be7ecf1735c7739b797f94d66de422ab4d`).
  `WaylandWindow::belongsToDesktop` (`src/waylandwindow.cpp:136`, both pins)
  can apply to Normal/Dialog windows sharing an application with a desktop
  window. Such rows pass the adapter candidate filter
  (`kwin/src/world-observe.ts:852-871`). During show-desktop they can remain
  shown, so the selected formula would falsely report them invisible.
- User decision: keep `minimized || hidden` as proven false; otherwise call
  `workspace.windowAt(frameCentre, -1)` synchronously. Exact live-lifetime
  membership proves true; absence, unreadability or a failed hit-test stays
  null, never false. Use the narrower arrival-sample route rather than changing
  general window-row visibility. No shared v4 shape, core predicate, token,
  D-Bus freshness channel or effect dependency changes.
- The host filters deleted, minimized, hidden, show-desktop-hidden,
  off-current-desktop and off-current-activity windows before hit-testing
  (`src/scripting/workspace_wrapper.cpp`, both pins). Negative count returns
  every matching window, so occlusion cannot falsify a positive witness.
- Lifetime identity, minimized, hidden, native home/output, frame and foreground
  facts are script-readable at both pins. No fallback is needed for those
  fields on the present audit evidence.
- Live host proof: two Wayland and two XWayland fixture windows on a workspace
  above freshly inventoried occupancy, counting sticky windows on every native
  desktop. Every frame-centre query returned all four overlapping fixtures with
  exact unique native lifetime joins. Show-desktop excluded all four while they
  stayed listed, non-minimized and non-hidden; toggling back restored membership.
  Fixtures and helpers were removed; the appended empty desktop remains.
- The earlier probe incorrectly excluded sticky occupancy and is not accepted
  as guide-compliant evidence. The corrected run supplies the accepted proof.
  Minimized/hidden false remains source-verified, not separately live-probed.

## Terminal semantics and review

- `settle_selection_feedback` latches arrival once at the first exactly
  correlated feedback row (`world_selection.rs:1283-1364`). With native selector
  proof resolved and all visibility-write verdicts present, failed arrival
  produces `completed-with-failures` with `focus:unproven`, not an indefinitely
  pending switch (`:1896-1945`). Missing feedback, member verdicts or pending
  native proof can still hold completion.
- Normative apply rule 3 requires fresh brackets or no-call reasons before an
  `awaitShown` focus; failed/blocked required shows withhold it and the grant
  stays spent. KDE has no controlled per-window visibility targets or writes:
  its native selector owns showing, so there is no required visibility setter
  to withhold on. Arrival is core accounting, not a new adapter frame-equality
  activation gate. KDE parses `switchSettled` but has no notification or
  presentation consumer for those terminal events.
- Measure populated-switch arrival failures during integration, preserving exact
  frozen desired and live frame comparisons. Size hints, native settling and
  scaling must not be papered over with a platform-specific predicate change.

## Verified focus increment

- Production entry through the real v4 decoder proves populated selection
  `complete` / `proven`, and healthy native foreground with an unknown shown
  witness proves `completed-with-failures` / `unproven`. Restoring the witness
  later neither upgrades the terminal nor reissues focus.
- Duplicate accepted callbacks do not re-actuate. Request-loss redelivery keeps
  the original feedback id and attempt stamp until a bound acknowledgement.
  Reply-loss terminal delivery is not established by that test.
- Targeted proofs cover native refusal, throws, setter-independent foreground,
  absent raise/stack targets, unreadable or changed lifetime, reentrant probe
  invalidation, divergent observation/fence counters, bounded retention and
  generation reset. General window-row visibility is unchanged.
- The first full adapter gate exposed seven regressions in legacy safety-shape
  and foreground doubles. The repair separated raw visibility from the existing
  minimized/hidden safety alias and supplied truthful foreground readers; it
  also made unreadable identity fail closed for proof.
- Current gates: adapter typecheck and full suite pass (1012 tests, zero skips);
  scoped core/protocol/service tests, formatting and strict all-target clippy
  pass. Independent focus-increment source review has no blocking finding.
  Immutable fixtures and enabled stage5 production-route proofs are unchanged.

## Interrupted apply accounting

- First native integration measured four populated switches across Wayland and
  XWayland fixture pairs. All switched desktops and restored native foreground,
  but none produced focus feedback or a terminal. Native frame snapshots matched
  published desired frames; without arrival brackets this is not an arrival
  proof or an exact-frame predicate failure rate.
- Synchronous native view/focus signals advanced the local invalidation fence.
  Earlier apply phases returned before the focus section, bypassing its spend
  and no-call accounting. This is an adapter producer defect, not evidence that
  the core must replay one-shot grants.
- A common interrupted-apply epilogue now spends remaining focus grants and
  reports correlated `not-called` / `superseded` evidence with empty arrival
  brackets, retaining fresh raw foreground and the original pre-apply broker
  clock. It never weakens the fence, invents arrival or reissues focus.
- Regression coverage reproduces synchronous native desktop/focus signals
  through the real decoder: native foreground restoration with no focus setter
  now yields one `completed-with-failures` / `unproven` terminal. Additional
  proofs cover earlier geometry interruption, remaining grants after a mid-focus
  break and a clock changed reentrantly during apply.
- Current adapter gates pass (1015 tests, zero skips) and independent follow-up
  review has no blocking finding. Native confirmation of this correction and
  measurement of actual arrival-predicate failures remain pending. No conclusion
  about all native KDE selections follows from this four-switch sample.
- The exact foreground/lifetime readers made the old `isActiveWindow` boolean
  callback unused. Its interface, implementation and test-only mock keys were
  removed separately; source search finds no remaining caller. Typecheck and
  all 1015 adapter tests remain green.

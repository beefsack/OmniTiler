# V4 KDE focus feedback and anchored border

## Goal and scope

- Port causal one-shot focus feedback and the native anchored-border consumer
  using the approved v4 contract. Restore the OmniTiler active border.
- Close populated-selection completion through truthful native feedback, never
  weakened core completion, fabricated acknowledgement or replayed effects.
- Healthy native populated switches must finish `complete` / `proven`, with the
  core-selected focus target actually dispatched at most once.
- Exclude state targets, scaled metrics and reconnect/rehome. Keep the active
  phase-1 migration and resize-diagnostics records unchanged.

## Acceptance

- Authorized focus and mechanical refusal observations retain exact effect,
  target, issuing revision, lifetime and original attempt correlation.
- Production entry-to-real-decoder regressions prove feedback and populated
  selection completion, including synchronous selector side effects, without
  reissuing spent one-shot work. Genuine supersession still reports no-call.
- Native evidence joins correlated wire to authoritative focus and border state,
  including witness/lifetime vetoes and frame-event anchor re-resolution.
- Full adapter and scoped Rust gates pass without new skips; immutable fixtures
  and enabled stage5 production-route proofs remain unchanged.
- Native presentation uses one in-place `SetPresentation` v2 format with the
  unchanged seven-key anchored v4 border. Mismatch hides border, underlay and
  preview quietly while tiling continues. Recovery retains all legacy coverage
  through real-path v2 recordings in a sibling fixture directory; legacy stage5
  bytes remain unchanged.
- Independent integrated review has no unresolved in-scope finding. Leave a
  usable unscoped non-trace build and state exact limitations.

## Approach and bounded units

1. Distinguish genuine supersession from synchronous selector side effects using
   causal evidence, not timing. Evaluate retaining undispatched focus grants for
   the next fresh apply before introducing another mechanism.
2. Correct the interrupted-apply path and regression, then measure healthy native
   populated switches and actual arrival-predicate failures.
3. Establish the real dispatch-recording path before porting the v2-only native
   consumer and sibling recovery fixtures. Preserve core-produced anchored
   metrics and existing coverage, lifetime and live-frame event fences.
4. Measure border witness dropouts during normal focus/move/resize, run offline
   and private nested fixture proofs, then integrated review. Remove touched
   obsolete paths separately when evidence permits.

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
  Native focus/terminal acceptance now has a bounded healthy sample. The
  anchored consumer and final integrated review remain pending.

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
- The first interrupted-apply correction added an epilogue that spends remaining focus grants and
  reports correlated `not-called` / `superseded` evidence with empty arrival
  brackets, retaining fresh raw foreground and the original pre-apply broker
  clock. It never weakens the fence, invents arrival or reissues focus.
- Its regression reproduces synchronous native desktop/focus signals
  through the real decoder: native foreground restoration with no focus setter
  now yields one `completed-with-failures` / `unproven` terminal. Additional
  proofs cover earlier geometry interruption, remaining grants after a mid-focus
  break and a clock changed reentrantly during apply.
- That healthy-switch outcome is insufficient and is not accepted: own selector
  side effects must not be treated as genuine supersession. The epilogue and
  regression need narrowing or replacement; undispatched work is not issued
  work. Issued effects remain spent, and no core replay is required on present
  evidence.
- Adapter gates passed (1015 tests, zero skips) and follow-up review found no
  blocking issue, but neither establishes healthy native acceptance. Measurement
  of actual arrival-predicate failures remains pending. No conclusion about all
  native KDE selections follows from this four-switch sample.
- Causal audit found workspace `currentDesktopChanged` and `windowActivated`
  signals bypass the existing exact-window own-homing echo handling. Pinned
  KWin selection restores focus synchronously before the desktop-change signal;
  focusing the already-active window does not emit another activation signal.
  Next correction: scope deferral to the exact setter and validate native view,
  activation subject and lifetime; unexpected signals must still invalidate.
  This avoids cross-reply authority retention. Retention was evaluated: existing
  core correlation accepts later feedback, but it would require additional
  adapter supersession bookkeeping and is unnecessary if same-apply causal
  evidence suffices. Neither route changes the core arrival predicate.
- The exact foreground/lifetime readers made the old `isActiveWindow` boolean
  callback unused. Its interface, implementation and test-only mock keys were
  removed separately; source search finds no remaining caller. Typecheck and
  all 1015 adapter tests remain green.
- The corrective scoped guard now validates both supported desktop-signal
  signatures, the exact requested native view/output and live activation
  lifetime/home/foreground. It validates again after the setter returns;
  unexpected intermediate signals remain invalidating even if final state
  returns to the target. Accepted own echoes coalesce after apply. No retained
  grant, core replay, shared shape or arrival predicate was added.
- The production real-decoder synchronous-selector regression now requires one
  core-target dispatch and `complete` / `proven`. Targeted guards cover a
  different native focus-chain member, unsupported or mismatched signal
  payloads, foreign/unlisted/replaced activation, final silent home drift,
  outside-setter events and native setter throws. Silent drift is a defensive
  read-back case, not an established native behavior.
- Current typecheck and full adapter suite pass (1028 tests, zero skips).
  Independent source review found no blocking issue; the final view-lifetime
  read-back was strengthened after review. The subsequent native sample below
  establishes bounded healthy-switch and arrival-predicate evidence.

## Native healthy-switch evidence

- The corrected fixture staging selected a desktop above freshly inventoried
  sticky-inclusive occupancy before any fixture spawn or controller start. Two
  Wayland and two XWayland fixtures joined by caller sentinel and PID to four
  unique native lifetimes, on two populated test desktops.
- Four alternating selections each produced one exactly correlated focus grant,
  one target-specific native setter attempt/result, one accepted feedback row
  and one `complete` / `proven` terminal with no failed members. All eight frozen
  member checks matched desired frame, native home, lifetime, shown state and
  observation brackets. Off-desktop unknown samples in the adapter's wider
  bracket set were not frozen members and are not counted as failures.
- This finite sample measured zero arrival-predicate failures, including zero
  exact-frame failures. It does not prove arbitrary client hints, scaling,
  multi-output behavior or physical shortcut delivery. Independent native
  inventories after each selection confirm the target foreground and frames.
- An earlier run admitted fixtures before moving above occupancy and is not
  accepted as guide-compliant evidence. The corrected run's initial controller
  startup lacked its planner and failed discovery; the later ready instance's
  correlated evidence establishes the results above, not the failed startup.
- Fixtures and helpers were removed, the controller and planner stopped, and
  the unscoped non-trace bundle restored. Appended empty desktops remain.

## Presentation freshness audit

- Every accepted apply in the healthy-switch run had a stale-coverage read
  (11 of 11). The native effect was absent: all effect-facts calls failed,
  captures truthfully carried no native invalidation witness, and the broker
  returned null coverage. The subscriber correctly sent no paint. This is not
  evidence of a joining witness dropping during normal native rendering; K5
  still needs an effect-loaded proof.
- Presentation marker bodies reached the bus intact. A synchronous probe
  wrapper completed with `undefined` before its asynchronous callback, so those
  helper markers are not callback-delivery evidence. A correctly asynchronous
  small marker joins to one exact return before the journey; no payload-size
  failure is established. Future asynchronous probes must not use the
  synchronous wrapper.
- The causal view/focus guard was compared once with own-homing handling. The
  latter has neither cross-entity view/activation payload validation nor final
  read-back. No clear simplifying merge preserves those distinct checks;
  post-apply observation is already shared.

## Approved native presentation port

- Keep the exact v4 border keys `visible`, `reason`, `subject`, `outsets`,
  `width`, `radius`, `witness`; script validation/pass-through must not derive
  absolute geometry. The effect resolves the subject's live frame and hides on
  lifetime or witness mismatch, without polling or rescaling core metrics.
- Use `SetPresentation` v2 only, preserving envelope, coverage, underlay and
  preview. Version mismatch hides all three surfaces without repeated noise.
- The recovery test currently replays legacy v1 dispatch and facts recordings. Establish their
  real producer and record v2 siblings before changing parser dependencies;
  hand-written replacement bytes and modification of legacy fixtures are out of
  scope. Update metadata/static version pins with the port.
- Remove `borderStrokeFromPublished` only after verifying no live callers, in a
  separate change with its active documentation updated. Historical records
  remain unchanged.

## Real dispatch recording dependency

- The existing real route is available offline:
  `kwin/tests/world-stage5-v4-record.ts` drives `startWorldAdapterEntry` through
  `broker_world_eval`; `world-stage5-fixture.test.ts` then drives the production
  presentation subscriber against the genuine projections with an independent
  complete-payload oracle. A new sibling recording can capture that subscriber's
  renderer bytes. The legacy recovery test's visible/hide/restore border checks,
  preview producer crossings, coverage fences and facts-source checks must all
  remain intact.
- Unchanged v4 projections are insufficient: they carry hidden borders because
  `world-report.ts` still hardcodes `presentation.witness:null`. Merely recording
  a v2 formatter would lose the legacy visible-border recovery assertions. The
  native capture-to-witness join must be implemented and proved before recording
  genuine visible anchored projections and repointing recovery.
- The broker preserves the typed presentation witness unchanged while stamping
  its own report observation, and separately strips/caches the effect-owned
  invalidation witness. Neither broker stamping nor core border eligibility
  currently creates the missing native witness. The capture identifier and its
  exact join to effect coverage remain the next bounded investigation; no
  counter equivalence or new wire format is established by this audit.
- Existing fixture bytes remain unchanged. The new sibling must be generated
  through the actual entry/broker/subscriber path, not by editing projections,
  converting legacy payloads or weakening recovery assertions.

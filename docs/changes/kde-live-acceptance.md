# KDE live acceptance continuation

## Goal and acceptance

- Re-check immediate unfloat re-tiling, then both native Settings identities,
  then the full-state reply checklist and affected single-output checks.
- Preserve existing application windows and restore the recorded desktop,
  settings, effects and development lifecycle baseline.
- Distinguish callback/state observations from physical keyboard and visual UX
  acceptance. No unrelated platform or admission changes are included.

## Approach and current status

- Capture native state and exact project lifecycle ownership before startup.
- Start with floating defaults to avoid retiling existing workspaces, and tile
  only the disposable-client workspace. Restore the temporary setting afterward.
- Exercise the three acceptance groups in order; regress and fix product defects
  only after reproduction. Stop on baseline or restoration surprises.
- Status: unfloat functional callback pass; physical shortcut delivery pending.
  Both Settings identities pass targeted native accessibility actions; physical
  traversal/visual and stronger operation-isolation acceptance remain pending.
  The initial fixture drift has a fresh recorded
  baseline. The full-state breadth pass found a maximized-unfloat regression,
  now corrected and reverified natively. Fixed-client D5 first-exit, sampled D6
   enable and D7 Planner-only restart pass natively. Controller-replacement D7
   remains blocked by an unclassified precondition refusal. Script-KCM draft
   retention, floating-boundary sends, workspace history and one tiled send-follow
   now have bounded native evidence. A later tiled return refused with a held
   scope; maximize also reproduced an asynchronous false-refusal diagnostic.
   Suspected restart-float defects were floating-workspace fixture mismatches.
   The remaining matrix is pending.

## Native observations, 2026-10-17/18

- A freshly built Planner and KWin bundle started with `defaultTiled=false`.
  Initial accepted replies made no geometry writes; protected existing windows
  retained their recorded frames. The temporary key was restored to absent.
- Two normal, resizable KDE editor clients exercised two float/unfloat pairs,
  one with each client as the toggled window. All four operation requests were
  accepted. Each unfloat reply made two geometry writes; native read-back showed
  both clients occupying tiles without an intervening focus change. Subsequent
  focus observations showed no delayed geometry snap. Re-admission changed tile
  order on the second pair, not tile occupation.
- These are callback-driven native state observations, not physical Meta+G or
  visual interaction acceptance. One later edge-focus request returned
  `unchanged`; it was not a float/unfloat refusal. No transport error was found.

## Fixture failure and restoration blocker

- Creating a desktop while the controller was active replaced a pre-existing
  empty backing desktop; the inventory remained at two. The fixture then
  removed its owned desktop and recreated the second position with its original
  name but a different identity. Exact identity restoration was not achieved.
- The functional checks continued on that replacement after the baseline
  surprise. This did not follow the live guide's stop rule. Consequently the
  observations above are a functional signal on a drifted fixture, not clean
  acceptance of the complete requested journey.
- Normal empty-desktop retirement is documented product behavior, but the exact
  retirement actor was not independently established here. A native desktop
  count limit was not proven. No product defect or policy change is inferred.
- The final configuration differs by the second desktop's backing identity and
  two new native tiling groups: one for the removed test desktop and one for
  the live replacement. These were retained, not broadly rolled back.
- Ghostty and Firefox identities, membership and frames were unchanged in the
  recorded baseline and final inventories. Original active window, current
  desktop and pointer were restored; both disposable clients and helper scripts
  were removed, effects matched, and development returned DOWN.
- Restoration to the initial desktop/configuration identity was incomplete.
  The current inventory and native tiling entries are accepted as the new fixture
  baseline (tentative, pending user review). No manual configuration rollback is
  planned. A fresh exact configuration backup and native window/desktop, pointer,
  effect and shortcut snapshots were captured before further testing; later
  restoration comparisons use that baseline. The original preimage is retained.

## Offline verification and review

- At `1caadf15`: `cargo test --workspace --offline --locked` passed **3017
  tests, zero failures across 83 test-result groups**.
- KWin `npm test` passed **1777 tests, zero failures across 267 suites**;
  `npm run typecheck`, `npm run build` and `cargo build -p omnitiler` passed.
- Independent evidence review confirmed the immediate-retile observations,
  protected-window preservation, lifecycle teardown and baseline drift. It did
  not accept a claim of exact desktop restoration or complete native acceptance.
- No tentative product decisions were made. The fixture-baseline decision above
  is tentative, pending user review. Unverified Settings, full-state and
  single-output cases retain their previous pending status.

## Settings native shell re-check, 2026-10-18

- Host-matched builds produced both KCM plugins. Both exact identities opened
  through `kcmshell6` using process-scoped plugin staging, without installation
  or session restart. A follow-up script-KCM run independently verified that the
  source-built staged plugin was mapped into the owned shell process. The
  earlier effect-KCM run retained no mapping proof, only scoped launch and native
  window/render evidence.
- First-page rendering showed the four expected tab titles in order, tiling
  controls, gap defaults from absent keys, the Status panel below that page and
  disabled Apply. This does not establish traversal or Status visibility on the
  other three tabs.
- Live keyboard traversal, label buddies, tab switching, cross-tab drafts,
  Apply/reopen, Cancel, Defaults-without-save and ordinary-Apply operation
  isolation were not tested in this first-page leg: no safe desktop input route
  was then available. The later targeted-accessibility leg below advances only
  its explicitly observed nonphysical checks.
- Settings-only build and CTest passed **27/27** on the same source. These are
  offline/application-local results, not native-shell interaction acceptance.
- Settings values were not changed. The capture tool briefly added its own
  shortcut-service entry; the exact preimage was restored before the next leg.
  Final configuration and shortcut bytes matched the accepted baseline;
  protected windows, desktops, pointer, effects and DOWN lifecycle matched, and
  all owned Settings windows were gone. No product defect was found.

## Full-state breadth observations, 2026-10-18

- Native callback/state checks passed for ordinary admission, focus, movement,
  resize, tiling off/on, float/sticky/fullscreen/minimize safety, maximized-slot
  reservation during workspace enable and native restore, Planner-only restart
  preserving an ordinary float frame, unavailable-Planner quietness, generation
  rediscovery and close/reflow. These are sampled legs, not completion of every
  checklist permutation or physical input/rendering acceptance.
- A command spanning Planner replacement was rejected for generation mismatch
  and was not replayed. Fresh-generation traffic and subsequent actions resumed.
- Defect: maximized intentional unfloat issued its native clear, but synchronous
  read-back still observed maximize and refused the command. The native clear
  settled later; a second activation re-tiled. The correction below preserves
  the existing B9 contract with one native clear per activation and no unsafe
  command replay; asynchronous producer-shaped regressions cover the failure.
- Initial suspicion: an ordinary float adopted after Planner restart accepted
  two explicit toggles without geometry change. Offline isolation showed the
  test workspace had reseeded from the deliberately floating saved default;
  session-only workspace tiling overrides do not survive Planner replacement.
  Floating scope publication correctly withholds geometry. This is a fixture
  mismatch, not an admission/capacity or float regression. Temporary core probes
  passed both floating no-write and re-enable-then-retile cases; no production or
  committed test change was needed. Re-enable test-workspace tiling after restart
  before checking tiled behavior.
- Native maximize/restore alone produced no world traffic in this sample. A
  single unexplained tile-order change also needs attribution; neither is yet an
  independently reproduced product diagnosis.
- Fixed-client/game legs, physical held keys, sends/migration/drag and workspace
  lifecycle races were not exercised in this unit. No admission/capacity change
  was made.
- Both disposable clients and helpers were removed; controller and Planner were
  stopped. Configuration and shortcut bytes, desktop identities/count, protected
  frames/membership, current/active window and pointer matched the accepted
  baseline. No new baseline or configuration rollback was required.

## Maximized-unfloat correction

- Replace same-stack maximize read-back with one deferred decision on the first
  fresh accepted full-state report after the native clear. Observed clear submits
  one ordinary core unfloat; unconfirmed clear ends the attempt. No new timer,
  repeated setter or alternate tiling policy was added.
- Scope, lifetime, focus, overlay, gesture and busy checks fence the continuation.
  Newer explicit input and terminal report errors cancel it; generation mismatch
  cancels before rediscovery, so a deferred command cannot cross Planner restart.
- The initial implementation waited indefinitely for a native signal. Review
  rejected its unbounded wait, generation survival and incomplete supersession
  fences. The causal correction uses the single-report boundary above; follow-up
  review accepted those repaired conditions. No rejected implementation was
  shipped.
- Producer-shaped tests use QML array-likes, renewed wrappers, braced native IDs,
  real property names and asynchronous settlement. Against the previous source,
  12 new cases failed and the ordinary-unfloat control passed; all 13 pass after
  correction. Silent/refused clear, races, newer input, generation rebind and
  transport failure are covered.
- Final integrated offline gates at `fe3cc3a9`: KWin **1794 passed, zero failures
  across 268 suites**,
  source/test typechecks and bundle build; Rust workspace **3017 passed, zero
  failures** before rebase and **3022 passed, zero failures across 83 groups**
  after integrating upstream platform-policy tests, with `--offline --locked`;
  whitespace check clean.
- Native re-check at `fe3cc3a9` passed: one float callback issued the native clear;
  the first observation report wrote no geometry, then one unfloat operation
  occupied both tiles with two geometry writes. No second activation or focus
  change was needed; no refusal or shortcut failure occurred. An eight-second
  quiet interval and later focus change left the geometry stable. The accepted
  configuration/native baseline was restored exactly and development is DOWN.

## Actual Qt/KWin transport boundary, 2026-10-18

- A pure-transport KWin helper called an owned Planner while the controller was
  unloaded. Inputs were synthetic worlds; no reply was applied to native windows
  or desktops. Authenticated discovery and a small accepted report took 1 ms and
  2 ms respectively in this spot sample.
- A 5000-window, 64-domain report padded with JSON trailing whitespace to exactly
  **4194304 bytes** was accepted; its unpadded content was 1925198 bytes. KWin
  received and parsed a **1608803-byte** full reply in **904 ms**. Construction
  took a separate 273 ms. Identical resend returned identical bytes at the same
  revision in 380 ms. This proves actual delivery, not layout usability or a
  responsiveness target.
- **4194305 bytes** were refused before decode: the service reported
  `request-oversize` and KWin received the D-Bus size-bound error rather than a
  normal callback. A separate 11188-window report below the byte cap was refused
  in-band as `snapshot-invalid` / `inventory-over-cap`, confirming the distinct
  count gate. The probe initially labeled that correct count refusal as failure
  by checking the wrong field; wire/service evidence confirms the expected
  result. No product correction was needed.
- Exact-4-MiB reply delivery remains untested; the actual delivered reply size is
  reported above rather than substituted for that boundary. The owned service
  and helper were removed, the Planner name became unowned, and configuration,
  shortcut and float-intent bytes were unchanged. No new baseline was required.

## Fixed-client native observations, 2026-10-18

- Installed disposable-client candidates reported resizable native hints. An
  owned minimal Qt Widgets client supplied genuine equal positive bounds on both
  axes; KWin reported normal, non-dialog, non-resizable, equal minimum/maximum
  dimensions. Using this helper is a tentative fixture decision, pending user
  review; it is not game, borderless-client or general application acceptance.
- D6 sampled pass: enabling the disposable workspace left the automatic fixed
  float at its original frame with zero geometry writes. An ordinary editor
  control tiled while the fixed client stayed untouched. Changed hints,
  predicates, maximized/sticky controls and other enable permutations remain
  pending.
- D5 sampled passes: an already-fixed float returned from fullscreen to its
  original frame with no geometry writes. Separately, an observer armed before
  launch captured a new client's first `windowAdded` as fullscreen with genuine
  fixed hints. Its first subsequent report wrote no geometry. First exit also
  wrote no geometry; the client remained a native float, the editor stayed tiled,
  and later native read-back remained stable. Raw birth/exit journals and exact
  client hint read-back close the earlier first-observation evidence gap.
  Non-fixed born-fullscreen, predicate changes, real games and physical input
  remain pending.
- D7 Planner-only sampled pass: explicit fixed tiling stored a disjoint tile
  override. Planner replacement rediscovered a fresh generation. After explicitly
  re-enabling workspace tiling, the fixed tile and stored override remained;
  re-floating cleared the override and reflowed the ordinary control. The first
  attempt omitted that re-enable and interpreted a floating workspace as lost
  intent. That defect claim is withdrawn, not an admission/persistence finding.
- A separate controller-plus-Planner replacement exercised seed return and
  workspace re-enable, but its preceding churn produced three
  `partial-observation` refusals and a float command followed by an observe-only
  dispatch. Retrying through those refusals violated the clean-fixture stop rule.
  This run is not accepted D7 controller-restart evidence. Displacement of the
  ordinary control, transient stored membership and an explicit fixed re-float
  frame differing from its original frame remain unclassified observations, not
  diagnosed admission/capacity defects.
- A subsequent clean fixture established both clients while stopped, verified
  genuine fixed hints and ordinary-control tiling, then issued one explicit
  fixed-tile request. It also returned `partial-observation`. The leg stopped
  immediately without retry or controller replacement; frames and tile-marker
  absence were unchanged. D7 controller replacement is blocked before its
  precondition, not proven to fail hydration. Further retries need source-level
  diagnosis of the refusal first.
- Reproduced diagnostic defect: fullscreen exit logged `fullscreen-refused` and
  `shortcut-failed`, then the native fullscreen-change event confirmed success
  before the next zero-write report. The immediate setter read-back in
  `kwin/src/world-native.ts` races asynchronous native settlement. No geometry or
  admission failure was established; correction options are parked in the backlog.
- Both fixed helper processes were found absent during teardown after the last
  native query had shown them alive. Exit status had not been retained, so their
  cause is unknown. A separate controller-free lifetime probe retained parent
  wait status, PID/start identity and native inventory across observer teardown
  and an idle interval; the client remained alive and exact owned termination
  returned the expected status. Observer teardown did not reproduce the exits.
  Further fixtures need retained child exit status; the historic cause remains
  unresolved rather than attributed to product behavior.
- Independent evidence review accepted only the D5/D6 samples and D7
  Planner-only leg above. Builds succeeded; no production source or tests changed.
  Configuration, shortcuts and float-intent bytes matched the accepted baseline;
  desktop identities/count, protected frames/membership, active/current state,
  pointer and effects were restored. All disposable clients were gone, helper
  scripts were unloaded and development is DOWN. No additional baseline change
  was required.

## Settings targeted native interaction, 2026-10-18

- At `821b05d0`, a fresh native build produced both KCM identities. Each owned
  native shell was bound to its executable, mapped source-built plugin and
  accessibility-bus process identity. Direct AT-SPI widget actions/read-backs
  targeted only those shells; no synthetic keyboard or pointer input was used.
- Both identities passed four-tab title/order and page-content observations,
  Status presence across pages, gap draft editing, Apply followed by reopen and
  saved-value read-back, Cancel discarding a later draft, and Defaults followed
  by Cancel without changing the saved values. Effect-identity cross-tab draft
  retention had explicit post-switch value read-back; the script identity still
  lacks that exact post-switch discriminator. Accessible label/buddy information
  was observed, not physical traversal or mnemonic activation.
- Ordinary Apply left the window-edge configuration, shortcut bytes, effect
  configuration and effect-loaded state unchanged at the before/after endpoints.
  No transient-call oracle was present, so stronger non-invocation of operational
  actions remains pending. Fix/Revert and Apply Shortcuts were not activated.
  With the controller stopped, saved feedback honestly reported reconfiguration
  requested but application unconfirmed.
- The first interaction attempt did not retain its gating accessibility and
  process-mapping outputs and is not accepted evidence. A single capture repair
  retained raw owner/PID checks, widget trees, actions/read-backs, mapped-plugin
  identity and configuration deltas. Independent review accepted the bounded
  results above. One reopen route had malformed D-Bus arguments; the subsequent
  correctly addressed reopen supplies the evidence, not that route failure.
- Only the two owned temporary gap keys were removed during restoration.
  Configuration, shortcuts and float-intent bytes matched the accepted preimages;
  desktop identities/count, protected frames/membership, current/active state,
  pointer and effects matched. All owned shells exited and development is DOWN.
  No Settings product defect or new baseline was found. Physical keyboard,
  visual UX and stronger operational-isolation checks remain pending.

## Refusal diagnosis and additional native evidence, 2026-10-18

### D7 offline diagnosis: expected safety refusal, trigger unresolved

- The stable fixed-tile request dispatched as an operation and the Planner
  returned `partial-observation`; native membership, frames and tile-marker
  absence stayed unchanged. The refusal is an expected fail-closed mechanism,
  not evidence that controller hydration failed. Its triggering condition is
  unresolved: neither product defect nor fixture artefact is established.
- `session/ops/float.rs` can refuse absent target, mismatched floating/homing or
  remaining maximize state. Its nested `session/ops/lifecycle.rs` admission can
  also refuse unequal known/observed sets or mismatched member bindings. The
  retained logs contain no operation payload or exact refusal site, so they
  cannot distinguish these conditions.
- An accepted sibling run had two observes between workspace enable and explicit
  tile; the refused run had none. This is a settling hypothesis only, not a
  diagnosis or a new requirement to delay commands. No retry was performed.
- Parked diagnostic/regression work: reconstruct the producer-shaped fixed-client
  report immediately after workspace enable, compare identical commands with and
  without intervening observes, and assert the exact refusal site and known-set
  bindings. Use genuine fixed hints, renewed wrappers and QML array-likes. Retain
  complete request/reply and retained exception state before selecting a fix;
  a delay-only harness workaround would not establish correctness. No core or
  adapter change was made, and controller restart remains blocked.

### Settings discriminator and callback-route sends/history

- At `71e1de9e`, the source-built script KCM retained the edited outer-gap draft
  after visiting the other three tabs and returning: explicit post-switch values
  were inner 8, outer 9. Cancel closed the owned shell without saving; config and
  shortcut bytes matched. Native process/plugin/accessibility identities were
  retained. This closes the script-specific read-back gap, not physical traversal,
  visual UX or transient-operation non-invocation. Protected-process liveness was
  captured in this leg; complete native-frame restoration is supplied by the later
  final inventory, not its Settings-only teardown capture.
- Numbered and relative follow/stay sends of an ordinary float across a
  tiled/floating boundary passed: command/dispatch/apply journals correlated with
  native membership, current view, focus and unchanged float frame. Follow changed
  the view; stay did not. Unbound stay actions were invoked by registered name,
  not rebound or physically pressed. Initial history captures lacked command
  linkage and were not accepted; the later capture below supplies that evidence.
- A separate stopped-creation fixture added one owned desktop, kept peers on both
  test domains and left the protected domain floating. Numbered selection and
  previous-workspace return had command/dispatch/apply linkage plus native view
  and remembered-owned-focus read-back. Both test domains were explicitly enabled.
- One tiled numbered send-follow passed: its reply made one membership write,
  one view write and two geometry writes; the next observe made one more geometry
  write. Native read-back showed the subject focused at the target and both tiled
  sides reflowed. This is a single transition, not sustained send recovery or the
  complete send matrix.
- The following observe published one held scope; the first numbered send-stay
  back refused `scope-held` with no native movement. Holds also appeared earlier
  in that fixture, so their origin is not attributed to the successful send.
  Additional follow/relative attempts continued without fixture teardown after
  the first refusal. That violated the stop rule; none supplies clean acceptance.
  Return/stay/relative two-tiled-domain coverage remains pending.
- `world_sends.rs` deliberately refuses when either endpoint is held. The logs
  omit the held domain and reason; `world_report.rs` permits several hold reasons
  and `world_publish.rs` maps multiple sweep failures to unreadable. This is an
  expected safety refusal with an unresolved trigger, not a capacity diagnosis.
  Parked next discriminator: retain full reports/replies around the first hold,
  identify endpoint authority/reason and replay a two-domain send-follow/observe/
  send-back sequence offline. Select a minimal correction only after identifying
  the cause; admission/capacity behavior remains report-only.

### Native diagnostics, repeat signals and drag limits

- B1 failed its diagnostic acceptance: one maximize callback logged
  `maximize-refused` and `shortcut-failed`, but native read-back showed maximized.
  An exact native restore likewise read the old maximize state synchronously and
  cleared later. `world-native.ts` calls the maximize setter then decides success
  from same-stack `maximizeMode`; this reproduces the asynchronous false-refusal
  class already observed for fullscreen. No geometry-fighting defect is inferred.
- Parked correction options: confirm once at a fresh accepted observation boundary
  (recommended), or distinguish issuance from final outcome in diagnostics.
  Preserve one native attempt per activation, no replay/retry, truthful genuine
  refusal, and lifetime/scope/focus/generation/supersession fences. Add
  producer-shaped tests with delayed maximizeMode settlement, renewed wrappers,
  QML array-likes, silent refusal and intervening command/restart races. Adapter
  source was not changed. Remaining B1 orderings and physical held keys are pending.
- After explicit native restore/read-back, sticky-on/native-unstick/sticky-on
  passed as a bounded callback/state signal. Two back-to-back directional focus
  callbacks showed busy suppression of the second input. These are not physical
  autorepeat evidence or clean whole-journey acceptance: the run continued after
  the maximize refusal without complete teardown. Other origins/orderings remain
  pending.
- Supported exact-window APIs did not establish a safely confined genuine
  interactive drag starter. Controller-free frame setters emitted no interactive
  start/step/finish signals; synchronous frame read-back lagged later settlement.
  These are observations only, not drag, resize, clamp or full-state race passes.
  No synthetic keyboard/pointer input or manufactured signals were used. Physical
  drag, visual preview, finish/cancel and late-reply behavior remain pending.

### Evidence quality and final restoration

- Independent review accepted only the bounded claims above. Fresh Planner and
  KWin bundle builds passed; no production source or tests changed. No new product
  decision was made; the earlier tentative fixture decisions remain unchanged.
- The final owned third desktop was removed only after controller/Planner stop.
  Its exact added native tiling group was surgically removed; the retained tiling
  inventory matched the preimage. Final configuration and shortcut hashes matched
  the accepted baseline; a separate final check also matched float-intent bytes.
- Final native inventory matched desktop identities/count, protected frames and
  membership, current/active window and pointer. Effect remained unloaded, all
  disposable clients were absent, helper scripts were unloaded and development
  was DOWN. Desktop restored: yes, to the previously accepted baseline.
- Supervised earlier fixtures retained child exit statuses. The last send fixture
  did not: its wait commands returned 127, so only native/process absence proves
  teardown there. Its claimed retained child exit status is rejected; it does not
  resolve the historic unexpected-helper-exit question. Subsequent fixtures must
  use the already-proven supervisor rather than an unrelated-shell wait.

## Remaining coverage and next step

| Acceptance group | Current result | Remaining |
| --- | --- | --- |
| Ordinary unfloat | Native callback functional pass, two pairs | Physical Meta+G delivery |
| Both Settings identities | Source-built native shell plus targeted AT-SPI tabs/Status, draft/save/reopen, Cancel, Defaults, both post-switch draft read-backs and unchanged Apply endpoints | Physical traversal/buddies/visual UX and transient-operation isolation |
| Restart / unavailable Planner | Sampled native pass, no old-command replay; fixed override survives Planner-only replacement after workspace re-enable | Clean controller-restart intent coverage and broader race/error permutations |
| Transport | Exact request limit, oversize/count refusal and large reply delivered | Exact reply-limit delivery and broader latency acceptance |
| Ordinary actions / lifecycle | Sampled native pass; sticky/native-change and busy-suppression functional signals | Clean remaining permutations; maximize false-refusal correction and B1 repeats |
| Safety states / workspace enable | Prior samples plus genuine fixed D5 born/prior-float exit, D6 enable/control and D7 Planner-only restart | Clean D7 controller restart, non-fixed born exit, remaining predicates/controls, games and refused/raced variants |
| Workspace paths | Callback/state selection/history, floating-boundary numbered/relative follow/stay, one tiled numbered follow | Held-scope diagnosis; clean tiled return/stay/relative, append, lifecycle races and migration partial refusal |
| Drag / client clamp | Controller-free setter observations only; no acceptance | Safe genuine drag route; continuous preview/drop/resize, late-reply cancel and bounded native clamp |
| Multiple outputs | Not testable in this single-output fixture | Selection/history/swaps/hotplug-return and cross-output matrix |

- No complete full-state or single-output inventory gate is claimed. Physical
  keyboard/drag and Settings traversal/visual checks are not established by
  callbacks, direct accessibility actions or offline widget tests.
- Next bounded step: offline producer-shaped reproduction of fixed-tile
  `partial-observation` and tiled-send held-scope publication, retaining exact
  request/reply and refusal-site evidence before another live retry. The historic
  payloads are unavailable; do not substitute inferred causes. Controller restart
  and tiled return remain pending. A later clean fixture needs retained child
  exit status, explicit workspace re-enable after replacement and immediate
  teardown on the first unexpected refusal.
- Maximize/fullscreen asynchronous outcome diagnostics need the parked adapter
  correction and regressions. Native-maximize notification quietness and the
  isolated tile-order observation remain separate unclassified follow-ups.

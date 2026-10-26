# Windows physical focus diagnostics

## Goal and scope

- Diagnose the three-window startup directional-focus report of 2026-10-26.
- Diagnose the subsequent physical occupied-workspace focus failure report;
  distinguish core targeting, native dispatch and observed arrival.
- Restore existing directional focus behavior without changing core policy,
  native safety/lifetime gates, at-most-once dispatch or workspace convergence.
- Ordinary applications and physical shortcut acceptance remain user-owned.

## Acceptance and approach

1. Join the incident's received commands, core desires and native refusals;
   establish whether recent workspace focus changes caused the defect.
2. Reproduce its ordering with realistic host-shaped desired-behavior coverage
   before fixing the smallest responsible mechanism.
3. Pass core/Windows package tests, strict all-target clippy, fmt and diff checks.
4. Independently review native focus boundaries, then verify startup with three
   fixture windows, all relevant directional routes, rapid workspace toggles
   and genuine mid-switch foreground supersession.
5. Verify stopped/restored owner, fixture shutdown and no control/recovery residue.

## Live scope

- Disposable fixture windows only; owner scoped to the fixture executable.
- Exact-owner production directional/workspace controls, independent native
  readbacks and fixture focus acquisition. No input into unrelated applications,
  registry changes, games or secure-desktop control.
- Stop live work if fixture foreground acquisition fails against another app.
- Injected keys do not establish physical Win+arrow behavior and the production
  injected-input filter remains unchanged.

## Evidence and decisions

- Incident: the first valid Left/Up commands reached core and selected the
  correct neighbors, but native setters reported refused/unproven. A later
  edge Right correctly rejected as unchanged; subsequent commands repeatedly
  refused as `catchup-required` until unrelated foreground changes. No workspace
  switch or accepted in-flight setter preceded the initial failures.
- The demonstrated starvation is present in HEAD, not introduced by the recent
  workspace-arrival, nonactivating-view or in-flight focus changes. A rejected
  command arms the sequencer's observation-only obligation, but the command
  adapter returned without servicing it and could idle indefinitely.
- User-approved 2026-10-26: service one owed observation-only pass on the
  same turn after core rejection or a catchup-required dispatch refusal, using
  that turn's captured products. Keep the rejected/refused command dropped;
  no queue, replay, core-policy change or weakened sequencer fence.
- Desired-behavior regression: three-window bootstrap, valid Left/Up from the
  same bottom-right origin, edge Right no-op, then fresh Left/Up desires from
  that origin without unrelated WinEvents. Native-shaped creation identities
  and per-command ticks retain the incident ordering. Before correction the
  post-rejection command returned no reply; after correction each valid command
  emits exactly one correctly targeted focus effect with a distinct id.
- Offline gates pass: `cargo test --locked -p tiler-core -p tiler-windows`
  (853 core library tests, 1,026 Windows library passes plus one existing ignored
  test, integration/doc suites); strict all-target package clippy, workspace fmt
  and diff checks. Independent review
  found no code blocker: no outstanding flight before the inner observation,
  no preceding native effects, and unchanged native safety/readback gates.

## Native evidence (2026-10-26)

- Three fixture windows existed before normal-owner startup. Fixture-only scope,
  source revision plus relevant dirty files, payload hash, exact process lifetime,
  native window joins and immediate settings preimages are retained externally.
  Fixture foreground was verified before bounded grant/input helpers; there was
  no input into an unrelated application and no injected-arrow exception.
- Twenty production directional controls: eleven successful focus changes covering
  Left/Right/Up/Down, and nine expected edge no-ops. Correct targets and accepted
  native setters were joined to independent foreground and fixture observations.
  Edge-no-op then valid focus continued without an unrelated foreground change.
- Two consumption-paced bursts made four and five alternating occupied-workspace
  selects, ending at opposite destinations. Production dispatch gaps were
  179-283ms; each switch applied its view and each final native foreground and
  fixture visibility matched the remembered target. Consumption, not foreground
  arrival, paced the requests.
- A genuine supersession hit applied the destination view but skipped its intended
  focus effect as `foreground-changed`: the bracketing baseline was a managed
  fixture member, the live identity was a separately created out-of-scope fixture
  competitor, and independent native/fixture reads confirmed competitor retention.
- Earlier two-select, multi-second-paced workspace pairs and a same-workspace
  select with empty effect rows were rejected as burst/supersession evidence.
  The consumption-paced sequences and explicit baseline/live veto above replace
  those claims. An initial workspace driver helper error was cleaned up; later
  fresh-baseline legs verified exact owner stop/restore, fixture shutdown, matching
  settings bytes/input state and no process, recovery or control-request residue.

## Outcome and remaining ownership

- Demonstrated command starvation after a core rejection is fixed and independently
  reviewed, with offline RED/GREEN and fixture-native verification. Core policy,
  native safety fences and today's workspace convergence are unchanged.
- The incident's two initial native setter refusals did not reproduce and their
  sub-gate is not identified by the old trace. This correction alone does not
  explain those refusals or prove physical Win+arrow delivery. The subsequent
  user confirmation and remaining workspace P0 are recorded below.
- The original user-owned check was three ordinary windows with physical
  Win+arrow, including an edge no-op followed by a valid direction. The user
  subsequently confirms Win+arrow fixed. Multiple-output and game acceptance
  are not implied by these fixture legs. Same-turn catchup is User-approved
  2026-10-26.

## Subsequent physical workspace report (2026-10-26)

- User confirms physical Win+arrow is fixed, but focus after workspace switches
  still fails. This confirms the directional recheck, not an explanation of the
  earlier two native setter refusals.
- The subsequent run contains one send-follow and thirteen numbered selects.
  Every select emits the expected core focus target, applies its destination
  view and dispatches an accepted setter. Every immediate foreground read is
  false (`foreground-unproven`). No select is skipped by foreground-change,
  stale, catchup, eligibility or game gates; no select setter reports refused.
- Per-switch evidence below uses tick plus effect id because an observation
  and a later command can share a tick. The later read is a fresh native
  foreground sample, not retained core focus. Its time is an observation time,
  not the activation time, and cannot attribute arrival to the setter rather
  than an intervening user action.

| Tick / effect | Operation | Core target | Dispatch | Later sample of target (mono ms) |
| --- | --- | --- | --- | --- |
| 94 / fx-47 | Send-follow to ws-2 | w23 | Skipped, stale; already foreground at entry | 9072 |
| 99 / fx-48 | Select ws-1 | w19 | Accepted, unproven | 11657 |
| 103 / fx-49 | Select ws-2 | w23 | Accepted, unproven | 13424, next select entry |
| 104 / fx-50 | Select ws-1 | w19 | Accepted, unproven | 17065 |
| 107 / fx-51 | Select ws-2 | w23 | Accepted, unproven | 18469, next select entry |
| 108 / fx-52 | Select ws-1 | w19 | Accepted, unproven | 18681, next select entry |
| 109 / fx-53 | Select ws-2 | w23 | Accepted, unproven | 18708 |
| 111 / fx-54 | Select ws-1 | w19 | Accepted, unproven | 19969 |
| 114 / fx-55 | Select ws-2 | w23 | Accepted, unproven | 20899, next select entry |
| 115 / fx-56 | Select ws-1 | w19 | Accepted, unproven | 21049, next select entry |
| 116 / fx-57 | Select ws-2 | w23 | Accepted, unproven | 21193, next select entry |
| 117 / fx-58 | Select ws-1 | w19 | Accepted, unproven | 21309, next select entry |
| 118 / fx-59 | Select ws-2 | w23 | Accepted, unproven | 31334 |
| 120 / fx-60 | Select ws-1 | w19 | Accepted, unproven | 32380 |

- Diagnostic limits: `workspace outcome:ok` means reply acceptance/application,
  not native focus completion (`selection.rs`). Foreground events can be beyond
  the first eight preview rows of `pending-edges`; omitted preview rows do not
  establish lost WinEvents. `ffc_enabled` describes focus-follows-cursor, not
  whether the nonactivating view's foreground-identity guard ran.
- Blocker: the trace does not distinguish delayed own activation from an
  unlanded activation followed by user repair, or top-level foreground from
  application keyboard-focus failure. It cannot identify a foreground-lock
  refusal or prove that every switch succeeded. No behavioral correction or
  new native acceptance is claimed from this run.
- Next: add bounded event-driven focus-flight resolution diagnostics, joined
  to the originating effect and exact native identity, without losing
  foreground edges to the generic preview cap; distinguish native activation
  from target GUI-thread focus if needed. Preserve at-most-once dispatch,
  lifetime/scope/supersession gates and existing core policy. No waiting,
  polling timer, input attachment or foreground-lock workaround is selected.
- Root-cause regression, implementation, offline gates and fixture-native
  verification remain blocked on discriminating evidence. Ordinary-application
  keyboard-focus and physical shortcut acceptance remain user-owned.

## Idle workspace landing investigation (2026-10-26)

- Goal: distinguish deferred adapter dispatch, late host activation, delayed
  observation and application keyboard-focus failure without changing behavior.
- Acceptance: bounded trace-only setter call/return and API/flags with exact
  native identity; correlated in-flight foreground-edge/resolution causes;
  pass wake sources; target GUI-thread active/focus at dispatch and landing.
  Keep all safety gates, at-most-once dispatch and event-driven scheduling.
- Reproduce with separate fixture processes and ordinary decorated/resizable
  windows, then close variants, leaving keyboard/mouse idle after each switch.
  Use native foreground events/readbacks, not command acceptance, as the oracle.
  If reproduced, add a desired-behavior regression before an event-driven fix;
  otherwise deliver trace rerun instructions and retain the unproven root cause.
- Units: incident timing join; diagnostic implementation and targeted tests;
  independent boundary review; core/Windows tests, strict clippy/fmt; bounded
  fixture-only native legs with exact stop/restore and baseline verification.
- Existing trace's `world-apply` emission follows the setter and immediate
  foreground read, but is not the setter-call timestamp. Consumed shortcut
  callback log rows precede those rows by 98-111 ms for ten directly joined
  selects. The other three lack a pre-dispatch select-down row; an earlier
  Win-down is not a substitute. Callback log emission is not callback delivery
  time. All thirteen setters ran on their command application, before its
  `workspace outcome:ok`, not on a later unrelated observation pass.
- No wait, poll, retry, foreground-lock workaround or core-policy change is
  selected. Diagnostics are observational, not a new product decision.

### Incident timing join

- All times below are existing log `mono_ms` emissions. Delta starts at the
  post-setter acceptance row, not the unavailable exact setter return. The
  target sample is fresh native foreground captured for that reply, not cached
  core focus; its row emission is later than capture. These are observation
  gaps, not measured activation delays.
- `next select` identifies the command carrying the fresh sample. Other rows
  identify the preceding pending-edge preview, not a proven unique wake cause.
  The old trace has no per-pass wake attribution. No target foreground edge
  occurs in the retained previews, but capped rows leave delivery unknown.

| Effect / tick | Acceptance row | First target row | Gap (ms) | Observing-pass evidence |
| --- | --- | --- | --- | --- |
| fx-48 / 99 | 10126 | 11657 | 1531 | observation 101; destroy-only preview, omitted 0 |
| fx-49 / 103 | 13116 | 13424 | 308 | next select 104 |
| fx-50 / 104 | 13424 | 17065 | 3641 | observation 106; destroy-only preview, omitted 0 |
| fx-51 / 107 | 18326 | 18469 | 143 | next select 108 |
| fx-52 / 108 | 18469 | 18681 | 212 | next select 109 |
| fx-53 / 109 | 18681 | 18708 | 27 | observation 111; hide/location preview, omitted 25 |
| fx-54 / 111 | 19500 | 19969 | 469 | observation 113; destroy-only preview, omitted 0 |
| fx-55 / 114 | 20723 | 20899 | 176 | next select 115 |
| fx-56 / 115 | 20899 | 21049 | 150 | next select 116 |
| fx-57 / 116 | 21049 | 21193 | 144 | next select 117 |
| fx-58 / 117 | 21193 | 21309 | 116 | next select 118 |
| fx-59 / 118 | 21309 | 31334 | 10025 | observation 120; destroy-only preview, omitted 0 |
| fx-60 / 120 | 32349 | 32380 | 31 | observation 122; create-only preview, omitted 0 |

### Delivered diagnostic contract

- Trace `world-apply.focus_dispatch`: effect id and target token plus full
  HWND/PID/process-creation identity; adapter `dispatched` versus actual
  `primitive_called`; actual `api`; `call_mono_ms`/`ret_mono_ms` strictly around
  `SetForegroundWindow`; raw `native_return` versus post-fence `setter` outcome;
  immediate `foreground` proof; pre-call `gui_dispatch` and immediate-proven
  `gui_landing`. A native acceptance overturned by a lifetime fence remains
  distinguishable from a refused primitive or an early veto.
- Trace `world-apply.visibility_posts`: actual `SetWindowPos` flag word or
  `ShowWindowAsync` show command, HWND, direction, call/return stamps and raw
  post result. Nonactivating flags are hide `0x4097`, show `0x4057`; minimized
  reveal can use `ShowWindowAsync(SW_SHOWMINNOACTIVE)`. A configured view lane
  or skipped focus row never masquerades as an executed native call.
- Trace `world-apply.inflight_events`: originating effect/token/full identity,
  clearing cause, callback-queue/drain/resolution stamps and target GUI-thread
  active/focus sampled at resolution. Causes distinguish own foreground edge,
  foreign supersession, nonactivating-guard own landing or supersession, newer
  accepted setter, lifetime end and generation reset. Quiet observation passes
  emit these rows too. No edge means no invented landing time.
- Trace `wake-attribution`: same-tick owner wait result flags and deadline
  source, exact foreground/location/create/destroy/show/hide counts, and bounded
  foreground delivery joins. These are a cause set, not a claimed unique waker;
  command kind rides the same-tick `world-apply`. Multiple turns can share a
  tick, so also retain file order and monotonic stamps.
- Callback queue, drain, setter and resolution stamps share process-relative
  `mono_ms`. `edge_event_ms` is the wrapping host WinEvent time basis, not that
  clock. GUI read failure is `unknown`; suppressed/unsampled is null. A later
  GUI sample does not establish keyboard focus at the earlier edge time.
- Row storage is bounded (8 per rendered section, 32 foreground metadata),
  with honest omission counts including upstream truncation. Metadata stays a
  matching prefix across drains, never shifts a later edge onto an earlier
  HWND. No titles, paths, input text or application content are added. Hook
  callbacks stamp primitive metadata only; GUI reads and serialization run on
  the owner outside callbacks. Extra GUI/visibility capture is trace-gated.
- Safety/lifetime/scope gates, command ordering, at-most-once spending,
  production foreground queue and all core behavior remain unchanged.

### Native verification and limits

- Two independent fixture processes, fixture-only scope, default decorated
  resizable windows, decorated nonresizable windows and steady-state borderless
  windows were exercised. Eighteen initial genuine accepted switches (10 default,
  4 nonresizable, 4 borderless) completed with 12-second input-idle dwells.
  Sixteen had asynchronous landing rows; two were immediately proven. One of
  those immediate landings had no foreground event on either independent hook;
  event absence alone therefore does not prove an unfocused target.
- The final payload was separately verified with four genuine cross-process
  default-window switches and 12-second input-idle dwells. Actual setter calls
  returned within the same millisecond; foreground callbacks queued after
  8-10 ms, drained after another 1-2 ms, and resolved after 20-25 ms total.
  Both independent hooks agreed on each host event time and target identity;
  observer callback lag was 0-16 ms. Target GUI active/focus were zero before
  dispatch and matched the target HWND at landing. Executed nonactivating hide
  and reveal posts preceded each focus setter with the expected flags.
- These are delivery/observation intervals, not exact host activation instants.
  No seconds-long delay reproduced. Foreground grant was established before
  these legs using one owned-foreground-verified harmless key pair and
  `AllowSetForegroundWindow`; no keyboard/mouse input or repair occurred during
  any measured dwell. This differs from ordinary-app physical-hotkey context.
- Separate negative evidence: without that grant, two genuine setters refused
  synchronously (one setup, one 12-second idle trial), with no later foreground
  edge. A same-target no-op trial is excluded from landing evidence. One runtime
  decoration toggle produced `unknown-homing` before the setter despite an
  applied view; steady-state borderless switches succeeded. That distinct hold
  has no identified root cause or recovery proof and was not corrected here.
- Exact payload hashes/lifetimes, source dirty state, fixture HWND joins,
  native observations, settings bytes and recovery evidence are retained
  externally. Earlier exploratory legs lacked complete per-leg payload hashes;
  the final leg has matching post-launch/pre-stop hashes and a verified
  provenance sidecar. An initial helper API-load failure ended before measured
  switches and was cleaned; it is not a product failure. Final exact-owner
  stop/restore and fixture shutdown left matching settings/SPI preimages, no
  owner/fixture processes, recovery ledger or control-request residue.
- Regression coverage includes actual-call versus early-veto/raw-return
  separation, post-setter fence honesty, GUI trace suppression, flight identity
  and edge timestamp joins, quiet clearing emission, actual visibility API and
  minimized fallback, source/renderer bounds, and cross-drain prefix integrity.
  Final locked core/Windows tests (853 core and 1,048 Windows library passes,
  one existing ignored Windows test, plus integration/doc suites), strict
  package all-target clippy, workspace fmt and diff checks pass. The native
  leg preceded the final raw-return field/serialization addition; that
  field-only correction is covered offline and changes no native calls.
  No latency root-cause regression or behavioral fix is claimed without
  reproduction.

### User trace rerun and remaining ownership

1. Run `mise exec -- just dev trace` (not `verbose`). Use two ordinary
   applications in different processes on two occupied workspaces; focus each
   intended arrival window once to establish remembered targets.
2. Physically select the other workspace, release the shortcut fully, then leave
   keyboard and mouse completely idle for at least 12 seconds. Observe whether
   the target becomes active and whether its keyboard-focus indicator returns.
3. If still unfocused, move the pointer once without clicking or another
   shortcut; report whether that changed focus. Do not repair before the idle
   interval ends. Repeat both directions, noting which attempts were late.
4. Stop with Ctrl+C. Retain the printed run log and its `.log.provenance.json`
   sidecar together, plus approximate failing-switch order and observations.
   No additional environment log-level variable is required.

- Next analysis: join command application to actual call/return and native
  identity, then callback queue/drain and flight clearing. A setter appearing
  only after an unrelated wake is adapter deferral; a prompt setter followed by
  a late host event points to host/app activation or event delivery; a prompt
  event queued but late owner drain points to owner servicing. Prompt top-level
  foreground with missing target GUI focus is a separate application-focus leg.
  Without a target event, use fresh readback/GUI evidence and clearing cause;
  do not attribute causality from silence or a newer setter's supersession.
- Diagnostics are ready; physical workspace focus remains unresolved and this
  note stays active pending the ordinary-application trace. No new tentative
  product decision was introduced; existing tentative decisions are unchanged.

## Fresh trace diagnosis and correction (2026-10-26)

- Scope: startup native refusals and the delayed occupied-workspace observation;
  the separate stale source-pager layout after send-follow is not changed.
- First fresh trace: valid directional commands reach the correct core targets
  and call `SetForegroundWindow` promptly, but its raw return is false. Calls
  return within the same millisecond; this is neither blocked dispatch nor an
  observation catchup refusal. The initially observed origin persists across
  later presses. No owner menu-mask prime precedes these setters while Win is
  held. Two early target foreground edges follow false returns; raw return
  alone does not establish that native foreground never changed.
- Second fresh trace: the initial setter also returns false. A Win-release E8
  mask and an outside foreground change both precede subsequent successful
  setters. The trace cannot separate their causal contributions. History shows
  that the world-model cutover dropped the previously approved pre-setter E8
  prime and thread-input attachment; the current primitive was a bare setter.
- Workspace timing join: select tick 23 posts its destination view, calls the
  setter at 8890 ms and returns accepted at 8891 ms. The target foreground edge
  queues at 8899 ms and drains at 8901 ms, coalesced with a Win+1 key-up. That
  settled-only command turn runs no world observation. The drained edge remains
  pending until a pager-message wake at 18910 ms produces observation at
  18917 ms, resolving the flight and refreshing pager state together. Timeout
  wakes in between produce no observation. The demonstrated delay is owner-side
  servicing, not a blocked native call or a late top-level foreground edge.
- Correction: after command-lane processing, pending foreground edges owe the
  existing observation-only reconcile pass on that same eventful turn. Real
  commands that already consumed the edges do not add a pass. No timer, wait,
  replay, retry, or core-policy change is introduced.
- Startup correction under verification: restore the existing bounded owner-thread
  E8 prime after native safety gates and before the single setter. Do not restore
  input-queue attachment. Raw setter return, lifetime fences and independent
  foreground proof remain distinct; trace records the prime's inserted count.
- Implementation refinement (User-approved 2026-10-26): prime-only restoration
  rather than restoring thread-input attachment. This retains the smaller native
  mechanism; native verification must not pre-grant the owner foreground rights.
- Independent boundary review found no blocking code defect. Scheduling coverage
  is explicitly paired: the real scheduler regression proves key-up-only turns
  service an owed pass (RED/GREEN); a separate native-shaped accepted-flight
  regression proves observation clears the own-landing edge without replay.
  The scheduler's offline enumeration stub does not execute native resolution.
  Prime-order and safety-veto regressions also fail before restoration and pass
  afterward. This is not yet native or physical-shortcut acceptance.
- Remaining verification: fresh-owner no-prime versus prime-only fixture startup
  with three pre-existing windows and no grant/priming helper; send-follow then
  switch back with idle input and joined native/pager latency; rapid toggles and
  genuine mid-switch supersession; exact restoration and residue checks. Retain
  ordinary-app keyboard-focus and physical Win+arrow acceptance as user-owned.

### Verification outcome and open acceptance

- Fresh-owner A/B used three pre-existing decorated/resizable windows in one
  fixture process, without preparatory owner grants, input priming or repair.
  Both the no-prime and prime-only binaries accepted their first directional
  setter and reached the expected native foreground and GUI focus. The new
  trace reports two inserted E8 events. This verifies the restored mechanism's
  execution, not the incident's causal hypothesis: the refusal did not reproduce.
  The first command was about five seconds after readiness, not immediate input.
- Two genuine cross-process occupied-workspace switches, after send-follow setup,
  retained their expected target across 12-second dwells. Setter calls returned
  within 1 ms; target foreground edges queued after 8-10 ms, drained after another
  1-2 ms and resolved as own landings after 22-25 ms. Landing-time GUI samples
  matched the target active/focus HWND; independent native/fixture reads confirmed
  persistence. No seconds-long servicing delay occurred in these CLI-driven legs.
- Pager logical current-view state updated 14/18 ms after setter return and
  focused state after 36/38 ms. Border show records followed after 45/50 ms.
  These are not pager cell-geometry paint or screen-presentation timestamps;
  independent window geometry/visibility reads do not prove pager geometry.
  Last-input and cursor samples matched across the idle baseline/dwell; the
  baseline began three seconds after dispatch. Its last-input timestamp matched
  the expected prime quantum, limiting earlier-input exclusion to that clock's
  resolution. One earlier dwell failed the input-idle gate and was excluded.
- The toggle legs do not meet rapid-switch acceptance: each pair contained an
  empty no-op followed by a genuine select about 1.5 seconds later. Final native
  targets matched both destinations, but this is only slow-paced convergence.
- Genuine mid-switch supersession remains unverified on the new payload. A
  post-command competitor acquired foreground only after own landing; a
  pre-command competitor never acquired foreground and the owner setter won.
  Neither ordering produced a `foreground-changed` veto. These attempts do not
  replace the earlier payload's genuine supersession evidence or current offline
  identity/supersession regressions.
- Exact-owner stop/restore, all fixture shutdowns, settings-byte/SPI preimages
  and no process/control-request residue were verified after every leg. Payload
  hashes, process lifetimes, native joins and detailed evidence remain external.
- Latest locked core/Windows package tests pass (853 core and 1,054 Windows
  library passes, one existing ignored Windows test, plus integration/doc suites).
  Strict package all-target clippy, package fmt and diff checks pass. After the
  upstream integration, workspace-wide fmt fails in unrelated macOS sources;
  those sources are outside this correction.
- The event-driven scheduling correction is independently reviewed and verified
  by paired offline obligation/resolution coverage. The broader P0 remains open:
  startup prime causality, immediate physical shortcut behavior, rapid native
  toggles, genuine current-payload mid-switch veto, actual pager geometry paint
  and ordinary-app keyboard-focus acceptance are not claimed complete.

## Physical acceptance (2026-10-26)

- User physical acceptance with ordinary applications on current main including
  `3fb4be07`: Win+arrow navigation works at launch; repeated rapid Win+arrow on a
  three-window workspace worked every time; switching between two workspaces
  landed correct focus every time. The physical workspace-switch focus P0 is
  closed. Earlier open physical-acceptance statements above are historical.
- User-approved 2026-10-26: E8 prime-only restoration without thread-input
  attachment. Physical acceptance does not isolate prime causality. Rapid native mid-switch veto, pager geometry paint,
  multiple-output and game breadth are not inferred from this acceptance.
- Separate rare send-follow arrival geometry/stacking failure is tracked
  independently; this acceptance does not close it.

# Windows Dual-Output Live Fixes

## Current status (2026-10-24)

- All four reported implementation defects are delivered: cross-output
  transfer (DPI/cross-DPI corrections with agent and user-physical
  confirmation), secondary pager (exact-anchor resolver with native smoke and
  restart correction), workspace-command routing and fresh-window admission.
  The interim foreground-first route and HWND-only admission prototypes below
  are historical; current routing uses the configured core cursor/focus policy.
- Cursor-first targeting (user-accepted 2026-10-20) is core-delivered: the
  [core closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)
  retains one `WorldEngine` owning cursor-first placement/switching. The
  "not implemented / awaits cursor-first" language below is historical
  2026-10-20. Windows settings, persistence and live application of the
  three-way placement are also delivered; user dogfood recheck remains.
- W2M-03/W2M-12 world-route contracts are delivered: migration has native
  fixture acceptance and T2 unchanged-source refusal has offline regression
  coverage. The "blocked" language below is historical. The remaining
  two-output physical matrix is unrun; see the [two-monitor
  checklist](windows-physical-checklist.md#two-monitor-pc). Parked product
  questions remain: undecided empty-workspace focus, edge-nearest versus MRU
  focus, and the other parked rows linked below.

## Goal and scope

Fix the reported cross-output transfer, secondary pager, workspace-command
routing and fresh-window admission defects, then advance the Windows
[two-monitor checklist](windows-physical-checklist.md#two-monitor-pc).
Windows adapter changes only; no new platform-specific core policy. W2M-03
and W2M-12 were blocked on world-route contracts at the time; those contracts
are delivered per the [core closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24) (see Current status above).

## Acceptance and approach

- Diagnose each reported failure from supplied traces and current source.
- Keep exact transfer/lifetime/visibility fences; do not accept a materially
  wrong allocation as a successful cross-output arrival.
- Historical 2026-10-20 approach: interim foreground-first workspace-command
  routing (`1e7bfaf4`) was retained while admission awaited cursor-first
  settings. Core and Windows settings adoption are now delivered (see Current
  status).
  Cold-start per-output placement preserved.
- Support secondary pager strips with exact tray/clock exclusion geometry or
  the narrowly recognized no-notification-area hierarchy described below;
  refuse unreadable and unknown layouts.
- Add targeted offline regressions, run native locked four-package build/test,
  format and strict clippy gates, and distinguish agent-driven observations
  from pending physical keyboard/display checks.

## Historical evidence (2026-10-20)

- Workspace commands preferred a still-present last-switch cache over native
  focus. Foreground-first resolution passes offline tests and an agent-driven
  two-output select journey through the exact-owner CLI using the same output
  resolver. Each output's test windows hid on its own workspace selection.
  User physical 2026-10-20 now confirms digit chords on each output, except
  the undecided empty-workspace focus case recorded below.
- Fresh admission uses native frame position. An unshipped correction retained
  pre-newcomer focus and preserved cold-start inventory. Independent review
  caught an intake-before-admission cache pollution race.
- Agent-driven live checks (2026-10-20): startup placements survived owner
  restart on both outputs; focused-side admission without activation passed;
  one activation direction passed, but the converse still admitted on the
  native side. This is an unresolved acceptance failure, not a passing
  two-output admission result. Trace then proved that previously enumerated,
  hidden, memberless newcomers polluted retention before admission. A
  membership-only intake correction fixed that direction, but the opposite
  focused interval did not reliably update retention. Further verification
  also exposed DPI-virtualized monitor bounds in the external harness. Both
  admission prototypes are parked outside the checkout; neither is shipped.
  Admission then awaited the accepted cursor-first settings (now core-delivered; see Current status); a later fixture
  must use physical monitor bounds and correlate genuine focus/retention.
- Reported transfer trace shows size scaling near the monitor DPI ratio and
  subsequent reverse scaling. Exact arrival verification correctly refused
  the mismatched frame. A separate PMv2 owned-window probe confirmed that a
  DPI transition changes frame insets: the first physical placement missed
  slightly; one identical placement with fresh insets matched exactly.
  This does not explain the much larger Firefox scaling or prove product-path
  correction. No transfer fence or geometry tolerance was changed.
- Secondary pager root cause: discovery enumerated secondary taskbars but
  supplied no tray edge, and layout refused every secondary. Discovery now
  gathers child facts, live DPI and auto-hide state. A pure Windows resolver
  prefers the leftmost valid exact tray/clock edge, otherwise recognizes the
  bounded no-notification-area hierarchy. Any unreadable fact, degenerate
  exact anchor, unknown visible child, unrecognized full-span carrier,
  right-side content or missing positive app-host evidence refuses.
- Secondary pager independent review caught unreadable/vacuous-census
  acceptance and overly broad full-span exemptions; these were corrected
  with pure producer-shaped regressions. Exact-anchor enumeration order is
  also covered: a clock cannot mask a farther-left tray boundary.
- Final-tree native pager smoke (2026-10-20): two visible, topmost carriers
  above the taskbars, correctly DPI-sized with the required two-physical-pixel
  gap. Alternating secondary-2, primary-2, secondary-1 message-driven picks
  selected three distinct output-scoped workspace identities successfully.
  The smoke used empty workspaces: it does not prove occupied-window hide or
  physical mouse delivery. Retained artifact/source-diff identity, raw owner
  log, geometry, stdout and restoration readbacks were inspected. The driver's
  PMv2 bar/carrier geometry is authoritative; separate wrapper monitor
  snapshots were DPI-virtualized and are not physical-coordinate evidence.
- Live sessions ended with verified graceful stop, independent restoration,
  byte-identical settings, unchanged relevant SPI/taskbar state, absent ledger
  and no owned window residue. Detailed evidence remains external.
- Final isolated workspace-routing gates (2026-10-20), Rust/Cargo 1.99.0,
  native MSVC: locked four-package build/tests, workspace format, strict
  all-target clippy and whitespace passed: 2,836 passed, zero failed, one
  ignored (core 1,042; protocol 27; FFI 40; Windows 1,727).
- Final pager gates (2026-10-20), same native route/toolchain: locked
  four-package build/tests, format, strict all-target clippy and whitespace
  pass: 2,849 passed, zero failed, one ignored; independent review accepted.

## Cross-DPI placement follow-up (2026-10-20)

- Status: correction implemented and agent-observed directional handler
  checks pass, including Firefox right-to-left. User physical 2026-10-20 now
  confirms Win+Shift+arrows cross-output moves and explicit send-follow;
  remaining W2M-01 topology and W2M-02 boundary legs are open.
- Trace-only per-window diagnostics now join setter/readback DPI, attempted
  and observed outer frames, fresh insets and PMv2 context. A separate
  `dpi-transition` event preserves the original transition and correction
  outcome instead of hiding it behind the final attempt's stable DPI.
- Reproduction with an owned Notepad window confirmed a DPI-sized mismatch
  after a native placement. The successful setter was classified as an
  app-held clamp, suppressing identical subsequent allocations. A later
  stable-DPI placement reached the exact allocation. This supports an
  invalidated placement basis, not a genuine minimum-size clamp; it does not
  establish which Firefox message handler caused the reported resizing.
- The Windows adapter now detects a DPI transition across a successful
  placement, revalidates the target and maps the same physical allocation
  through fresh insets. If still mismatched, it reasserts under the same
  lifetime, scope, eligibility and foreground-veto gates. Exact fresh frames
  need no rewrite. Failed/refused corrections and further detected DPI
  transitions remain pending rather than becoming permanent clamp pins.
  Exact arrival verification, transfer fences and shared core are unchanged.
- User 2026-10-20 accepted: one immediate rebase per apply, then yield
  further DPI changes to native-event reconciliation. This avoids trapping
  the owner inside repeated native setters while the host is changing DPI;
  it is not a timed retry loop or permission to accept a wrong allocation.
  Stable-DPI app-held clamps retain existing suppression. Unreadable DPI
  preserves existing classification rather than guessing a transition.
- Final-artifact native evidence: nonactivating external move-only stimuli
  caused owned Notepad and Paint windows to reconcile back to retained source
  membership across opposite DPI transitions. Both emitted `corrected`, then
  exact product readback and independent PMv2 final frames, with no remaining
  mismatch. Both corrections reasserted the same outer target, so unchanged
  inset math alone cannot justify skipping a mismatched allocation's reassert.
  The intermediate external landing was not independently sampled.
- The initial move-only stimuli proved adapter placement convergence, not
  cross-output membership or follow focus. The directional-handler evidence
  below now covers those properties separately. Drag attempts refused
  `unchanged` before a transfer and remain a separate unresolved observation.
  The existing shortcut-proof journey requires uniform DPI; it was not
  weakened to manufacture mixed-DPI acceptance.
- Producer-shaped regressions cover physical reouter math and DPI-aware
  readback decisions in both directions, exact-after-transition, stable
  clamps, pending unwritten/second-transition cases and unreadable inputs.
  Native gate-interleaving fault coverage remains absent. Independent review
  accepted the correction with these live-acceptance limits.
- Final native locked four-package build/tests, workspace format, strict
  all-target clippy and whitespace pass: 2,852 passed, zero failed, one
  ignored. Final live runs ended with verified owner stop, independent
  restore, identical settings bytes, unchanged relevant SPI/taskbar state,
  absent ledger and no owned-window residue. Detailed provenance, raw logs
  and restoration readbacks remain external.

### Directional verification and observation triage

- User 2026-10-20 accepted: `directional (--move|--focus) --direction
  (left|right|up|down)` adds an exact-owner, single-pending control alongside
  workspace/resize controls. It resolves the live foreground and dispatches
  through the existing production directional handler; owner, origin,
  lifetime, scope, suspension, topology, arrival and focus fences remain.
  Like workspace controls, it works independently of keyboard takeover.
  Proof owners refuse. Injected keyboard filtering and helper proof gates
  are unchanged. Serial CLI wrapper plus production outcome lines identify
  agent-driven handler evidence, not physical shortcut delivery. Independent
  review accepted its owner/proof/production-handler fencing; argument,
  transport refusal and production-dispatch regressions pass. Stop/restore
  clean up the exact owner's pending directional request.

### Directional handler outcomes (2026-10-20)

All checks below used serial exact-owner CLI requests on owned disposable
applications in normal tiling. Each dispatch wrapper was joined to the
production outcome, plan/readback and independent PMv2 frames. These are
agent-observed native outcomes, not injected-key or physical-user acceptance.

| Check | Observed outcome |
| --- | --- |
| Paint moves right-to-left and left-to-right, occupied destinations | Pass: committed arrival on target and departure from source, exact planned visible frame, source survivor reflow, foreground retained on mover, both current workspace identities unchanged. |
| Paint moves both ways, empty current destination | Pass: same arrival/focus/reflow checks; setup workspace selection was separate, and the transfer changed neither output's current workspace. |
| Local move before exhausted-edge crossing, both directions | Pass: Firefox rightward and Paint leftward each produced a local `move-applied`, then a cross-output `move-applied` with exact arrival. |
| Firefox occupied destination, both directions | Pass: isolated-profile Firefox arrived exactly; the right-to-left path emitted DPI rebasing and committed successfully rather than rolling back. Source-survivor reflow was covered on the rightward leg; the leftward leg had no source survivor. |
| Firefox right-to-left into empty destination | Pass: exact arrival, both Paint source survivors reflowed, foreground retained and current workspaces unchanged. Firefox's converse empty-destination leg was not run. |
| Horizontal cross-output focus, both directions | Pass: selected the last observed focused eligible target, with unchanged geometry and current workspaces. Setup focus hops also update recency; this did not independently discriminate recency from geometric fallback. |
| Focus toward empty destination, both directions | Pass: `unchanged`, with foreground, geometry and current workspaces unchanged. |

- Cross-DPI transfers emitted `dpi-transition: corrected` before exact
  readback and `move-applied`. No transfer tolerance or fence was relaxed,
  and no further production correction was needed during these checks.
- Notepad directional coverage was parked because singleton/tab brokerage
  made multiwindow ownership binding ambiguous; it was not claimed passed.
  Paint supplied the complete occupied/empty matrix, with Firefox coverage
  above. A failed Firefox setup across owner restart was discarded: restore
  correctly revealed hidden Paint windows, invalidating the intended empty
  destination. The final Firefox checks used a fresh single-owner fixture.
- All final runs ended with verified owner exit, independent restore,
  identical settings bytes, unchanged relevant SPI/taskbar state, absent
  ledger/control requests and no owned-app residue. Native artifact/source
  identity, raw logs and PMv2 readbacks remain external.
- Final locked native four-package build/tests, workspace format, strict
  all-target clippy and whitespace pass: 2,858 passed, zero failed, one
  ignored. The reouter regression also includes the exact fresh eight-pixel
  inset vector from Firefox's verified right-to-left transfer.

### Parked-observation triage

- Historical drag traces report Notepad
  `drag-drop-refused`, `kind: unchanged`, `hover_prior: none`, with Paint on
  the intended destination. They lack the reply variant and settled
  source/destination keys, pair rows and release-point correlation needed to
  distinguish routing/capture failure from a legitimate snap-back. Those
  traces do not contain `cross_output`; its presence in current source is
  not historical evidence that the pair route ran. The bounded follow-up
  below now distinguishes an input-fixture failure from two remaining routes.
- The reported transient Paint disappearance was a teardown artifact, not a
  demonstrated product failure. In both the initial failed fixture and a
  later parked run, sequential Paint-then-Notepad closes align with
  `managed: 2 -> 1 -> 0`; the gaps between the two drops match the gaps
  between close completions. No pre-cleanup disappearance was established.
  Reopen only with an independently observed pre-cleanup loss.

### Additional two-output checks (2026-10-20)

Agent results below used scoped disposable applications and injected mouse
input; later user physical 2026-10-20 results are explicitly labeled. No
complete checklist row is closed by these bounded legs.

| Row | Accepted evidence | Remaining gap |
| --- | --- | --- |
| W2M-02 | Paint tiled explicit sends follow/stay in both horizontal directions: production `send-applied`, exact PMv2 arrival, unchanged currents, source reflow where a survivor existed, mover focus for follow and source MRU for stay. Intentional-float follow and immediate return now both commit membership with the frame unchanged. User physical 2026-10-20: Win+Ctrl+Alt+arrows explicit send-follow works as designed. | Empty-destination, null-source-focus, other floating-boundary and rebound-stay coverage remain open. |
| W2M-06 | Occupied cross-output native title-bar drop passed twice in one direction: pair-route reply, release point, preview, plan and exact arrival agree. Cancel restored the source frame. User physical 2026-10-20: cross-output title-bar drag with drop overlay passed between outputs with different DPI. | Earlier empty destination refused `unchanged`; reverse DPI start classified as resize and refused `malformed-input`. The user check did not clearly cover empty-singleton/reverse-DPI-start legs, so they remain open, as does forced failed arrival. |
| W2M-13 | Secondary pager mouse picks selected occupied/empty views, hid/revealed owned windows and left the other output unchanged. Event-only restart recheck recovers both exact visible carriers after a naturally empty secondary census, with scoped hooks reinstalled for the new shell. User physical 2026-10-20: tiling survived Explorer restart, primary/secondary pager positions correct; primary anchor churn looked fine once. | Display changes, physical pager selection and repeated anchor-churn coverage remain unproven; the earlier agent recheck did not observe primary churn. |
| W2M-10 | Exact-owner forced loss with one hidden output claim, both apps visible after recovery, independent restore and clean ledger; a fresh owner then tiled surviving apps on both outputs exactly. | The second hide was blocked by a cursor guard. Watcher-versus-restore reveal attribution is unknown. A later clean owner exit had no identified initiator, so complete dual-hidden recovery is unproven. |
| W2M-08 | No cycle acceptance. Setup refused before a valid owned foreground could be established. | Fill/append/close/send/switch cycles on both outputs need a valid foreground fixture. |
| W2M-07 | No quiet-hidden acceptance. The incomplete dual-hide fixture left one app visible. | Sticky, unreadable-output, stale-reply, delayed-send and forced-refresh legs need their own bounded fixtures. |
| W2M-04 | Not run. | Settings Apply plus owner restart and a reliable focus fixture are still needed for each scope mode. |

#### Drag fixture and unresolved drag routes

- Absolute mouse input was normalized against the virtual desktop but lacked
  `MOUSEEVENTF_VIRTUALDESK`. A separate cursor probe reproduced primary-only
  mapping without the flag and exact virtual-desktop mapping with it. The
  in-repository drag driver now includes the flag on every absolute move;
  button-only events are unchanged and its offline mock checks the contract.
  Historical `unchanged` traces alone do not prove their intended drop point.
- The corrected external fixture proved the occupied pair route. Empty-drop
  refusal is separate: the singleton route reached the destination but no
  singleton projection was produced. The exact Engine branch is unproven;
  the discriminating offline singleton-shape case below precedes any fix.
  No shared-core change is selected.
- Reverse-direction Paint changed size during its DPI transition and settled
  as `PointerResize`; start `move-kind` was `unknown`. This does not prove why
  native move classification was unavailable. Attribute the start hit-test
  before changing gesture classification or geometry fences.
- Follow-up offline singleton replay through the real Engine projects and
  admits the sole mover with an empty destination and the retained producer
  shape. That shape alone does not reproduce the native refusal or exonerate
  other Engine inputs. New refusal-only `dispatch-diagnostic` lines record
  event domain/bounds/gaps, revision, mover frame/hints, source, release point,
  resize boundaries and retained destination count without native identities
  or application content. Reproduce the exact adapter event before a fix.
- START diagnostics now distinguish cursor-unavailable, hit-test-unavailable
  and a successful raw hit, with the sampled cursor and already-captured start
  frame. Classification still uses the same single bounded hit-test and
  unchanged mapping. Historical raw hit/timeout remains unknown. Further live
  drag input was parked because stable exclusive input was not established;
  Terminal foreground alone does not prove outside-user interference.

#### Accepted explicit output-send control and float blocker

- User 2026-10-20 accepted: `send-output (--follow|--stay) --direction
  (left|right|up|down)` queues one exact-owner request for a normal owner and
  calls the existing `workspace_output_send_one` / `run_output_send` route.
  Owner/proof/suspension/origin/lifetime/scope/topology/arrival fences and
  injected-key filtering are unchanged. Independent review accepted this
  production-handler boundary. Stop/restore clean up `send-output.request`.
- New controls now participate in the owner's existing idle request check;
  this also repairs the missing idle wake for the earlier directional control.
  No new polling loop was introduced.
- The original float follow send retained its physical frame on the source
  output but moved membership to the destination. The next explicit send failed the
  Windows dispatch gate `home.output == command output`, returning `unmanaged`
  for a managed float. Independent source review identifies an adapter gate
  contradiction, not evidence that a core change is required. The correction
  recognizes only a non-sticky, Engine-proven admitted float at its membership
  home using the existing float-send eligibility proof. Wrong-origin tiles,
  unadmitted floats, sticky and hidden subjects still refuse; full scope,
  lifetime/tag, overlay, topology and arrival fences remain. Direction ranking
  when a float's physical frame differs from its membership home remains TBD in the
  [minimal discriminator](../spec/reference-outcomes.md#windows-dual-output-transfer-discriminators);
  no new float-placement policy is selected.
- Native follow-right then immediate return-left with follow now both emit
  `send-applied` for an owned Paint float. Its physical frame stays unchanged,
  mover foreground is retained on follow, and neither output's current view
  changes. Four real-Engine adapter regressions cover success, wrong-origin
  tiles, unadmitted/sticky/hidden subjects and lifetime/scope drift. This is a
  bounded two-output handler pass, not complete W2M-02 or physical delivery.
  Return-with-stay is covered offline, not by that native run.
- Offline tests cover arguments, request validation, proof/owner refusal and
  the production dispatch entry's unmanaged path. Successful poll-to-dispatch
  behavior is covered by the live journey, not an end-to-end offline fixture.

#### Secondary pager restart correction

- Explorer rebuilds secondary children asynchronously. The single restart
  discovery cached `tray_left: None`; unlike primary facts, secondaries were
  never re-read without another broadcast or topology change. A later native
  child census matched the already accepted narrow hierarchy, distinguishing
  stale proof from a genuinely unsupported layout. The first restart journey
  therefore remained refused indefinitely despite recoverable facts.
- The earlier tentative five-second reproof deadline is superseded and removed.
  Windows supplies child create/show/hide/destroy/location WinEvents. User
  2026-10-20 accepted: reuse the owner thread with two Explorer-process-scoped
  hook bands and validate taskbar ancestry before invalidation. Broadcasts,
  native taskbar edges and topology changes revalidate even a proven or empty
  cached secondary census. Primary facts remain fresh on each event turn;
  pure primary/secondary tray-boundary regressions verify carrier geometry.
  The accepted hierarchy, tray exclusion and refusal policy are unchanged.
- Callbacks only enqueue bounded plain facts: at most 64 events plus a
  coalesced overflow flag, consumed once. Ordinary File Explorer subtrees do
  not invalidate anchors. Suspended owners drop the scoped hooks and defer
  taskbar filtering/enumeration until resume; no new game-PID callback,
  per-pump PID lookup or secondary retry timer was added. Offline suspend and
  scope tests are not live game-performance evidence.
- The first event-only restart failed: secondary census became empty and
  stayed empty despite a later provable hierarchy. The tray received
  `TaskbarCreated`, but disposable pager windows had disappeared before the
  broadcast; only their procedure set pager rediscovery. The durable tray now
  forwards that broadcast into the same atomic flag. Primary appearance also
  refreshes a late shell scope; partial hook installs retire together and retry
  on a later broadcast/resume/appearance. Bounded hook-scope outcome logs make
  reinstall or refusal visible without native process identities.
- Final artifact restart recheck passes: naturally empty secondary census
  reproves, hooks reinstall for the rebuilt shell, and both carriers return
  visible/topmost with exact independent PMv2 anchors. A fresh-owner control
  also proved the rebuilt hierarchy. No keyboard/mouse injection was needed.
  Primary tray boundary did not churn in the recheck, so the earlier sampled
  mismatch is not closed by unchanged before/after geometry. Physical picks,
  display changes and real-game incremental cost remain separate acceptance.

#### Clean owner exit attribution

- Retained loss-run evidence proves the first owner was deliberately terminated
  by the exact-owner emergency-stop leg. The second owner exited cleanly well
  before its configured deadline; no initiating stop is retained. Its cause
  remains unknown, not attributed to outside input or a product failure.
- The three clean loop exits previously shared an indistinguishable terminal
  record. A bounded normal `loop-exit` cause now distinguishes `stop-file`,
  `tray-stop` and `deadline`, preserving existing precedence and behavior.
  Native explicit-stop and short-deadline runs emit their respective causes
  and restore cleanly. Tray Stop remains unexercised; complete dual-hidden
  W2M-10 recovery is still unproven.

#### Gates, restoration and limits

- Latest native locked four-package build/tests, workspace format, strict
  all-target clippy and whitespace pass on Rust/Cargo 1.99.0 MSVC:
  2,888 passed, zero failed, one ignored.
  The earlier drag-driver offline mock passed, including virtual-desktop flag
  checks; that script is unchanged in this follow-up.
- Finished journeys verified owner absence, independent restoration, identical
  settings bytes, unchanged relevant SPI/taskbar state, no owned carriers/apps
  or ledger/control requests, and preservation of unrelated applications.
  Raw logs, artifact hashes, preimages and PMv2 readbacks remain external.
- Non-input pager restart and naturally foregrounded Paint float checks resumed
  successfully. Injected drag work remains parked after the earlier foreground
  refusals/cursor mismatch; outside-user interference was not established.
  Resume input-dependent legs only with a stable fixture; do not weaken guards.
  W2M-05/09/11, physical chords and display actions remain user checks;
  W2M-03/12 retain their world-route blockers. Admission and cursor-versus-focus
  targeting remain outside this follow-up.

### Foreground fixture recheck (2026-10-20)

- Status: fixture acceptance withheld; no product-path live leg resumed. Two
  ownerless Paint probes acquired foreground and returned exact cursor samples
  on both outputs; one same-output native title-bar nudge moved the owned
  window. These observations do not establish a compliant reusable fixture or
  stability under the scoped owner.
- The external probe primed last-input with an E8 key pair before acquiring
  foreground. Its guard excluded Terminal but did not require an owned input
  target. Three primes were issued with the pre-injection foreground reading
  as a pre-existing, unverified Notepad window. No state preimage or effect
  readback exists for that window; effects are unknown, not reported absent.
  Live work stopped under the guide's unexpected-effect/ownership boundary.
- Review also found teardown outside `finally`. Completed runs closed every
  created Paint window and released the mouse button, but failure cleanup was
  not proven. The move probe's geometry restoration readback differed from
  its requested rectangle; exact restoration is not claimed. The disposable
  window was subsequently closed. No product owner, hook or settings write
  ran, and no test-app residue remained in the read-only follow-up.
- The existing repository drag driver's foreground helper likewise primes
  before checking the receiving foreground. Do not reuse that acquisition
  route unchanged. A compliant follow-up needs an owned-target input guard or
  non-keyboard acquisition, cleanup in `finally`, and fresh under-owner
  foreground/cursor/caption proof before any product-dependent input leg.
- W2M-06/08/04/07 input legs remain manual checks until that fixture contract
  is accepted. Primary tray-boundary churn was not attempted in this recheck;
  its earlier acceptance gap remains open. No production code or product
  guard changed, and no new checklist pass or product decision is recorded.

### Input-driver containment (2026-10-20)

- Status: input safety is contained by refusal, not by an accepted live
  fixture. The repository drag driver's `-Live` entry now refuses before
  effects; `Set-MdForeground` and mouse/key helpers also refuse. Native input
  injection methods and keyboard priming are removed. `-Stop` recovery remains.
  Both external foreground/move probes refuse before native setup, launch or
  evidence-file mutation; their retained raw logs are unchanged.
- The first offline guarded repair failed independent review: button/release
  batches bypassed mouse-target proof, negative coordinates could skip guards,
  root-owner and actual HWND-to-PID validation were incomplete, and shared-host
  creation provenance and failure-path release were unproven. No live use
  occurred. Rather than expand this into a larger ordinary-app harness, the
  corrected route removes injection and disables live entry. Legacy stages and
  launch helpers remain disabled historical code, not a reusable fixture.
- Independent re-review accepts containment only. Final offline driver mock,
  live-library parser/seam check and 45-case fault mock pass. Bare entry points
  parse cleanly; repository `-Live` and both external `-Run` entry points return
  the expected refusal before effects. Retained raw logs are byte-identical
  across these probes. Whitespace checks pass. No Rust product code changed in
  this follow-up, so preceding native build/test/fmt/clippy results are prior
  evidence, not rerun gates.
- User 2026-10-20 accepted: retain disabled legacy stages for continuity
  rather than build a larger ordinary-app automation fixture. Re-enabling the
  old entry is not a fix; any replacement still needs independent invariant
  review, finally cleanup and under-owner foreground/cursor/caption proof.
- The live guide now states the invariant at point of use: every synthetic
  batch needs fresh test-created foreground-process proof; every mouse batch
  additionally needs target/current-cursor `WindowFromPoint` and root-owner
  proof. Cleanup in `finally` cannot bypass it. No Terminal input, unowned
  priming or elsewhere fallback is allowed.
- No desktop mutation, owner launch or synthetic input ran in this follow-up.
  In particular, no new input reached a non-owned window. The preceding six E8
  events with an unverified Notepad foreground remain an incident with unknown
  effects; disabling the driver does not establish absence of those effects.
- Foreground/cursor/caption stability under the scoped owner remains unproven.
  Primary tray-boundary churn and W2M-06 empty-singleton/reverse-DPI drags were
  not resumed; W2M-08/04/07 remain manual checks. No production fix, reference
  outcome or checklist pass is inferred. Admission, output-targeting policy,
  W2M-03/12 and physical-only rows remain outside this follow-up.

### User physical checks (2026-10-20)

- User physical 2026-10-20, W2M-01: Win+digit on each output passes except
  switching to an empty workspace focused another output's window, which a
  following workspace send then moved. The desired behavior is undecided;
  [minimal action sequence](../spec/reference-outcomes.md#empty-workspace-focus-across-outputs)
  awaits Tier 1 outcomes and a user decision. Win+Shift+arrows cross-output
  moves pass. No complete exhaustion/empty/topology matrix is inferred.
- User physical 2026-10-20, W2M-02: Win+Ctrl+Alt+arrows explicit send-follow
  works as designed; no rebound-stay or full boundary-matrix pass is inferred.
- User physical 2026-10-20, W2M-13: after Explorer restart tiling survived
  and primary/secondary pager positions were correct. Primary anchor churn
  looked fine once, not a repeated stress check or display-change pass.
- User physical 2026-10-20, W2M-06: cross-output title-bar drag with drop
  overlay passed between outputs with different DPI. Empty-singleton and
  reverse-DPI-start legs were not clearly covered and remain open.
- User physical 2026-10-20: Task Manager stays untiled; it runs elevated by
  default, an expected unmanageable class, not a defect.
- Evidence above is the user's physical report, not new instrumented agent
  testing; no complete checklist row is closed.

## Remaining units

1. Primary tray-boundary churn looked fine once in user physical 2026-10-20
   checks; tiling and pager positions survived Explorer restart. Repeated churn,
   display changes, physical pager selection and game cost remain unproven.
2. Manual check: with stable owned foreground and cursor guards, replay empty-singleton and
   reverse-DPI drag. Join new dispatch and START probe diagnostics, reproduce
   exact event inputs offline, then select an adapter-only repair or seek
   approval if shared-core behavior must change. Do not infer raw hit causes.
3. Complete remaining W2M-01 vertical-local, exhaustion/empty and topology
   physical discriminators; user physical 2026-10-20 already confirms
   Win+Shift+arrows crossing and Win+digit routing except the undecided empty
   workspace focus case. The bounded handler matrix is not complete row acceptance. Optional
   follow-ups: unambiguous Notepad fixture, Firefox's converse empty-destination
   leg and a recency-vs-geometric-fallback focus discriminator. The admitted
   float return gate now has bounded live evidence; other W2M-02 legs remain.
4. Manual checks: re-establish a stable foreground/input/owner fixture, then run W2M-08,
   W2M-04, W2M-07 and dual-hidden W2M-10. Keep physical-only, fixture-dependent
   and explicitly blocked legs parked without inferred passes.
5. User dogfood recheck of defect 4 under the delivered configurable output
   targeting. Core and Windows implementation and both-output fixture acceptance
   are recorded in the [stage 2c outcome](archive/core-world-model-rearchitecture.md#stage-2c-accepted-outcome-2026-10-22).

## Decisions and limits

- User 2026-10-20 accepted: configurable output targeting supersedes the tentative foreground, pointer,
  last-switch cache, stable-order policy and "admission never pointer" claim:
  [cross-platform cursor-first targeting](../decisions.md#cursor-first-output-targeting)
  replaces R-OUT-05's focused-output default on the Tier 1 evidence. Workspace
  switches default to cursor output (COSMIC), with focused output (i3/sway) as
  an option. New-window placement selects COSMIC (default: cursor output,
  split its last-focused window), Hyprland (cursor output, cursor-closest
  window), or i3/sway (focused output, its last-focused window). Workspace
  sends act on the focused window's output regardless of cursor. Historical
  2026-10-20: interim foreground-first commands (`1e7bfaf4`) remained pending
  adoption. Core/Windows targeting and settings are now delivered (see above).
  User-approved 2026-10-26: the three-way placement setting subsumes
  the launch half of workspace-switch targeting. Send relative-target scope
  and focus-follows-cursor/cursor-warp defaults are recorded in the linked
  decisions; core/Windows adoption is delivered, with native FFC hover-follow
  still unverified and default off.
- User dogfood 2026-10-20 reports cross-output window moves working well.
  Cross-output focus instead chose W3 from `O1=W1`, `O2=H[W2 W3]` on Win+Right;
  the user expected edge-nearest W2, not destination MRU. Record this as the
  [separate focus discriminator](../spec/reference-outcomes.md#cross-output-focus-edge-discriminator);
  the 2026-10-20 "not an approved shared-core change" wording is historical.
  User decision D1 2026-10-28 selects edge-nearest (shared-core policy, no
  setting); implementation pending with D2-D5 details unresolved. Trace
  verdict: no instrumented per-window trace is retained in-repo proving W3
  was O2's MRU, so bug versus working-as-designed is unproven; external raw
  logs were not inspected and the Tier 1 source audit is not a retroactive
  diagnosis.
- User 2026-10-20 accepted: secondary bars without a notification area
  may anchor at bar-right minus the existing gap only for the observed narrow
  HWND hierarchy: readable descendants, positive visible left app-button
  host, exact full-bar composition bridge and only zero-area input sites or
  recognized left launcher content. Unknown visible hierarchies refuse.
  HWND-less composited pixels are not observable through this census; this
  is bounded shell-topology support, not a universal visual exclusion proof.
  A quick physical check looked good; broader physical acceptance remains pending.
- Parked admission prototypes used an HWND-only freshness hint; a successor
  must retain the existing full identity/tag fences and avoid treating an
  enumerated-but-memberless hidden window as genuine established focus.
- Physical shortcut acceptance is bounded to the user physical 2026-10-20
  results above; monitor changes/hotplug and sleep/wake remain unaccepted.
  Shell restart also has the user's tiling/pager-position check; owner loss has
  only bounded agent evidence. No complete lifecycle or two-output checklist
  acceptance is established.

# KDE dual-output live testing and fixes

## Goal and acceptance

- Verify native two-output behavior with mixed scales and vertically offset
  outputs against the [coverage inventory](../research/two-output-test-environment.md#7-check-mapping-exact-triage-wording)
  and [multi-output backlog](../backlog.md#multi-output-setup).
- Record authoritative before/after state, distinguish passing subcases from
  incomplete gates, fix reproducible defects with regression coverage, and
  repeat affected live journeys.
- Preserve existing windows and restore settings, output configuration and
  development lifecycle state.

## Approach

- Establish owned native test clients, diagnostic capture and input delivery.
- Exercise static focus, transfer, workspace, geometry and overlay journeys.
- Exercise reversible topology changes and isolated process recovery.
- Prioritize defects by severity; retain unsupported product outcomes as TBD
  discriminating rows rather than inventing behavior.

## Must-pass checklist

Results combine the native passes and continuations on 2026-10-14.
Partial coverage does not complete an inventory gate. The physical input gate
was repaired and reviewed before the latest journeys. Earlier unowned-active
emissions remain excluded. A later unowned-active native maximize callback
also violated fixture ownership; its effect was restored and further live
checks stopped. Accepted subcases and remaining gaps are qualified below.

| ID | Inventory | Journey | Outcome / evidence |
| --- | --- | --- | --- |
| T01 | D06, #46 | New native windows join the focused output; left/right focus crosses outputs without wrong-domain writes | PASS prior subcases plus corrected opposite-pointer admission in both directions. Final-source inverse repeat has an owned managed focus, both outputs occupied, unchanged opposite pointer and routed newcomer focus/membership. Offline pre-debounce focus-race regressions pass. PENDING: clean physical cross-focus, wrong-domain-write audit and remaining birth/activation timing variants |
| T02 | D13, #43, #46 | Directional move exhausts local structure then crosses; explicit output send-follow/stay, empty/occupied targets and floating boundaries | PASS prior subcases plus local perpendicular V-to-H restructuring both directions and occupied-target left R4 native-written/followed/arrived. BLOCKED: occupied-right fixture lacks a protected-free shown source; no read-only exact-tree route, floating-workspace action or genuine fixed-size client in this fixture. PENDING: full exact D13 tree proof, physical stay bindings and inverse occupied R4 |
| T03 | #14 | Per-output workspace switching/history, shared-scope refusal where required, preset-specific arrows and letter bindings | PASS prior sampled callbacks retained. BLOCKED populated history: history/switch callbacks absent from the bounded action fixture. PENDING: both-output ring cases, shared-scope refusal and presets |
| T04 | #15 | Relative workspace send follow/stay, ring ends, trailing-empty fill and sole-mover source emptying on both outputs | PASS primary-scope next/previous follow, native arrival/state-confirmed, source retirement and other-view independence; stay retained view/membership. Secondary shown current outside its managed ring refused before commit twice. Stay-focus fixture lacked a same-output eligible peer and is not defect evidence. PENDING: complete ring-end/previous/stay matrix on both outputs |
| T05 | #42 | Whole-workspace migration preserves identity/order/tree/tiling state, occupied target and hidden workspace; overlay and float members | BLOCKED: migration actions are registered but unbound and absent from the bounded callback fixture. Native per-output capability verified; direct controller effective mode and migration acceptance remain pending |
| T06 | #34, #44, N4 | Mixed-scale/origin geometry, gaps, border, underlay and preview; dynamic mode/scale/work-area changes and restoration | PASS prior geometry subcases retained. PENDING: rendering and dynamic mode/scale changes. Exact effect identity was confirmed loaded only during final cleanup; earlier unrelated-name queries do not establish its loaded state |
| T07 | #1, #34 | Fullscreen/maximize on each output, carry/restore, reserved slot and no unsolicited fullscreen geometry/focus writes | PASS tiled-overlay subcases: five-second fullscreen hold; fullscreen carry left and maximize carry right with arrival/follow; destination peer retained half-slot; native overlay exit restored exact original modes/frames. PENDING: intentional-float overlay, born-state and real-game legs. Earlier synchronous/expando probes remain invalid |
| T08 | #47 | Pointer drag across outputs to occupied/empty targets; preview/drop agreement, snap-back/cancel and source removal | BLOCKED: protected frames leave no safe cross-output grab/path/drop in the current shown views; plain titlebar dispatch lacks a guarded destination/path route. No drag emitted. Atomic wrapper offline-reviewed; protected-free fixture, preview/drop, cancel and removal proof remain pending |
| T09 | #49 | Hidden/sticky domains quiet; immediate/delayed and rapid transfers reconcile without ping-pong or stale replay | PASS prior quiet/sticky and two correlated successive output-transfer round trips. Latest rapid fixture parked: wrong-direction no-target dispatch and timeout-bound clients are not transfer evidence. No new PASS. PENDING: in-flight rapid stress, actual delayed arrival/reply and stale replay |
| T10 | #14, #51 | Disable/re-enable displacement/return and primary change; exact topology restoration; physical unplug remains a distinct leg | PASS prior sampled software cases plus inverse disable/displace/return with exact frames and output configuration; explicit workspace move while absent was not undone by return. Return affinity of changed donor contents remains qualified. PENDING: new-content discriminator and physical unplug |
| T11 | #14 | Output-local trailing empty workspace: fill, close/empty, switch, stable spare and independent other output | PASS prior subcases plus two relative-follow fills, source retirement, sole-client close and three-second stable empty shown view with the other output unchanged. PENDING: direct per-output spare identity, switch/history route, repetition and complete inverse matrix; global append/order alone is not identity proof |
| T12 | #30, #50, N3 | Isolated Planner loss/recovery on two outputs; sleep/wake and native gaming remain separate legs | PASS Planner termination produced confirmed owner loss/narrow refusal with controller alive; manual fresh Planner replied, then managed development restart restored consistent lifecycle state. Later correlated transfers prove successful changed plans. PENDING: automatic supervision, sleep/wake and native gaming |
| T13 | #48 | KWin restart with surviving Planner and fresh-ID adoption | PENDING: restart not exercised |
| T14 | #2 | Wide source with two destination candidates | PENDING: requires a third output |
| T15 | #43 | Vertical output arrangement and work-area gap; up/down crossing and no-candidate refusal | PASS gated physical explicit up/down sends across touching output edges with arrival/follow; physical-gap and absent-direction no-candidate refusal; exact topology restoration. Physical output gaps are not panel/work-area gaps. PENDING: exhausted R4 up/down crossing and rendering |

## Initial accepted live evidence

- Base revision: `e48d33a`; native clients were normal, resizable Wayland
  Konsole windows, verified by their platform selection and display socket.
  Window snapshots carried stable identities, output, focus and frame state.
- Two newcomers opened on the focused first output and took native focus.
  Second-output newcomer admission has not been tested.
- Left/right focus callbacks selected local peers and crossed the output
  boundary in both directions. Remembered target-leaf selection is implemented
  in `kwin/src/plan-adapter.ts:9675-9676`; these observations do not select a
  new nearest-window policy.
- Explicit output-right sends first placed one client alone on the destination,
  then placed the other beside it. Destination readback: two equal-share
  logical frames, with 8-pixel outer/inter-window gaps inside the
  panel-adjusted, vertically offset work area. Source snapshots showed both
  clients absent. Matching reconcile/send plans were applied.
- Seven local move callbacks produced applied plans and native layout changes;
  the fixture did not isolate an exhausted edge, so it does not prove D13/R4.
- Protected real windows survived every captured snapshot; no close or keyboard
  input was sent to them. Both owned clients were closed and their processes
  verified gone.

## Initial desktop finding and baseline resolution

- Initial fixture preservation finding: controller startup retired a pre-existing
  backing desktop and appended a new spare. The desktop count remained four,
  but identity/order differed from the baseline after teardown. Two new
  native default-tiling groups were also persisted. They were left intact;
  no broad configuration rollback was attempted.
- Root cause: `kwin/src/workspace-native.ts:1869` adopts live shared desktops
  as managed; `ensureTrailingEmptyDesktop` at `:644-670` retires non-trailing
  invisible empties; `removeManagedEmptyShared` at `:3487-3529` performs native
  removal. The removal/create diagnostic pair was captured at startup.
  The fixture lacked per-desktop occupancy/visibility readback at that instant,
  so it cannot independently prove every removal guard.
- This mechanism is selected product behavior, not an established product
  defect: [workspace decisions](../decisions.md#workspaces)
  explicitly permit adopted managed empty retirement. Existing tests at
  `kwin/tests/workspace-native.test.ts:309-324,353-369` cover it. Changing
  cleanup to retain all pre-existing empties would change product intent.
- Corrected the stale R-WS-10 KDE source description that claimed the adapter
  only writes membership. No product behavior or new durable decision changed.
- Approved by user 2026-10-14: use the current desktop set as the baseline.
  Normal product retirement/appending of desktops and persisted default tiling
  groups are observations of selected behavior, not harness restoration
  failures. The original desktop identities were not recreated.
- The continuation captured desktop memberships, visibility and per-output
  current views before startup and after teardown. Native per-output read
  capability is present. Requested mode defaults to `per-output-local` when
  the configuration key is absent (`kwin/src/plan-adapter-entry.ts:543`).
  Differing views corroborate that mode, but direct controller effective-mode
  readback is still pending; capability/default/absence of degradation logs
  must not substitute for it.

## Continuation evidence (2026-10-14)

- The repaired input fixture creates a correctly named keyboard/pointer uinput
  device and detaches the holder's standard streams. Device registration was
  observed; an owned active native terminal received exactly `x`, and KWin's
  cursor readback changed after relative motion. This proves input transport,
  not every shortcut or any drag journey. The diagnostic driver resolves the
  current KWin PID and checks exact script cleanup; a positive journal marker
  established capture before relying on command diagnostics.
- Evidence keys `newc` and `fl1`: a newcomer joined the second output's shown
  workspace and became active; leftward focus dispatched and selected an
  owned client on the other output. Pointer and focus agreed for admission,
  so focused-output versus pointer-output routing remains untested.
- Evidence keys `mv1`, `mv2`, `mv3`: leftward R4 from a sole source progressed
  through `r4-transfer-started`, `r4-native-written`, `r4-followed`, `arrived`;
  the next move changed local structure without output departure. The
  rightward crossing started transfer but ended `stale-scope`, then source
  and destination reconciles applied and native readback placed the mover on
  the destination. Arrival alone does not prove successful terminal/follow
  handling; do not promote this leg to PASS.
- Evidence keys `so1..3`: explicit output-left follow into an occupied target,
  output-right follow into an empty target and a second rightward follow into
  that now-occupied target. Native membership/output/focus readbacks agree
  with arrival and `state-confirmed`; the occupied destination had two
  equal-share frames with logical gaps. Planner correlations `plan-1-w0..w2`
  explicitly identify `op=send-to-output`, despite the incorrect KWin route
  label fixed below. These are bounded state/callback acceptance, not a clean
  physical end-to-end gate.
- Evidence keys `st1..2`: output-left/right stay via `invokeShortcut`, with
  mover arrival and `stay-confirmed`; source focus remained on the source
  output. Empty-default stay bindings were not physically tested.
- Evidence keys `flt1..3`, `stk1..3`: ordinary float enabled via callback,
  retained free geometry/focus through a directional half-snap, and was
  disabled via callback. The float-off leg has journal evidence only. Sticky
  readback became all-desktop, send refused `non-tiled-focus` before commit,
  and later readback cleared sticky. Sticky-off lacks a dedicated command
  journal. Neither ordinary float movement nor sticky refusal proves
  floating-workspace transfer or automatic-fixed eligibility.
- A physical float-toggle chord was not attempted after its conflicting
  native holder was identified. Configuration and the foreign holder were
  left intact; callback float tests do not resolve that shortcut leg.

## Fixture failure and excluded acceptance

- FAIL - owned-active input precondition: the input helper had no enforced
  active-window ownership check. A printable self-test key was emitted while
  an unowned terminal was active. Its effect on terminal contents is unknown.
  Five subsequent shortcut emissions also occurred while unowned windows
  were active after earlier focus operations. Live work stopped on review.
- Evidence keys `fr2`, `so3f2`, `st2f2`, `fb1`, `fb2` are excluded from clean
  physical acceptance. A consumed compositor shortcut or a correct final
  snapshot cannot repair the failed emission precondition. The initial
  owned-terminal input proof remains separate from the later unsafe self-test.
- The input fixture is not reusable until every key/click emission is gated
  by fresh owned-active readback and every focus-changing action forces a
  new check. Prove refusal with an unowned active window without emitting
  input; prove the positive path with a freshly verified owned client.

## Product defect and offline fix

- FAIL - output-send diagnostic labeling: Planner `op=send-to-output` and
  native cross-output arrival were reported by KWin as
  `route=send-to-workspace`. Root cause in the pre-fix adapter:
  `kwin/src/workspace-send-adapter.ts:4256` hardcoded `diag()` route, `:4123`
  defaulted terminal release to the workspace route, and `:4226` hardcoded
  late replies. This caused an incorrect workspace-send classification during
  evidence review; joining Planner correlations corrected it.
- Fixed active diagnostics/release using the flight's cross-output flag;
  preflight refusals use the requested operation. Late replies retain their
  originating route/component in the request callback, including after a
  newer opposite-kind flight starts. No wire operation or actuation changed.
- One failed implementation approach used a mutable latest-route field for
  late replies; review showed that a newer flight could overwrite it. The
  causal correction binds the route in the original callback, with regressions
  for both output-to-workspace and workspace-to-output flight succession.
- Approved by user 2026-10-14: correct diagnostic route tokens to match
  the actual operation, including late migration replies; transfer semantics
  are unchanged.
- Offline gates passed after the diagnostic change: KWin typecheck, build and
  full suite (1497 tests). The latest combined suite passes 1502 tests.
- Live re-verification passed after restart: evidence keys `soR`, `soL` and
  `stR` show output-route diagnostics with native arrival, follow
  `state-confirmed` or callback `stay-confirmed`. A subsequent relative
  workspace send retains `route=send-to-workspace`; operation routing is not
  globally relabeled. The follow legs used reviewed owned-active input.

## Reviewed input fixture and latest accepted evidence

- The first proposed gate was rejected: it trusted cached query files, accepted
  a missing ownership PID and allowed a skipped post-check. The corrected
  fixture queries KWin internally immediately before one short chord/motion/
  complete drag, requires exact recorded launched-window identity and positive
  PID, bounds snapshot age to one second, and always queries/logs after an
  emitted batch. Devices are released/destroyed within each batch; legacy
  emitters and standalone button-down are disabled.
- Offline gates passed: 27 policy cases and 10 real-wrapper cases with mocked
  query/event I/O. Non-owned-active refusal emitted zero presses/motion;
  missing/stale queries, invalid ownership, protected overlap and malformed
  chords refused. Independent correction review accepted the gate. Read-only
  queries demonstrated marker correlation and sub-second snapshot freshness.
- Evidence key `post-launch`: a native resizable newcomer joined the focused
  first output's shown workspace while the pointer stayed on the second
  output. The exact launched PID matched native window identity. The inverse
  opposite-pointer leg is still pending.
- Later physical focus presses were admitted from owned active clients but
  selected pre-existing/local peers. A changed post-active identity is normal
  for a focus command, not by itself an emission-precondition failure. These
  fixtures did not isolate the cross-output edge, so they do not establish a
  new focus defect or complete physical cross-focus acceptance.
- Evidence keys `t03-audit-retrieved`, `t04-audit-retrieved`, `t04stay` and
  `t04rel-post`: current ordinal no-op, unresolved ordinal/history refusals,
  first-output relative-next switch and send-follow, spare append, emptied
  source retirement and callback stay. The other output's shown view remained
  unchanged. These sampled results do not prove populated history or both
  output rings.
- Initial mode probes incorrectly wrote a JavaScript `maximized` property and
  read/reset fullscreen within one synchronous script. They are excluded.
  Corrected probes use native `setMaximize` and `fullScreen`, then independent
  settled readbacks of `maximizeMode`, flags, frame, focus and membership.
  Evidence keys `hdmi-max-moderead`, `dp-max-read`, `hdmi-fs-held` and
  `dp-fs-held` show full work-area maximize/full-output fullscreen on each
  scale, stable five-second fullscreen holds and restored tile frames.
- A separate follow-input batch's mandatory post-query timed out. Its native
  destination snapshot is retained as qualified state evidence, but the batch
  is excluded from clean physical acceptance. The timeout remains unresolved.

## Rightward R4 defect, correction and re-verification

- FAIL reproduced with one owned source client on an otherwise empty shown
  workspace: `r4-transfer-started`, then `geometry-scope` refusal. Added
  fence/sub-comparison diagnostics identified `window-count fresh_len=0 flight_len=1`
  after native setters, before `r4-native-written`. Native
  arrival later reconciled; the inverse leftward transfer arrived cleanly.
- Root cause: combined scope collection requires both output and desktop
  membership (`kwin/src/plan-adapter-entry.ts:3341-3357`). During own
  relocation, the mover can be absent from both collected scopes. The old
  count guard in `r4FreshScopeAllowsMover` rejected that absence before its
  documented source/half/target tolerance could run. Exact native flush
  timing is inferred, not independently proven.
- Corrected `kwin/src/plan-adapter.ts:10952` to allow only a post-setter
  geometry observation exactly equal to the flight set minus its mover,
  with retained live-reference native output and single desktop each in the
  source/target scope. Domain/scalar/bounds and all non-mover guards remain.
  Pre-transfer observation must still contain the mover; follow/arrival
  requires fresh exact target placement.
- Independent review rejected the first correction because it also admitted
  absent movers before any native setter. The causal narrowing disables that
  exception at pre-transfer and adds regression coverage. Review accepted
  the corrected boundary; no unresolved blocking finding remains.
- Five regressions cover the sole-mover gap, unrelated domain drift,
  pre-transfer absence, occupied-target gap and missing non-mover refusal.
  The sole-mover test failed before the repair and passed afterward.
  Latest typecheck/build/full KWin suite passed (1502 tests).
- PASS live after the final narrowing and controller restart: evidence keys
  `r4r4-post-journal` and `r4l3-post-journal` show
  `r4-transfer-started`, `r4-native-written`, `r4-followed`, `arrived`, with
  native destination membership/output/focus. No rejecting fence appeared.
  Occupied R4 remains offline-covered, with its live repeat pending.
- Approved by user 2026-10-14: implement the existing half-placement
  tolerance only at the post-setter write fence and retain named rejecting
  fence/sub-comparison diagnostics. Tier 1 COSMIC, Hyprland, i3 and sway all
  cross to the adjacent output with the mover followed/focused; this delivers
  that cleanly instead of through recovery. No new transfer policy is selected.

## Parked investigations and acceptance

- The [R4 discriminator](../spec/reference-outcomes.md#kde-directional-transfer-settlement-discriminator)
  now records the reproduced gap and corrected sole-source settlement;
  arbitrary partial-write failures and reference outcomes remain TBD.
- Direct effective-mode readback, inverse opposite-pointer admission, exact
  D13 perpendicular local-wrap, fixed-size and floating-workspace legs remain
  pending. A diagnostic that computed effective mode from requested mode and
  native capability was rejected as direct controller evidence. Requested
  `per-output-local` and native read/write capability are verified separately.
- Native rendering cannot be accepted from geometry or request logs. The
  later effect-loaded readback was false; a fresh compatible effect/session
  is required for overlay rendering acceptance.
- Cross-output drag needs a reviewed atomic native-grab gesture and owned
  drop fixture. Options: verified owned titlebar drag, or atomic modifier-drag.
  Recommendation: offline-test/review modifier-drag before physical use.
- All other unexercised checklist rows remain pending. KWin restart, sleep,
  real games and third-output selection need their distinct environments or
  session-boundary journeys; no nearby pass substitutes for them.

## Initial verification and restoration

- Initial checkout clean; development controller/planner stopped.
- Initial output topology verified: two enabled outputs, mixed scale,
  vertically offset, distinct priorities.
- Development controller and Planner stopped through the owned teardown;
  packaged enablement restored. Original active real window refocused.
- Output mode/scale/position/priority/rotation unchanged and re-verified.
  Shortcut configuration byte-identical to baseline. Project enablement key
  restored; desktop identity/order and new tiling groups remain different.
- Native effect binary was not rebuilt or replaced; overlay rendering and
  loaded-binary freshness are not claimed by the geometry evidence.
- Diagnostic scripts unloaded; no owned test windows remain.
- Documentation diff whitespace check passed. No executable source changed;
  existing lifecycle regression coverage supports the policy explanation.

## Continuation restoration

- Owned test clients were absent in closing/post-teardown snapshots; both
  pre-existing terminals survived. The original active terminal was refocused.
  Owned Planner and test-client process IDs were subsequently absent.
- Development mode returned DOWN: Planner name unowned, controller unloaded,
  no receipt/recorded process, packaged enablement restored. The transient
  effect was unloaded with readback; rebuilding/staging did not prove a fresh
  binary in the already-running compositor. No overlay acceptance is claimed.
- Output enablement, mode, position, scale, rotation and priority match the
  baseline. No output mutation occurred. Shortcut configuration is
  byte-identical to its backup. Desktop spare rotation and added default tiling
  groups are normal product observations under the baseline treatment above.
- Input releases preceded device destruction. No named fixture uinput device
  or input-holder/test-launcher process remained at final read-only checks.
  Temporary diagnostic scripts were unloaded by their exact owned IDs.
- Terminal-content restoration cannot be claimed: the stray printable key's
  effect is unverified. No further input was used to attempt a repair.
- Executable changes are confined to send diagnostics and their regression
  tests. Final typecheck/build/full test suite and diff whitespace check passed.

## Latest fixture failure and final restoration

- FAIL fixture ownership: one native maximize callback ran while a pre-existing
  terminal was active, outside the physical emitter's ownership guard. It
  changed that window to the full work area, then was toggled back. Evidence
  keys `corr-check`, `corr-restore` and independent
  `audit-post-restore-moderead` prove the disturbance and final restoration.
  No printable key was emitted in this continuation. The earlier stray-key
  terminal-content uncertainty is not repaired by these mode readbacks.
- Further live checks stopped. Direct owned native-mode checks remain valid
  subcase evidence; the accidental callback is not accepted as a test step.
  All focus-dependent callback invocations need the same fresh owned-target
  precondition as physical emission before fixture reuse. Options: guarded
  callbacks or direct exact-owned native setters. Recommendation: guard both
  action routes and retain the zero-emission refusal test.
- Both pre-existing terminals survived with native `maximizeMode=0`,
  `fullScreen=false`, original membership, restored frames and original focus.
  Shown views restored. All owned clients/processes, device batches, diagnostic
  scripts, Planner, tray and development wrappers are gone.
- A manually restarted controller left the development receipt pointer stale;
  initial `dev-off` stopped the Planner but failed controller restoration.
  Exact owned-receipt stop unloaded the controller; a subsequent `dev-off`
  completed and removed owned development state. Use lifecycle-managed reload
  rather than direct receipt replacement in a future fixture.
- Final development state DOWN; packaged enablement restored to its pre-run
  true value, with controller unloaded as at baseline. Shortcut configuration
  is byte-identical to backup. Output enablement/mode/position/scale/rotation/
  priority match baseline; no topology mutation occurred. Product desktop
  spare retirement/appending and default groups remain observations.
- Native effect not replaced in the compositor; later readback was unloaded.
  Identified empty receipt-directory residue is preserved. Final diff whitespace
  check passed; no full inventory gate is complete.

## Next action

- Repeat the corrected occupied-target R4 journey and physical cross-output
  drag under the whole-target wrap decision below. Exact tree/spare
  diagnostics and the remaining inventory legs still need distinct evidence.
- Keep the failed rapid fixture parked. A replacement needs persistent clients,
  correct native direction and actual overlapping-flight/delayed/replay evidence.
- Reach missing history, floating-workspace and migration actions through the
  ownership-checked fixture. Rendering, remaining workspace matrices, born-state
  and real-game admissions, new-content topology and session-boundary checks
  remain distinct pending legs.

## Recovery and fixture re-review (2026-10-14)

- The interrupted development run remained UP with its matching Planner and
  controller receipt. No owned test clients or uinput holder remained. Its log
  followers belonged to the development process group, rather than orphaned
  test input helpers. Both protected terminals retained their identities,
  process, holding desktop, output and normal native modes. Controller tiling
  had changed their frames; the holding desktop was still shown on one output,
  so structural isolation had not completed.
- Current executable diff matches the recorded output-send diagnostic and R4
  post-setter corrections. Fresh typecheck, build and full KWin tests pass (1502,
  zero failures); diff whitespace check passes. No additional product repair
  was found in the interrupted run.
- The single action wrapper covers key/pointer batches, complete atomic
  modifier-drags, allowlisted shortcut callbacks and generated native actions.
  It requires fresh exact launched internal identity plus positive matching PID
  before and after, rejects protected/foreign active windows, and checks that
  protected desktops are hidden. Post-query timeout, foreign activation or
  action failure stops subsequent actions. Generated native scripts recheck
  ownership inside KWin; callbacks have fresh external pre/post checks.
  Legacy input emitters are disabled. Independent offline re-review passes
  69 policy, 14 emitter and 91 action-wrapper checks, including zero-effect
  refusals, timeout handling and modifier releases. No live input was emitted
  during this recovery.
- Post-query timeout root cause in the temporary query helper: under pipefail,
  `journalctl | grep -q` can return 141 when the consumer exits after finding
  the marker. An offline large-stream probe reproduces that false-negative.
  Capturing to a file before searching avoids SIGPIPE; marker polling now has
  a wall-clock deadline. This closes the identified polling cause, but does
  not itself prove a new live action's post-query.
- FAIL fixture readiness: old isolation scripts reference a retired desktop.
  Replacement scripts passed helper-level mock checks but independent review
  found no implemented live dispatcher and a restore occupancy guard that
  rejects the protected occupants of its intended holding target. Native-state
  query/schema integration was also incomplete. Two unsuccessful fixture
  approaches are retained as a blocker; neither was dispatched. These are
  harness failures, not product-test outcomes. Recommendation: implement a
  minimal view-only dispatcher with exact protected identity/PID, fresh live
  desktop/output resolution, occupancy guards and independently verified
  setup/restore before reuse. Protected windows need not be moved for isolation.

## Recovered topology evidence

- Evidence keys `t08-inv-post`, `t10-disabled-post`, `t10-reenabled-post` and
  `t10-swapped-post`, joined to their native output configurations, establish
  the sampled T10 software subcases. The second-output client's identity/PID
  and backing desktop remain constant through disable and return. Its native
  output changes to the survivor while disabled, then returns with its prior
  frame. The survivor client, current view and both protected placements remain
  unchanged. Priority exchange changes neither client's placement. Original,
  restored and final output configurations match exactly.
- Evidence keys `t15-arranged`, `t15-arranged-post`, `t15-restored` and
  `t15-restored-post` establish vertical topology setup, native work-area frame
  remap and restoration. These configuration/state subcases do not require an
  input-wrapper log. Up/down crossing and no-candidate input acceptance remain
  unproven; recovered drag and other input artifacts also lack sufficient
  contemporaneous gate evidence. They do not replace a fresh isolated run.

## Recovery restoration

- Verified development-process-group SIGINT completed normal owned teardown:
  controller unloaded, Planner/tray names unowned, receipt removed and packaged
  enablement restored. Final `dev-status` is DOWN. No owned test client, input
  device, log follower or temporary diagnostic script remains.
- After controller teardown, exact identity/PID-bound native restoration and a
  separate settled query prove both protected frames, output, membership and
  original active window equal the interrupted run's capture. Independent
  native readback shows `maximizeMode=0`, `fullScreen=false`, `minimized=false`
  for both. No input, close or kill targeted either protected terminal.
- Output enablement, mode, position, scale, rotation and priority match the exact
  captured baseline. Owned enablement keys are restored; desktop spare churn
  and default tiling groups remain accepted lifecycle observations. One retired
  empty desktop was not recreated; the protected holding view was restored and
  the other output retains a live non-holding view. No native effect binary was
  replaced. Earlier terminal-content uncertainty remains unverified.

## Further native outcomes (2026-10-14)

- Fresh development startup was healthy. Ownership-checked journeys ran with
  real windows visible; independent offline checks passed 69 policy, 14 emitter
  and 91 wrapper cases. Exact owned-active pre/post validation, timeout refusal
  and visible-window drag geometry checks remained enforced.
- FAIL D06 / REQ-OUT-05, evidence `t01-cur4`, `t01-post`: focus on the second
  occupied output, pointer on the first; newcomer mapped and activated on the
  first. `observeNative` selects the already-active window's native output at
  `kwin/src/plan-adapter-entry.ts:2778-2819`, then its current desktop at
  `:2826-2831`; it has no pre-newcomer focused-output routing stage. Native
  activation has already changed that focus when this observation runs. This
  confirms the observe-first implementation gap, not a stale transfer.
  Parked: implement focused-output routing with an explicit timing/activation
  boundary, or change the requirement to native placement. Recommendation:
  retain D06 and design the narrow routing boundary consistent with D01 native
  activation ownership. No source change or unproven regression is claimed.
- T02 evidence `t02-preL`, `t02-postL1`, `t02-postR1` shows local perpendicular
  restructuring before crossing; projected frames prove V-to-H, not arbitrary
  internal tree topology. `t02-postL2` and its command journal show occupied left
  R4 native-written/followed/arrived. Right repeats remained local because a
  further local sibling existed; this is an incomplete edge fixture, not FAIL.
- T04 evidence `t04c1post`, `t04c3post` proves primary next/previous follow.
  `t04c2post` and `t04c2post2` prove unchanged native state after
  `current-out-of-ring` refusal. `workspace-native.ts:1721-1739` assigns
  unassigned pre-existing desktops to the primary scope; `:1401-1415` refuses a
  shown current outside the active output's ring. This is current implementation
  behavior, not evidence that secondary startup scope is the desired product
  outcome. The discriminator leaves visible-current seeding/cross-scope
  resolution TBD. Recommendation: retain the narrow refusal while that policy
  is unresolved. Stay-focus failure was rejected: the expected peer was on the
  other output, not an eligible source-MRU candidate.
- T15 evidence `t15-wunit-*` distinguishes physical gaps from panel/work-area
  gaps. `listPositionCandidates` at `plan-adapter-entry.ts:786-799` requires
  exact full-output edge touch. A physical gap refused correctly; closing it
  produced physical up-send arrival/follow and down-send return. No nearest-
  output fallback or changed adjacency decision was added.
- T10 evidence `t10invdis`, `t10invre` proves inverse displacement/return with
  stable native identities and exact frames. `t10exmoved`, `t10exfinal` proves
  explicit membership intent was retained after re-enable. The moved workspace
  returned to the re-enabled output; its former donor remained on the survivor.
  This is not proof of the separate changed-contents return-affinity policy.
- T12 recorded confirmed Planner name loss and narrow owner-absent refusal with
  the controller still loaded. A manually restarted Planner replied under a
  fresh owner; an unchanged-focus rejection proves transport, not changed-plan
  acceptance. Managed development teardown/restart reconciled stale Planner
  process metadata without replacing the controller receipt manually. Later
  `plan-1-w4..w7` transfers independently prove changed-plan acceptance.
- T09 evidence `t09-r1..r8` supports quiet/sticky and one settled round trip.
  The initial rapid-failure labels contradicted the side-by-side topology and
  are excluded. Reproof `t09reproof-a1..a4` records exact left/right action
  tuples, fresh pre/post ownership and `plan-1-w4..w7`, each arrived and
  state-confirmed; final state after three seconds equals the starting state.
  These successive guarded transfers do not prove overlapping in-flight or
  delayed/stale-replay stress.
- One narrowly causal query repair followed T09 fixture failure: a relative
  helper invocation passed a relative source path to KWin. `loadScript` and
  introspection succeeded but `run` returned `Could not open` that source.
  Canonicalizing the helper directory repaired source resolution; a fresh
  correlated query completed with exact cleanup and a 22-ms snapshot age.
  Query freshness/ownership policy was not relaxed.
- T07 evidence `t07-held-moderead`, `t07-carried-moderead`,
  `t07-maxcarried-moderead`, `t07-final-moderead` proves native held modes,
  cross-output tiled-overlay carry, retained peer half-slot and exact exit
  restore. Intentional-float overlays and rendering are not inferred.
- T11 evidence `t11closedB`, `t11settledB` proves inverse sole-client close,
  empty shown view and three-second stability with the other view unchanged.
  Direct per-output spare identity and a full fill/switch/close cycle remain
  pending; global desktop order alone is insufficient.
- Rendering remains untested. Some effect queries used an unrelated identity;
  the exact `omnitiler-active-border` effect was confirmed loaded during final
  cleanup and unloaded by normal owned teardown. Controller script readiness
  and native-effect readiness are separate; no binary replacement occurred.

## Latest restoration

- Development state DOWN, controller unloaded, Planner/tray names unowned and
  owned receipt/process state absent. Exact owned test clients, input devices,
  launchers, log followers and temporary diagnostic scripts are gone.
- Output enablement, mode, scale, position, rotation and priority match the
  original capture exactly; shortcut backups are byte-identical. No broad
  settings rollback was used. Packaged-script enablement returned to true under
  normal teardown and current configuration readback. Normal desktop/spare and
  default-group changes remain lifecycle observations.
- Both protected terminals survive with the same identities and process. No
  input, close or kill targeted them in this round. They retain their backing
  desktop but now reside on the survivor output's hidden view following the
  explicit-move topology discriminator; original output/frame/focus restoration
  is not claimed. Final exact-identity readback shows `maximizeMode=0`,
  `fullScreen=false`, `minimized=false` for both; its diagnostic script was
  unloaded. Earlier terminal-content uncertainty remains unverified.
- No additional product source changes were made. Existing executable gates
  remain the latest typecheck/build/1502-test pass; current record-only diff
  whitespace check passes. No complete inventory gate is claimed.

## Focused-output admission correction and breadth repeat (2026-10-14)

- Protected placement is restored: independent settled native reads match both
  original output, membership and frames, on a shown view, with normal modes.
  Early property-write readbacks did not converge; later reads matched exactly.
  The cause of that delayed state change is unestablished. No protected close,
  key, button, minimize or output-transfer method was used for restoration.
- D06 root remains the post-activation native-domain selection in
  `plan-adapter-entry.ts`. The adapter now retains the exact added reference
  and the last active eligible managed domain, proves that target is still
  shown/tiled, transfers output then membership before admission, and focuses
  the newcomer only after fresh placement/safety proof. Partial writes never
  continue using the pre-write observation.
- Routing refuses ambiguous births, adopted/seen windows, busy flights,
  missing native transfer support, fullscreen/held born-fullscreen, maximize,
  floating/sticky, fixed-size and transient/unclassifiable clients. Target
  fullscreen guards include all-desktop peers. This is not a general game
  classifier; the existing flag-based KDE boundary and separate real-game
  acceptance remain in force.
- Approved by user 2026-10-14: keep this host-placement boundary in the KDE
  adapter for 0.1. Move output selection into Rust core/Planner in post-0.1
  Push code down, alongside born-maximized. The current stateless Planner
  receives an already selected native domain; moving this small selection down
  would require a new focus/visible-
  workspace input contract without removing the native timing/safety checks.
  Shared tiling/admission policy remains in Rust.
- Independent review found and corrected exact-birth, shown-target,
  missing-transfer, transient, partial-write and sticky-fullscreen gaps. Adapter
  integration regressions exercise both directions and zero-write refusals;
  post-move float changes refuse focus.
- A further reproducible timing defect used the last debounced domain after a
  known managed focus shift immediately before birth. Synchronous, read-only
  capture on that managed activation now preserves the true pre-birth domain;
  unknown newcomer activations cannot replace it. Both burst regressions failed
  before correction and pass afterward.
- Latest executable gates: typecheck, build and full KWin suite PASS (1535,
  zero failures). No Rust/protocol or native-effect source changed.
- Final-source live evidence `w16-t01b` proves focused-first/opposite-pointer
  admission with newcomer focus; `w16-t01c` separately proves the originally
  failing inverse with a settled owned managed anchor and both outputs occupied.
  Its newcomer-route token is `routed`; membership, output and active readback
  agree, and the pointer remains opposite. The intra-round birth output was not
  separately captured. A first-window `no-managed-domain` outcome is native
  placement evidence only, not proof of the routing repair.
- One latest inverse pointer-positioning step ended over a protected frame.
  No key/button was emitted and the owned anchor stayed active; frames and
  membership remained exact. Future opposite-pointer fixtures must use a
  protected-free panel/gap point, rather than relying on active ownership alone.
- The initial final-source fixture failed because its terminal launcher had an
  unterminated shell quote, not because the adapter closed clients. Removing the
  malformed title line restored the persistent-client fixture; syntax checks
  and subsequent mapped clients prove the causal repair. No product acceptance
  is inferred from the failed fixture's close diagnostics.
- T02's occupied-right edge remains blocked by protected source peers; no
  exhausted-right move was accepted. Moving an owned peer into that view caused
  ordinary reflow, then its removal restored both protected frames exactly.
  Exact tree structure remains unavailable; projected frames do not substitute.
- T08 stopped before emission: no safe cross-output path/drop was available and
  plain titlebar dispatch did not supply the atomic path/destination guards.
- T09's latest PASS claims were rejected on evidence review: output-right from
  the rightmost output is a no-target refusal, unrelated reconcile correlations
  are not transfer proof, and a timeout-bound client disappearing is not arrival.
  Two fixture failures remain parked. R4's no-wire-ack design does not establish
  workspace/output-send delayed-reply or stale-replay acceptance.
- T11 adds two relative-follow fills, emptied-source retirement, sole-client
  close and stable three-second empty-view evidence with the other view
  unchanged. Scoped spare identity is not exposed by the fixture. T03/T05's
  switch/history and migration routes remain unavailable.
- T06 remains pending: the exact effect was observed loaded during normal
  development lifecycle, but no rendering observation was made. Script-loaded
  queries are invalid for effects. Correct Effects readback confirms unloaded
  after teardown; no live binary replacement or rendering PASS is claimed.
- Final restoration: both protected placements are exact and visible on the
  original shown view; both output views restored. Development is DOWN,
  controller/Planner/receipt and owned clients/helpers/devices absent. Shortcuts
  and owned enablement keys match backups; output configuration is unchanged.
  Normal desktop/spare/default-group churn is retained as lifecycle observation.
  Original focus/pointer restoration and terminal-content verification are not
  claimed. No full inventory gate is complete.

## Approved test procedure (2026-10-14)

- Verify owned-active identity before and after every input action. Structural
  isolation of non-test windows is not required; pointer paths must avoid
  non-test window frames. Earlier structural-isolation fixture failures do not
  impose a prerequisite on future tests. Safe owned grab/path/drop geometry
  and fresh ownership checks still apply to drag fixtures.
- Desktop ID/order/count changes and default tiling groups produced by normal
  OmniTiler lifecycle are observations, not restoration failures. Restore
  unrelated settings and protected-window placement under the live procedure;
  do not recreate retired empty desktops merely to reverse normal lifecycle.

## Integrated verification (2026-10-14)

- After integrating concurrent Windows/macOS deliveries, settings-only native
  build with warnings denied and CTest pass (26/26); KWin typecheck and full
  suite pass (1535/1535); Rust workspace tests pass (1855), formatting and
  all-target Clippy with warnings denied pass; diff whitespace checks pass.
- Non-macOS lint integration required cfg-gating macOS-only helpers/imports
  and constants. macOS validation order is preserved; no macOS live or host
  build acceptance is inferred from Linux checks.
- Latest KCM changes are build/test verified. Staging and live reopen remain
  deferred: the staging recipe also replaces the effect payload, and the
  loaded-library identity was not verified. Dual-output live inventory gaps
  remain as qualified above.

## Occupied-target R4 insertion correction (2026-10-14)

- Delivered in `e94723f`.
- Reproduced the wrong target tree offline with the reported logical geometry:
  left 2048x1152, right 1920x1080 offset down 72. Rust topology uses logical
  bounds; output scales do not participate in this insertion branch.
- Root cause: `crates/tiler-core/src/session/ops/move.rs` occupied R4 insertion
  unconditionally nested beside the remembered target leaf. Remembering far-side
  W1 in `L=H[W1,W2]` produced `H[H[W1,W3],W2]`; the mirrored rightward case
  likewise stranded the mover inside the target instead of nearest the source.
- Corrected parallel-root far-side insertion to use the existing whole-root
  fallback. The two mirrored tests in `session_cross_output.rs` fail on the old
  implementation and pass with exact root-child order and complete geometry.
  Near-side remembered-leaf, nested and perpendicular behavior is retained.
- A separate right-then-left journey passes with exact intermediate and return
  root shapes. Fixture admission first needs an R3 escape to establish the flat
  triple; both subsequent moves are R4. This journey also passes before repair
  because the departed remembered leaf becomes invalid and triggers fallback;
  it is a continuity check, not the RED discriminator. Native observation can
  select a surviving target leaf; the far-side fixtures isolate that failure.
- At this revision, the product-intent question was parked in the
  [minimal discriminator](../spec/reference-outcomes.md#kde-occupied-r4-target-shape-discriminator).
  The then-documented policy permitted remembered-leaf/root insertion; COSMIC R4
  always wraps the whole target, even a parallel root. R-OUT-01 establishes i3
  flat tail insertion and sway's parallel-left flat branch, but sway's exact
  wrapper and Hyprland's exact shape are TBD. No established Tier 1 majority
  overrides COSMIC under the reference-tier rule. Recommendation: whole-target
  D-axis wrapping for all occupied R4 targets. The later
  [whole-target decision](#whole-target-r4-wrap-decision-2026-10-14) selects
  and implements that broader change.
- Native exact-tree re-verification remains pending. The existing live record
  lacks a direct scoped tree diagnostic, so projected frames are not promoted
  to proof of the corrected hierarchy.
- Integrated offline gates: KWin typecheck and full suite 1547/1547; Rust
  workspace 1858 passed, formatting and all-target Clippy with warnings denied;
  diff whitespace check passes. Independent review accepted the mirrored
  nearest-source correction without selecting the broader target-shape policy.

## Cross-output drag lifecycle correction (2026-10-14)

- Delivered in `2adc1fc`.
- Log evidence: 3378 `drag-preview-refused reason=absent` lines, 206 settled
  preview lines, 75 busy/backoff pairs, 774 `kind=reconcile` lines and 16 stale
  replans. Reconcile line count is not an applied-plan count. The single
  unknown-window route diagnostic does not establish the cause of every refusal.
- Root causes in `kwin/src/plan-adapter.ts`: pointer-domain construction searched
  only the foreground observation for the mover; native output reassignment can
  put it in a hidden observation before Finish. Preview reply scope required
  the mover to remain on the foreground source. Hidden reconcile dispatch,
  deferred work and reply actuation bypassed the existing interactive hold.
  Refusal and busy/backoff diagnostics were emitted on every sampled tick.
- Pointer-domain construction now finds the mover in exactly one hidden holder
  when absent from foreground, then resolves the pointer destination. Preview
  replies accept a mover now native on that destination. Drop preserves its
  Started source binding; projection remains read-only. Missing/ambiguous mover,
  unreadable/ambiguous destination and existing owner/generation/sequence guards
  remain refusals. Same-output preview uses the same existing path.
- Foreground and hidden reconcile dispatch, queued work and in-flight reply
  actuation now share the interactive hold. Hidden refresh does not dispatch
  during it. Release converges from fresh observations; work-area reprojection
  retains its existing exemption. Final drag-drop intents still use their
  ordinary route rather than becoming reconcile work.
- Identical refusal/backoff diagnostics log once per drag and reason until a
  changed reason or a dispatched preview resets the state. Callback refusal
  still runs for each sample; diagnostic suppression does not change behavior.
- New `kwin/tests/drag-cross-output-regression.test.ts`: 12 cases cover both
  native-output crossings, same-output control, reply-time native arrival,
  absent/multiple hidden holders, refusal and busy deduplication, foreground
  hold/reply suppression, actual hidden drift, hidden in-flight reply suppression
  plus release/reapply, and drop routing with original source binding.
  Before repair nine cases fail and three pass; repaired suite passes 12/12.
  Independent review identified the initial missing hidden-flight/recovery proof;
  those causal cases are now explicit rather than inferred from foreground tests.
- Full KWin typecheck and 1547/1547 tests pass; Rust workspace 1858 passed,
  formatting and all-target Clippy with warnings denied pass; whitespace check
  passes. No native-effect or protocol change was needed.
- Physical drag remains unverified. Preview dispatch/callback tests do not prove
  rendering. A projected destination becoming foreground before native mover
  arrival retains its existing stale-sample refusal; a later sample/drop
  re-resolves. This timing subcase is not promoted to physical acceptance.
- Development readback remains DOWN. No new live input, test clients, output
  mutation or lifecycle startup was performed for these corrections.

### Physical repeat

1. Start `just dev` and retain its printed combined log location. Put two
   ordinary tiled test windows on L and one on R; focus the intended mover.
2. Use the usual titlebar or Meta+left-button drag from L to an edge of R's
   existing tile, outside its center/stack zone. Keep the button held for three
   seconds after crossing, move between two valid edges, then release.
3. During the hold, require a target-slot preview and stable non-dragged frames
   on both outputs. At release, require preview clearance and destination
   insertion matching the preview, without snap-back or source restore fights.
4. Repeat R to L, then a same-output control. Repeat onto an empty shown target
   workspace. Finally cross onto an occupied target and cancel with Esc; require
   preview clearance and the selected cancellation behavior without stuck holds.
5. Inspect the combined log for the gesture correlations: no repeated identical
   `drag-preview-refused reason=absent` or busy/backoff flood, and no reconcile
   geometry actuation while held. A real refusal should remain identifiable.
6. Close the test windows and stop development with Ctrl+C; confirm
   `just dev-status` is DOWN. Record each subcase independently; a dispatch diagnostic
   alone does not prove a visible preview or successful placement.

### Final integration verification

- Integrated the macOS owned-AX capability delivery. Its new macOS-only imports
  and holder frame implementation needed target-OS gates for non-macOS lint;
  unsupported stubs now explicitly ignore their PID argument. Native macOS code
  paths are unchanged; no macOS host acceptance is inferred.
- Final Rust workspace tests pass 1870 cases, formatting and all-target Clippy
  with warnings denied pass. KDE source is unchanged by integration; its
  typecheck and 1547/1547-test evidence remains current. Diff whitespace passes.
- At this revision, occupied-target shape selection and physical R4/drag
  acceptance remained pending; the broader dual-output inventory stays active.

## Whole-target R4 wrap decision (2026-10-14)

- User 2026-10-14 selects Tier 1 COSMIC observed R4 whole-target wrapping
  ([rules, S20 and S23](../cosmic-move-conformance.md#the-rules)); i3 flat
  append was considered and rejected. Predictable placement independent of
  target focus history preserves the existing target layout unchanged.
  [Durable decision](../decisions.md#move-layout-and-output-commands), item 5.5.
- Acceptance: every occupied exhausted directional crossing adds a new
  top-level D-axis split around the entire target, mover nearest source.
  Left gives `H[H[A,B],W]`, Right `H[W,H[A,B]]`, Up `V[V[A,B],W]`,
  Down `V[W,V[A,B]]`. Same-axis nesting is intentional. Single-leaf targets
  are wrapped the same way; empty targets retain a lone mover. Explicit
  output-send admission and local R1/R2/R3 rules retain their existing policy.
- Implementation removes remembered-leaf selection, beside-insertion and its
  unused helper from the shared core. It supersedes `e94723f`'s far-side-only
  repair: both near- and far-side memories now produce whole-target wraps.
  No normalization flattens the new same-axis group.
- RED/GREEN: the new near-side left regression failed before repair with
  `H[W1,H[W2,W3]]` instead of `H[H[W1,W2],W3]`, then passed afterward.
  Horizontal and vertical regressions cover both remembered sides, nested
  and perpendicular targets, complete geometry, source collapse and focus.
  Exact wrapped-child equality proves target identity, children and shares
  survive. Existing all-direction empty/single-target tests remain passing;
  the right-then-left journey asserts both intermediate and return shapes.
- Windows consumes the shared core behavior. Its existing transfer tests
  assert routing/geometry rather than the old tree shape; no Windows edits
  were needed. KWin's conformance model already wraps the entire target, so
  no adapter or KWin test changes were needed. Stale core/protocol test
  comments were aligned with the selected shape. Specification and reference
  Ours lines now distinguish this policy from explicit-send admission.
- Independent source review accepted the whole-wrap branch, retained target
  subtree, source collapse, focus and same-axis nesting. Focused cross-output
  tests pass (20). Final offline gates pass: Rust workspace tests, formatting,
  all-target Clippy with warnings denied and diff whitespace. KWin files did
  not change; its existing typecheck and 1547-test evidence remains current.
- Pending user exact-tree journey: `L=H[W1,W2,W3*]`, `R=W4`; move right ->
  `L=H[W1,W2]`, `R=H[W3,W4]`; move left -> `L=H[H[W1,W2],W3]`, `R=W4`.
  Verify the nested same-axis group and W3 focus. Native proof is deferred;
  projected frames do not substitute for hierarchy. The broader dual-output
  inventory remains active.

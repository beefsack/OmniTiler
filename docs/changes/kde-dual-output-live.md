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

## Current continuation (2026-10-18)

- Accepted bounded fix: a successful native output send can change desktop
  membership to the destination's captured current while output readback still
  names the source. Complete wire capture and immediate native branch evidence
  prove this delayed-output echo. Accept that exact same-flight shape alongside
  the synchronous target-output echo, with unchanged view, listing, safety and
  freshness gates. Both migration members now receive their membership writes;
  arbitrary third locations still supersede. Arrival and follow remain separate.
- Per-member diagnostics retain bounded gate/setter/proof tags without raw
  identities. Producer-shaped RED/GREEN tests cover delayed output propagation,
  both-member application, late arrival without replay, ordinary sends and
  unrelated-desktop refusal. Independent safety review accepts this KDE-only
  correction. No new product policy, retry or wire contract is selected.
- T05 remains blocked. The parked global-inventory confirmation defect is real:
  KDE repeats its native desktop inventory under each output, so source-list
  absence cannot confirm identity migration. A member-homing prototype settled
  before target follow; a view-gated correction reached target follow but left
  source refill ambiguous after its one native attempt. Retention then produced
  repeated removal feedback/no-op dispatches after member closure. Neither core
  prototype ships; existing core settlement is unchanged. Do not repeat either
  approach without replaying the complete arrival/refill/feedback sequence.
- Original T02 fixture passes right-follow, left-follow return and stay both
  ways: two owned peers per shown workspace, complete request/reply bytes,
  native arrival/focus/views and subsequent no-replay observations. The earlier
  retained `scope-held` did not recur. These passes used the core prototype;
  the final KDE-only repeat stopped at the protected-active fixture gate before
  its first command, so final-tree native revalidation remains pending.
- Ordinary topology subcases pass on the prototype-integrated tree: local
  horizontal precedence, horizontal cross-focus both ways, vertical-local focus
  and occupied R4 crossings both ways. Projected allocations/native frames match
  whole-target wrapping, but contain no internal tree: exact-tree acceptance is
  still pending. Three submissions without dispatch and later delayed focus
  evidence remain unrooted; invocation alone is not acceptance.
- Reversible opposite-edge rearrangement selected the new touching neighbor;
  the old absent direction did not cross. A positive physical gap prevented
  output-send crossing/wrap. FULL rectangles and work areas were captured
  separately. Post-gap local-focus settlement remains pending. Panel/work-area
  mutation was not attempted without an exact reversible API; a true split edge
  needs a third output and was unavailable.
- Full wire capture uses correlated bus method calls and returns, positively
  checked before each accepted leg. Initial diagnostic summaries were not full
  payloads and are excluded from complete-wire claims. Evidence keys include
  `wire.jsonl`, `mig-journal.txt`, `t05pre/post`, `t02sendR/L`, `t02stayR/L`,
  `l2r4pre/post/ret`, `bpostR`, `cpostR`, and baseline/restoration snapshots.
- Final integrated accepted-tree gates: Rust workspace **3280/3280**, KWin **1885/1885**,
  typecheck, script/Planner build, strict all-target workspace Clippy, formatting
  and portable-core gate pass. Dependency-notice verification remains blocked by
  an uncached platform dependency under frozen/offline checking; no dependency
  changed.
- Restoration: protected frames/membership/outputs and shown views match their
  captured baselines, including settled recovery of an arbitrary frame after
  teardown. Output configuration, panels and shortcuts are restored; owned
  clients/scripts are gone and development is DOWN. Newer external focus/pointer
  changes were preserved, not overwritten. Empty-desktop/name/default-tiling
  churn remains the documented lifecycle exception.
- Exact next action: replay the captured delayed member arrival, target follow,
  source-refill attempt and repeated feedback offline. Diagnose refill setter
  resolution/readback and terminal feedback before changing confirmation or
  retry policy. Then perform one bounded T05 repeat; inverse migration, exact
  tree, panel/work-area and final-tree native matrix remain open.

## Earlier continuation (2026-10-18)

- Startup ownership and friendly names completed 2026-10-18:
  [bounded outcome](archive/kde-startup-desktop-ownership.md). Core now owns
  single-homed startup conversion; KDE applies membership-only moves after
  backing proof and renames newly bound desktops. Native shared/distinct-current
  enable, all sampled exceptional states, independent switching, both output-send
  directions and normal controller restart pass. Exact protected/configuration/
  runtime restoration verified; normal desktop lifecycle churn retained. This
  does not complete the remaining multi-output inventory or T05 migration.

- Priority: reproduce the reported output-local workspace count/switch fault,
  correct confirmed implementation defects, recover the unbounded-size T05
  regressions, then resume the remaining non-drag inventory legs.
- Diagnostic reproduction: an output with a one-entry ring accepts ordinal 1
  as a no-op and rejects ordinal 3 with `selection-unknown`; the other output
  switches to ordinal 3. Native desktop creation without arguments throws
  `Insufficient arguments`. The full-state KDE create bridge uses that invalid
  call, preventing the required scoped spare from being bound.
- Investigate separately: startup adoption places unassigned natives on the
  primary scope, while publication retains another output's current view as a
  duplicate row. Do not change startup partition policy without resolving the
  existing visible-current discriminator. Adjacency extraction is out of scope.
- Evidence limits: this reproduction retained correlated redacted summaries and
  native views, not complete request/reply payloads or direct effective-mode
  readback. Original window frames were not captured, so frame restoration is
  unverified. These are diagnostic observations, not clean inventory acceptance.
- Teardown verified development DOWN, original memberships/views/focus and output
  configuration, byte-identical shortcut configuration and original float-store
  absence. Normal empty-desktop retirement was retained. Future live legs require
  full frame baselines, complete request/reply capture and direct mode evidence.
- Bounded work: repair the native creation contract with producer-shaped tests;
  independently review lifecycle safety; repeat with a complete fixture; recover
  T05 tests by stash identity and prove or repair their current production route.
- Offline correction: use KWin's void `(position, name)` API with a zero-based
  append position. Bind only a unique, fresh, exact-name native desktop from a
  complete snapshot; each token has its own pre-call baseline and cannot share a
  binding. Ambiguous native attempts never repeat; read-only preflight failures
  remain unattempted and recoverable; delayed proof settles exactly once.
- Review rejected shared-baseline/id-only recovery because it could steal a
  sibling or unrelated desktop. A correction armed before a read-only baseline
  and permanently held an unattempted token; that causal defect was narrowed to
  arm immediately before the native call. Regression fixtures now drain feedback
  before native invalidation and supply real desktop names; wire golds unchanged.
- Accepted offline evidence: KWin typecheck/build and **1845/1845** tests pass;
  focused lifecycle/entry/native/state/catchup tests **104/104** pass. Independent
  lifecycle review accepts the final correction. No core scope-policy change.
- Tentative implementation choice: newly created native desktops carry their
  logical token as name for void-API attribution. Existing names are not changed;
  workspace identity remains native-ID based.
- Complete-wire native repeat proves the required-argument creation succeeds
  once, with direct effective-mode evidence (`per-output-local`). It exposed a
  second implementation defect: the locally proven logical-to-native binding
  never reached the next report, so core retained its unbound token and adopted
  the new desktop elsewhere. Native name was not a logical binding. A trailing
  selection retained an unresolved view transfer; an output send to the duplicate
  current refused `cross-domain-mismatch`. T05 was not attempted in that run.
- Binding feedback is corrected offline: publish the locally proven binding over
  missing/generic fallback rows until a real core mapping acknowledges it; then
  retire the pending proof so later core migration remains authoritative. Four
  production-entry regressions cover immediate/delayed proof, fallback echoes,
  acknowledgement, subsequent migration and independent output tokens. The
  original missing-binding regression failed before the fix. Independent review
  accepts the handoff; combined KWin typecheck/build and **1850/1850** tests pass.
- This repeat restored exact baseline frames/modes, membership/views/focus,
  pointer, outputs, shortcuts, float-store absence and unloaded effects; development
  is DOWN. Normal empty-desktop retirement was retained. Startup also cleared a
  pre-existing maximized client under born-maximized admission; its exact mode and
  frame were restored. No new maximized-policy acceptance is claimed.
- Remaining single-homing fault: current-view publication emits a foreign-homed
  native in another scope; binding feedback appends it there without removing the
  retained old membership. Which output owns an initially shared native current
  is unresolved. Recommendation: decide startup ownership, then enforce one ring
  per native and separate visible-current rows from ownership. Do not repair it
  by relying on reply row order.
- Binding correction reverified with complete wire/native evidence: the next
  report carries the proven logical/native/output tuple, core consumes the token,
  and scoped rings reach two bound entries including a spare. Numbered selection
  changes the intended output's view only. Baseline restoration was exact.
- T05 now reaches the production migration command with two ordinary source
  members (bounded minimum, INT_MAX maximum) and an occupied target. Core accepts
  and plans the whole migration: the original size-hint refusal is cleared live.
  Native application failed: only the first member changed output, its desktop
  became the destination's old current, the second stayed behind, and later
  source refill ran without completed migration. No migration PASS is claimed.
- Root cause: pinned KWin `finishInteractiveOutputChange` synchronously replaces
  source-current membership with destination-current membership during
  `sendClientToScreen`. The adapter misclassified that exact native echo as newer
  external input. Corrected ordinary sends and migration to continue to the
  requested desktop within the same armed flight only after unchanged pre/post
  views, exact listing, safety, freshness and successful native-call proof.
  Third desktops, view races, unreadable views and overlay flips still stop.
- Six producer-shaped native-host/apply regressions fail before the correction
  where appropriate and pass after; independent safety review accepts it after
  synthetic fixture-identity cleanup. Integrated KWin typecheck/build and
  **1862/1862** tests pass. Native echo correction still needs a fresh T05 repeat;
  core-owned topology landed independently in the meantime, so earlier native
  evidence remains revision-specific. No topology extraction was performed here.
- Failed T05 teardown restored exact protected frames/modes, membership/views,
  focus/pointer, outputs/shortcuts/effects and float-store absence. Development is
  DOWN; owned clients/scripts are gone; normal desktop lifecycle churn remains.
- Native repeat at `b38455d5` still fails with the same partial-migration signature
  on the integrated direction-only/FULL-topology route: first member reaches the
  destination's old current desktop, second remains, destination never follows,
  later source refill runs. Core accepts and plans both INT_MAX members; no
  refusal is emitted. Offline echo coverage is not live acceptance. Park T05:
  collect bounded per-member branch evidence (captured views, membership-read
  availability, echo sub-gate and membership-set result) before another fix or
  acceptance attempt. Exact protected/configuration/runtime restoration verified.
- T03 independent native subcases at `b38455d5`: **10/10 pass**. Each output had
  two uniquely bound populated test workspaces; numbered selection, relative
  previous/next and two previous-history toggles changed the intended view and
  restored its owned focus, with the other output unchanged. Complete request/
  reply bodies and native before/after state agree. Ring wrap, presets, shared
  refusal and physical shortcut delivery remain separate pending legs.
- T02 explicit output-send follow into a uniquely bound occupied shown target
  passes left-to-right with native arrival/focus and unchanged views. The inverse
  refuses `scope-held` without native mutation. Its preceding observation reply
  held the retained source peer/domain as `unreadable` and source view as
  `incomplete-inventory`; the command request itself was complete/readable. The
  arming cause is unrooted. Park inverse/stay: replay that exact observation
  offline with per-scope hold evidence before retrying. Both stay legs unattempted.
- Both independent units restored their exact current baseline, including real
  frames/modes/membership/views/focus/pointer, output configuration, shortcuts,
  effects and float-store absence; development is DOWN, owned resources absent.
  Normal desktop lifecycle churn remains. No complete inventory gate is claimed.
- Final integrated offline check: isolated-source core library **410/410** tests
  pass, including exact T05 advisory hints; Rust formatting passes. Latest KWin
  typecheck/build and **1862/1862** test pass remain current; subsequent changes
  are records only. This does not override the native migration/send failures.

## Approach

- Establish owned native test clients, diagnostic capture and input delivery.
- Exercise static focus, transfer, workspace, geometry and overlay journeys.
- Exercise reversible topology changes and isolated process recovery.
- Prioritize defects by severity; retain unsupported product outcomes as TBD
  discriminating rows rather than inventing behavior.

## Must-pass checklist

Results combine the native passes and continuations on 2026-10-14 and 2026-10-15.
Partial coverage does not complete an inventory gate. The active-window input
gate was repaired and reviewed before the earlier journeys. Earlier unowned-active
emissions remain excluded. A later unowned-active native maximize callback
also violated fixture ownership; its effect was restored and further live
checks stopped. The latest pointer-path violation is separately recorded below.
Accepted subcases and remaining gaps are qualified below.

| ID | Inventory | Journey | Outcome / evidence |
| --- | --- | --- | --- |
| T01 | D06, #46 | New native windows join the focused output; left/right focus crosses outputs without wrong-domain writes | PASS prior subcases plus corrected opposite-pointer admission in both directions. Final-source inverse repeat has an owned managed focus, both outputs occupied, unchanged opposite pointer and routed newcomer focus/membership. Offline pre-debounce focus-race regressions pass. PENDING: clean physical cross-focus, wrong-domain-write audit and remaining birth/activation timing variants |
| T02 | D13, #43, #46 | Directional move exhausts local structure then crosses; explicit output send-follow/stay, empty/occupied targets and floating boundaries | PASS original two-peer right-follow/left-return and both stays on the prototype-integrated tree; no retained hold or replay. Local precedence and occupied R4 geometry pass both ways. PENDING: final KDE-only native repeat (fixture refused before dispatch), exact D13 tree proof, physical stay bindings and floating boundaries |
| T03 | #14 | Per-output workspace switching/history, shared-scope refusal where required, preset-specific arrows and letter bindings | PASS 2026-10-18 ten current-world callback/native-state cases: numbered, adjacent relative previous/next and history toggles on each uniquely bound populated output-local ring; other output unchanged. PENDING: full ring ends/wrap, shared-scope refusal, presets and physical delivery |
| T04 | #15 | Relative workspace send follow/stay, ring ends, trailing-empty fill and sole-mover source emptying on both outputs | PASS primary-scope next/previous follow, native arrival/state-confirmed, source retirement and other-view independence; stay retained view/membership. Secondary shown current outside its managed ring refused before commit twice. Stay-focus fixture lacked a same-output eligible peer and is not defect evidence. PENDING: complete ring-end/previous/stay matrix on both outputs |
| T05 | #42 | Whole-workspace migration preserves identity/order/tree/tiling state, occupied target and hidden workspace; overlay and float members | PARTIAL: INT_MAX accepted; source-output-lag echo corrected and both members move without replay. FAIL full settlement: core global-inventory confirmation is incompatible; unshipped confirmation prototypes lose target follow or retain an ambiguous source refill with no-op feedback storm. Inverse, exact tree/state and hidden/overlay/float matrix pending |
| T06 | #34, #44, N4 | Mixed-scale/origin geometry, gaps, border, underlay and preview; dynamic mode/scale/work-area changes and restoration | PASS prior geometry subcases retained. Latest normal development startup established effect/oracle availability during the run. PENDING: mapped-binary freshness, rendering and dynamic mode/scale changes; load/setter diagnostics do not prove rendering |
| T07 | #1, #34 | Fullscreen/maximize on each output, carry/restore, reserved slot and no unsolicited fullscreen geometry/focus writes | PASS tiled-overlay subcases: five-second fullscreen hold; fullscreen carry left and maximize carry right with arrival/follow; destination peer retained half-slot; native overlay exit restored exact original modes/frames. PENDING: intentional-float overlay, born-state and real-game legs. Earlier synchronous/expando probes remain invalid |
| T08 | #47 | Pointer drag across outputs to occupied/empty targets; preview/drop agreement, snap-back/cancel and source removal | BLOCKED: earlier crossings prove fallback recovery only. Latest effect-loaded same-output attempt violated pointer-path ownership through acceleration overshoot and is excluded from clean acceptance. Relative synthetic motion/drag now refuses before device allocation; physical preview/drop, hold, re-cross, cancel and removal proof remain pending |
| T09 | #49 | Hidden/sticky domains quiet; immediate/delayed and rapid transfers reconcile without ping-pong or stale replay | PASS prior quiet/sticky and two correlated successive output-transfer round trips. Latest rapid fixture parked: wrong-direction no-target dispatch and timeout-bound clients are not transfer evidence. No new PASS. PENDING: in-flight rapid stress, actual delayed arrival/reply and stale replay |
| T10 | #14, #51 | Disable/re-enable displacement/return and primary change; exact topology restoration; physical unplug remains a distinct leg | PASS prior sampled software cases plus inverse disable/displace/return with exact frames and output configuration; explicit workspace move while absent was not undone by return. Return affinity of changed donor contents remains qualified. PENDING: new-content discriminator and physical unplug |
| T11 | #14 | Output-local trailing empty workspace: fill, close/empty, switch, stable spare and independent other output | PASS prior subcases plus two relative-follow fills, source retirement, sole-client close and three-second stable empty shown view with the other output unchanged. PENDING: direct per-output spare identity, switch/history route, repetition and complete inverse matrix; global append/order alone is not identity proof |
| T12 | #30, #50, N3 | Isolated Planner loss/recovery on two outputs; sleep/wake and native gaming remain separate legs | PASS Planner termination produced confirmed owner loss/narrow refusal with controller alive; manual fresh Planner replied, then managed development restart restored consistent lifecycle state. Later correlated transfers prove successful changed plans. PENDING: automatic supervision, sleep/wake and native gaming |
| T13 | #48 | KWin restart with surviving Planner and fresh-ID adoption | PENDING: restart not exercised |
| T14 | #2 | Wide source with two destination candidates | PENDING: requires a third output |
| T15 | #43 | Vertical output arrangement and work-area gap; up/down crossing and no-candidate refusal | PASS prior gated up/down sends; new opposite-edge selection and positive-gap no-cross/no-wrap on the prototype-integrated tree, exact output restoration. Physical gaps are not panel/work-area gaps. PENDING: panel/work-area mutation, post-gap local focus, exhausted R4 up/down and rendering |

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

### Follow-up drag failures and correction (2026-10-15)

- Delivered corrections: `ad4dae9` (native drag sessions, reply recovery and
  stable-preview reuse), `a2d7824` (counted Planner summary coalescing),
  `b350e2e` (bounded ambiguous-refusal stages). Portable lint integration uses
  the concurrent `7655bb6` correction.
- A later two-output run still produced 6204 preview-settled records and 12659
  Planner summaries. Its first preview session reached Finish; subsequent
  verdicts reported `drag-unknown-window`, with ordinary stale reconciles
  continuing. These observations do not prove a permanently wedged flight.
- Acceptance for the follow-up: consecutive cross-output gestures remain
  eligible; same-gesture re-cross uses logical destination bounds; lost preview
  replies and abandoned idle holds recover with bounded diagnostics; repeated
  samples do not cause redundant planning or outcome-log floods. Keep native
  cancellation and refusal behavior and verify occupied, empty and same-output
  controls separately.
- Foreground-only Started/verdict lookup is independently reproduced when the
  mover belongs to another observed output. Cross-domain lookup now retains
  fresh observed eligibility. A destination becoming foreground before mover
  arrival also has a reproduced overly narrow preview reply fence.
- Offline regressions fail before those repairs and pass afterward. Preview
  reply expiry now invalidates late replies, including when no pending sample
  exists. Idle hold release gains a once-only abandonment diagnostic.
- Same-gesture right-then-left logical-domain coverage passes; coordinate
  misalignment is not yet reproduced and is not claimed fixed.
- Exact semantic reuse avoids redundant Planner calls for unchanged pointer,
  scene and hover state, excluding native frame drift. Membership, logical
  bounds/gaps, hints/eligibility, exact native reference, owner/generation and
  applied layout epoch invalidate reuse. Only `unchanged` and
  `unsupported-capability` refusals reuse; ephemeral refusals always retry.
  Identical rendered rectangles skip redundant overlay calls. A 200-sample
  stationary-pointer probe produced two plans, one overlay set and one settled
  line; this does not establish physical smoothness while moving.
- KWin logs preview state transitions and terminal counters. Planner summaries
  coalesce repeated preview outcomes, retain failure/recovery transitions and
  flush omitted-correlation spans/counts at a boundary or after 64 repeats.
  Non-preview commands retain complete summaries. Independent review corrected
  refusal-state loss, missing non-preview flushes, omitted layout/ref cache keys
  and accidental per-sample stage logging before acceptance.
- Tentative, user review pending: use a two-second per-preview reply deadline,
  exact semantic reuse without a new sampling-rate timer, and counted
  state-change diagnostics. These choices preserve the selected resolver,
  cancellation and interactive-hold behavior.
- Native synthetic crossings reached an empty target, an occupied target and
  the reverse output. Each oracle call failed while the effect was unloaded;
  each hold expired and subsequent reconcile restored tiling. These are
  fallback-placement/recovery observations, not preview rendering, drop-intent
  or preview/drop-agreement passes. One occupied crossing reproduced a Planner
  `cross-domain-mismatch` refusal, then preview recovery. No
  `drag-unknown-window` or `malformed-topology` was seen in this short repeat.
- A reverse drop activated a non-test window; further input stopped. An
  ownership-checked refocus refused without effects. Owned clients exited,
  development returned DOWN and pre-existing output/membership/frame sets,
  output configuration and shown views match baseline. Original focus was not
  restored; packaged enablement returned true through managed teardown.
- Missing effect/oracle readiness excludes all clean-drop acceptance in that
  repeat. Three-second holds, edge sweeps, same-output control, continuous
  re-cross and Esc remain pending. The existing atomic input fixture cannot
  express the hold/sweep sequence; no repeated harness redesign was attempted.
- The original run's parsed `drag-unknown-window` verdicts establish an oracle
  reply path there; the later missing-oracle repeat cannot explain those events.
  Its 37 Planner `cross-domain-mismatch` and 56 `malformed-topology` samples
  remain unrooted. Adapter-faithful retained Planner fixtures did not reproduce
  them. Bounded `direction=drag-stage` diagnostics now identify the failing
  convergence/capture/resolver/projection stage on a repeat; no guard was widened
  on speculation, and wire outcomes remain unchanged.
- Offline RED/GREEN regressions cover cross-domain Started/verdict lookup,
  destination-foreground reply timing, missing-reply recovery with and without
  queued samples, idle abandonment diagnostics, reuse/invalidation, refusal
  retry, summary preservation and stage/count logging. Same-gesture re-cross is
  a passing coordinate control, not a RED reproduction of visual misalignment.
- Integrated gates pass: Rust workspace build and 1932 tests, formatting,
  all-target Clippy with warnings denied; KWin typecheck and 1565 tests;
  settings-only warnings-denied build and CTest 26/26; whitespace checks.
  Integration needed the non-macOS probe stub to mirror its native function's
  argument-count lint allowance; native behavior is unchanged.
- Remaining P0 acceptance: physical smoothness and renderer alignment, exact
  drop/preview agreement and the ambiguous Planner refusal bands. Next action:
  run the physical-repeat sequence below with the effect loaded and its oracle
  endpoint available, retain the combined log, and inspect `direction=drag-stage`
  plus preview terminal counters. The record remains active for those gaps.

### Native effect readiness and fixture stop (2026-10-15)

- Normal KDE development uses the checkout's dev-staged native effect, not a
  system-installed replacement. `just dev` preflights discovery and loads
  `omnitiler-active-border` before `dev-on`
  (`just/linux.just:905-933,1159-1168`); the helper verifies the pinned compositor
  owner and loaded state (`scripts/dev-native-effect.sh:495-513`). Lower-level
  `dev-on` alone does not load the effect. Owned transient loads are unloaded
  during normal teardown; an unloaded effect after teardown is expected.
- Read-only discovery and ABI checks passed. A detached normal `just dev trace`
  run began DOWN, built/staged the native payload, reported a successful
  transient load and reached UP. Effect readback was loaded during the run;
  the retained combined log contains a fresh native `drag-verdict` and
  `drag-measure` for the owned mover. No system install or session restart was
  needed to establish effect/oracle availability. The previous missing-effect
  run's exact startup cause is not established; it did not reproduce under this
  normal entry point.
- Mapped-library freshness remains unproven: ABI/build compatibility and a
  rebuilt stage are not mapped-inode/hash evidence. The entry point's stale
  binary warning is unconditional and proves neither freshness nor staleness.
  A user logout/login is the documented route to remove this uncertainty after
  rebuilding, not a prerequisite inferred from this successful transient load.
- FAIL fixture pointer-path ownership, evidence keys `w-effect-readiness/ptr`,
  `post1` and `combined-final`: a same-output modifier-drag requested relative
  motion of -360 but the actual vertical travel was -667. The pointer finished
  over a non-test window while the active window remained owned. The precheck
  used raw deltas as logical coordinates; the postcheck verified active identity
  but not the cursor path. Its successful return therefore did not establish
  safe pointer delivery. Further synthetic pointer work stopped.
- That gesture produced two preview-set/clear pairs, 12 preview dispatches and
  an `unsupported-capability` drop refusal followed by stable reconcile. The
  fresh native verdict and rejection are transport/refusal evidence only.
  No held-preview screenshot was captured; cursor-aligned rendering, valid
  drop/preview agreement and next-drag acceptance are not claimed.
- Cross-output occupied L-to-R and R-to-L, continuous re-cross, three-second
  hold/two-edge sweep, empty target and Esc remain pending. The fixture cannot
  express held checkpoints/sweeps safely; plain drag lacks destination/path
  guards, and accelerated relative motion cannot prove the requested path.
  Options: physical journeys, or a separately verified coordinate-controlled
  fixture. Recommendation: physical repeat below rather than more fixture work.
- Temporary fixture repair now rejects `move`, `drag` and `moddrag` at policy,
  action-wrapper and emitter entry points before device allocation/emission.
  The acceleration-overshoot regression verifies zero device opens, queries and
  events. Independent static review accepted the fail-closed repair; keyboard
  and single-point click ownership/freshness checks remain covered. Historical
  restore-script manifest omissions were classified as review-only evidence,
  restoring the wrapper audit. Offline fixture gates: policy 71/71, emitter
  16/16, wrapper 92/92, acceleration regression 13/13. These helpers are temporary,
  not a delivered repository harness or a passing live drag fixture.
- No `cross-domain-mismatch`, `malformed-topology` or `direction=drag-stage`
  appeared in this same-output probe. It does not root the original 37/56 bands
  or reproduce cross-output overlay misalignment. Moving CPU cost was not
  measured: sample spacing and reply latency cannot substitute for profiling.
  No production guard or timing policy was changed on that evidence.
- Restoration: baseline/final/post-teardown snapshots match both pre-existing
  windows' frames, output, membership and original active identity; shown views
  and output configuration match, shortcut contents are restored. Normal
  desktop/spare rotation remains a lifecycle observation. Pointer position was
  not restored. Development is DOWN, effect unloaded, Planner/tray names
  unowned, and owned clients, diagnostic scripts and fixture devices are absent.
- Fresh integrated offline gates pass at `2af0848`: Rust workspace build and
  1932 tests, formatting, all-target Clippy with warnings denied; KWin typecheck
  and 1565 tests; settings-only warnings-denied build and CTest 26/26. The shared
  workspace-scope policy is included. This continuation changes records only;
  product fixes remain the preceding delivered commits, and native P0 remains
  open. Existing tentative timing/reuse/diagnostic choices remain user-review
  pending; no additional product decision was selected.

### Coordinate-controlled fixture follow-up (2026-10-15)

- The temporary pointer's newly appeared KWin InputDevice object was identified
  by exact device name, vendor/product, pointer capability and sysName; identity
  was rechecked before each property write. Flat profile with zero acceleration
  read back successfully. No real input device was reconfigured.
- Small same-output motion-only probes and their return legs had exact logical
  travel. This establishes local fixture positioning, not cross-output mapping.
  During a held native drag across the mixed-scale seam, a planned endpoint
  missed by 150 logical pixels and the fixture aborted/released. The mover's
  native output changed, but that is not a successful drop or preview result.
  Scale, native drag relocation and coordinate handling are not independently
  distinguished; do not label the shortfall a product rendering defect.
- Initial journey labels were invalid: both supposed cross-output drags stayed
  on the first output, and timed screenshots were taken after release. A later
  fixture proved two owned clients on the first output and one on the second,
  but its runs remained development-DOWN. Its exact same-output held arrivals
  and screenshots are native fixture evidence only, not OmniTiler acceptance.
  Additional fixture refusals covered an over-bound hop, a stale checkpoint
  after screenshot capture and an empty target without owned pointer cover.
- No occupied cross-output preview/drop, continuous re-cross, empty-target drop,
  Esc or next-drag product leg is accepted. The misalignment and original
  mismatch/malformed-topology bands remain unreproduced and unrooted; Planner
  behavior cannot be evaluated from controller-DOWN runs. Moving preview CPU
  cost also remains unmeasured. Further synthetic journeys are parked in favor
  of the physical repeat below; no absolute-axis fallback was introduced.
- Temporary safety checks cover strict device ownership/profile restoration,
  motion-only probes with zero button events, fresh owned-active/cursor checks,
  per-leg foreign-window checks and abort releases. Shell-background rows are
  excluded only with an explicit false normal-window flag; hidden protected
  rows require proven-hidden views. Offline helper regressions pass 40/40.
  Legacy relative-motion verbs remain disabled. This fixture is not a delivered
  repository harness, and passing local probes do not authorize seam motion.
- Final restoration used one motion-only action within an owned first-output
  cover, restoring the original follow-up pointer exactly before closing that
  cover. Independent baseline/final native snapshots match original pointer,
  focus, both protected placements/memberships and shown views. Test-device
  properties were restored and verified before destruction; only its generated
  configuration section was removed. Output configuration is unchanged, owned
  clients/scripts/devices are absent, and final `just dev-status` is DOWN.
  Normal desktop/spare identity rotation remains a lifecycle observation.
- Evidence keys: `wu-driver-run`, `wu-journey-evidence/fixture-proof`,
  `wjourney`, `J1.leg2`, `J4.hold2`, and `wu-restore-evidence/restoration` /
  `final`. No production source changed and no new product decision was made.
  Existing Rust/KWin/Settings gates remain applicable to unchanged product
  source; this record-only diff passes whitespace checks.

### Physical repeat

1. From DOWN, start `just dev trace` and retain its printed combined log location.
   Require the `omnitiler-active-border` transient-load message, loaded Effects
   readback and the native oracle endpoint before testing. If rebuilt binary
   freshness is uncertain, stop, log out/in, then repeat startup. Put two ordinary
   tiled test windows on L and one on R; focus the intended mover. Use physical
   input while relative-motion fixture verbs remain disabled, with paths over
   test windows only.
2. Use the usual titlebar or Meta+left-button drag from L to an edge of R's
   existing tile, outside its center/stack zone. Keep the button held for three
   seconds after crossing, move between two valid edges, then release.
3. During the hold, require a target-slot preview and stable non-dragged frames
   on both outputs. At release, require preview clearance and destination
   insertion matching the preview, without snap-back or source restore fights.
4. Repeat R to L, then cross right and back left without releasing and check
   preview/cursor alignment throughout. Run a same-output control and repeat
   onto an empty shown target workspace. Finally cross onto an occupied target
   and cancel with Esc; require preview clearance and the selected cancellation
   behavior without stuck holds. Start another drag after each terminal action.
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

## Whole-workspace migration unbounded maximum-size refusal (2026-10-15)

Status: current production world route fixed by `3c94a819`, confirmed offline
2026-10-18 with the exact T05 size shape; native repeat remains pending. The
historical legacy-route RED regressions below still describe the retired route,
not the production entry, which exclusively uses `DescribeWorld`.

- Current producer regression preserves min 176x150 and raw INT_MAX maxima in the
  world report without automatic-floating classification. A real `WorldEngine`
  regression accepts that report and produces a migration transfer; advisory
  size-hint use filters the maximum sentinel to absent. Bounded fixed-size pins
  and malformed-negative hint controls remain covered. The earlier proposed
  omission from the wire is not the current world contract.
- Isolated-source verification executes the new core migration test successfully;
  KWin producer regression and combined 1850-test suite pass, with typecheck/build.
  Legacy observer/adapter fixtures are not production acceptance. A later owned
  T05 fixture reached the core and passed this size gate; native application then
  exposed the output-send desktop-echo defect described in the continuation.

- Live T05 probe: an ordinary resizeable client reported a real minimum size
  (176x150) and Qt's unconstrained maximum (2147483647 x 2147483647, INT_MAX,
  meaning "no maximum"). Whole-workspace migration refused before commit with
  `omnitiler:route-diag component=workspace-migrate route=migrate-workspace
  stage=request ... event=refuse outcome=scope-invalid ... gate=pre-commit
  phase=request reason=scope-invalid`.
- Causal chain (revision-bound line locators):
  1. `readConstraintSize` (`kwin/src/plan-adapter-entry.ts:389-400`) rejects
     only negative/non-numeric extents, so INT_MAX passes through as a
     concrete `maxSize`.
  2. `observeMigrateWorkspace` (`kwin/src/plan-adapter-entry.ts:991`, hint
     read at about `:1339-1347`) copies it onto the observed source window.
  3. `isMigrateSize` (`kwin/src/workspace-send-adapter.ts:5954-5971`) accepts
     only integers in `0..16384`; `validateMigrateObservedWindow`
     (`:5997-6002`) therefore fails, `freshMigrate` (`:4482-4501`) returns
     null and the request refuses `scope-invalid` (`:4365-4368`).
- Expected (fix direction, tentative): an unbounded maximum is advisory "no
  maximum" and is omitted from the observation and wire (`max_size` absent),
  never refused. Bounded minimum/maximum pairs stay exact; negative extents
  stay absent. An unbounded hint never classifies an ordinary client as an
  automatic (fixed-size) float. Decide during the fix whether the omission
  belongs in `readConstraintSize` (affects every consumer, including ordinary
  reconcile and send wires) or only in the migrate observer; check how the
  ordinary reconcile wire currently treats INT_MAX before choosing.
- RED regression 1, `kwin/tests/workspace-migrate.test.ts`, in
  `describe("migrate observer origins and phases")`, named "omits
  Qt-unbounded maxSize hints while keeping bounded minSize exact":
  - `obsWorld` with currents `out-1: ws-1`, `out-2: ws-9`; windows on `ws-1`
    / `out-1`: `win-a` (min 176x150, max 2147483647x2147483647), `win-b`
    (min = max = 400x300), `win-n` (min width -5, height 100); `win-t` on
    `ws-9` / `out-2`; active `win-a`.
  - `observeMigrateWorkspace(world.workspace, new Map(), new Set(), GAPS,
    "right", OBS_POLICY)` returns `status: "ready"`.
  - `win-a`: `minSize` `{w:176,h:150}`, `maxSize` undefined, `fixedAuto`
    false, `floating` false. `win-b`: min and max both `{w:400,h:300}`.
    `win-n`: min and max undefined.
  - Observed RED: `win-a.maxSize` is `{w:2147483647,h:2147483647}`.
- RED regression 2, `kwin/tests/workspace-migrate-engine-fixture.test.ts`, in
  `describe("migrate engine fixture")`, named "migrates an ordinary client
  carrying Qt-unbounded maxSize with the real Engine" (production observer,
  adapter request and real Planner):
  - `makeHarness` windows: `m-win-1` (left, `ws-1`, min 176x150, max
    INT_MAX x INT_MAX), `m-win-2` (left, `ws-1`), `m-win-t` (right, `ws-9`).
  - `settlePlan()`, record both left frames, then
    `handle.requestWorkspaceMigrate("right")`.
  - Expect: no log line contains `outcome=scope-invalid`; both frames unchanged
    before the plan; `flushOp("migrate-workspace")` reply has
    `outcome: "planned"`, `kind: "migrate-workspace"`; the `m-win-1` wire entry
    has `min_size {w:176,h:150}` and no `max_size`, `fixed_auto` or `floating`
    keys; both movers end on output `out-right` with desktops `["ws-1"]`; the
    right output's current desktop is `ws-1`.
  - Observed RED: the `scope-invalid` refusal above.
- With both regressions added, the two migrate test files ran 83 tests: 81
  pass, these 2 fail.
- After the fix: GREEN both regressions, full KWin typecheck and suite, then
  repeat live T05 with an ordinary resizeable client on the migrating
  workspace.

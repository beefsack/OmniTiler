# Core platform switches: shared policy implementation

- Status: both removals approved 2026-11-12 under
  [C6](core-v4-review.md#user-decisions-2026-11-12); implemented offline.
  Final full gates and independent complete-diff review passed offline. The KDE
  follow-up below verifies selected command-route native legs; physical input
  and other-platform acceptance remain pending.
- Scope: shared send/focus policy, constructor/profile removal and regression
  coverage. Source/history research below describes `4c419cee`, before removal.
  Native results below are historical reports, not new tests.
- Acceptance: trace both switches through callers, distinguish host facts from
  policy, check stable Tier 1 sources, record minimal discriminators and options.
  [Layering](../principles.md#layering) and [Simplicity](../principles.md#simplicity)
  apply. Native delivery and unresolved reference outcomes remain TBD.

## Implementation outcome

- Automatic fixed-size float sends now use the existing slotless deferred
  transfer without `fixed_send_admission`. Sticky, explicit tile suppression,
  dual-overlay and ordinary identity/domain safety refusals remain. The
  `fixed_size_admission` classification mechanism is unchanged.
- `maximized_focus_fence` and the separate native world refusal are removed.
  Native and composed maximized origins skip local candidates and cross in all
  four directions when an adjacent eligible destination exists, otherwise stay.
  Composed floats use the existing remembered tiled destination; their inclusion
  is an assumption, not a separately explicit float answer. See the owning
  [decision](../decisions/window-state.md#directional-maximize-and-fullscreen-operations).
- Removed the empty `WorldEngineConfig`, constructor/reset profiles and unused
  pure fence predicate. Removed its three obsolete `window-policy.json` cases
  and corresponding test dispatches rather than skipping them. Immutable
  `world-wire/v4`, `stage5` and `stage5-v4` are untouched. The separate
  [anchor-setting removal](c9-product-evidence.md#fixed-largest-implementation)
  refreshes active settings witnesses through their recorders.
  Historical `focus-refused-maximize` wire vocabulary remains for codec witnesses.
- Regressions cover automatic send admission with slotless geometry and safety
  controls; local maximize skip, adjacent-output crossing, no-neighbor stay and
  destination overlay preservation for native/composed tiled/float origins.
  Up/Down tests use actual vertical adjacency, not a horizontal-only fixture.
  Ordinary/fullscreen navigation controls retain their existing behavior.
- Offline Rust run: `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`
  passed 3378 tests, zero failed, one filtered. Temporarily replaying the retired
  default automatic-send refusal and native maximized-focus refusal failed eight
  regressions (one send, seven focus); restoring the exact source hashes returned
  the implementation to the passing tree. This is a retired-semantics mutation
  check, not a historical checkout run. Final gates after the anchor-setting
  removal passed: format, three-package all-target build/clippy, the same Rust
  command (3377 passed, zero failed/ignored, one filtered), KDE typecheck and
  1302 tests (zero failures/skips, 191 suites). Independent complete-diff review
  covered both original partial implementations and their corrections, with no
  functional findings. Stale explanatory comments were clarified afterward.
- Native acceptance is not established. KDE must check fixed-float same/cross-
  output sends and native-maximize local-stay/adjacent-cross behavior with both
  overlays preserved. macOS must adopt shared fixed-float sends and composed-
  maximize tiled/float crossing in all directions. Windows must adopt the shared
  constructor and verify native-maximize crossing while preserving source and
  destination state. macOS/Windows compilation and native checks are pending.

## KDE follow-up (2026-11-13)

- Fixed a foreground dev-tooling teardown defect: background pipelines retained
  only their trailing formatter PID, so cleanup could leave upstream log
  followers alive and block before `dev-off`. Raw FIFOs now connect explicitly
  tracked follower/formatter processes; cleanup signals and waits for every
  stage. The hermetic single-PID SIGTERM regression blocks upstream followers
  and verifies teardown plus process absence. The tooling suite passes 486
  assertions. Independent review found no blocking issue; the owned-tray FIFO
  path is inspected, not covered by the new blocking-upstream regression.
- Fresh fixture-scoped detached startup passes the evaluated-readiness gate:
  current-owner journal records ready/discovered and matched accepted
  dispatch/applied rows after the pre-start cursor. All 22 subsequent read-only
  helpers retain fresh nonces, raw signed-ID replies, source hashes and
  pre-load collision checks; named-plugin and exact-object absence are verified
  after each unload and before either controller teardown. No helper failure,
  retry, collision or carry-over is observed in that run.
- An earlier follow-up startup evaluated successfully but remains qualified:
  its helper had an empty nonce, no retained raw load reply and a post-load
  check mislabeled as pre-load. Helper plugin/object absence before controller
  unload is retained. It also unnecessarily restored the current desktop and
  removed its appended desktop; the later run preserves normal lifecycle churn.
  Successful later accounting does not erase those earlier evidence gaps.

| Check | Result and evidence boundary |
| --- | --- |
| Fixed-size automatic float, cross-output send with stay | PASS by command route. A second fixture remains on the source and receives focus; the sent fixture changes output while retaining float classification and native size. |
| Native-maximized source, leftward cross | PASS by command route. Focus skips a same-output candidate and reaches the other output; the source's maximized frame is preserved. |
| Native-maximized source, left edge with no neighbor | PASS by command route. Focus and source frame stay unchanged. |
| Native-maximized source, Up and Down without vertical neighbors | PASS by command route. Focus and source frame stay unchanged in each direction. |
| Native-maximized source, rightward cross | PASS by command route. Focus crosses; both maximized source and destination overlay frames are preserved. |
| Physical shortcut delivery | NOT RUN. `invokeShortcut` bypasses xkb; command acceptance is correlated with production outcomes and native readback, not physical input. |

- The native script maximize getter is unavailable in these captures. Fixture
  mode readback establishes the initial maximized source; subsequent checks join
  native placement-area frames and production maximize/held facts. The later
  source has no separate fixture mode snapshot, a narrower evidence boundary
  than complete native-mode readback at every step.
- [Largest-fallback admission and native settings build](c9-product-evidence.md#native-build-repair-and-kde-follow-up-2026-11-13)
  are verified separately. Settings-page/Apply and pending-target option 2 are
  NOT RUN; [intent restart](retained-intent-desire.md#additional-kde-restart-evidence-2026-11-13)
  is partial. Earlier same-output send/follow and cross-output follow observations
  are retained with their original helper-lifecycle qualification, not repeated
  or upgraded by these legs.
- Logs were copied before each teardown. The final run omitted saved `dev-off`
  output and in-directory final status receipts; subsequent independent and
  direct read-only checks confirm dev DOWN and no fixture, tiler, log-follower
  or input-helper processes. No product defect is established by these legs.
  Evidence review accepts the table with the qualifications above; native
  acceptance is not a blanket PASS.

## Additional scoped follow-up (2026-11-13)

- No fixed-float send or maximize directional leg was repeated. The
  [capacity/native follow-up](c9-product-evidence.md#native-recovery-correction-and-scoped-follow-up-2026-11-13)
  records fresh evaluated readiness, bounded minimum sanity, inconclusive settings
  page inspection, and a new helper-accounting deviation. Its later teardown
  does not upgrade those lifecycle receipts or the earlier native observations.
- Pending-target option 2 is NOT RUN: staging an unavailable retained target and
  a newer report of another bound focused member was not reliable as a light
  direct leg. Manual acceptance: hold target N unavailable, retain its request,
  observe known C focused in a newer report, issue an implicit non-focus command,
  and verify C changes while N's exact request survives; contrast same-batch and
  unknown-foreground refusals, then recover N. Physical shortcut delivery,
  enabled hover and removed stored-key behavior remain untested.
- The [native maximize restart reproduction](retained-intent-desire.md#scoped-native-maximize-restart-reproduction-2026-11-13)
  is a startup-policy defect candidate, not evidence against the already observed
  command-route directional-overlay preservation.

## Historical wiring (citations at the inspected revision)

| Item | Evidence |
| --- | --- |
| Definitions/defaults | `crates/tiler-core/src/world_engine.rs:1656-1680`: `WorldEngineConfig`, send=false/fence=true. `new` delegates to it; constructor sets both Engine values (`:1832-1847`), reset retains config (`:2038`). Engine fields/defaults: `engine.rs:104-111,459-460`; Session: `session.rs:781-797,957-958` (both false before world configuration). Core-relative paths below start at `crates/tiler-core/src/`. |
| Propagation | `engine.rs:669-692,904-908` setters update retained/new sessions; `session/world.rs:825-849` setters/getters; pair/split copy both (`session.rs:1356-1357,1585-1588`). |
| KDE / macOS / Windows | KDE `crates/omnitiler/src/world_broker.rs:439-441` and macOS `crates/tiler-macos/src/world_runtime.rs:230-240` use defaults: false/true. Windows explicitly uses true/false (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:80-88`, bootstrap/reset `:212,632`). This is the existing Windows world route, not evidence of a completed v4 port or native parity. |

## 1. `fixed_send_admission`

- **Difference:** with a slotless automatically fixed-size float F on tiled WS1,
  explicitly send F to tiled WS2 (or another output). False refuses automatic
  origin; true permits the existing state/size-preserving deferred transfer.
  Both retain sticky, explicit-tile-override and other safety refusals.
- **All gates:** `session/world.rs:681-737` checks retained exception/home/base
  first; flag arms are held born-fullscreen (`:710-714`, observed `fixed_auto`),
  ordinary float (`:719-725`) and float with exactly one overlay (`:729-735`,
  observed or retained automatic origin). True removes only automatic-origin
  exclusions, never `fixed_suppress`. Caller `session/ops/workspace.rs:144-155,919-947`
  routes exceptions and preserves flags/size without tree mutation; Engine's
  focus-sync fallback also checks eligibility (`engine.rs:3324-3339,3372-3377`).
  v4 workspace/output sends reach those paths (`world_sends.rs:529-536,671-697,788-795,911-937`).
  Either endpoint in workspace-floating mode bypasses the switch through
  membership-only transfer (`:584-634,829-877`); this is not a universal send ban.
- **Why / classification:** introduced by `b6539be1` (2026-10-16) for Windows
  ordinary fixed/Settings sends, deliberately preserving KDE/macOS behavior
  ([original record](windows-all-member-workspace-sends.md):5-12,25-39).
  Constructor profile accepted in `075f4c32`, implementing `0869f08c`
  ([record](archive/windows-full-world-state-replies.md):37-83). Recorded reason
  is product rollout/isolation, not host inability. No recorded host limitation
  justifies false on KDE/macOS. The later [send decision](../decisions/workspaces.md#workspace-sends-and-floating-state)
  (`:68-73`) selects automatic sends everywhere; `docs/porting/world-contract.md:72-75`
  agrees. Classification: historical platform preference, now stale policy residue.
  Windows same-output fixed-Settings carry was reported in the original record
  `:96-113`; it does not prove all hosts, cross-output, or transient-owner behavior.

## 2. `maximized_focus_fence`

- **Literal difference:** maximize tiled B, with a left sibling; directional
  focus left. Session true refuses `Unchanged`, false allows normal planning.
  At an edge with an occupied adjacent output, true also refuses crossing,
  false permits the cross plan subject to ordinary gates. Fullscreen exempt.
  These are Session discriminators, **not two different v4 native outcomes**.
- **All gates / reachability:** `session/ops/focus.rs:94-108` gates local tiled
  focus, `:490-501` cross-output focus from tiled or float origins. But v4
  `world_directional.rs:154-164` unconditionally refuses native maximized focus,
  and `world_operations.rs:535-547` returns before either Session path. Thus
  changing the constructor flag alone changes neither native v4 outcome.
  Composed maximize is separate: floats refuse (`world_operations.rs:561-572`),
  tiled subjects skip local siblings and may cross (`:620-703`), independent of
  the flag. This includes macOS, whose raw maximize is null and capability false
  (`crates/tiler-macos/src/runtime_core.rs:326-352`); null does not mean maximize
  policy is absent. `window_policy.rs:357-359` is an independent pure predicate,
  not this switch; its core calls at `:603,1175` are inside tests (`:491`).
- **Why / classification:** `1c26f2d9` (2026-10-10) implemented G-06's product
  focus fence, keeping Windows unchanged pending adoption, with KDE flags then
  adapter-local ([original record](archive/reference-comparison-implementation.md):42-45,78-79;
  `1c26f2d9:docs/decisions.md:1495-1511`). October 18 constructor rationale was
  preservation of adapter-owned gates, not an inability to focus other windows
  (constructor record `:54-79`). No host incapability is recorded. Classification:
  historical policy/adoption switch plus accidental stale/duplicate fencing.
  **November 1 explicitly rejected a universal no-op** and selected skip-local,
  cross-if-adjacent, else stay (`docs/decisions/window-state.md:29-35`,
  `docs/spec/functional-spec.md:154,325`). Native v4 contradicts that selection;
  composed tiled behavior already has its shape. C6's "both live" is true for
  Session reads, not for an observable native v4 flag difference; its untraced
  world short-circuit is now established. Neither forcing true nor merely
  deleting the flag repairs the native/composed discrepancy.

## Reference evidence and separate choices (no recommendation)

- [Minimal sequences and stable-tag citations](../spec/reference-outcomes.md#core-platform-switch-discriminators)
  extend R-WS-05/R-MAX-08/R-OUT-11 rather than substituting fullscreen for maximize.

| Tier 1 source | Fixed-float send to tiled target | Maximized focus: single output / adjacent output |
| --- | --- | --- |
| i3 4.25.1 | Allowed, float/size retained | Unsupported maximize fixture / unsupported |
| sway 1.12 | Allowed, float/size retained | Unsupported maximize fixture / unsupported |
| Hyprland v0.56.2 | Allowed, float/size retained, monitor-relative position | Stays at defaults / final target/state TBD |
| COSMIC epoch-1.10.0 | Allowed, fresh tiled admission rather than float retention | Stays for default-layout Left / final target/state TBD |

| Decision option | User consequences and deletions | Wire/fixtures and macOS adoption |
| --- | --- | --- |
| Send: shared true | KDE/macOS gain tiled-scope automatic-float sends; Windows retains them. Collapse three arms to `!fixed_suppress`; delete send field, accessors, propagation and profile override. No evidenced host prohibition; native carry/owner coupling still unproven. Matches the existing send selection, unlike shared false. | No new wire field; remove profile-specific expectations, cover shared send outcomes. macOS must deliver retained membership/placement without inventing intent or resize. |
| Send: shared false | KDE/macOS unchanged; Windows loses approved fixed-float/Settings sends at tiled boundaries. Delete same plumbing, retain automatic-origin refusal arms. No evidence any host requires this restriction; revises the existing send selection. | No wire addition; shared refusal coverage replaces enabled cases. macOS gains no send behavior. Floating-workspace bypass still requires explicit treatment if a universal refusal is intended. |
| Send: neutral capability | Candidate `independent_window_transfer`: host can reassign this member's workspace/output without resizing, changing float/overlay state or forcibly moving an owner group. Not an automatic-origin preference. Delete platform profile; gate shared send policy on a declared host fact. A per-window limitation cannot truthfully be encoded as a blanket platform false. | New typed capability (possibly per-window), codec/contract/fixture and producer work. Windows has bounded same-output positive evidence; KDE/macOS truth values, cross-output and owner-dependent exceptions are TBD, **not** false inferred from current defaults. |
| Focus: shared true or false literally | True retains universal native v4 refusal; false removes Session fences but world refusal survives. Delete focus config/storage/propagation either way; neither alone changes native v4 users. Removing the world fence as well under false enables ordinary sibling traversal, also contrary to the selected skip-local policy. No evidenced host prohibition for either action. | No wire addition; revise internal profile tests and add world-level native/composed, same/cross-output discriminators. macOS composed paths are unaffected unless deliberately unified. Both literal choices need the November 1 selection reconciled. |
| Focus: delete switch, implement already-selected shared policy | Replace terminal native fence and Session cross refusal with skip-local/cross-or-stay under existing gates, consistent with composed tiled route. KDE/Windows native-maximize origins gain adjacent-output escape; local focus still stays. Composed floats' refusal must be reconciled too, not silently retained as parity. Deletes focus profile/plumbing and duplicate policy rather than adding a host distinction. | No new wire field; shared fixtures must distinguish local stay, cross, destination overlay preservation, native/composed and float bases. macOS keeps null native maximize and uses existing composed ownership; native delivery remains unverified. |
| Focus: neutral capability | Candidate `overlay_preserving_focus_transfer`: host can focus an eligible destination while preserving source/destination maximize overlays. False means a demonstrated host restriction, never "prefer a fence". Delete profile; shared routing owns policy, capability narrows only impossible transfers. None of KDE/Windows/macOS has an evidenced false value; reports remain TBD until grounded. | Handshake/codec/contract/positive-negative fixture and producer work, and duplicate fences still need reconciliation. Existing `maximize` distinguishes native vs composed, not permission to leave an overlay (`connection.rs:91-124`). macOS must report its actual focus ability independently of native maximize support. |

- Wire evidence: neither switch is in closed handshake keys
  (`crates/tiler-protocol/src/connection_protocol.rs:211-267`); a capability adds
  coordinated schema and conformance work, unlike deletion. Existing fixture:
  `test-fixtures/world-wire/v4-active/messages/handshake-emulated.json:1`.
  Removing both switches also removes the otherwise empty `WorldEngineConfig`
  and its constructor/reset profile plumbing; no automatic-origin facts vanish.
- Historical verification: local pinned-source/history inspection and whitespace
  review only. The options above are research alternatives, not pending product
  choices: the approved shared removals and implementation are recorded above.
  Unsupported references and native carry/focus remain TBD.

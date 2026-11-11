# V4 phase 3: macOS port

## Purpose, status and gates

- Entry-point plan for porting `tiler-macos` to the approved v4 boundary, in
  usable increments modelled on KDE's [dogfoodable slices](archive/v4-phase2-kde-port.md#later-dogfoodable-slices).
  The linked contracts, decisions and native records own the detail; this plan
  neither replaces them nor reopens approved behavior.
- KDE gate evidence from the upstream acceptance record: under the [migration decision](../decisions/contract.md#contract-authority-and-v4-migration),
  KDE phase 2 is complete offline, including remaining v3 removal, and the
  [user-run physical ST acceptance passed 2026-11-10](v4-phase2-kde-state-targets.md#2026-11-10-physical-state-target-acceptance)
  apart from desktop/stacking and compositor-boundary (KWin restart) recovery.
  The user deferred those sub-legs until after macOS start; they remain open KDE
  acceptance, not start gates. ST is recorded in the
  [single acceptance checklist](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs).
  The KDE reconnect (RC) leg-2 fix and merge decision
  must land before this port reaches its reconnect/rehome slice and before 0.1. Other KDE acceptance remains separate, not implicitly
  cleared by opening this port.
- **Started under the amended gate (2026-11-10); slice 1 implementation in progress.**
  Under the [migration decision](../decisions/contract.md#contract-authority-and-v4-migration),
  KDE phase 2 offline completion including v3 removal is done. macOS may start
  while KDE ST/RC work continues. Neither slice 5 (state targets) nor slice 7
  (physical reconnect/rehome) acceptance is claimed until both user-run KDE
  ST/RC checks pass and resulting core changes are absorbed; see the
  [single acceptance checklist](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs).
  The RC leg-2 fix and merge decision remain pending before acceptance and 0.1.
  Other KDE acceptance is separate, not implicitly cleared by opening this port.
- The [pre-port review outcomes](#pre-port-review-outcomes-2026-11-10) are decided;
  their implementation/native proof remains below. Shared
  [host-settled geometry design](../decisions/layout-and-admission.md#host-settled-geometry-design-timing)
  is deferred until after slice 1 and brought for user approval before slice 3;
  approval remains required before any core change.
- Port/build/live work runs on a macOS host through the [development environment](../macos-dev-environment.md)
  and [live-testing guide](../live-macos-testing.md). Use the signed
  [fixture bundle and exact scope](../live-macos-testing.md#fixture-first-live-tests),
  not direct executable launch. Physical/input/visual acceptance is not inferred
  from dispatch receipts or offline gates.
- KDE and macOS use report scale 1. The shared per-output resize-minimum,
  keyboard-step and group-edge scaling prerequisite gates Windows, not macOS;
  [port ordering and any later overlap](../decisions/contract.md#contract-authority-and-v4-migration)
  remain unchanged. Native/release scope stays in the [macOS 0.1 gate](../decisions/macos.md#macos-0-1-scope-and-gate)
  and [remaining port acceptance](macos-world-v2-port.md#original-backlog-lines-1767-1959).

## Pre-port review outcomes (2026-11-10)

- Records-only outcome: the approved choices below amend existing policy;
  this review changes no code or v4 contract and claims no native acceptance.
- Records verification: `git diff --check` passed; changed decision links and
  heading anchors were checked. No build, mocked suite or native leg was rerun.
- Evidence boundary from the read-only pre-port review (reported execution,
  not rerun for this records change):
  `mise exec -- cargo build --locked --offline -p tiler-core -p tiler-protocol -p tiler-macos`
  failed with 80 errors from retired v3 APIs. `scripts/macos-dev.test.sh`
  reported `PASS=104 FAIL=0`, mocked only. Native evidence was unrun because
  the current `tiler-macos` HEAD did not build. Source-read facts below are not
  native proof; the historical signed-fixture result does not establish v4 acceptance.

| Approved topic | Pending port implementation / native evidence |
| --- | --- |
| [Fullscreen games](../decisions/macos.md#fullscreen-games) | Slice 5: remove blanket pre-core fullscreen-send policy (`runtime_thread.rs:2242-2267`); core decides membership/policy and adapter reports truthful native refusal. Keep known-exitability-only explicit exit and no autonomous exit. Exact switch/send mechanics remain TBD in the [discriminator](../spec/reference-outcomes.md#v4-macos-platform-fit-discriminators). |
| [Fixture/scoped input](../decisions/macos.md#fixture-and-scoped-input-defaults) | Slice 1: JSON/CLI raw acquisition feeds core revisioned validation/defaulting/per-field diagnostics. Remove missing-bundle-to-own (`native_facts.rs:301-321`) and bundle-wide product self-exclusion, including unscoped probe/helper exclusions (`dev_selection.rs:75-100`); retain scoped test-safety fences. Keep event-first AX recovery plus provisional tunable deduplicated 1 s reread. Native chord/conflict, minimize/restore notification coverage, stale/unwatched suppression and M2 continuity remain evidence, not M7 paint proof. |
| [Development preview](../decisions/development.md#tentative-macos-development-preview) | Preserve signed LaunchServices foreground dev, ordinary apps on each display's current user Space, bounded exact-run teardown with clean owner proof, and scoped finite runs. After slice 1 builds, prove v4 native rebuild/teardown/readiness, permission survival across rebuild and GUI-session recovery. M6 still requires core `exist:false` for native-Space removal. Signed fixture tiling passed 2026-10-20 (`crates/tiler-live-fixture/README.md:228-267`); a chord/workspace harness with explicit fixture ownership remains missing. Retire legacy helper exceptions only after replacement proof. |
| [Explicit Apply only](../decisions/reference-and-shortcuts.md#macos-preview-presets-and-native-shortcut-ownership) | Remove Authentic startup Apply (`tile.rs:2091-2127`) so `SESSION_PERSISTENCE_NOTE` (`desktop_shortcuts.rs:58-61`) is true. After slice 1 builds, verify native per-drain refusal (`workspace_send_managed.rs:753-838`, `desktop_shortcuts.rs:438-458`; test `workspace_send_managed_tests.rs:985-1034` present, unrun) and first-run setup cost. Other preset/ownership choices remain tentative. |

## Contract map

- Authority: [v4 world contract](../porting/world-contract.md), reconciled by the
  [approved M1-M9 resolutions](v4-migration.md#m1-m9-technical-resolution-pending-0c-approval)
  and later amendments. The [native experiments](macos-v4-platform-fit.md#outcome-and-limits)
  are bounded evidence, not implementation or native acceptance. The
  [port notes](../porting/macos-port-notes.md) retain historical v2 inventory;
  their v4 normalization target and native-mechanics inventory are useful, not
  permission to retain retired shapes. Older [macOS decisions](../decisions/macos.md)
  must be read under the newer v4 authority, especially own-process exclusion,
  creation, removal and maximize.

| V4 boundary | macOS obligation and linked constraint |
| --- | --- |
| [Ownership](../porting/world-contract.md#ownership-and-g4-exceptions), [wire/conformance](../porting/world-contract.md#conformance) | Core owns policy, classification and bindings; native AX/CG/Space mechanics stay local. One exact v4 route, not a legacy converter. |
| [Handshake/lifetime](../porting/world-contract.md#handshake-and-lifetime), [settings](../porting/world-contract.md#revisioned-settings) | Adapter lifetime, core-minted generation, readiness and replay fences; revisioned raw settings with per-field diagnostics. M1: only known own-process identity excludes internal UI, never same bundle/executable in another process. |
| [Screens](../porting/world-contract.md#screenreport), [scale](../porting/world-contract.md#scale-and-rounding) | FULL/work rectangles and truthful completeness; CG/AX point-space scale 1, not backing scale. [Native normalization](../porting/macos-port-notes.md#v4-native-coordinate-normalization) remains a port target. M9: null unknown metadata/connector; no display-ID/UUID/ordinal substitute; ambiguous reconnect holds. |
| [Window facts](../porting/world-contract.md#windowfact), [classification](../porting/world-contract.md#neutral-classification-vocabulary-and-applicability), [native observations](../porting/world-contract.md#native-observation) | Neutral facts, fixed unsupported-fact declarations and explicit unknowns, not adapter membership/policy echoes. M2: retained AX identity plus process fences; positive close retires/remints, omission/off-Space absence does not. |
| [Commands/results](../porting/world-contract.md#commands-and-results), [desired world](../porting/world-contract.md#desired-world-and-intent) | Port existing native-Space [selection/send mechanisms](../decisions/macos.md#managed-native-spaces-input-and-transfers) under core authorization; no adapter-local replan or policy fallback. Preserve FIFO input and latest-only native publication. |
| [Attempts/settlement](../porting/world-contract.md#raw-attempts-and-settlement), [visibility/focus](../porting/world-contract.md#visibility-and-focus-ordering) | Native before/after evidence and original spent stamps; setter success is not arrival. M5: without causal creation proof keep `created:null`, hold and never retry. M6/G2: only core `exist:false` authorizes removal, subject to fresh occupancy/pin/minimum/completeness vetoes. |
| [State policy](../porting/world-contract.md#window-state-policy), [stacking](../porting/world-contract.md#persistent-stacking) | M4: `maximize=false`, native `maximized=null`, core-composed work-area overlay; Zoom is ordinary drift, not product maximize. Follow the [composed amendment](v4-migration.md#core-composed-maximize-amendment). Fullscreen exitability and provisional SIP-on sticky/above limits remain truthful; [source findings](macos-v4-platform-fit.md#m-numbered-findings) do not prove universal impossibility. |
| [Intent storage](../porting/world-contract.md#opaque-intent-storage), [recovery](../porting/world-contract.md#freshness-and-recovery) | M3: atomic opaque storage before dependent writes, exact-lifetime verification and intentional-float retention across orderly within-session restart. No title/ID guesses; storage survival remains open. Composed maximize restarts to base without an M3 extension. |
| [Cross-output capability](../porting/world-contract.md#handshake-and-lifetime) | M8: `tiledFloatTransfer=false` provisionally means float-no-remap, not a general move ban. Prove target-display frame plus native Space membership with W2 arrival; until proved, fail with native observation, no guessed remap. |
| [Presentation/renderer](../porting/world-contract.md#presentation-and-renderer-input) | K5/M7: prove local capture/event/renderer causal joins and frame-event re-resolution; suppress unwatched/stale eligibility. Existing unsigned diagnostics are not signed-route acceptance; no new FrameWitness field is evidenced. |

- Carry the [eight OPEN scenario gates](v4-migration.md#phase-0c-approval-outcome-2026-11-02)
  and [per-platform port gates](../backlog.md#open-work), including K5/M7,
  storage survival and exact-lifetime recovery; approval is not closure.
- Track the existing P1 M3, M4, M8 and post-v4 session-lifecycle work, plus the
  **P2**, not P1, [responsiveness comparison](macos-v4-platform-fit.md#responsiveness-evidence-floor-and-design-overhead).
  Lifecycle checks cover lock/unlock, sleep/wake and logout/login with normal
  relaunch; the [observed development-service context loss](macos-v4-platform-fit.md#gui-session-lifecycle-risk-and-post-port-follow-up)
  is not a demonstrated tiler defect.

## First-slice source baseline

- The original plan's source-only baseline is now supplemented by the reported
  failed build above. No native execution or passing v4 build is established;
  historical v2 build results do not establish current v4 buildability.
- Production [world_builder.rs](../../crates/tiler-macos/src/world_builder.rs)
  still imports/constructs the retired `WorldReport` in `build_report`, carrying
  `owner`, `views`, host capabilities and policy/settings fields. Current
  [WorldReportV4/ScreenReport](../../crates/tiler-core/src/world_report.rs)
  instead separate handshake/settings and use v4 facts, attempts and presentation.
  No v4 handshake/settings/report producer was found in the macOS source.
- Production [runtime_core.rs](../../crates/tiler-macos/src/runtime_core.rs)
  `collect_window_facts`/`minimized_fact` still construct removed
  `floating/fit_excluded/fixed_auto/fixed_suppress/observed_workspace` fields,
  bool state facts and old native-membership shapes instead of current nullable
  neutral `WindowFact` fields. These are source-confirmed API mismatches, not
  a complete per-error compiler inventory; the reported build independently fails.
- Test-only legacy constructors also remain in
  [pager.rs](../../crates/tiler-macos/src/pager.rs),
  [world_dispatch.rs](../../crates/tiler-macos/src/world_dispatch.rs) and
  [world_command_map.rs](../../crates/tiler-macos/src/world_command_map.rs),
  including old screen and presentation shapes. First restore a genuine v4
  production loop and its necessary tests; do not mask breakage with aliases or
  a compatibility report. Retained shared enum/codec arms alone prove no macOS
  v4 producer support.

## Proposed dogfoodable slices

- Each row retains the prior report/reply foundation and leaves a build usable
  at its explicitly proven level. Connection alone is preparatory; slice 1 ends
  at ordinary fixture tiling. Later functionality is not claimed early.

| Order | Increment and acceptance evidence |
| --- | --- |
| 1. Handshake and report/reply | Replace the baseline above with genuine handshake, revisioned settings, v4 capture/decode/apply and necessary attempt/storage plumbing. Prove real producer bytes through real core/protocol, readiness, narrow holds, generation/restart fences and ordinary signed-fixture admission/reflow; no repeated handshake on a bound wait. |
| 2. Selection | Port native selector and view-attempt correlation; prove focusless terminal selection and continued ordinary tiling. Explicitly bound/document populated-selection limitations; no fabricated focus acknowledgement. |
| 3. Arrival | Port W2 native before/after membership and settlement. Prove fixture transfer to the intended native home, refusal/unknown holds and arrival before follow/departure; setter acceptance alone never passes. |
| 4. Focus feedback | Port causal retained-request focus evidence and anchored-descriptor consumption under the approved unified design. Prove authorized focus/refusal without replay, exact lifetime joins and K5/M7 native event/render evidence, including stale/unwatched suppression. |
| 5. State targets | Port fullscreen/sticky/stacking and composed-maximize frame application, attempts and recovery. Prove base restore, external-clear/fullscreen replacement, send/work-area changes and restart-to-base; retain durable intent/preimages where required. M4 implementation dependencies belong in shared core, not adapter-owned policy. Acceptance waits for both KDE ST/RC passes and absorption of resulting core changes. |
| 6. Scaled metrics at scale 1 | Apply logical metrics once, separate native fractional-coordinate normalization from scaling, and recheck tiling/rendering on backing-scale changes while report scale stays 1. Do not absorb the Windows non-unit-scale prerequisite. |
| 7. Physical reconnect/rehome | Supply M9 physical identity facts and prove user-run disconnect/return, ambiguous identity holds, FULL/work geometry and rehome behavior without regressing prior slices. Rehomed members are not new admissions. M8 crossing and two-display acceptance remain explicit gates. Acceptance waits for both KDE ST/RC passes and absorption of resulting core changes, including the RC fix/merge decision. |
| 8. Scenario runner | Add a fixture-first ordered native runner with exact owned-state cleanup, plus offline genuine-producer journeys using real native shapes. Cover earlier slices and FIFO bursts; transport/readiness receipts never substitute for native outcomes. Physical keys, paint, hotplug and crash recovery remain separately named evidence. |
| 9. Remaining v2/v3 removal | Map each supported legacy assertion to genuine v4 coverage; port uncovered behavior first. Cull obsolete implementations separately with no-live-path evidence and review. Retain immutable recorded fixtures, active migration records and both shared stage5 proofs. Cull proved-obsolete touched code earlier; no dedicated broad sweep. |

- For each slice, run the relevant [macOS allowlisted build/test/fmt/clippy gates](../macos-dev-environment.md#4-prove-the-portable-offline-cargo-baseline-allowlist-only)
  and affected producer regressions. Native legs require fresh identity joins,
  independent AX/CG/Space readback and exact owned-resource cleanup under the
  [live guide](../live-macos-testing.md). Record code presence, offline execution,
  native readback, physical/visual proof and unrun conditions separately.
- State the actual usable surface after each native increment. Development
  dogfoodability is not conformance across all adapters or release acceptance;
  unsupported behavioral outcomes stay TBD in the existing
  [minimal discriminators](../spec/reference-outcomes.md#v4-macos-platform-fit-discriminators).

## Slice 1 implementation and evidence

- Scope: replace the retired production boundary with real v4 handshake,
  revisioned raw settings, neutral reports, decoded replies and fenced apply.
  Ordinary signed-fixture admission/reflow is the native acceptance floor;
  later slice behavior and M3 restart survival are not inferred from it.
- Separate signed-fixture recovery leg: retain identified fixture windows on
  an off-current or unreadable native Space through the held interval, with no
  inferred close or writes while held. Switch to that owned Space, obtain a
  fresh complete capture, and prove those same lifetimes are picked up and
  tiled normally using independent AX/CG frame readback. Report this outcome
  separately from first admission and close/reflow; callback or switch receipts
  alone do not pass it.
- Implementation units: restore the macOS-owned production boundary and its
  affected tests; remove approved self-exclusion policy; separately remove
  automatic startup shortcut Apply; exercise genuine producer/core/protocol
  journeys; then run allowlisted gates and signed-fixture native acceptance.
- Storage ordering: incorporate opaque intent writes before dependent native
  writes, returning only exact-live-lifetime records. A nondurable or failed
  store cannot authorize dependent writes. The shared contract already supplies
  these lanes; ordinary observation-only admission/reflow emits no intent save
  (`world_publish.rs::drain_intent_saves`). M3 storage survival and re-identification
  remain separate acceptance work.
- The macOS-owned boundary now contains handshake/settings and report/reply
  codec dispatch; this is code presence, not native acceptance.
- Current offline follow-up (2026-11-11; test execution reported, source and
  retained logs reviewed): the three superseded lifecycle expectations are
  repaired without production changes. Full macOS library: raw exit 0,
  **1519 passed / 0 failed**. Scoped Native integration fakes now provide
  explicit neutral facts through the real macOS capability handshake, not
  all-supported capabilities or production raw-fact shortcuts. Original
  behavior assertions remain: `tile_entry` 8/0 and `runtime_freshness` 9/0
  (both raw exit 0); `runtime_phase1` 34/5 (raw exit 101). A termination-focus
  case also varied between earlier runs; stability is not established.
  The last full integration baseline was 89/61 (raw exit 101), before these
  repairs; no combined post-repair result is claimed. Classification (18
  baseline failures), own-bundle gate (1), and scoped topology/selection (2)
  remain untriaged. Fresh core library and combined strict clippy remain unrun.
- Protocol follow-up: the first full run passed 228 tests before a launch
  stall at `target_lanes`; a second passed 109 before `intent_wire`. Neither
  produced a final raw exit code (tool timeouts, not suite passes).
  `target_lanes --list`, each of its two tests individually, and both together
  subsequently passed with raw exit 0; both bodies took 0.00 seconds.
  `intent_wire --list` reproduced the stall after a 0.15-second completed
  build, before harness output. An active-process sample stayed entirely in
  `_dyld_start`: this is a pre-main launch/runtime stall, not an identified
  hanging test body or slow compilation. Why dyld stalls remains unknown.
  Full protocol verification remains incomplete.
- Offline checkpoint (2026-11-10): allowlisted locked/offline build and macOS
  all-target test compilation pass. A fresh full macOS library run passed all
  1474 tests. After absorbing the upstream sticky-placement fix, core library
  passed 2085 tests and the build, compilation and focused producer/restart
  regressions passed again. Modified Rust sources pass edition-2024 formatting.
  Integration fixtures remain under repair: 65 passed / 85 failed initially;
  explicit neutral facts in scoped captures improve this to 83 passed / 67
  failed. Missing synthetic raw facts and input codec routing remain to audit.
  The mocked development shell suite passes 104/0. No native acceptance yet.
- Directional routing correction: production focus/move mapping now emits v4
  implicit directional commands instead of codec-invalid legacy operations.
  Only the command-correlated focus effect dispatches; no admission focus is
  authorized by this change. Producer-map byte round trips pass for both
  commands. Real-engine focus controls, exhausted-edge typed refusal and
  consumed-sequence replay pass. The integration focus walk retains the edge
  and recovers with unchanged geometry/workspace desires. Its Accepted envelope
  counter does not imply command success; the command refusal is separately
  pinned. Identical desired frames may be rewritten on reconciliation, so this
  proves no frame change, not zero native setter calls. All four focus/gap
  integrations and the two affected scoped focus/move tests pass. The library
  passed 1475 tests before adding the final edge proof. After absorbing the
  dynamic-workspace lifecycle change, the fresh full library result is 1473
  passed / 3 failed; those three tests assert superseded spare-backing/native
  minimum expectations. Core library passes 2100 tests, allowlisted build and
  macOS all-target compilation pass. Fresh integration result: 86 passed / 64
  failed. Most failures still use rawless synthetic fixtures; three genuine
  close/focus-loss failures are isolated below.
- Raw settings checkpoint: focused acquisition/connection tests pass for CLI
  precedence, explicit null versus omission, invalid-field diagnostics with
  valid siblings retained, and revision/generation refusals. Core-effective
  values supply the existing presentation carrier; no hot reload is claimed.
- Explicit Apply: the automatic Authentic startup Desktop-N holder mutation
  was removed. The focused startup no-mutation regression passed; explicit
  Apply CLI and per-drain refusal preservation also passed focused checks.
  Native first-run cost and holder refusal readback remain unrun.
- Native readback, physical/visual proof and signed-fixture admission/reflow
  remain unrun. The earlier close/focus-loss blocker is superseded by the
  linked implementation below; the admission/explicit-focus mismatch remains
  open. Source checks are not native acceptance. Slice 1 is
  incomplete; no usable native surface or conformance acceptance is claimed.
- Neutral-fact follow-up: hidden-app and minimized rows now preserve available
  raw facts separately (`minimized` versus `host_hidden`), including process
  identity. Focused regressions passed during implementation. Test captures
  now supply explicit ordinary native facts and production macOS capabilities;
  this restored genuine admission and hide/restore geometry assertions instead
  of accidentally exercising all-null classification holds.
- Unpersisted-save safety: a session-lifetime latch now prevents a later reply
  without writes from bypassing an earlier unpersisted intent write. Two focused
  tests passed during implementation. This is fail-closed plumbing, not a
  durable store or M3 restart acceptance. A production-dispatch regression now
  proves the initial save refusal and a later observation-only reply remain
  no-write, while a save-free control applies.
- **Shared focus correction implemented:** direct focus now refuses the staged
  minimized/host-hidden target before retained membership can authorize it;
  directional focus also refuses a hidden source. Existing refusal vocabulary,
  observations, held layout and separate minimized restoration are preserved.
  The genuine macOS producer regression passes both complete and incomplete
  inventory contrasts:
  `mise exec -- cargo test --locked --offline -p tiler-macos --lib world_builder::tests::first_transition_same_report_focus_never_restores_hidden`
  Independent source review accepted the narrow guard; focused core tests pass.
   Core/protocol build and behavioral targets pass. The workspace unsafe
   allowlist test fails on three unchanged fixture attributes; protocol strict
    clippy fails on two unchanged single-element loops. After upstream changes,
    core strict clippy also reports eight `unnecessary_get_then_check` findings
    in frame-policy tests. macOS strict clippy is blocked by those dependencies.
    Attribution to the base is source comparison only. Full gate
   closure remains pending. See the
  [original blocker](macos-core-trailing-pager.md#core-blocker-same-report-minimized-target-during-an-inventory-gap).
- M5: an observed appended native Space is no longer claimed as causal creation
  identity. The staged attempt preserves native orders and setter evidence with
  `created:null`; its focused stage/codec regression and the repaired native
  attempt lifetime suite pass. No causal native creation proof is claimed.
- Genuine producer journey: native-shaped captures admit 1, then 2, then 3
  ordinary windows through production dispatch/codecs/core and a mocked frame
  sink. An incomplete inventory retains all lifetimes without native writes;
  complete recovery and positive-absence close/reflow pass. A rebuilt-report
  codec check is supplemental, not an internal transaction transcript.
- Restart fences: sessions obtain a fresh OS-random nonce for incarnation and
  generation seed; entropy failure remains unbound. Focused tests pass old
  traffic rejection, reused-PID-shaped isolation and entropy failure. These
  fences do not establish M3 durable intent recovery.
- Reply decoding uses actual macOS capabilities before sequencing/apply.
  Dead pre-v4 report/session policy plumbing is removed; normalization with
  remaining test readers is test-only. No compatibility report is retained.
- Latest verification limits: the integration suite remains red; a fresh full
  gate closure remains pending. Focused passes do not establish a usable native
  surface.

### Blocker: suppressed admission focus changes the next implicit subject

- Historical minimal offline case: observe A focused; admit ordinary B to its right
  while observing A still focused; suppress automatic native admission focus;
  issue explicit focus-right. `admission_never_steals_focus_but_explicit_focus_dispatches`
  reproduces no queued focus request instead of one request for B.
- Source/log agreement: `world_publish.rs::assemble_publication_rows` calls
  `world_sends.rs::record_published_focus`, which records admission effects in
  the canonical implicit-focus slot. macOS
  `world_dispatch.rs::dispatch_focus_effects` deliberately spends but does not
  actuate admission effects. The next command log names B (`1001_12`) as subject
  while observed focus remains A (`1001_11`), with `internal=unchanged` and
  `wire=operation-refused`. `focus-accepted` only counts the accepted envelope;
  it does not prove a successful command or a ticket/effect correlation bug.
- Resolution approved 2026-11-11: macOS shall actuate published admission focus
  through [one retained focus request for every cause](retained-focus-requests.md),
  replacing admission suppression without changing candidate selection. Core's
  execution-time logical cursor is separate from observed foreground; adapters
  need only the latest publication, with no current-command ticket requirement.
  Q1-Q3 fix any-batch non-focus preservation, same-report cause precedence and
  fresh-hover identity. Implementation and per-cause skipped-publication proofs
  remain pending; the red reproduction above is not a new result for that target.
  Preserve FIFO implicit subjects, selection dependencies and native safety.
  No assertion weakening is authorized. The other four phase-1 focus failures
  are not yet causally attributed to this case; native acceptance remains open.
- Minimal discriminator and unsupported outcomes are retained in the
  [platform-fit table](../spec/reference-outcomes.md#v4-macos-platform-fit-discriminators).

### Superseded blocker: positive close with no observed focus

- Historical minimal case: admit A/B in separate app lifetimes, focus A, positively close
  A while its app remains alive, and report complete AX/CG absence plus B's
  ordinary facts with no new observed focus. Three scoped close/reflow tests
  failed: survivor desires had no frames and the occupied scope held Unreadable.
  The covering close journey was a distinct passing case.
- Independent offline review instrumented an isolated source copy and reported
  the exact complete close report and shared sweep refusal: `focus-mismatch`,
  followed by `Hold(Unreadable)`, zero frames. No repository source was changed
  for that probe. Source inspection at that checkpoint confirmed the focusless
  Reconcile escape in `tiler-core/src/engine.rs` excluded reports with staged
  closes; `world_publish.rs::classify_sweep_hold` mapped the refusal to Unreadable.
- Resolution: the [approved core-owned positive-close implementation](core-owned-close-refocus.md)
  now reflows readable survivors without fabricated observed focus and retains
  eligible close-refocus requests in the latest publication. All four scoped
  close regressions pass in that record. The reproduction above is historical,
  not a current unresolved design choice. Native close/reflow and held-Space
  recovery acceptance remain unestablished; use the linked user-run procedure
  for close/refocus, not the superseded automated preparation.
- Other reviewed boundaries, source-only: ambiguous creation retains
  `created:null`; native retirement is guarded by core `exist:false` and fresh
  occupancy proof (currently unknown occupancy prevents removal); unsupported
  capabilities and unpersisted intent writes remain fail-closed. These checks
  do not close native M3/M5/M6 or FIFO/render acceptance.

### Draft user-run off-current Space recovery check

- Assessment only; unrun and not full held/unreadable acceptance. Resume after
  the open offline focus blocker and verification gates are resolved. Use the
  [signed-bundle preflight, bounded launch and stop route](core-owned-close-refocus.md#user-run-macos-native-acceptance)
  with a fresh log directory, replacing TextEdit scope with
  `io.github.omnitiler.live-fixture`. Do not overlap product instances.
1. Follow the [fixture bundle setup and ownership joins](../../crates/tiler-live-fixture/README.md#macos-bundle-launch-and-scope);
   create two disposable ordinary fixture windows on an existing user Space.
   Do not create/remove Spaces or move unrelated windows for this check.
2. Start the fixture-only bounded product run. After admission/reflow settles,
   retain fixture `get`/`list`, run ownership, and signed-route
   `scope-inspect --scope io.github.omnitiler.live-fixture` output: native window
   ids, Space membership, AX frames and CG bounds. Keep the same fixture process
   and both windows alive throughout.
3. Physically switch to another existing user Space, leave the fixture windows
   untouched, and switch back before the bounded run expires. If the fixture
   remains visible on the other Space, the off-current leg was not established.
4. Obtain fresh complete scoped readback. Expect the same owned native windows
   to remain and tile normally; compare independent AX/CG frames and report
   disappearance, held/non-recovery state, jumps or jank rather than retrying.
5. Let the bounded product run stop; require `shutdown_clean:true` and released
   interception/border. Shut down only the verified fixture instance and verify
   process/window/socket absence using its documented cleanup route.
- Evidence limit: native ids/frames plus visual recovery do not prove retained
  core lifetime identity or zero setters during the away interval. This does
  not induce arbitrary AX unreadability. Full acceptance still needs joined
  core-lifetime/publication and no-write evidence; do not infer it from physical
  switching, callbacks or `scope-inspect`'s launch-date presence bit.

## Transferable KDE lessons

- Fail closed: unknown never matches; setter acceptance is not native settlement ([runner oracles](v4-phase2-kde-scenario-runner.md#selected-design)).
- Fixture-first runners own setup and exact owned-state cleanup, not the whole desktop ([runner lifecycle](v4-phase2-kde-scenario-runner.md#outcome-and-accepted-evidence), [fixture rules](../live-kwin-testing.md#fixture-first-live-tests)).
- Check native shortcut holders and reverse key collisions before injected chords ([selection evidence](v4-phase2-kde-scenario-runner.md#selection-scenario)).
- Batch FIFO commands after one capture and apply only the latest publication; resolve implicit focus at execution ([rapid-input decision](../decisions/commands.md#kde-rapid-input)).
- Rehomes are not admissions and must not spend admission focus effects ([reconnect correction](v4-phase2-kde-physical-reconnect.md#disconnect-rehomed-members-are-not-admissions)).
- Offline journeys must use the genuine producer/core route and fakes seeded from real shapes ([producer journeys](v4-phase2-kde-scenario-runner.md#offline-genuine-producer-journeys), [captured-shape evidence](v4-phase2-kde-scenario-runner.md#outcome-and-accepted-evidence)).
- Green offline suites do not close native acceptance ([evidence separation](kde-v4-acceptance-checklist.md#scope-counts-and-result-rules)).
- Never use pointer-click activation as a runner shortcut ([selection runner gate](v4-phase2-kde-scenario-runner.md#selection-scenario)).
- Never repeat a possibly executed mutating transfer without fresh unambiguous proof ([settlement rule](../decisions/workspaces.md#transfer-and-view-settlement)).

## Open decisions and unknowns for the macOS port

- The pre-port choices above are approved; implementation/evidence remains.
  Shared host-settled geometry design still needs user approval after slice 1
  and before slice 3, and before any core change.
- M3: select/prove atomic storage and within-session reacquisition, numeric
  launch-date/process/session fences and re-login namespace changes; no store
  adoption is established by finite CG ID churn experiments.
- M4's product route is decided, not reopened here; shared implementation and
  native restore/clear/send/restart outcomes remain unproved. M5 causal creation,
  M6 retirement consumer and M2 lifetime continuity still need native proof.
- M8 target-display/Space arrival, M9 connector normalization and identical-monitor
  reconnect/re-login stability remain unknown; multi-display integration stays
  [parked](../decisions/macos.md#drag-and-drop) unless its scope is revisited.
- K5/M7 signed-route witness joins, event coverage/watch-cap behavior and
  frame-event re-resolution remain open; do not infer paint proof from callbacks.
- SIP-on foreign sticky/above mechanisms, Settings own/foreign-process distinction,
  FULL-bounds two-display validation, SkyLight group-underlay evaluation and
  physical input/fullscreen-game behavior retain their existing
  [platform-fit limits](macos-v4-platform-fit.md#m-numbered-findings),
  [0.1 scope](../decisions/macos.md#macos-0-1-scope-and-gate) and backlog gates.
- Establish the bounded within-macOS responsiveness baseline and post-port
  session-lifecycle/recovery evidence before extending claims across platforms.
- All nine lessons above have linked point-of-use records; no new generic
  lessons file or uncited lesson is proposed.

# V4 phase 3: macOS port

## Purpose, status and gates

- Entry-point plan for porting `tiler-macos` to the approved v4 boundary, in
  usable increments modelled on KDE's [dogfoodable slices](archive/v4-phase2-kde-port.md#later-dogfoodable-slices).
  The linked contracts, decisions and native records own the detail; this plan
  neither replaces them nor reopens approved behavior.
- **Not started; start gate pending.** Under the [migration decision](../decisions/contract.md#contract-authority-and-v4-migration),
  KDE phase 2 must be complete offline, including remaining v3 removal (done),
  and the user-run KDE state-target check must pass (pending in the
  [single acceptance checklist](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs),
  ST, marked MAC gate). The KDE reconnect (RC) leg-2 fix and merge decision
  must land before this port reaches its reconnect/rehome slice and before 0.1. Other KDE acceptance remains separate, not implicitly
  cleared by opening this port.
- Complete the [pre-port backlog items](../backlog.md#open-work): review the
  tentative [fullscreen-games](../decisions/macos.md#fullscreen-games),
  [fixture/scoped-input](../decisions/macos.md#fixture-and-scoped-input-defaults)
  and [development-preview](../decisions/development.md#tentative-macos-development-preview)
  decisions on macOS, bringing each to the user with a recommendation; resolve
  Authentic's startup Apply versus the [explicit-Apply decision](../decisions/reference-and-shortcuts.md#macos-preview-presets-and-native-shortcut-ownership);
  and design the shared [host-settled geometry rule](learned-size-limits.md#original-backlog-lines-1038-1060)
  before this port, with design approval before any core change.
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

- Source inspection only: no macOS toolchain is available here; no compilation,
  cargo tests or native execution was performed for this plan. Historical v2
  build results do not establish current v4 buildability.
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
  an executed compiler-error inventory.
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
| 4. Focus feedback | Port causal one-shot focus evidence and anchored-descriptor consumption. Prove authorized focus/refusal without replay, exact lifetime joins and K5/M7 native event/render evidence, including stale/unwatched suppression. |
| 5. State targets | Port fullscreen/sticky/stacking and composed-maximize frame application, attempts and recovery. Prove base restore, external-clear/fullscreen replacement, send/work-area changes and restart-to-base; retain durable intent/preimages where required. M4 implementation dependencies belong in shared core, not adapter-owned policy. |
| 6. Scaled metrics at scale 1 | Apply logical metrics once, separate native fractional-coordinate normalization from scaling, and recheck tiling/rendering on backing-scale changes while report scale stays 1. Do not absorb the Windows non-unit-scale prerequisite. |
| 7. Physical reconnect/rehome | Supply M9 physical identity facts and prove user-run disconnect/return, ambiguous identity holds, FULL/work geometry and rehome behavior without regressing prior slices. Rehomed members are not new admissions. M8 crossing and two-display acceptance remain explicit gates. |
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

- Pre-port choices above remain user decisions: confirm/amend/retire tentative
  rules, resolve startup Apply, and approve shared host-settled geometry design.
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

# Windows full world-state reply adoption

## Goal and status

- User direction 2026-10-18: **parked pending core-owned output topology;
  core-produced migration/refill acceptance blocked.** Windows will not build
  its own adjacency mapping. [Shared proposal](shared-core-owned-output-topology.md)
  is ready for cross-stream review; KDE stream is expected to deliver core+KDE.
  W2 prototype preserved on `wip/windows-world-diff`, not accepted or enabled.
  Additive constructor prerequisite and W1 offline capture/ordering fixtures
  delivered under the accepted constructor selection below. The earlier W1
  prototype remains withdrawn; the replacement starts from native sources.
  W2-W4 and runtime adoption remain pending. Windows remains on its working
  legacy domain API. No host behavior change.
- Goal: complete observed-world reports and host diffs against one full reply;
  move workspace lifecycle, scope, history and mapping policy into the retained
  in-process world state.
- Contract: [full-state D1-D5 and capacity B](archive/core-full-state-replies.md).
  Reference adapters: `kwin/src/world-*` and
  `crates/omnitiler/src/world_service.rs`.
- Preserve recovery ledger, native window properties, lifetime/gesture fences,
  refused-target suppression, follow proof, D16/D23, overlays and game safety.
  No new native capabilities, settings, polling or process boundary.
- Shared-core/protocol behavior changes beyond the selected additive
  construction API are outside this change. Coordinate
  with [D23/13f delivery](shared-core-d23-rehome-and-13f-maximize-base.md),
  consuming its approved additions rather than implementing a competing rule.

## Constructor prerequisite and accepted selection

| Policy | Current Windows owner | Current world constructor |
| --- | --- | --- |
| Fixed-size admission | Enabled | Enabled |
| Automatic fixed-float send admission | Enabled | Disabled |
| Core maximized directional-focus fence | Disabled; native adapter fences remain | Enabled |

- Evidence: `tiling_sys/run_loop.rs` enables fixed-size admission and
  `fixed_send_admission`; `WorldEngine::new` enables fixed-size admission and
  the maximized fence but not fixed sends. `WorldEngine::engine()` is read-only,
  and `WorldReport` exposes neither differing policy switch.
- The missing fixed-send opt-in is a confirmed compatibility blocker:
  `Session::fixed_send_admission` otherwise refuses automatic-origin floats.
  Windows' approved 2026-10-16 D16 extension explicitly permits these sends;
  KDE/macOS behavior was deliberately unchanged
  ([decision](../decisions.md#workspaces)).
- Maximized-policy parity also needs an explicit configuration route rather
  than assuming Linux's constructor policy is interchangeable. Windows'
  legacy event stamps leave maximized/sticky flags adapter-owned, whereas a
  complete world observation must carry truthful state and retain native gates.
  The existing `world_route_carries_linux_policy_opt_ins` test locks the Linux
  constructor choices.
- Options considered before acceptance:
  1. Recommended: additive `WorldEngine` construction accepting the existing
     Engine policy configuration, keeping `new` and KDE defaults unchanged.
     Windows keeps fixed admission/send enabled and the core maximized fence
     disabled. Prove both profiles and D16 sends in shared tests. No JSON field
     or wire semantic change is needed for construction-time configuration.
  2. Add explicit report/protocol policy fields. This expands the shared
     contract and requires defaults, validation and both adapters' regression
     coverage; unnecessary for startup-only platform opt-ins.
  3. Defer adoption; retain the legacy Windows path. No behavior regression,
     but the backlog item remains unimplemented.
- Do not fake `fixed_suppress`, automatic origin or maximized state to bypass
  the mismatch; those facts have distinct user-override and native-safety meaning.
- User accepted 2026-10-18 (T1): option 1, additive
  constructor configuration with existing constructors/defaults unchanged.
  `WorldEngine::new_with_config(core_generation: &str, config:
  WorldEngineConfig) -> Option<Self>` accepts exactly `fixed_send_admission`
  and `maximized_focus_fence`. `Default` remains `false`/`true`; Windows uses
  `true`/`false`. Fixed-size admission stays enabled in both profiles.
  Delivered in `0869f08c`; [accepted architecture decision](../decisions.md#engine-architecture-and-convergence).
- No KDE, macOS or wire/protocol changes. The KDE and macOS streams should be
  told of this additive core API; notification is pending. Their existing
  constructor calls and behavior remain unchanged. No D23/13f changes included.
- Exact next action: route the shared topology proposal to KDE/macOS; resume
  Windows W2 acceptance after core-owned topology delivery, not a Windows
  native-to-world adjacency implementation.

## Ordered releasable slices

- Each slice keeps the existing native owner path working until its replacement
  is active and covered. Scaffolding is not behavioral adoption. Use typed
  in-process reports, retaining the core's existing size/validation gates; do
  not introduce JSON transport or a second native policy authority.

| Slice | Bounded change | Acceptance before landing |
| --- | --- | --- |
| W1: observation and ordering fixtures | Add complete world capture from topology, managed workspace inventory/bindings, active/hidden/minimized lifetimes, explicit closes, constraints, holds and native flags. Add tested in-process generation/bootstrap, observation/request/command counters, dirty-until-ack and feedback retention. Keep existing actuation active; exercise the new boundary offline only. | Real-shaped Windows fixtures cover unreadable inventories, hidden/iconic omission versus close, generation reset, stale replies, command consumption and observation-only catch-up. Inactive scaffolding cannot mutate the desktop. |
| W2: full host-diff fixtures | Build the host-diff/feedback actuator against an offline native facade, not the running owner. Cover geometry/property authority, stable logical creation/binding recovery, occupied removal, transfer ambiguity and at-most-once effects. | Production-shaped fixtures cover fresh-frame diffs, overlay/durable-float holds, refusal context, identity/gesture/watermark rechecks, partial application, creation deduplication and follow proof. The running legacy owner is unchanged. |
| W3: atomic runtime cutover | Replace the active Engine path with the in-process world boundary for all observation, local/cross-output operations, selection, tiling toggles, history, scope mapping, trailing lifecycle, sends and migration together. Preserve native registry/recovery ownership; mark possibly executed before dispatch and prove membership/view read-back. Reconcile D23/13f additions already delivered by their shared change. | Existing Windows regressions plus world fixtures pass in local/global-unique/shared modes. Cover admission, fixed/float/overlay sends, follow/stay, startup/history/swaps, trailing creation/removal, partial transfer/refusal and shared migration refusal. No competing active legacy Engine or adapter trailing-policy fallback. Independent safety review and bounded owned-window smoke pass before landing. |
| W4: legacy cutoff and completion | Remove unreachable per-operation application/session policy queries and transitional scaffolding; retain renderer-only presentation, native safety and useful regression fixtures. | No active direct session query remains where a full reply replaces it. Full native gates/CI and any smoke affected by cleanup pass; remaining physical/two-output acceptance is explicitly distinguished. |

- W3 is intentionally one behavioral cutover: routing geometry through a new
  world Engine while sends keep mutating a separate legacy Engine would split
  retained topology and policy authority. W1/W2 must not introduce that state.

## Verification and completion

- After every implementation slice: `mise exec -- cargo build --locked`,
  `cargo test --locked` and strict all-target `cargo clippy --locked`, each
  selecting `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi` and
  `tiler-windows`; all Cargo commands run through `mise exec --`.
  Also `mise exec -- cargo fmt --all -- --check`, `git diff --check` and CI.
- No unsafe code outside `crates/tiler-windows/src/ffi/`; no dependency changes.
- Behavioral slices require the live Windows preflight/recovery protocol and
  owned disposable helpers only. Cover admission, focus/move, workspace
  selection and follow/stay sends, float/fullscreen/minimize restore and
  multi-workspace trailing; scope out the operator's terminal and other apps.
  Verify foreground identity immediately before every injected key; dismiss
  menus without global keystrokes and verify complete desktop restoration.
- Completion requires no unapproved Windows-observable changes, passing
  regressions, truthful partial/refused outcomes, and no unresolved safety
  finding. Archive this record only after acceptance; backlog status remains
  unimplemented until the runtime cutover and acceptance are delivered.

## Evidence

- Documentation-only planning boundary; no production edits or live tests.
- Native package build/test and strict all-target clippy pass with `--locked`
  for `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi` and
  `tiler-windows`: **2,601 passed, 0 failed, 1 ignored** across 65 suites.
  Workspace formatting and whitespace checks pass.
- Existing focused tests pass: `world_route_carries_linux_policy_opt_ins`,
  `automatic_fixed_float_send_refuses_not_tiled` and
  `automatic_fixed_float_send_admits_with_opt_in`. These prove current opt-in
  behavior, not completed world adoption.
- `WorldCommand::Operation` implementation already covers Windows' local,
  cross-output drag/directional, migration and release routes. Its older
  restrictive documentation is not evidence of a missing command handler.
- Constructor prerequisite: build/test/strict all-target clippy, formatting
  and whitespace checks pass; **2,606 passed, 0 failed, 1 ignored**. Five new
  core tests prove unchanged default replies and opt-ins, actual automatic
  fixed-float refusal/carry, maximized focus behavior, and configuration
  retention through reports/incarnation reset. Independent review accepted
  the isolated two-file core change; documentation/naming findings corrected.
- Constructor commit `0869f08c`: CI run 39040453638 passed all eight jobs,
  including native Windows/macOS, Rust/KWin and native effect/settings gates.

## Withdrawn W1 prototype and revised entry point

- Two W1 prototype revisions were reviewed; neither was accepted or landed.
  The first modeled host snapshots in parallel DTOs and lost undelivered
  closes or updated feedback during acknowledgement. The correction fenced
  individual flights but retained unsafe observation mappings and incomplete
  dirty-until-ack semantics. The prototype was preserved outside the source
  tree for diagnosis and withdrawn; no Windows code from it is delivered.
- Remaining findings in the rejected correction:
  - Non-iconic hidden rows forced overlay, fixed-origin/override and exclusion
    flags false instead of carrying verified retained facts or explicit holds.
  - Invisible non-iconic rows were omitted as if minimized. Frameless retained
    rows were described as minimized despite also representing unreadable or
    scope-excluded windows. Hidden, iconic and unreadable are distinct facts.
  - A separately supplied membership key was not checked against the observed
    window identity, permitting mismatched lifetime/flag association.
  - An accepted report cleared dirty state when its observation watermark was
    current even if newer feedback or an absent-domain close remained queued.
    Tests manually invalidated again and therefore missed this lost catch-up.
- Native evidence to start the redesign: `tiling_sys/observe.rs` defines
  `ObservedWindow`; `retained.rs` defines `RetainedRow`, `HiddenRecord` and
  `PendingClosed`; `observation.rs` creates frameless rows for both unreadable
  and scope-excluded evidence. `workspace.rs` owns `WindowKey` and committed
  membership. Derive lifetime keys directly from those identities and preserve
  their existing distinctions, rather than accepting a parallel guessed state.
- Options:
  1. Recommended: restart W1 as a smaller source-first capture fixture unit.
     Prove visible/hidden/iconic/unreadable classification and retained safety
     flags before adding the sequencer. Build actual managed membership,
     topology and binding assembly, then add ordering/acknowledgement tests.
  2. Salvage the prototype sequencer in isolation, with the remaining dirty
     obligation regressions fixed, before rebuilding capture. This preserves
     more scaffolding but risks designing around the same parallel model.
- Required discriminators: hidden overlay/fixed intent stays reserved/held;
  frameless unreadable/scope-excluded is not minimize; mismatched lifetime
  cannot inherit lanes; updated feedback and undelivered closes remain dirty
  after an unrelated acknowledgement without an extra native invalidation.
- No new product decision was selected by the withdrawn prototype. Its source
  and acknowledgement findings are covered by the accepted replacement below.

## W1 accepted offline outcome and handover

- `tiling_sys/world_capture.rs` consumes the real `TileLoop`,
  `Option<&[ObservedWindow]>`, `RetainedRow` and `MonitorArea` sources, not
  parallel input DTOs. Fixtures construct actual managed membership, claims,
  identities and local/global-unique/shared workspace tables. Existing opaque
  output tokens bind native device keys without lossy identifier rewriting.
- Hidden claims require valid claim tags and matching lifetime identities;
  stored member tags and claim tags remain distinct properties. Capture
  consumes the existing native lifetime/audit prerequisites, never substitutes
  for live revalidation. Iconic omission is explicit minimized membership;
  unreadable, scope-excluded and other unsupported safety states hold rather
  than fabricate flags or minimize. Failed enumeration holds even empty
  managed inventories. Only explicit queued closes are reported.
- Verified fullscreen/maximized, sticky, automatic-fixed and both tile-override
  lanes retain their distinctions. Ordinary fullscreen reservation is separate
  from born-fullscreen floating holds. Raw work bounds plus the outer-gap field
  produce exactly one inset; primary output comes from native monitor order.
  `fit_excluded` is not treated as generic write authority: unsupported native
  safety facts use explicit holds, with reply-level regressions.
- `tiling_sys/world_sequencer.rs` exercises typed reports through an offline
  Windows-profile `WorldEngine`: explicit generation bootstrap/reset,
  single-flight observation/request/command ordering, stale-reply catch-up,
  command consumption and counter-exhaustion refusal. A new command during
  catch-up is explicitly refused, never silently dropped or replayed.
- Dirty work includes observation, feedback and native-owned close obligations.
  Updated feedback and absent-domain closes survive unrelated acknowledgements
  without another invalidation. Close receipts prune only exact delivered
  token/homing pairs in the current adapter/core namespace; no append-only
  close acknowledgement history is retained. Reset clears obsolete feedback
  before operation identities can be reused. Delayed replies, loss callbacks
  and deferred receipts cannot consume a newer flight or generation's facts.
- Capture and sequencing fixtures were independently reviewed before their
  dependent use. Review corrections include native-source-only inputs, truthful
  hidden/safety holds, single-inset bounds, explicit command refusal,
  generation-scoped feedback and correlated exact-fact acknowledgements.
  Both modules remain `#[cfg(test)]`: the running owner, native properties,
  recovery ledger and desktop actuation are unchanged. No further shared core,
  protocol, KDE or macOS edits, dependencies, unsafe code or policy decisions.
- Final native gates: four-package locked build/test and strict all-target
  clippy, workspace formatting, unsafe guardrails and `git diff --check` pass.
  **2,645 passed, 0 failed, 1 ignored across 65 suites**, including 21 capture
  and 18 sequencing fixtures. No live smoke was run: this slice is offline-only
  and introduces no behavioral runtime path. Behavioral cutover still requires
  the bounded owned-window smoke and restoration evidence.

### Exact next slice: W2

- Build the full host-diff/feedback actuator against an offline native facade;
  preserve current runtime ownership. Start with fresh-native-frame authority,
  lifetime/gesture/watermark rechecks, overlay/float holds and recovery ownership,
  then creation binding, occupied removal, transfer ambiguity and follow proof.
- Do not turn W1/W2 modules on in the running owner before the W3 atomic cutover.
  Capture currently reads retained float exceptions from the legacy owner;
  W3 must replace that source with the single world authority, not retain two
  active Engines or policy caches.
- Carry these safe-hold limitations into W2/W3, not silent workarounds:
  - Ordinary hidden members without verified fullscreen evidence hold their
    domain; show preimages record minimized/maximized, not fullscreen. Prove
    a truthful native evidence route before runtime adoption rather than
    defaulting fullscreen false or broadening the recovery claim model casually.
  - Fresh fullscreen seeds can hold in the existing world sweep, including
    born dual-flag cases. The fixture proves no unsafe authority, not liveness.
    Establish parity before W3; any shared semantic change needs separate review
    and must not overlap the D23/13f work.
  - Per-workspace tiling overrides are not a `WorldReport` observation field;
    W3 must prove bootstrap preserves native modes through the existing world
    command contract before any desired state can actuate.
  - Adjacency is intentionally empty, matching the native domain builder.
    User direction 2026-10-18 supersedes adapter mapping: shared core must
    retain FULL monitor positions and derive targets. Windows supplies its
    existing FULL topology geometry, not KDE heuristics or a Windows map;
    [proposal](shared-core-owned-output-topology.md).
- W2-W4, runtime/live adoption and physical/two-output acceptance remain pending.
  The backlog item remains unimplemented until cutover and acceptance.

## W2 design-first outcome: parked

- Two prototype revisions left five hazards reproduced by eight diagnostic
  failures: **32 passed, 8 failed**. One subsequent design-first correction
  preserves all original fixture assertions and passes **50 W2 fixtures**.
  Full prototype, detailed comparisons, tests and history are preserved on
  `wip/windows-world-diff` (`677f49fa`, compatibility update `d3bf1fb9`).
- Independent design review accepted the facade-local design below.
  Implementation review verified the five hazard fixes but **rejected W2
  acceptance** because real Windows capture cannot produce a migration/refill
  roundtrip. No further semantic redesign followed that rejection.

### Setter gates and attempt ownership (tentative)

- Reuse Windows classification, lifetime/member-tag checks, gesture/watermark
  fences and `RefusedTracker`. Every setter requires current reply authority,
  controlled scope, resolved native binding and fresh identity/safety evidence.
  Row-local safety drift holds; identity/watermark failure stops. Only
  `effective_frame`, never canonical allocation, is a frame target.
- Membership ambiguity and view attempts are distinct local setter phases
  under existing core operation ids. Mark each phase before dispatch, after
  all gates pass. Membership arrival cannot clear a spent view phase. Exact
  target proof can complete without dispatch; otherwise never repeat a
  possibly executed view. Prune/reset with operation retirement/generation.
  No new protocol ids, retries, policy authority or shared semantics.

| Hazard | Owner and setter permission | Feedback/proof |
| --- | --- | --- |
| Float safety drift | Stable `frame_operation`; strict native eligibility plus fresh overlay/sticky/fit-excluded/gesture/lifetime gates. Ordinary floating intent alone is not a refusal. Spend only after gates pass. | Exact readback is `Applied`; explicit refusal plus unchanged preimage is `RefusedProven`; unreadable/partial/throw is `PossiblyExecuted`. Holds do not consume unattempted placement. |
| False view refusal | Retained view id owns one view phase. Capture immediate preimage and read the exact output after dispatch. | Target proof is `Applied`; explicit false plus unchanged readable preimage is `RefusedProven`; success with old/unreadable proof or unresolved throw is `PossiblyExecuted`; fresh third-location proof is `SupersededExternal`. Retain feedback through W1 acknowledgement. |
| Repeated follow switch | Window-transfer id owns membership and follow-view phases separately. Follow needs arrival proof and current scope/authority; arrival never rearms its view phase. | One transfer outcome, no invented view id. Follow success needs arrival and target-view proof. Failed follow stays locally fenced; late target proof can finish without switching again. |
| Held migration refill | Parent migration id owns member/target-view phases; actual paired `{parent}-refill` id owns refill. All members arrive before target view; target proves before refill. Refill additionally needs unheld transfer, controlled scope, resolved binding and unspent phase. | Held refill never switches or fabricates refusal. Apply the view proof table to actual attempts. Parent stays partial while required refill unresolved; successful member/view phases never replay. |
| Stale focus membership | One-shot effect, spent before dispatch. After safety/lifetime checks, use fresh `NativeOps::read_homing`; require controlled live scope and `read_view(live_output) == live_workspace`, never observation-time homing. | Foreground readback proves success even after throwing setter. Missing homing/view holds. No invented core feedback; attempted effect stays spent. |

- Windows comparison: `send_native.rs` rechecks lifetime/overlays and verifies
  committed transfer; `send_follow.rs` selects after arrival proof, then
  reports verified focus without replay. `focus_actuation.rs` uses live
  identity/foreground proof; `tiling/reconcile.rs` owns refusal/clamp context.
  Reuse these mechanisms, not a new retry policy.
- KDE comparison: `kwin/src/world-apply.ts` separates float authority,
  membership arrival, exact-output view proof, migration/refill ordering and
  one-shot focus. Copy the contract, not setter-return shortcuts or early
  clearing of follow fences. `crates/omnitiler/src/world_service.rs` owns
  authenticated single-flight discovery/report transport; settlement stays
  in `WorldEngine`. No KDE/service edits were made.
- Existing shared limitation: window transfers settle by homing alone in
  `WorldEngine::settle_retained`. Follow failure is best-effort/no-repeat,
  not guaranteed eventual follow. A stronger guarantee needs separately
  approved shared semantics. Residual evidence gap: float clamp/throw fixtures
  use synthetic ids and prove facade no-repeat/feedback retention, not actual
  retained-float settlement. Applied float, creation/binding, view refusal and
  follow success/failure have core-produced re-observation roundtrips.

### Blocker, preservation and exact next action

- `core_migration_refused_without_adjacency` builds two outputs with real
  Windows inventory/bindings/areas through W1 capture/sequencing and proves
  `migration-refused`. Capture preserves the native domain builder's empty
  adjacency; both `world_commands.rs::start_migration` and
  `operation_migration` require carried adjacency. Synthetic migration/refill
  tests are not sufficient integration evidence. Do not inject KDE adjacency
  merely to turn the missing product route into a passing test.
- Existing native resolution does exist: `migration.rs` uses
  `select_migration_target` over full monitor rectangles. Its shared selector
  is useful parity evidence, not a reason to add another Windows authority.
- User direction 2026-10-18 supersedes the earlier native-to-world mapping
  options: keep adoption parked until core owns monitor topology and derives
  adjacency from reported FULL rectangles. Windows will not implement an
  adjacency mapping. [Cross-stream proposal](shared-core-owned-output-topology.md)
  separates current world/legacy selection rules and their explicit review.
  After shared delivery, prove actual migration/refill capture, feedback and
  re-observation with empty adapter adjacency before accepting W2. The working
  legacy owner remains active meanwhile; prototype evidence is not acceptance.
- Before integrating concurrent best-effort admission, four-package locked
  build/test/strict all-target clippy, formatting and whitespace gates passed:
  **2,695 passed, 0 failed, 1 ignored across 65 suites**. No new unsafe,
  dependency, runtime, core/protocol/KDE/macOS edits or live tests.
- After integrating `cf422d2f`, added shared fields broke Windows test
  literals. A mechanical fixture-only compatibility update on the WIP branch
  supplies shared admission defaults and non-float classification for tiled
  rows, without assertion/facade changes. Build, strict all-target clippy,
  formatting and whitespace pass; full tests stop with **2,349 passed,
  3 failed, 1 ignored across 45 suites** (partial run):
  `preview_sticky_journey_agrees_with_drop`,
  `rejected_hydration_holds_float_writes_for_one_observation` and
  `next_consistent_observation_revalidates_after_rejection`. These existing
  preview/restart regressions need separate admission-adoption triage.
- Main receives this analysis/backlog update only, not prototype or
  compatibility source changes. All WIP source remains on the pushed
  `wip/windows-world-diff` branch; modules/fakes are `#[cfg(test)]`. No current
  green full-gate claim. CI does not run on preservation-branch pushes under
  the current workflow. Upstream native compile/behavior reconciliation is
  separately required, not hidden by the branch split.
- Upstream `cf422d2f` CI run 39053584901 failed its Windows test job before
  this documentation-only update; the other seven jobs passed.
- Exact next action: KDE stream reviews and delivers core-owned topology with
  KDE integration; then Windows resumes blocked migration/refill acceptance
  without Windows adjacency code. Reconcile the preserved prototype's native
  gate failures against current main before claiming new acceptance. W3 remains
  blocked on accepted W2 plus hidden/fullscreen/bootstrap parity. Do not
  enable the facade.

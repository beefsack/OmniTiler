# Windows full world-state reply adoption

- Status: superseded, archived 2026-10-24. The W1/W2/W2a/W2b/T2 staged work
  below is the historical adoption path; the completed [core world-model
  rearchitecture](core-world-model-rearchitecture.md#closing-outcome-2026-10-24)
  is the accepted implementation and preserves the T2 staged-transaction
  semantics unless a later boundary supersedes them. Current port contract:
  [world adapter guide](../../porting/world-adapter-guide.md).

## Goal and status

- W2a delivered as a reviewed, gated offline diagnostic increment, 2026-10-18.
  Core-owned [FULL topology](shared-core-owned-output-topology.md)
  (`f9bd2df1`) unblocked prototype reconciliation: Windows reports FULL monitor
  rectangles and empty adjacency, and sends directional intent only. No Windows
  target selection or adjacency mapping was added.
- W2 acceptance is still blocked on positive migration/refill authority and T2
  pre-commit refusal parity; see [W2b proposals](#w2b-cross-stream-proposals).
  W2a is not runtime adoption or positive migration acceptance. W1/W2 modules
  remain `#[cfg(test)]`; Windows keeps its working legacy domain API. Recovery,
  native properties, admission and desktop behavior are unchanged. W3/W4,
  hidden/fullscreen/bootstrap parity and native acceptance remain pending.
- Goal: complete observed-world reports and host diffs against one full reply;
  move workspace lifecycle, scope, history and mapping policy into the retained
  in-process world state.
- Contract: [full-state D1-D5 and capacity B](core-full-state-replies.md).
  Reference adapters: `kwin/src/world-*` and
  `crates/omnitiler/src/world_service.rs`.
- Preserve recovery ledger, native window properties, lifetime/gesture fences,
  refused-target suppression, follow proof, D16/D23, overlays and game safety.
  No new native capabilities, settings, polling or process boundary.
- Shared-core/protocol behavior changes beyond the selected additive
  construction API are outside this change. Coordinate
  with [D23/13f delivery](../shared-core-d23-rehome-and-13f-maximize-base.md),
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
  ([decision](../../decisions.md#workspaces)).
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
  Delivered in `0869f08c`; [accepted architecture decision](../../decisions.md#engine-architecture-and-convergence).
- No KDE, macOS or wire/protocol changes in the constructor prerequisite.
  Their existing constructor calls and behavior remain unchanged. No D23/13f
  changes included. W2a now proves the Windows profile against a separately
  bootstrapped default-profile engine using actual captured reports.
- The topology prerequisite has landed; the next prerequisite is W2b's shared
  migration authority and Windows capacity-send refusal contract, not adapter
  adjacency.

## Ordered releasable slices

- Each slice keeps the existing native owner path working until its replacement
  is active and covered. Scaffolding is not behavioral adoption. Use typed
  in-process reports, retaining the core's existing size/validation gates; do
  not introduce JSON transport or a second native policy authority.

| Slice | Bounded change | Acceptance before landing |
| --- | --- | --- |
| W1: observation and ordering fixtures | Add complete world capture from topology, managed workspace inventory/bindings, active/hidden/minimized lifetimes, explicit closes, constraints, holds and native flags. Add tested in-process generation/bootstrap, observation/request/command counters, dirty-until-ack and feedback retention. Keep existing actuation active; exercise the new boundary offline only. | Real-shaped Windows fixtures cover unreadable inventories, hidden/iconic omission versus close, generation reset, stale replies, command consumption and observation-only catch-up. Inactive scaffolding cannot mutate the desktop. |
| W2a: gated host-diff diagnostics, delivered | Reconcile the parked host-diff/feedback facade onto current main; retain offline-only ownership. Cover geometry/property authority, creation/binding recovery, occupied removal, transfer ambiguity, at-most-once effects and actual FULL-topology directional migration ingress. | 55 fixtures and native gates pass; review accepts the diagnostic boundary only. Actual migration remains held; a passing diagnostic reproduces the T2 gap instead of claiming refusal parity. The running legacy owner is unchanged. |
| W2b: full host-diff acceptance, pending | Rework migration as core-owned workspace/output mapping plus adapter actuation; retain accepted adapter-only staged-clone T2 refusal without policy duplication. | Real W1 capture/core-produced migration/member/target-view/refill/feedback/arrival roundtrip; Windows capacity-float output-send and R4 refusal preserve source Engine and membership. No held-scope override or functional use of diagnostic float origin. |
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

### Original next slice: W2

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

## Historical W2 design-first outcome: parked

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

## W2a reconciled offline outcome

- Reapplied only the parked Windows facade/fixtures from `677f49fa` and
  `d3bf1fb9` onto current main. No old core or documentation changes were
  replayed. The three preview/restart failures reported above no longer
  reproduce. `world_capture` exposes its existing eligibility classifier to
  the sibling test-only facade; no parallel safety classifier was introduced.
- Accepted boundary: source-first fresh-frame/property authority, overlay and
  float holds, lifetime/gesture/watermark fences, refusal tracking, logical
  creation/binding recovery, occupied-removal refusal, partial/ambiguous
  transfers and at-most-once follow/view/focus fixtures. Retired output and
  operation view proofs are now pruned, with a regression fixture.
- Actual W1 capture/sequencing sends `MigrateDirectional` with source context
  and direction only. The report asserts exact FULL rectangles distinct from
  work bounds and empty domain adjacency. Core produces `Migration` plus the
  correlated source-refill View and member provenance. The unobserved target
  scope is held; the facade truthfully performs no membership/view writes and
  invents no attempt feedback. Re-observation retains the obligation. This is
  ingress/hold evidence, not positive migration/refill convergence.
- T1 fixtures prove Windows fixed-send opt-in and maximized-fence opt-out;
  separately bootstrapped default-profile engines refuse the same commands.
  `focus-mismatch` for default automatic-fixed send and `unchanged` for default
  maximized focus are the existing shared refusal vocabulary.
- The T2 diagnostic uses a real captured two-output world, a 25x25 destination
  and direction-only send. It reproduces current core acceptance, capacity
  floating desire, offline membership write and `Applied` feedback where legacy
  Windows refuses `float-no-remap`. Diagnostic origin is asserted only; facade
  code never branches on it. This test must become a refusal regression after
  the shared prerequisite lands. It is explicitly not accepted product behavior.
- Independent review accepted only this `#[cfg(test)]` diagnostic boundary.
  Review corrected stale applied-view proof retention and confirmed that both
  proposed shortcuts below would violate the existing contract. No production
  owner, core/protocol/KDE/macOS code, dependency or unsafe change; no live test.
- A subsequent stale-envelope regression reproduced fence loss: pruning before
  freshness validation let a wrong-generation reply retire an ambiguous view's
  spent phase. The narrowly causal correction moves pruning after validation;
  the regression now proves rejection preserves tracking and later fresh
  replies cannot replay the view. Follow-up independent review accepted it.
- Verified four-package locked build/test and strict all-target clippy,
  workspace formatting, unsafe guardrail and whitespace checks pass:
  **2,840 passed, 0 failed, 1 ignored across 71 suites**, including **55 W2
  fixtures**. CI `39099764587` for `54a1be6b` passed all eight jobs.
  Follow-up `39100032800` for `7d5f74b5` passed Windows, macOS, KDE, native,
  shell, licenses and notices on its first attempt; Linux workspace tests
  failed in unchanged macOS `runtime_freshness` test
  `saturated_queue_overmarks_but_newest_and_retry_converge` (line 1148:
  `EngineQueueFull` instead of `Accepted`). No macOS code was changed. The
  failed-job rerun passed unchanged: attempt 2 is successful with all eight
  jobs green. The first-attempt macOS queue-test nondeterminism remains a
  separate residual risk, not a Windows regression or a repaired macOS test.
- Reconciled the evidence update over concurrent
  [split-edge selection delivery](world-split-edge-selection.md) through
  `5a084477`. Repeated four-package locked build/test, strict all-target clippy,
  formatting and whitespace gates pass: **2,852 passed, 0 failed, 1 ignored
  across 71 suites**. All 55 W2 fixtures still pass; neither parked diagnostic
  outcome changed. Selection remains entirely core-owned.
- Tentative delivery choices: preserve/reconcile the test-only facade instead
  of enabling it; split out W2a diagnostics rather than falsely accept W2;
  retain a passing current-behavior T2 discriminator until shared correction.
  No new durable product decision; T1, T2 and core topology ownership stand.
- User-approved 2026-10-19: the W2a delivery choices above stand; this does
  not enable runtime adoption or establish positive T2 acceptance.

### W2b cross-stream proposals

- **User direction 2026-10-20:** the narrow default-off migration-authority
  capability below is not approved in that form. Under the
  [core-owned world model](../../decisions.md#core-owned-world-model), a
  whole-workspace migration changes core's workspace-to-output mapping and the
  adapter actuates that new world state. Rework the contract with the
  core/platform boundary inventory and discussion before planning. T2's
  adapter-only staged-clone direction remains accepted; the original proposals
  below are historical, not the current implementation plan.
1. **Migration authority (shared contract; no change made):** source workspace
   exists only on source output before migration. `world_publish.rs` publishes
   missing target sessions as `Hold(IncompleteInventory)` and migration rows
   with held authority. `world_diff.rs` correctly requires controlled scope,
   so no member arrives to satisfy `migration_confirmed`. KDE's current
   `world-apply.ts` instead consumes `command_result.members` for migration
   without this scope gate. `world_report.rs` describes those members as
   provenance, with actuation riding snapshot rows and retained transfer.
   Resolve the contract explicitly: publish narrowly actionable migration
   authority while retaining ordinary holds, or explicitly specify transfer
   authority precedence. Do not copy KDE's bypass or edit Windows scope facts.
   Then prove positive member -> target view -> refill -> topology/binding
   confirmation through actual W1 capture, feedback and re-observation.
   The concurrent split-edge record also parks KDE's global-native-inventory
   migration-confirmation mismatch; authority and final settlement must both
   be specified, not hidden by successful member writes alone.
2. **T2 pre-commit capability gate (shared prerequisite; no change made):**
   `world_sends.rs::execute_output_send` accepts the Engine's capacity-float
   output-send outcome and retains a normal window transfer. Legacy Windows
   discards its staged candidate for `float-no-remap`. A post-reply adapter
   hold cannot preserve the already-mutated world Engine. Propose an additive
   Windows capability/configuration route that refuses an unsupported
   capacity-float crossing before world commit, preserving other platforms'
   defaults and ordinary/D16 float behavior. Cover output-send and R4 paths.
   Do not infer capabilities from `FloatOrigin`: its public contract explicitly
   prohibits functional branching. No new protocol/feedback vocabulary or
   adapter capacity policy was introduced here.

- Minimal [action-sequence discriminators](../../spec/reference-outcomes.md#windows-world-adoption-authority-discriminators)
  preserve the two gaps; unsupported native outcomes remain TBD.
- Exact next action: inventory platform-owned policy as input to the
  core/platform boundary discussion, then rework migration around core-owned
  mapping and adapter actuation; retain T2's accepted staged-clone direction.
  Replace held/current-gap
  diagnostics with positive migration/refill and source-preserving T2 refusal
  regressions. Only then accept W2 and resume W3's atomic cutover. Hidden and
  fullscreen liveness, tiling-mode bootstrap, all-mode parity and native
  owned-window acceptance remain separate W3/W4 prerequisites. The active note
  stays open; do not enable the facade.

## W2b assessment, 2026-10-20

- Assessment only. Migration recommendations below are **superseded by the
  User 2026-10-20 core-owned mapping direction**, not approved in that form;
  the T2 adapter-only direction is **User 2026-10-20 accepted**.
  These supersede the assumption above that both blockers require
  core additions. No runtime or core behavior changed; W2/W3 remain blocked.
  Apply [Layering](../../principles.md#layering): prefer a sensible adapter
  alternative before adding generic, default-off core support.
- Fetched history through `0f76c595`: the intervening change concerns macOS
  native-Space selection, not these migration/topology paths.
- No live validation: W1/W2 are test-only and the running owner is legacy.
  Exact-owner `workspace`, `directional` and `send-output` controls cannot
  exercise world migration authority; there is no migration control. The
  existing W1-capture migration fixture remains the evidence: real producer
  types, complete inventories/bindings, FULL geometry and empty adjacency,
  real core migration/refill, silent target hold, retained re-observation.
  It is offline ingress/hold evidence, not a capture of a new native journey
  or positive two-output settlement. No synthetic input or live effects used.

### 1. Migration authority

- **Root cause recap:** after core plans the migration it retains the workspace
  under the target output, while the adapter still reports it on the source
  output. Target scope is held (`Hold(IncompleteInventory)`), so members never
  move. This is a mapping/observation authority mismatch, not permission to
  invent target observations or bypass holds in the adapter.
- **Current decision:** core owns workspace-to-screen mapping; migration is a
  core mapping change actuated by the adapter. Rework publication, observation
  reconciliation and settlement around that model, coordinated with the
  core/platform boundary inventory. The following option assessment and narrow
  capability design are retained as a **superseded proposal**, not an approved
  plan. Positive convergence and native acceptance remain TBD.

| Option | Assessment and trade-off |
| --- | --- |
| Adapter retained-transfer precedence over Hold | Mechanically small, but not a sensible current-contract fix: held scopes forbid writes (`decisions.md`, Engine Architecture), and `CommandResult::Migration.members` is provenance, not authority. A missing destination and genuinely incomplete evidence can both carry `IncompleteInventory`; `TransferTarget.held` fences ambiguous attempts, not scope completeness. Caching the one-command member result adds adapter authority across later replies. This would copy KDE's bypass, not resolve it. |
| Report an empty target session before arrival | No core edit, but invents native membership/binding facts for a workspace still belonging only to source. Violates W1 truthful observation; reject. |
| Narrow core publication capability | One shared authority rule using existing scope/member/transfer fields; requires approval and coordinated adapter checks, but no protocol change or second policy owner. Recommend this option. |

- **No-core verdict:** no small, safe, contract-preserving complete solution
  found. Ordinary output-send and R4 already use readable destination domains
  and retained Window transfers; they do not need the missing-workspace
  exception. Whole-workspace migration creates this gap after relocation.
  Evidence: `world_publish.rs` migration redirects and missing-session lane;
  `world_report.rs::CommandResult::Migration`; Windows `world_diff.rs` 3b.
- **Minimal proposed capability:** additive construction flag, provisionally
  `migration_destination_membership = false`, following T1 and
  `aligned_per_output_workspaces`, never a host check. When opted in, only a
  retained migration's exact target may receive Controlled scope and exact
  member membership authority before target observation. Require complete
  inventory/topology, readable source evidence, a live native binding and a
  destination absent because that migration relocates the workspace, not an
  unreadable/missing observation. Keep frame authority held/untouched, no
  effective frame, and unrelated scopes/properties held as before. Continue
  publishing that exact membership-only set on later replies, not just the
  command result, only while the parent remains retained. Settlement,
  refusal or supersession removes that authority; explicit closes remove
  affected lifetimes. Genuine safety/gesture/identity holds still win.
- Windows migration apply must check every member's membership authority as
  well as scope; the current loop checks only target scope before enumerating
  rows. Target view follows all verified arrivals; paired source refill follows
  verified target view. Refuse shared mode per D2; preserve local/global-unique
  semantics, float/overlay remaps and at-most-once phases. No `FloatOrigin`
  logic, new wire field, authority enum or feedback vocabulary.
- **Settlement is part of acceptance, not implied by member writes:** for
  disjoint sets require member homing, target view, retained source-refill
  proof and truthful topology/binding relocation. Current non-aligned
  `settle_retained` only tests `migration_confirmed` (target lists backing ID,
  source does not); do not claim it already verifies all phases. Aligned
  `settle_aligned_migration` already uses member/both-view proof. Explicit
  closes, partial/unreadable arrivals and external supersession must retain
  their existing no-replay rules, including empty migrations.
  Strengthened disjoint settlement belongs to the opted-in contract; existing
  construction defaults retain their current publication/settlement behavior.
- **KDE implication:** align `kwin/src/world-apply.ts` 3b to scope plus member
  authority; stop treating `result.members` as a Hold bypass. Independently,
  the [parked global-native-inventory mismatch](world-split-edge-selection.md#parked-limitation-and-native-acceptance)
  makes source-native absence impossible on the replicated ring. Keep scoped
  inventory confirmation for disjoint sets; use truthful per-output logical
  binding/member/both-view proof for aligned sets. Never filter a global
  inventory to fabricate absence. Authority delivery alone cannot close that
  settlement gap; core/KDE must prove retirement together.
  Land KDE's authority checks before enabling the new publication capability;
  KDE must explicitly opt in with the aligned contract. Do not ship the old
  bypass alongside the enabled capability.
- **Decision outcome:** the default-off, membership-only migration capability
  is not approved in that form. Use core-owned mapping and adapter actuation;
  preserve truthful observations and prove settlement in the reworked contract.

### 2. T2 capacity-float crossing refusal

| Option | Assessment and trade-off |
| --- | --- |
| Refuse before sending intent from current float flag or frame/target size | Existing floats are ordinary/D16 sends and must remain allowed. T2 concerns a tiled mover becoming a float at the destination. Size alone cannot predict D2/D3, gaps, hints or exact R4 wrapping; reproducing those rules in Windows duplicates core policy. A blanket crossing refusal is overbroad. |
| Stage one cloned WorldEngine, adopt or discard | Sensible no-core candidate, mirroring legacy Windows' candidate Engine transaction. Run the exact command report on a fresh clone; adopt only supported results, otherwise discard before native effects. No second retained authority or projection policy. Extra command-time clone above the world's internal staging; observation-only reports need no extra clone. Recommend proving this boundary before considering core additions. |
| Core pre-commit opt-in | Parked fallback only if the adapter transaction cannot satisfy the remaining discriminators simply. Do not add a capability merely because post-reply refusal on an already-adopted engine is too late. |

- **Path coverage:** the same transaction surrounds output-send and R4; no
  adapter target selection or local-vs-cross prediction is needed. Inspect
  only a fresh command's matching cross-output Window transfer and functional
  tiled-to-floating facts, never diagnostic origin. Local moves and fitting
  tiled sends pass. Exclude whole-workspace Migration commands/transfers:
  D7/D8's established native float/overlay remaps are not changed by T2.
- **Discard contract:** emit local `float-no-remap`, do not adopt/ack the
  candidate or actuate/report attempted transfer feedback. Release the exact
  flight with `note_rejected_or_lost`; observation-only catch-up retains dirty
  facts, feedback and closes. Burned command/request numbers are not reused;
  the rejected intent is never replayed. Supported results adopt the same
  evaluated clone, not a second execution or re-capture. This stays inside
  the future single Windows world owner; it does not retain legacy authority.
- **Initial offline prototype:** the five-row clone/adopt proof covered
  output-send/R4 refusal and supported float/tiled/local moves, but not source
  repair, prior flights or close retention. It has been replaced by the
  [expanded offline proof](#t2-offline-staged-transaction-2026-10-20) below;
  existing raw-route current-behavior diagnostics remain unchanged.
- **No-core verdict/recommendation:** a viable bounded adapter transaction
  exists; prefer completing its test-only proof. No T2 core capability is
  justified at this assessment boundary. If robust classification needs
  duplicated projection policy or growing transaction machinery, return for
  review rather than silently adding core policy.
- **User 2026-10-20 accepted:** retain the no-core staged transaction as the T2
  direction, keeping the core capability parked. This accepts the adapter-only
  staged-clone refusal direction, not runtime acceptance; it remains test-only
  and unwired.

### Assessment evidence and next action

- Focused Windows world-diff tests: **56 passed**, including the five-row
  clone/adopt prototype and unchanged migration-hold/T2-gap diagnostics.
  `mise exec -- cargo test --locked -p tiler-windows --lib world_diff_tests`
  and `mise exec -- cargo clippy --locked -p tiler-windows --lib --tests`
  pass. No full native gate or live acceptance is claimed for this
  assessment-only increment.
- Next: use the platform-owned policy inventory for the core/platform boundary
  discussion, then rework migration under the accepted core-owned world model;
  T2's adapter-only direction is unchanged. After agreed planning/contracts
  and remaining offline proofs, replace diagnostics with positive migration
  convergence and source-preserving refusal regressions; only then resume
  W2 acceptance and W3 runtime/native planning.

## T2 offline staged transaction, 2026-10-20

- **User 2026-10-20 accepted:** continue the adapter-only staged-clone refusal direction,
  not runtime acceptance; it remains test-only and unwired.
  Migration requires rework under the core-owned mapping direction above. No core,
  protocol, runtime-owner or native behavior change; W2/W3 remain unaccepted.
- `WorldSequencer::deliver_production` is now called by the incomplete
  [world-owner cutover](core-world-model-rearchitecture.md#stage-2b-partial-implementation-checkpoint-2026-10-21),
  not an accepted native runtime. The `#[cfg(test)]` `deliver_staged` is a
  thin branch-indicator wrapper over it, never a second staging route. A
  fresh candidate evaluates the exact report once, then adopts a supported
  outcome (accepted/consumed/duplicate adopt the candidate exactly once;
  rejection preserves the committed engine) or drops an unsupported
  crossing before returning local `float-no-remap`. Callers release the
  refused flight and catch up observation-only without replay.
- Classification uses a newly Accepted command's exact `op-<command_seq>`
  Window transfer, source output/floating fact and desired mover floating
  state. No `FloatOrigin`, target selection or capacity projection rule is
  duplicated. Rejected candidates drop; Duplicate preserves cached semantics.
  CommandConsumed must adopt its fresh observation/revision without running
  the command or T2 classifier. An initial prototype dropped that observation;
  the corrected regression compares against an independent raw world engine.
- **Incremental cost:** exactly one explicit candidate-clone call site,
  reached only by Send/SendDirectional/MoveDirectional and Operation
  SendToOutput/cross-output Move. A potentially crossing directional command
  still pays that clone when core ultimately chooses a local move; Windows
  does not predict core's result. Observation-only, focus, local-only
  Operation and whole-workspace migration routes add no clone. The world's
  existing internal staging is unchanged. The staged classifier lives in
  the production sequencer body (single site); the test wrapper and the
  owner delegate to it. Counts are
  structural plus tested branch indicators, not a measured latency budget.

| Offline discriminator | Accepted evidence |
| --- | --- |
| Output-send and R4 exhaust target | Directional send, directional move and Operation cross-output Move return local `float-no-remap`; complete retained Engine debug state, revision, desired reply, transfer inventory and real managed membership remain unchanged. Refused candidates expose no applicable reply. Watermark-valid host-diff catch-up proves no membership/remap/view change. |
| Source repair, not target exhaustion | Shrink source work area to 24x24 while FULL topology stays fixed and D7 selects the mover itself. Same-report and later stale tiled-flag sends reject `focus-mismatch`; R4 rejects `partial-observation`. Observation-only catch-up accepts the ordinary source float with unchanged homing and no transfer. Direct/Operation variants are covered; no second baseline clone was needed for these fixtures. This is not a theorem for every possible future observation shape. |
| Close/feedback obligations | Actual homed PendingClosed plus feedback survive local decline; only accepted catch-up receipts acknowledge/prune the exact facts. Burned command sequence is not reused. |
| Prior flight and freshness | Existing transfer fences a new move as `transfer-in-flight`, preserving Engine state and the original transfer. Duplicate, reused and stale request checks retain core semantics; CommandConsumed adopts observation/close/feedback processing without replay. |
| Supported float/tiled/local operations | Ordinary and automatic-fixed floats complete membership-only facade arrival with identical mover frame and unchanged stay views. Fitting tiled sends and local moves adopt and continue; known local-only Operations skip the extra clone. |
| Migration and unrelated paths | Observation/focus/local-only/migration fixtures exercise the direct branch with zero extra clone; migration's existing transfer/hold is preserved, not repaired or reinterpreted. |

- **Explicit-target limitation:** `WorldCommand::Send` and Operation
  SendToOutput use `start_send`, whereas SendDirectional uses
  `start_output_send`. Explicit cross-output native targets currently return
  core `cross-domain-mismatch` in these fixtures; staging preserves that
  refusal and source state. Those rows do not prove accepted explicit-output
  capacity fallback or API equivalence. Resolving that pre-existing shared
  contract mismatch is not part of this no-core proof.
- **Verification:** five executable T2 tables plus the unchanged 55 W2 tests:
  **60 passed, 0 failed** via `mise exec -- cargo test --locked -p tiler-windows
  --lib world_diff_tests`. Full Windows library: **1,341 passed, 0 failed,
  1 ignored** via `mise exec -- cargo test --locked -p tiler-windows --lib`.
  Strict Windows all-target clippy (`--all-targets -- -D warnings`), changed
  Rust-fragment formatting, ASCII/LF and whitespace checks pass. Independent
  review accepted after consumed-observation and fixture corrections. Full-file
  formatting checks report pre-existing differences outside this
  increment; no unrelated reformatting was applied. Four-package native gates
  and physical remap/shortcut/game acceptance are not claimed.
- **Next:** complete the linked world-owner cutover's strict gates, deletion
  audit and native verification. This historical offline proof does not
  establish acceptance of the currently incomplete runtime integration.

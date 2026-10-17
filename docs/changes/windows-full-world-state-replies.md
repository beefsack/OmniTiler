# Windows full world-state reply adoption

## Goal and status

- 2026-10-18: **parked: native-to-world cross-output migration mapping is
  unresolved; core-produced migration/refill acceptance cannot run.** The
  compiling W2 prototype and diagnostic fixtures are preserved offline, not
  accepted for runtime adoption. Additive constructor and W1 capture/ordering
  fixtures delivered under the tentative selection below. The earlier W1
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

## Constructor prerequisite and tentative selection

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
- Options:
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
- Tentative selection, pending user review (2026-10-18): option 1, additive
  constructor configuration with existing constructors/defaults unchanged.
  `WorldEngine::new_with_config(core_generation: &str, config:
  WorldEngineConfig) -> Option<Self>` accepts exactly `fixed_send_admission`
  and `maximized_focus_fence`. `Default` remains `false`/`true`; Windows uses
  `true`/`false`. Fixed-size admission stays enabled in both profiles.
- No KDE, macOS or wire/protocol changes. The KDE and macOS streams should be
  told of this additive core API; notification is pending. Their existing
  constructor calls and behavior remain unchanged. No D23/13f changes included.
- Exact next action: choose the native-to-world migration mapping described
  in the parked handover below before resuming W2 acceptance.

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
    W3 must retain existing native cross-output resolution or establish its
    faithful world-command mapping, not copy KDE topology heuristics.
- W2-W4, runtime/live adoption and physical/two-output acceptance remain pending.
  The backlog item remains unimplemented until cutover and acceptance.

## W2 setter and attempt-ownership design (tentative)

- This bounded redesign keeps W2 offline and reuses native classification,
  lifetime/tag checks, `RefusedTracker` and proof-gated follow. It adds no
  shared semantics, native capability or policy owner. Core operation ids
  remain the only feedback identities; local setter-phase fences are not new
  protocol operations. Independent design review is required before code.
- Every setter requires current reply/watermark, controlled property and
  scope, resolved native binding, fresh matching lifetime/tag and live safety
  evidence. Row-local safety drift holds; identity/watermark failure stops.
  Geometry additionally requires no gesture, hidden/iconic/unreadable state,
  overlay, sticky or fit-excluded lane. A stable explicit float token does
  not bypass these gates. Preserve legitimate floating placement: ordinary
  float intent alone is not a new refusal. Only `effective_frame` is writable.
- Attempt memory is generation-scoped and pruned to live desired operations.
  Membership and view are distinct local setter phases even when a follow or
  migration uses one core operation id. Keep the existing transfer ambiguity
  fence, plus a view-attempt fence keyed by its owning core id, marked BEFORE
  dispatch. Membership arrival may settle membership but never clear a spent
  view phase. Exact target-view proof can complete a spent phase without a
  setter; otherwise it cannot dispatch again. Reset/retirement, not a repeated
  full reply or membership proof, retires phase ownership.

| Hazard | Setter permission and owner | Feedback and proof |
| --- | --- | --- |
| Float safety drift | `frame_operation` owns the explicit placement. Apply the same fresh sticky/fit-excluded/overlay and lifetime gates as tiled frames, retaining strict native eligibility. Ordinary frame refusal stays in `RefusedTracker`; explicit float is armed before its setter. | Float `Applied` needs exact readback. `RefusedProven` needs explicit refusal plus unchanged preimage. Unreadable/partial/throw stays `PossiblyExecuted`; no write means no fabricated refusal. |
| False view refusal | A retained view id owns its single view phase. Capture the immediately preceding view, fence before switching, then read the exact target output. | Target proof is `Applied`; explicit refusal AND proven unchanged preimage is `RefusedProven`; success with old/unreadable proof or unresolved throw is `PossiblyExecuted`; fresh third-location proof is `SupersededExternal`, never refusal. Feedback is queued after proof and retained through W1 acknowledgement. |
| Repeated follow switch | Parent window-transfer id owns membership and follow-view phases. Only proven arrival permits follow; do not clear view-attempt memory when arrival clears membership ambiguity. Scope/held authority and freshness still gate each possible view setter. | Emit one transfer outcome, not a synthetic view id. `Applied` requires arrival plus exact target view for follow; a possibly executed follow remains fenced. A subsequent proof can complete without another switch. Follow focus still requires live view proof and never replays an effect. |
| Held migration refill | Parent migration id owns member moves and target-view phase. Actual paired `{parent}-refill` view id separately owns refill phase. All members must arrive before target view; target view must prove before refill. Refill needs its own unheld transfer, controlled scope, resolved binding and unspent phase. | Held refill never switches or emits invented refusal. Each attempted view uses the same proof truth table. Parent cannot be declared fully applied while a required refill remains unresolved; preserve partial progress and each phase fence, without replaying successful members/views. |
| Stale focus membership | The one-shot effect owns focus and is spent before dispatch. After lifetime/safety revalidation, source membership from fresh `NativeOps::read_homing`, require that live scope controlled and `read_view(live_output) == live_workspace`, not observation-time membership. | Foreground readback alone proves focus success. Missing homing/view holds without focus. Effects emit no invented core feedback. A dispatch remains spent regardless of setter result. |

- Existing Windows comparison: `send_native.rs` rechecks lifetime tags and
  overlay snapshots before committed membership and verifies the transfer;
  `send_follow.rs` selects only after that proof, reports only verified focus
  and does not replay a failed tail. `focus_actuation.rs` revalidates live
  identities and foreground proof. `tiling/reconcile.rs` owns clamp/transient
  refusal context. Reuse these mechanisms, not a new retry policy.
- KDE comparison: `kwin/src/world-apply.ts` separates explicit float authority,
  membership arrival, exact-output view proof, migration/refill ordering and
  one-shot focus. Its stable operation/attempt model is the reference, not an
  excuse to copy a setter-return shortcut or clear a follow fence early.
  `crates/omnitiler/src/world_service.rs` provides authenticated single-flight
  discovery/report transport; settlement remains in `WorldEngine`, not service
  policy. No KDE/service edits are part of W2.
- Acceptance floor: preserve all 40 fixtures (the eight diagnostic failures
  are mandatory), add core-produced creation/binding and follow/migration
  roundtrips through capture/sequencing/feedback, and cover repeated target-view
  attempts after late membership proof plus ambiguous refill outcomes. No
  fixture weakening. If review requires a shared contract change or material
  architecture choice, stop rather than conceal that gap in the facade.
- Independent design review accepted this facade-local design. Binding
  clarifications: spend float/view phases only after every safety/authority
  gate passes; fresh focus scope follows live homing, and foreground proof
  remains authoritative even after a throwing setter. Keep distinct view-phase
  memory until operation retirement/reset, not membership settlement.
- Existing shared limitation, not a new guarantee: window-transfer settlement
  confirms membership alone (`WorldEngine::settle_retained`). Follow-view
  failure feedback cannot keep a proven-arrived transfer alive indefinitely;
  the tail is best-effort and cannot be replayed after retirement. W2 must
  prove truthful failure/no-repeat and next-report convergence, not claim
  guaranteed eventual follow. Guaranteed follow would require separately
  approved shared-contract work and is outside this change.

## W2 prototype history and blocking evidence

- W1 commit `c73e4cc7`: CI run 39046589869 completed successfully across all
  eight jobs, including the native Windows gate.
- W2 is not accepted. The preserved `world_diff.rs` facade and
  `world_diff_tests.rs` remain test-only; the legacy runtime, desktop actuation,
  recovery ledger, core/protocol semantics and other platform adapters are
  unchanged. Do not enable this prototype for W3.
- An initial implementation and its correction did not establish the required
  safety/settlement invariants. Review of the correction initially missed
  remaining hazards; focused regressions reproduced them and acceptance was
  withdrawn. The implementation and failing fixtures are preserved for diagnosis.
- Pre-redesign reproducible evidence:
  `mise exec -- cargo test --locked -p tiler-windows --lib world_diff_tests`
  reports **32 passed, 8 failed**. `git diff --check` passes. The eight failures
  cover five remaining hazards:
  - Explicit float placements write after live sticky or fit-excluded drift;
    their fresh setter gate checks native classification but omits these
    adapter safety lanes.
  - A view setter returning success with unreadable or third-location proof
    incorrectly emits `RefusedProven`; neither result proves non-execution.
  - The follow-view path can dispatch again after a possibly-executed switch;
    membership proof clears the transfer fence before the view attempt.
  - The inline migration-refill path ignores its held transfer/scope authority
    and can switch a view that must remain untouched.
  - Focus gates the observation-time homing instead of fresh native homing;
    a moved lifetime can be focused while its actual workspace is not shown.
- Earlier passing prototype gates were not accepted W2 evidence. The single
  design-first attempt and current outcome supersede that failure count below.

## Parked W2 outcome and handover

- Independent design review accepted the setter/phase-ownership design above.
  Implementation review verified all five hazard fixes but **rejected W2
  acceptance**: real Windows reports cannot produce the required migration and
  paired-refill roundtrip. No further facade redesign was made.
- Current evidence: **50 W2 fixtures pass, 0 fail**, including all 40 original
  fixtures with their assertions preserved. Formatting and a `clone_on_copy`
  helper correction do not change fixture meaning. New core-produced tests
  prove follow success/failure/no-repeat and homing settlement, view refusal,
  explicit float placement settled by re-observation, and creation/binding
  inventory reconciliation. Target-view late-proof and ambiguous-refill
  no-repeat/third-location tests remain synthetic, not migration integration.
- Exact blocker: `core_migration_refused_without_adjacency` builds two native
  outputs, managed inventories/bindings and monitor areas through W1 capture
  and sequencing. `MigrateWorkspace` returns `migration-refused`. Capture
  preserves the native domain builder's empty adjacency; both shared migration
  routes require carried adjacency (`world_commands.rs`, `start_migration`
  and `operation_migration`). No core-produced migration/refill can be tested
  on that boundary until the mapping choice is resolved. Do not synthesize
  KDE adjacency solely to turn this into a passing integration test.
- Existing native resolution is not missing: `migration.rs` pins the active
  workspace and resolves the target with `select_migration_target` over full
  monitor rectangles. What is unresolved is how that existing native choice
  maps faithfully into one world authority without importing KDE heuristics
  or preserving competing active policy paths.
- Options:
  1. Recommended: approve a bounded native-to-world mapping design reusing
     that existing Windows selector, then prove actual core-produced migration
     and refill through capture, feedback and re-observation before accepting
     W2. Keep the runtime on the legacy owner until the W3 atomic cutover.
  2. Defer world adoption and retain the working legacy Windows path. Keep
     this prototype offline as diagnostic evidence; no parity claim.
  3. Broaden the shared command contract for directly resolved targets. This
     requires separate coordinated core/protocol review and is not needed or
     authorized by this facade change; prefer option 1 if it can preserve the
     existing contract.
- Residual evidence boundaries: float clamp/throw fixtures transport synthetic
  ids and prove facade no-repeat/feedback retention, not actual retained-float
  core settlement. Follow remains best-effort under membership-only shared
  confirmation. No live smoke or physical/two-output native acceptance was
  performed. Hidden/fullscreen and tiling-bootstrap prerequisites remain.
- Preservation is releasable scaffolding, not W2 delivery: both modules remain
  `#[cfg(test)]`, including `NativeOps` and `FakeNative`. The only supporting
  capture edit exposes the unchanged classifier to sibling test modules.
  Runtime, ledger, native properties and desktop behavior are unchanged;
  no new core/protocol/KDE/macOS/dependency/unsafe edits.
- Current four-package locked build/test and strict all-target clippy pass;
  **2,695 passed, 0 failed, 1 ignored across 65 suites**. Workspace formatting,
  unsafe guardrails and `git diff --check` pass. These gates establish that the
  parked code keeps the existing release build working, not W2 acceptance.
- Exact next action: user decision on option 1 versus continued deferral;
  then design/review the mapping and resume the blocked migration/refill
  acceptance. W3 remains blocked; do not enable the facade.

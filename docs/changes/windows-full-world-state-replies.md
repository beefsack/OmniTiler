# Windows full world-state reply adoption

## Goal and status

- 2026-10-18: additive constructor prerequisite implemented under the tentative
  selection below; W1 prototype rejected after correction and withdrawn.
  Windows observation/actuation migration remains pending.
  Windows remains on its working legacy domain API. No host behavior change.
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
- Exact next action: redesign W1 from actual native capture as described below,
  using the delivered Windows constructor profile.

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

## W1 acceptance blocker and revised entry point

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
- No new product decision or core contract gap was discovered. W2-W4 and live
  smoke remain pending; no desktop mutation or restoration claim was made.

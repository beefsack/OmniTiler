# Windows full world-state reply adoption

## Goal and status

- 2026-10-18: slicing investigation complete; implementation blocked on the
  existing world constructor's platform-policy configuration. Windows remains
  on its working legacy domain API. No behavior change delivered.
- Goal: complete observed-world reports and host diffs against one full reply;
  move workspace lifecycle, scope, history and mapping policy into the retained
  in-process world state.
- Contract: [full-state D1-D5 and capacity B](archive/core-full-state-replies.md).
  Reference adapters: `kwin/src/world-*` and
  `crates/omnitiler/src/world_service.rs`.
- Preserve recovery ledger, native window properties, lifetime/gesture fences,
  refused-target suppression, follow proof, D16/D23, overlays and game safety.
  No new native capabilities, settings, polling or process boundary.
- Shared-core/protocol behavior changes are outside this change. Coordinate
  with [D23/13f delivery](shared-core-d23-rehome-and-13f-maximize-base.md),
  consuming its approved additions rather than implementing a competing rule.

## Blocking configuration gap

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
- Exact next action: approve and coordinate the additive constructor route,
  or select another option, before starting W1 production integration.

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
  unimplemented while the constructor gap blocks adoption.

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

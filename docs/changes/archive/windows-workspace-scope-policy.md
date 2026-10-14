# Windows workspace scope policy and settings

## Goal and scope

- Deliver item 12 stage A: shared pure workspace scope/order/history/ring,
  assignment and visible-elsewhere swap policy, plus Windows schema-v1 mode
  configuration and its Settings selector.
- Preserve existing core/KDE behavior. Owner/runtime integration is stage B;
  hotplug integration is stage C. Native testing is outside this delivery.

## Acceptance

- Port pure policy cases from `kwin/tests/workspace-previous-relative.test.ts`,
  including scoped isolation, stable-ID invalidation, focus-only observations,
  wrapping beyond nine and the trailing empty, assignment and swap plans.
- Missing `core.workspace.mode` backfills `per-output-local`; accept only
  `per-output-local`, `global-unique`, `shared`; cover round-trip and UI state.
- Save mode immediately; adoption on owner restart is reserved for stage B.
- Record the four tentative workspace decisions and an unresolved shared-mode
  hotplug discriminating sequence with TBD reference outcomes.
- Pass locked build/test/clippy for core, protocol, KWin effect FFI and Windows,
  workspace format and diff checks; inspect CI for other core build paths.

## Approach and bounded units

1. Add opt-in pure core policy and case-matched tests; inspect KDE/reference
   seams and expose a compact API for the Windows owner.
2. Add Windows settings/schema/UI selector and focused tests.
3. Review contracts and evidence, record decisions/reference/backlog updates,
   run final gates, reconcile concurrent upstream changes, commit and push.

## Decisions

- The mode tokens match KDE; local is the default, additive to schema v1.
- Mode changes require owner restart; current runtime mode remains in force.
- Global-unique retains scoped numbering and swaps a visible-elsewhere target.
- Shared hotplug will mirror KDE's lack of project displacement in stage C;
  unsupported reference outcomes remain TBD.

## Outcome and accepted evidence (2026-10-15)

- Delivered opt-in `tiler_core::workspace_scope` (721 production lines), with
  28 integration tests (894 lines). Existing `workspace.rs` behavior unchanged.
- Windows schema-v1 mode string defaults/strict validation, typed startup
  accessor, round-trip and three-choice Settings selector delivered; ten new
  settings/UI tests. Apply text distinguishes restart-required scope from
  live settings. Runtime adoption remains pending.
- Locked build/test/clippy passed for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`: 2,024 passed / 0 failed / 1 ignored
  across 55 suites (48 more passed than the supplied prior baseline; this
  change adds 38 tests). Workspace format and `git diff --check` passed.
- CI inspection: KWin tests build `tiler-protocol --example planner_eval`;
  that locked build also passed. Core's FFI path is covered by the requested
  locked gates. CI has no wasm build path. Linux/macOS native gates and KDE
  TypeScript execution were not run on this platform; no KDE source changed.
- Review correction: migration refill now filters stale assignments against
  live IDs and refuses a dead target current; batch observation emits a real
  transition after re-priming even when its previous ID is unchanged. Both
  have regression tests. No unresolved review findings.
- Initial fixture expectation errors were corrected against the existing
  observation/lifecycle contract; no alternate product behavior was adopted.
- Four tentative decisions recorded in `docs/decisions.md#workspaces`;
  shared-mode hotplug discriminator added with unsupported outcomes TBD.

## Core contract for Windows adoption

- `WorkspaceMode`, `DEFAULT_WORKSPACE_MODE`, `WORKSPACE_MODE_TOKENS`,
  `mode_token`; Windows `WorkspaceSettings::mode_enum()` supplies validated
  startup mode. `parse_workspace_mode` is permissive and is not validation.
- `ScopedTopology` -> `rebuild_local_mapping` / `rebuild_global_mapping`
  returns retained `mapping` and newly `adopted` IDs; `rebuild_shared_order`
  uses live native order. Empty removed-output mapping keys conservatively
  remain until displacement lifetime is known; the adapter must prune them.
- `scoped_ring_ids`, `resolve_numbered`, `resolve_relative_step`,
  `resolve_relative_send`, `resolve_trailing`; trailing takes a live-filtered
  ring. `scoped_trailing_facts` feeds existing `workspace::plan_trailing`;
  the adapter owns the global live-count floor and creation/removal effects.
- `ScopeHistory::{observe,observe_all,resolve_previous_for,prime,discard_output}`
  plus shared observation/resolution/priming methods return stable IDs and
  `PreviousTarget` results. Record only actual completed view transitions,
  including after partial focus/geometry failures if selection completed.
  Reconnect selection never reads previous history; discard/re-prime are
  explicit operations, not automatic return selection.
- `plan_global_swap` -> `SwapPlan`, then `apply_swap_assignments`; exchanged
  workspace IDs append to destination scopes, matching KDE assignment order.
- `plan_migration` / `apply_migration` accept `MigrationRequest` with live IDs,
  observed current views and a pre-mutation previous-ID snapshot. Preserve
  source current if still scoped, otherwise MRU/last-remaining refill.
  Shared refuses. These are plans only, with no HWND, focus or geometry work.

## Fixture coverage and remaining work

- Case-matched pure coverage: R-WS-08 two-view toggle, same activation,
  native/send-follow observation; R-WS-15 local/global-unique isolation and
  one shared history; R-WS-16 surviving-empty versus removed/swapped-out ID;
  R-WS-11 wrap/trailing/>9 in every mode; bad deltas and swap/assignment plans.
- The four KDE R-WS-17 displacement/return journeys remain for hotplug
  integration. Pure discard/re-prime/re-record pieces are covered. Shortcut
  catalog/entry/stop-inert fixtures and transfer effects belong to the owner.

## Stage B integration handover

- `crates/tiler-windows/src/workspace.rs:95` `ManagedWorkspaces`: replace or
  delegate duplicated history fields/helpers (:110-123, :307, :344-400,
  :439-494) through the core policy, add the startup mode and scope-aware
  assignment/active-view semantics. Cleanup :729/:744/:784 and migration
  :985/:1006 must use the same scoped contract.
- `crates/tiler-windows/src/tiling_sys.rs:31404-31409`: actual startup settings
  adoption before `sync_monitor_outputs` :31435. `apply_live_settings`
  :30936, default-live lane :30960: compare saved mode with retained startup
  mode and report restart-required drift without adopting it live.
  `settings.rs:2903` `LiveSettings`, :2929 `PollOutcome` and tray status are
  reporting seams; the saved document must retain the pending choice.
- `tiling_sys.rs:21570` `workspace_do_select`, observation :21846; selection
  routing :24679-24715; `workspace_do_send_native` :22240,
  `workspace_do_send` :22676 (relative resolver :22798); `workspace_tick`
  :24822; history operations :27827-27829; `poll_foreground_workspace`
  :28169. Cover all producers, including CLI, native foreground and verified
  send-follow, through the mode-aware policy. `chord_output` :16327 and
  `workspace_owner.rs:26` `output_context` choose the command output.
- Native membership/visibility/recovery must follow both sides of a global
  swap; shared mode needs one workspace selection/history across outputs,
  while retaining output-bound Engine domains. Retain existing failure
  semantics, tiled/floating flags, remembered focus and send follow/stay.
- `sync_monitor_outputs` :28308 and disconnect observation :28471 are stage C
  seams; do not treat pure history primitives as full hotplug delivery.
- Open questions: no unresolved pure-policy contract; tentative product
  decisions await user review. Exact shared hotplug switch/focus outcomes
  remain TBD in the discriminator. Live work follows the platform testing
  runbook.

# Windows workspace scope owner

## Goal and acceptance

- Deliver item 12 stage B using the shared scope policy: adopted startup mode,
  scoped numbered/relative/previous selection and sends, completed-transition
  history, shared ring/history, global-unique visible-workspace swaps, and
  scope-aware trailing cleanup.
- Preserve the default per-output-local behavior and existing tests. Monitor
  synchronization and displacement/return integration remain stage C.
- Keep new Windows owner logic in bounded modules, with thin runtime call sites.
- Verify locked build, tests and strict clippy for core, protocol, KWin effect
  FFI and Windows; workspace format and diff checks; bounded single-output
  native journeys with verified graceful stop and independent restoration.

## Approach and bounded units

1. Mode/state wiring and startup/settings adoption.
2. Selection and completed-transition history.
3. Follow/stay sends and migration scope compatibility.
4. Global-unique swap effects and shared output-domain selection.
5. Scoped trailing/cleanup, independent review and final offline/native gates.

## Decisions and boundaries

- Reuse the stage A contracts and tentative decisions in
  [Workspaces](../../decisions.md#workspaces).
- Retain output-bound Engine domains and existing partial-failure semantics.
- Two-output visibility, transfer and hotplug journeys require separate
  physical evidence; single-output checks cannot establish those outcomes.

## Outcome and evidence

- Delivered startup-adopted mode, saved-mode restart drift/status, scoped
  numbered/previous/relative/trailing selection and follow/stay sends through
  CLI, hook and foreground routing. History records after completed activation,
  including later focus/geometry failure; same-view/focus-only observations
  never create history. Local paths retain the legacy resolver/effect sequences
  and all existing tests pass unchanged.
- Global-unique swaps both views, appends exchanged assignments per the core
  oracle, transfers member homing, remembered focus and retained Engine layouts,
  preserves per-key floating/overlay/recovery ownership, and observes both
  completed views. Swapped-out previous IDs invalidate per the existing KDE
  regression, without recreation. Shared selects all output-bound domains onto
  one logical ID, with one ring/history; shared whole-workspace migration refuses.
- Scoped lifecycle creates one shared trailing ID, protects visible/occupied/
  displaced/in-flight/unmapped retained state, and prunes ordinary empty middle
  IDs and retired empty domains. Sticky members do not occupy every workspace.
- New modules under `crates/tiler-windows/src/` (physical lines, including blanks):
  `workspace_scope_owner.rs` 775 (658 production),
  `tiling_sys_scope_effects.rs` 1,087,
  `tiling_sys_scope_sends.rs` 185,
  `tiling_sys_scope_trailing.rs` 1,204 (531 production),
  `tiling_sys_scope_tests.rs` 1,746 and
  `tiling_sys_scope_send_tests.rs` 781. Model primitives add 286 lines to
  `workspace.rs` (2,781 total); runtime call sites add 223/remove 101 lines
  in `tiling_sys.rs` (61,902 total); `lib.rs` adds one module declaration.
  The model primitives stay beside existing membership/entry mutation APIs;
  new scope orchestration and tests live outside the oversized runtime file.
- Independent review corrected explicit swap relocation destination inversion
  and overbroad Engine-session retention. A mutation-discriminating regression
  asserts both relocation successes before the later reconcile can heal a move;
  stale-empty versus in-flight/unmapped protections are separately covered.
  Follow-up review accepted both fixes; no unresolved serious findings.
- Final native MSVC locked gates passed for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`: build, tests, strict all-target clippy,
  workspace format and diff checks. Tests before upstream integration: 2,072
  passed / 0 failed / 1 ignored across 55 result suites, including doc tests;
  48 more passed than stage A. Rebased integration `bba6317` repeated every
  gate successfully: 2,078 passed / 0 failed / 1 ignored across 55 suites.
  Six additional tests came from upstream core/protocol drag-preview diagnostics;
  relocation, session pinning and shared-placeholder contracts are unchanged.
  Windows library: 791 passed / 1 ignored. Linux/macOS native and physical
  multi-output checks are not established by these gates.

## Single-output native evidence (2026-10-15)

- Evidence binds the built artifacts to the stage A base plus this integration
  diff, with SHA-256 and exact process/creation/session identities, medium
  integrity, an identified single-output baseline and tagged disposable helpers.
  Only helper windows entered management. Full provenance remains outside the
  repository; no application content is recorded here.
- Independent hide/restore preflight: helper visible -> hidden; verified owner
  exit alone left it hidden; independent restore revealed the same helper with
  its original rectangle, then exact-identity closure and a clean ledger.
- In each startup mode, numbered selection, Previous toggle, relative wrapping,
  numbered and relative follow/stay sends, trailing selection/send and invalid
  numbered refusal have observed CLI/native evidence. Follow exposed the mover
  on its destination with foreground proof; stay retained the source view and
  hid the moved member. Focus-only observations created no workspace action.
- In each mode, changing the saved mode produced `restart-required` and the
  pending settings lane while the adopted mode stayed fixed. Saved settings
  were restored byte-for-byte after every run and at final cleanup.
- Initial shared relative/trailing send checks refused `view-changed`/`unmanaged`:
  the fixture activated a hidden helper after emptying its source, triggering a
  legitimate foreground workspace switch before dispatch. Those refusals are
  not counted as successful sends. One causal fixture repair established visible
  mover/current-view/foreground state before each request. The focused repeat
  observed successful relative follow, relative stay and trailing follow send,
  with native visibility/membership checks and no such refusals.
- Final `mise exec -- just stop` returned `stopped clean`; owner readiness false,
  zero helper actors, no live overlays/hooks, clean ledger/request state and
  byte-identical saved-settings preimage. Physical keyboard delivery and visual
  multi-output acceptance remain unproven by CLI observations.

## Tentative decision and stage C handover

- Shared startup primes from the first live output's active ID in stable-key
  order, with no fabricated previous transition. This is tentative pending
  user review; the minimal two-output startup discriminator is recorded in
  [reference outcomes](../../spec/reference-outcomes.md#windows-shared-startup-discriminator).
- `tiling_sys.rs:28387` `sync_monitor_outputs` retains its prior behavior.
  Disconnect observation/discard at `:28550`/`:28556` and reconnect observation
  at `:28618` still use local history. Stage C must wire adopted-mode mapping,
  history discard/re-prime/record and shared order/domain synchronization there;
  startup-only `tiling_sys_scope_trailing::prime_after_sync` is not a hotplug fix.
- Reuse core rebuild-local/global/shared-order policy and scope history; prune
  empty removed-output assignment keys only after displacement lifetime is known.
  Shared IDs are multi-homed: preserve output-bound Engine sessions and avoid
  the unique-source relocation arm stealing a sibling layout. Non-local cleanup
  must continue protecting displaced and actual in-flight state without pinning
  ordinary empty sessions indefinitely.
- Required physical two-output journeys: local/global isolated Previous/relative
  rings on both outputs; global visible-elsewhere swap and swap-back selection
  with distinct layouts, floats/overlays and focus; shared selection and follow/
  stay from each output while preserving both layouts and one history/trailing;
  visible/occupied cleanup protection; shared startup anchor discriminator.
- Then all R-WS-17 disconnect/reconnect journeys: active-on-removed versus
  active-on-survivor, explicit send while absent, new contents before return,
  previous invalidation/re-priming, shared no-displacement outcome, mixed-DPI
  geometry/focus continuity and clean stop/restore. Nearest-survivor discrimination
  requires three outputs. Unsupported reference outcomes remain TBD.
- Parked: stage C mode-aware hotplug and physical two-output/keyboard acceptance;
  no additional contentious product choice or unresolved stage B blocker.

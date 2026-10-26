# Platform-neutral stress acceptance

## Goal and scope

- Share high-volume send/send-follow, workspace-select, directional focus/move
  and window-churn definitions with an independently authored final-world oracle.
- Deliver a Windows fixture-only native runner and complementary core-only
  batched/reordered host-report coverage. Preserve the ordered `world_acceptance`
  baseline; add no product IPC or product-policy changes.
- Keep KDE/macOS native runners and physical shortcut/game acceptance out of scope.

## Acceptance and approach

1. Strict platform-neutral definitions specify layout, membership, focus, shown
   workspace and pager expectations; mutation tests reject incomplete evidence.
2. Core-only execution covers ordered, batched and reordered simulated host replies.
3. Windows execution uses real fixture windows and the running normal owner's
   existing exact-owner controls, with configurable inter-action delay including
   zero. Queue acceptance is not native completion.
4. Join final native frames/visibility/foreground to fixture caller identities;
   compare membership/view and actual pager renderer payload separately. Reject
   missing, stale, omitted, pending or mismatched evidence. Check intermediate
   hide/reveal at explicit checkpoints without settling every stress action.
5. Exercise multiple native delays including zero, report genuine failures without
   weakening safety or changing core policy, and verify exact cleanup/restoration.
6. Document the connector contract for later ports, run focused offline gates and
   obtain an independent review of the native lifetime/recovery boundary.

## Bounded units

- Shared definition contract, initial cases, core-only driver and rejection tests.
- Windows connector, native readbacks, pacing, offline seam tests and live runs.
- Independent review, accepted evidence, documentation and integration.

## Tentative technical decisions

- Tentative (pending user review): reuse the existing single-pending exact-owner
  request-file controls. Zero delay means no added pacing; transport availability
  may serialize publication, but per-action native convergence must not pace a burst.
- Tentative (pending user review): distinguish native frame/visibility/foreground
  observations from owner-reported membership/view and renderer-input pager
  evidence. Renderer payload evidence is not proof of pixels reaching the screen.
- Tentative (pending user review): strict JSON definitions use logical home/away
  aliases, independently authored normalized tile rectangles, explicit fixture
  focus-acquisition actions, and separate retained focus versus nullable actual
  foreground. Initial cases cover one tiled zero-gap output, 102 commands each.
- Tentative (pending user review): ordered, batched and split/reordered simulated
  host receipts execute the same definitions. Native convergence is checked only
  at explicit observation checkpoints, not after each stress command.

## Evidence and outcome

- Existing ordered baseline and native connector limitations inspected. Prior
  withdrawn connector drafts are not reused as accepted evidence.
- Current `mise exec -- cargo test --locked -p tiler-protocol` passes 29 library
  tests (27 stress tests) plus 33 codec tests. Both cases accept all schedules;
  report counts are duo 186/149/151 and trio 189/155/157 for
  ordered/batched/reordered. These are simulated-host proofs only.
- Current `mise exec -- just world-stress-check`, `world-stress-mock` and
  `git diff --check` pass. The PowerShell checks do not establish runtime
  interface fidelity or safe recovery.
- Strict all-target `tiler-protocol` clippy and workspace formatting checks pass.
- Initial native connector review rejected invented freshness values, absent
  bootstrap windows, width/edge confusion, per-command convergence and
  success-only cleanup. A source-informed correction addressed those defects,
  but remaining recovery and evidence defects below still block execution.
- No native owner/fixture was launched. Delay 0 and nonzero live runs are all
  unperformed. No product defect or native acceptance is claimed.
- Offline checkpoint: both dispatch and the live journey entry mechanically
  refuse `-Live` before any effects. The seam check executes this refusal;
  the draft connector is preserved but cannot launch until repaired.

## Blocking connector defects

- Recovery must be effect-aware and lifetime-bound. Settings are currently
  modified before the pre-existing-owner refusal. Cleanup can then write settings
  or call restore without a run-owned owner binding; its stored owner identity
  is not revalidated against the target before stop. The separate `-Stop` route
  catches cleanup failure and exits zero. Startup timeout lacks a complete
  bound-owner recovery path. Do not execute this draft live.
- Per-command dispatch joins poll at 400ms, masking the intended unpaced burst;
  command publication/consumption must not add a hidden convergence-sized gap.
- The connector pins the initial away workspace id while the shared core driver
  rebinds that alias after empty-workspace pruning. A later ring refresh currently
  refuses legitimate alias lifecycle changes.
- Checkpoint joins take the first fresh row and reject pending work immediately
  rather than awaiting bounded settlement. Pager emission is change-gated, but
  the connector requires a newly emitted row at every accepted revision.
- Pager miniature geometry and exact miniature focus identity are not observed;
  current marks/count checks cannot establish the requested pager layout oracle.
- Native settings/output targeting, physical geometry scaling and fresh
  HWND/process-lifetime joins still need end-to-end validation against the
  actual configured owner before native evidence can be accepted.

## Next acceptance step

- Repair the recovery invariant first with injected failures before launch,
  after settings configuration, during owner readiness and after command
  dispatch. Verify no foreign-owner stop/restore or settings writes occur.
- Reconcile the production-generated trace and fixture schemas, alias lifecycle,
  unpaced publication and change-gated pager payload evidence; then review those
  invariants before fixture-only live runs at zero and multiple nonzero delays.
- Keep this note active and the backlog open. The current implementation has
  unresolved correctness and recovery defects and is not native acceptance.

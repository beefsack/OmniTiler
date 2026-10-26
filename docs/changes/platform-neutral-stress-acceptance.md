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
- Tentative (pending user review): persist recovery intent before settings or
  launch effects in the existing leg file; revalidate exact owner/fixture
  lifetimes before recovery and preserve foreign owners and global settings.
- Tentative (pending user review): fence both control request files for ordered
  publication, join dispatch outcomes separately at observation barriers, and
  retry native/pager lag only within a bounded checkpoint. Corrupt evidence,
  identity failures and consumed transport/settlement deadlines fail closed.
- Tentative (pending user review): retain the latest emitted pager payload by
  revision and emission order, including same-revision changes. Extend existing
  trace rows with DPI, cell/miniature rectangles and opaque window tokens;
  independently project authored tiles with the renderer's rounding contract.
- Tentative (pending user review): keep home at ring head and rebind away to
  ring position two after empty-workspace pruning, matching the core driver.
- Tentative (pending user review): seed hidden snapshot caches from fresh
  exact-lifetime physical frame proof at successful hide commit; never from
  desired geometry or setter acceptance. Unreadable proof retains the existing
  conservative fallback. Core frame-evidence precedence remains unchanged.

## Evidence and outcome

- Existing ordered baseline and native connector limitations inspected. Prior
  withdrawn connector drafts are not reused as accepted evidence.
- Current `mise exec -- cargo test --locked -p tiler-protocol` passes 29 library
  tests (27 stress tests) plus 33 codec tests. Both cases accept all schedules;
  report counts are duo 186/149/151 and trio 189/155/157 for
  ordered/batched/reordered. These are simulated-host proofs only.
- Current `mise exec -- just world-stress-check`, `world-stress-mock` and
  `git diff --check` pass. Offline evidence does not establish native acceptance.
- Strict all-target `tiler-protocol` clippy and workspace formatting checks pass.
- Initial native connector review rejected invented freshness values, absent
  bootstrap windows, width/edge confusion, per-command convergence and
  success-only cleanup. Source-informed corrections addressed those defects;
  current remaining acceptance is real execution, not relaxed oracles.
- Fixture-only native runs at delay zero reached the hide/reveal checkpoints
  but no complete case passed. Nonzero delays and the trio case remain unrun.
- The connector was enabled after independent recovery/oracle review. It is
  now mechanically parked before effects at both entries pending the hidden
  frame decision below; the offline seam check executes the refusal.
- Recovery repair passes 65 offline fault steps, including actual cleanup
  calls under effect shadows for partial settings writes, unready startup,
  post-dispatch failure, foreign owner, same-executable PID reuse, gone fixture,
  malformed ledger and nonzero detached stop failure. Independent review
  accepted the bounded repair after correcting fail-open ready queries and
  the mock's stop-versus-restore ledger lifetime. Live timing remains unproven.
- Native evidence repair adds real JSON roundtrip faults, strict omission
  checks, ordered cross-family transport fencing, independent dispatch/apply
  cursors, fresh native lifetime joins and a real checkpoint-loop simulation
  spanning pending intent, settled desire, native lag and late pager emission.
- Miniature faults reject duplicates, omitted nonfocused windows, swapped
  positions and inside-cell layout errors. Midpoint cases distinguish Rust's
  away-from-zero rounding from PowerShell's default ties-to-even rounding.
- Windows library tests pass 1071 cases, including 39 pager tests and four
  hidden-snapshot regressions; core tests pass 925 cases. Strict
  clippy and formatting pass. Trace remains bounded and omits native identity.
- Live interface corrections: bounded owner duration now honors the product's
  600-second maximum; concurrent trace reads tolerate only a torn final line;
  directional dispatch uses `dispatched`, unlike workspace dispatch's `ok`.
- Native testing found and repaired a Windows stale hidden-frame cache bug:
  a same-pass exact write readback was superseded by an older cached fact,
  creating a permanent constraint-clamp hold. Fresh hide-commit snapshot seeds
  preserve the existing core contract. Independent review accepted the repair;
  two subsequent native runs passed `hide-F1` and `show-F1`.
- Two further harness corrections have discriminating offline regressions:
  re-settle after pager/ring revision lag and retain the last dispatch tick
  across explicit focus observations. Neither changes the expected world.
- Latest delay-zero run fails `hide-S1`: the sent hidden member remains at
  its prior half-width native frame while its logical destination allocation
  is full width. No frame write is attempted after hiding, consistent with
  `world_diff.rs`'s hidden-window safety fence. This is an unresolved oracle
  contract question, not a claimed second product defect.
- All native attempts ended with independently checked owner/fixture and
  request/ledger absence and settings restored byte-for-byte. Recovery baseline
  checks passed. One earlier run received a stop request of unknown origin;
  it is not acceptance evidence.
- Delay-zero sample: control publication took 43-93ms per command, with zero
  transport queue wait in that sample. CLI process overhead limits this native
  route; zero delay does not mean physical keyboard-rate ingress.

| Case | Delay (ms) | Result |
| --- | --- | --- |
| home-away | 0 | No full pass; hide-F1/show-F1 pass after cache fix; hide-S1 blocked on hidden-frame oracle |
| home-away | 10, 50 | Not run after the contract blocker |
| trio-churn | 0, 10, 50 | Not run after the contract blocker |

## Remaining native acceptance

- Recovery now refuses before settings mutation, persists effect intent, binds
  owner and fixture lifetimes, fails nonzero on incomplete recovery and preserves
  foreign owners. Native stop/restore and cleanup passed the attempted legs;
  injected failure recovery remains bounded offline evidence.
- Dispatch is joined at observation barriers, not per command. Transport-only
  fencing preserves order across the two existing control families. Measured
  zero-delay timing includes CLI overhead, not per-command native settlement.
- Alias rebinding, bounded settlement and retained change-gated pager evidence
  now have offline coverage; verify them against the actual owner and renderer.
- Pager miniature geometry and exact focus identity are observed in the existing
  trace and compared to independent expected tiles; renderer-input evidence
  remains distinct from on-screen pixel proof.
- Native settings/output targeting, physical geometry scaling and fresh
  HWND/process-lifetime joins still need end-to-end validation against the
  actual configured owner before native evidence can be accepted.

## Blocking decision and next acceptance step

- Decide whether hidden native frames must immediately equal logical allocation.
  The runner currently requires equality; the adapter deliberately forbids
  automatic hidden-window frame writes. Changing that fence is not authorized
  by this test work. Recommendation: distinguish logical allocation from hidden
  native geometry, assert hidden identity/visibility and retained frame proof,
  and require exact tile geometry on reveal. This recommendation is not adopted.
  [Minimal discriminator](../spec/reference-outcomes.md#hidden-frame-stress-discriminator).
- After that decision, revise the oracle or product only as authorized, review
  the correction, and resume both cases at delays 0, 10 and 50.
- Keep this note active and the backlog open until real execution establishes
  native acceptance or reports a concrete tiler defect.

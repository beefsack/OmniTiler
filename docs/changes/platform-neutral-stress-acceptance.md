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
- Tentative (pending user review): hidden-window acceptance checks exact
  identity, hidden visibility and retained frame proof. Require exact native
  geometry against logical allocation only on reveal. This follows the
  existing adapter safety fence against hidden frame writes; product behavior
  and that safety fence remain unchanged.
- Tentative (pending user review): expose exact raw miniature source frames in
  the existing bounded pager trace. Hidden native frames must match that
  token/revision-bound retained proof; miniature quantization alone cannot
  establish exact retained geometry. Track stability per hide epoch, reset on
  actual reveal, close or replacement lifetime.
- Tentative (pending user review): emit the existing bounded accepted-world
  trace snapshot when fresh pager content or foreground changes, even without
  an actuation attempt. Unchanged/duplicate presentations remain quiet. Retain
  strict renderer/world revision joins; never mix a newer pager with old world
  proof. Final re-verification inspects any newer world progress before reuse.
- Tentative (pending user review): explicitly focus the bootstrap anchor before
  the trio case's first admission. This removes unspecified bootstrap foreground
  without changing authored layouts, product admission policy or safety gates.
- Tentative (pending user review): fence observation checkpoints on exact
  lifetime-bound admission ticks as well as command-dispatch ticks. Reject
  missing caller/token mappings before roster sorting; never drop them silently.

## Evidence and outcome

- Existing ordered baseline and native connector limitations inspected. Prior
  withdrawn connector drafts are not reused as accepted evidence.
- Current `mise exec -- cargo test --locked -p tiler-protocol` passes 30 library
  tests (28 stress tests) plus 33 codec tests. Both cases accept all schedules;
  report counts are duo 186/149/151 and trio 190/156/158 for
  ordered/batched/reordered. These are simulated-host proofs only.
- Current `mise exec -- just world-stress-check`, `world-stress-mock` and
  `git diff --check` pass. Offline evidence does not establish native acceptance.
- Strict all-target `tiler-protocol` clippy and workspace formatting checks pass.
- Initial native connector review rejected invented freshness values, absent
  bootstrap windows, width/edge confusion, per-command convergence and
  success-only cleanup. Source-informed corrections addressed those defects;
  current remaining acceptance is real execution, not relaxed oracles.
- Fixture-only native runs reached hide/reveal checkpoints, but no complete
  native pass is accepted. Latest strict results are tabulated below.
- The original connector was parked for the hidden-frame decision below.
  The revised oracle now passes independent review for native validation:
  exact raw renderer source frames replace the rejected self-seeded hidden
  frame draft, with sub-quantum corruption and hide-lifecycle regression tests.
- Initial revised-oracle runs produced apparent home-away passes, but review
  rejected a runner fallback that joined newer pager evidence to an older world
  row. Those passes are withdrawn. The fallback is removed; fresh pager-content
  changes now trigger accepted-world trace rows, but native reruns still expose
  a proof gap: an unchanged subsequent accepted reply can stamp the pager with
  a newer revision before the next pump. Final idle re-verification rejects newer
  pending/corrupt progress. No future-pager/old-world fallback remains.
- Initial trio runs at each delay exposed an unspecified bootstrap admission
  anchor. An explicit existing focus observation now pins that precondition;
  authored expected layouts are unchanged, with exactly one extra core report.
- Recovery repair passes 65 offline fault steps, including actual cleanup
  calls under effect shadows for partial settings writes, unready startup,
  post-dispatch failure, foreign owner, same-executable PID reuse, gone fixture,
  malformed ledger and nonzero detached stop failure. Independent review
  accepted the bounded repair after correcting fail-open ready queries and
  the mock's stop-versus-restore ledger lifetime. All attempted native recoveries
  passed; injected failure coverage remains offline evidence.
- Native evidence repair adds real JSON roundtrip faults, strict omission
  checks, ordered cross-family transport fencing, independent dispatch/apply
  cursors, fresh native lifetime joins and a real checkpoint-loop simulation
  spanning pending intent, settled desire, native lag and late pager emission.
- Miniature faults reject duplicates, omitted nonfocused windows, swapped
  positions and inside-cell layout errors. Midpoint cases distinguish Rust's
  away-from-zero rounding from PowerShell's default ties-to-even rounding.
- Current Windows library tests pass 1074 cases (one ignored), including four
  hidden-snapshot regressions plus exact raw-source and presentation trace
  tests; core tests pass 925 cases. Strict
  clippy and formatting pass. Trace remains bounded and omits native identity.
- Latest PowerShell seam check and 171 offline fault steps pass, including
  strict revision rejection, final reuse/newer-progress rejection and the
  admission fence. Both live entries execute a pre-effect parked refusal.
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
- Before the revised oracle, delay-zero testing failed `hide-S1`: the sent hidden member remained at
  its prior half-width native frame while its logical destination allocation
  is full width. No frame write is attempted after hiding, consistent with
  `world_diff.rs`'s hidden-window safety fence. The tentative oracle decision
  below resolves this mismatch without a product change.
- Strict trio runs then exposed a stale pre-admission world-row join. The
  harness now fences bootstrap and subsequent admission observations using the
  exact token/HWND/PID/creation-bound event tick; checkpoint tests prove the
  pre-admission row cannot pass. The repaired fence has not run live yet.
- All native attempts ended with independently checked owner/fixture and
  request/ledger absence and settings restored byte-for-byte. Recovery baseline
  checks passed. One earlier run received a stop request of unknown origin;
  it is not acceptance evidence.
- Delay-zero sample: control publication took 43-93ms per command, with zero
  transport queue wait in that sample. CLI process overhead limits this native
  route; zero delay does not mean physical keyboard-rate ingress.

| Case | Delay (ms) | Latest strict result | Publication sample |
| --- | --- | --- | --- |
| home-away | 0 | Fail at hide-S1: pager/world proof gap | 5 commands / 1.455s = 3.436/s |
| home-away | 10 | Same proof-gap failure | 5 commands / 1.690s = 2.959/s |
| home-away | 50 | Same proof-gap failure | 5 commands / 1.933s = 2.587/s |
| trio-churn | 0 | Fail at admit-3: stale pre-admission proof; repaired offline | No product commands reached |
| trio-churn | 10 | Same admission-fence failure; repaired offline | No product commands reached |
| trio-churn | 50 | Not rerun after the same deterministic fence failure twice | No strict result with revised oracle |

- These are partial-run rates, including fixture focus/checkpoint overhead,
  not sustained burst rates. The zero-delay contiguous-command interval was
  101.231ms (about 9.88 actions/s); the 43-93ms CLI service sample above is a
  separate earlier measurement. This route cannot prove true unpaced input
  starvation resistance. No persistent product-control route was found and
  no new product IPC was added.
- An earlier 10ms run left a workspace transfer pending for 60s with one
  fixture member still visible. The hide race is backlogged, not silently
  repaired or classified as a passing retry. Later strict runs stop earlier
  and do not reproduce or disprove it.

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

## Hidden-frame decision and next acceptance step

- Tentative (pending user review): adopt hidden identity/visibility and retained
  frame proof, with exact geometry against logical allocation checked on reveal.
  Do not change product behavior or weaken the hidden-window write fence.
  [Minimal discriminator](../spec/reference-outcomes.md#hidden-frame-stress-discriminator).
- The oracle correction is applied and independently reviewed. Resume both
  cases at delays 0, 10 and 50 only after closing the proof-binding gap below.
- Existing transport inspection found no persistent product control route.
  Fixture batching controls fixture windows only. Direct request-file writes
  would skip the CLI's same-path medium-caller gate, so they are not used.
  Retain the existing CLI route and report its measured effective rate; a
  faster persistent exact-owner product transport requires separate approval.
- Keep this note active and the backlog open until real execution establishes
  native acceptance or reports a concrete tiler defect.

## Remaining proof-binding blocker

- Latest strict home-away runs all show an accepted world row at revision 39
  and a three-cell pager payload at revision 40 on the same owner tick, with
  no world row for revision 40 or later. The strict runner correctly refuses
  the newer payload against old world proof; the two-cell retained payload
  cannot describe the three-workspace ring.
- Two approaches have failed: widening the pager cap was rejected for mixing
  unbound evidence; logging only changed fresh pager presentations still misses
  a later unchanged accepted reply's stamp before the renderer pump. Preserve
  the strict cap; the native runner is mechanically parked before effects.
- Recommendation, not adopted: emit an exact accepted-world proof paired to
  the snapshot actually used whenever the pager payload trace emits, retaining
  the corresponding report/reply provenance. This closes observability without
  changing tiling or hidden-write safety, and avoids logging every accepted
  observation. The required snapshot lifetime/correlation design needs review
  before another implementation attempt. No new product IPC is proposed.
- Next: resolve that trace-proof design, verify admission fencing live, then
  rerun all six legs and the incomplete-hide regression. Full native acceptance
  and true burst-rate starvation coverage remain open.

# Platform-neutral stress acceptance

## Closing outcome (2026-10-27)

- Windows scope complete: both unchanged 102-command definitions pass native
  execution at 0, 10 and 50ms, with strict layout/membership/view/focus/pager
  joins and independently verified cleanup/settings restoration. Shared
  ordered/batched/reordered simulated-host schedules and native offline gates
  pass. KDE/macOS native connectors and physical-input acceptance remain out
  of scope; CLI publication overhead is not keyboard-rate evidence.
- The final product repair refreshes directional CLI origins from an actual
  observation before the early foreground lookup. Ownership, suspension,
  elevation, scope, lifetime, foreground and downstream freshness checks remain
  unchanged. No waits, timers, settlement delays, admission/tag effects, core
  changes or new public surface were added.
- Regression first failed with stale-cache `unmanaged`, then passed with the
  freshly bound origin. Six new tests cover refresh, foreign foreground,
  observation failure without stale fallback and pre-observation refusal
  fences. The gate regression does not apply focus downstream; the full live
  matrix supplies that end-to-end evidence. Independent review accepted the
  repair after correcting cache-side-effect and test-coverage comments.
- Final gates: native core/protocol/Windows build and 2928 passing tests (one
  ignored), strict all-target clippy, workspace fmt, stress seam check plus
  104 definition-walk and 197 fault steps, live-library seam check plus 45
  mock checks, and clean diff checks. The Windows library contributes 1085
  passing tests. Applicable Linux evidence for earlier shared work remains
  separate; this final repair changes Windows-only source.
- The earlier intermittent incomplete-hide failure is not reproduced by the
  corrected home-away runs or its separate 10ms replay; it remains backlogged.
  The following history preserves withdrawn evidence and intermediate failures;
  the closing outcome and latest matrix below supersede those status claims.

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

## Approved technical decisions

- User-approved 2026-10-27: reuse the existing single-pending exact-owner
  request-file controls. Zero delay means no added pacing; transport availability
  may serialize publication, but per-action native convergence must not pace a burst.
- User-approved 2026-10-27: distinguish native frame/visibility/foreground
  observations from owner-reported membership/view and renderer-input pager
  evidence. Renderer payload evidence is not proof of pixels reaching the screen.
- User-approved 2026-10-27: strict JSON definitions use logical home/away
  aliases, independently authored normalized tile rectangles, explicit fixture
  focus-acquisition actions, and separate retained focus versus nullable actual
  foreground. Initial cases cover one tiled zero-gap output, 102 commands each.
- User-approved 2026-10-27: ordered, batched and split/reordered simulated
  host receipts execute the same definitions. Native convergence is checked only
  at explicit observation checkpoints, not after each stress command.
- User-approved 2026-10-27: persist recovery intent before settings or
  launch effects in the existing leg file; revalidate exact owner/fixture
  lifetimes before recovery and preserve foreign owners and global settings.
- User-approved 2026-10-27: fence both control request files for ordered
  publication, join dispatch outcomes separately at observation barriers, and
  retry native/pager lag only within a bounded checkpoint. Corrupt evidence,
  identity failures and consumed transport/settlement deadlines fail closed.
- User-approved 2026-10-27: retain the latest emitted pager payload by
  revision and emission order, including same-revision changes. Extend existing
  trace rows with DPI, cell/miniature rectangles and opaque window tokens;
  independently project authored tiles with the renderer's rounding contract.
- User-approved 2026-10-27: keep home at ring head and rebind away to
  ring position two after empty-workspace pruning, matching the core driver.
- User-approved 2026-10-27: seed hidden snapshot caches from fresh
  exact-lifetime physical frame proof at successful hide commit; never from
  desired geometry or setter acceptance. Unreadable proof retains the existing
  conservative fallback. Core frame-evidence precedence remains unchanged.
- User-approved 2026-10-27: hidden-window acceptance checks exact
  identity, hidden visibility and retained frame proof. Require exact native
  geometry against logical allocation only on reveal. This follows the
  existing adapter safety fence against hidden frame writes; product behavior
  and that safety fence remain unchanged.
- User-approved 2026-10-27: expose exact raw miniature source frames in
  the existing bounded pager trace. Hidden native frames must match that
  token/revision-bound retained proof; miniature quantization alone cannot
  establish exact retained geometry. Track stability per hide epoch, reset on
  actual reveal, close or replacement lifetime.
- User-approved 2026-10-27; implemented and exercised live: whenever the Windows pager payload
  trace emits, emit exact accepted-world proof paired to the snapshot actually
  used, with corresponding report/reply provenance. Retain strict renderer/world
  joins and final re-verification of newer world progress. Review snapshot
  lifetime/correlation design before implementation; no product behavior change.
- User-approved 2026-10-27: explicitly focus the bootstrap anchor before
  the trio case's first admission. This removes unspecified bootstrap foreground
  without changing authored layouts, product admission policy or safety gates.
- User-approved 2026-10-27: fence observation checkpoints on exact
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
- Fixture-only native runs now complete both cases at all three delays after
  the directional-origin freshness repair. Latest strict results are below.
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
- Windows library tests now pass 1085 cases (one ignored), including four
  hidden-snapshot regressions plus exact raw-source and presentation trace
  tests; core tests pass 925 cases. Strict
  clippy and formatting pass. Trace remains bounded and omits native identity.
- Latest PowerShell seam check and 197 offline fault steps pass, including
  strict revision rejection, final reuse/newer-progress rejection and the
  admission fence and paired accepted-world proof. The temporary proof-binding
  parking refusals are removed; explicit live flags and pre-effect range gates remain.
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
  `world_diff.rs`'s hidden-window safety fence. The approved oracle decision
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

| Case | Delay (ms) | Prior strict result before paired proof | Publication sample |
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
  no new product IPC was added. User 2026-10-27 accepts the measured CLI rate
  for now: about 3.4 actions/s overall and 101ms between contiguous commands.
- An earlier 10ms run left a workspace transfer pending for 60s with one
  fixture member still visible. The hide race is backlogged, not silently
  repaired or classified as a passing retry. Later strict runs stop earlier
  and do not reproduce or disprove it.

### Paired-proof implementation and latest validation (2026-10-27)

- Retain one bounded accepted `world-apply` projection with the original
  report geometry/provenance, reply identity, delivery and tick. Replace it
  only on a fresh Applied acknowledgement beside the accepted reply; rejected,
  stale and duplicate deliveries do not relabel retained proof. Trace-disabled
  fresh acceptance clears proof rather than carrying stale evidence forward.
- The synchronous pager pump carries the exact rendered reply's request,
  observation and revision. When its existing payload change gate emits, it
  emits the retained proof only on equality of that full triple, marked
  `paired: true`. No regeneration from newer reports or caches, no revision
  cap widening, and no extra emission on quiet pumps. The failed
  presentation-only forced logging is removed. Independent review accepted
  the design; a request-key check and real-reply capture/pump regression
  address its minor findings.
- Offline coverage includes unchanged revision 39/40 content, real successive
  accepted snapshots, retained-proof replay isolation, request mismatch,
  same-revision payload changes, quiet/trace-off gates and honest omissions.
  Runner joins and final newer-progress re-verification remain unchanged.
- A stale seam check invoked `-Live` expecting the former parked refusal;
  after unparking, that unexpectedly launched a focus-acquisition failure.
  Cleanup passed; the check now inspects parking statically and cannot launch
  a native journey. That aborted run is not acceptance evidence.
- Initial paired-proof live attempts both stopped before any product command:
  fixture focus acquisition failed while an out-of-scope fullscreen elevated
  foreground caused owner suspension and pager suppression. A standalone
  fixture focus readback succeeded, so no fixture-focus defect is established.
  No foreign-window actions or safety bypass were used. Both attempts ended
  with owner/fixture absence, clean control/recovery state and byte-identical
  settings restoration. Detailed evidence remains outside the repository.

| Case | Delay (ms) | Latest paired-proof native result |
| --- | --- | --- |
| home-away | 0 | Pass: 102 commands, 16 checkpoints plus final |
| home-away | 10 | Pass: 102 commands, 16 checkpoints plus final |
| home-away | 50 | Pass: 102 commands, 16 checkpoints plus final |
| trio-churn | 0 | Pass: 102 commands, 19 checkpoints plus final |
| trio-churn | 10 | Pass: 102 commands, 19 checkpoints plus final |
| trio-churn | 50 | Pass: 102 commands, 19 checkpoints plus final |
| incomplete-hide replay (home-away) | 10 | Pass: all hide/reveal checkpoints plus final; earlier race not reproduced |

- After the foreground precondition cleared, the unchanged trio case exposed
  a native bootstrap precondition gap: the owner retiled the initial pair
  opposite to authored `initial_facts`. The connector had checked roster/view
  but not initial allocations. The runner now verifies exact authored tiles
  against latest settled proof and fresh identity-bound native frames before
  the action loop. An exactly proven pairwise swap permits one existing
  exact-owner directional move and a strict synthetic setup checkpoint; exact
  layout issues no setup command, all other shapes refuse. Setup is excluded
  from the 102 commands and publication timing; definitions/oracles are unchanged.
  Every latest leg used one setup move. Independent review and orchestrator
  shadows cover quiet-proof reuse, newer-progress rejection, native lag,
  identity refusal, single-move ordering and count separation.
- Trio next exposed a departure-reader cursor race: `Wait-StressDeparted`
  scanned one log snapshot but advanced to a second read's count, skipping
  concurrently appended final departure rows. Owner proof was already clean
  within milliseconds, not after the 30-second deadline. Advancing only to
  the inspected snapshot's count fixes the race without changing deadlines.
  A deterministic mid-poll append test fails before the fix and passes after;
  pending, corrupt-omission and lingering-title faults remain strict.
- Latest passing legs verify original report/reply-bound paired proof,
  retained-older pager payloads, bounded native/pager lag resettlement and final
  newer-progress re-verification. Trio admission fencing now passes live.
  Each completed or failed leg independently ended with owner/fixture absence,
  clean request/recovery state and byte-identical settings restoration. No
  safety gates were bypassed. The subsequent directional CLI freshness repair
  below is the only final product change.

- Final native offline gates pass: build and tests for core/protocol/Windows
  (2928 passed, one ignored), strict all-target clippy and workspace fmt;
  stress seam check, 104 definition-walk steps and 197 fault steps;
  live-library seam check and 45 mock checks; `git diff --check` clean.
  Observed toolchain: Rust/Cargo 1.99.0, Windows MSVC. These complement, not
  replace, the six complete native passes. Applicable Linux gates for earlier
  shared work remain separate from this Windows-only repair.
- The separate incomplete-hide replay repeats the unchanged home-away case at
  10ms; no standalone reduced reproducer exists. It passed, but non-reproduction
  does not close the earlier intermittent-hide backlog item.

### Zero-delay directional origin defect and repair

- User-approved 2026-10-27: reproduce the verified-focus/stale-origin sequence
  with a failing regression, repair Windows origin freshness after reveal,
  preserve all safety/freshness contracts, then run native gates and all six
  stress legs. No waits, timers, settlement delays, core policy change or new
  public surface.
- The pre-repair trio zero-delay run passed authored bootstrap, admission,
  close/re-admit and send/hide/reveal checkpoints, then failed joining command
  sequence 31: directional focus reports `unmanaged` at tick 94. Twenty-four
  stress commands were queued before the next observation drained that refusal.
- The trace establishes a refused view attempt at tick 88, accepted hide/view
  at tick 90 and an empty visible observation at tick 91. Explicit fixture
  focus then triggers the existing hidden-foreground workspace return path.
  Tick 93 accepts the return to home and proves focus on the fixture lifetime;
  7ms later tick 94 rejects directional focus, before the next observation
  republishes visible managed origins. Tick 97 accepts the same lifetime again.
- Source inspection finds the early CLI origin lookup reads only
  `snap_origins` in `tiling_sys/cli_directional.rs`, while the foreground return
  in `tiling_sys/output_sync.rs` can reveal and focus before that cache is
  refreshed. The verified focus followed by cache-only `unmanaged` refusal is
  a concrete product freshness defect; the precise earlier hide-refusal gate
  is not identified by the current trace.
- Do not mask this with per-action settlement or weaker foreground/visibility
  gates. The harness focus observation checks actual foreground but does not
  fence view settlement; adding that fence changes the current stress pacing
  contract. The product repair instead takes a fresh observation and calls
  `publish_managed` before the directional CLI's early origin lookup. Cache
  refresh performs no native writes or membership/tag mutation; admission
  remains in the unchanged downstream dispatch. Observation failure refuses
  without consulting stale origins. Production foreground reads are unchanged;
  the existing test-aware wrapper makes the gate regression scriptable.
- `directional_cli_refreshes_stale_origins_after_reveal` fails on pre-fix code
  (`unmanaged` instead of `dispatched`) and passes after the repair. Foreign
  foreground, observation failure, suspension, elevation and proof-owner tests
  preserve refusal ordering and no-actuation boundaries. The complete
  directional CLI test group passes 10 tests, including six new regressions.
- Latest trio zero-delay passes all 102 commands and 19 checkpoints plus final;
  all 51 directional CLI dispatches carry origins, with zero `unmanaged`
  outcomes. The other five matrix legs also pass. One strict synthetic
  bootstrap checkpoint and at most one setup move remain separate from each
  case's command count and publication timings.

## Native acceptance evidence and limits

- Recovery now refuses before settings mutation, persists effect intent, binds
  owner and fixture lifetimes, fails nonzero on incomplete recovery and preserves
  foreign owners. Native stop/restore and cleanup passed the attempted legs;
  injected failure recovery remains bounded offline evidence.
- Dispatch is joined at observation barriers, not per command. Transport-only
  fencing preserves order across the two existing control families. Measured
  zero-delay timing includes CLI overhead, not per-command native settlement.
- Alias rebinding, bounded settlement and retained change-gated pager evidence
  pass offline and in the complete native legs against the owner and renderer.
- Pager miniature geometry and exact focus identity are observed in the existing
  trace and compared to independent expected tiles; renderer-input evidence
  remains distinct from on-screen pixel proof.
- Native settings/output targeting, physical geometry scaling and fresh
  HWND/process-lifetime joins pass in all six complete matrix legs. This is
  fixture-only CLI acceptance, not physical shortcut, game or pixel proof.

## Hidden-frame decision and next acceptance step

- User-approved 2026-10-27: adopt hidden identity/visibility and retained
  frame proof, with exact geometry against logical allocation checked on reveal.
  Do not change product behavior or weaken the hidden-window write fence.
  [Minimal discriminator](../../spec/reference-outcomes.md#hidden-frame-stress-discriminator).
- The oracle correction, paired-proof repair and directional freshness repair
  are applied, independently reviewed and exercised live. Do not reintroduce
  earlier proof-mixing approaches or hide failures with settlement delays.
- Existing transport inspection found no persistent product control route.
  Fixture batching controls fixture windows only. Direct request-file writes
  would skip the CLI's same-path medium-caller gate, so they are not used.
  User-approved 2026-10-27: two modes share definitions and oracles. Retain the
  existing CLI for deterministic correctness; lower-priority keyboard mode uses
  injected real shortcuts through the hook path plus the test window tool for
  churn, covering real-input bursts. First check that the tiler accepts injected
  shortcuts and every stress action has a shortcut. About 100ms/action already
  exceeds typical human input speed. No batch file/stdin CLI is planned; it is
  a possible follow-up only if keyboard mode cannot cover a case, requiring user
  approval as a new public CLI feature.
- Windows native acceptance is established by the closing matrix. Other
  platform runners, keyboard ingress and intermittent-hide reproduction remain
  separate backlog items.

## Proof-binding repair and final acceptance

- Before the paired-proof repair, strict home-away runs all showed an accepted
  world row at revision 39
  and a three-cell pager payload at revision 40 on the same owner tick, with
  no world row for revision 40 or later. The strict runner correctly refuses
  the newer payload against old world proof; the two-cell retained payload
  cannot describe the three-workspace ring.
- Two approaches have failed: widening the pager cap was rejected for mixing
  unbound evidence; logging only changed fresh pager presentations still misses
  a later unchanged accepted reply's stamp before the renderer pump. Preserve
  the strict cap; the temporary native parking refusal is now removed.
- User-approved 2026-10-27; implemented and verified offline and live: emit exact
  accepted-world proof paired to
  the snapshot actually used whenever the pager payload trace emits, retaining
  the corresponding report/reply provenance. This closes observability without
  changing tiling or hidden-write safety, and avoids logging every accepted
  observation. Snapshot lifetime/correlation received independent review and
  exact request/observation/revision regression coverage. No new product IPC.
- The verified-focus versus stale CLI-origin-cache race is repaired and all
  six native legs pass. Next for this Windows scope: none. Other-platform
  connectors and true input-burst starvation coverage remain separate work.

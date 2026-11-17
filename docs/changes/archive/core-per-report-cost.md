# Core per-report cost

## Goal and scope

Reduce steady-state `WorldEngine::handle_report` cost without changing behavior,
contract bytes, settlement rules or fixtures. Remove superlinear reconciliation
work and skip unchanged domains through local optimizations only. This precedes
C4 and C1/C2/C5; the eventual latency regression gate must guard those changes.

Reference: [F20 and historical measurements](../core-interface-review.md#f20-steady-state-cost-is-a-full-reconciliation-sweep-that-grows-faster-than-window-and-workspace-counts).

## Acceptance

- A repeatable release-build scale harness measures fresh, converged unchanged,
  externally changed and command-bearing reports across window, output and
  occupied-workspace counts.
- Instruction-count profiling attributes expensive reconciliation work before
  fixes, comparing scales with inclusive costs.
- Unchanged domains avoid redundant reconciliation; changed domains preserve
  all existing ordering and settlement behavior.
- Existing Rust, KWin and development-script checks remain green; frozen
  fixtures remain untouched.
- The approved p95 steady-state latency target is documented and checked with
  the native release harness; CI enforces a cheap instruction-count ceiling
  and growth bound instead of wall-clock timing.

## Approach and bounded units

1. Rebuild and validate the scale harness, including convergence assertions and
   fresh request identities so duplicate-reply caching cannot mask cost.
2. Profile the scale cases with Callgrind, identify the superlinear term, and
   remove it locally.
3. Skip unchanged domains with targeted invalidation regressions.
4. Measure again; implement the approved budget and regression-gate form.
5. Run full checks and record the outcome.

Architecture restructuring, contract changes, platform behavior changes and
live desktop testing are out of scope.

## Current evidence and profiling method

- Historical F20 release medians were about 27 ms for 200 windows on one output
  and 10.6 ms for 50 windows with 20 workspaces per output. These are not current
  baselines and do not attribute a cause.
- Valgrind 3.27.1 and `callgrind_annotate` are available in the development
  environment. Use a release-with-debuginfo example and inclusive Callgrind
  instruction counts across the expensive unchanged cases. No dependency
  change is required.
- Local scan, rollback-backup, hinted-projection and conservative unchanged-domain
  optimizations are implemented below, with the approved gate described below.
- The initial unchanged-only harness is now extended with changing-hint and
  accepted-command cases, with explicit input-change and settlement oracles.

## Rebuilt unchanged-report baseline

Run `cargo run --release -p tiler-core --example report_scale -- all 31 3`.
Each of 12 cases has three fresh-engine repetitions, three warmup calls and
31 measured unchanged calls per repetition, with deterministic case rotation.
Typed reports exclude protocol decode from timing. Each output has a synthetic
4000x4000 work area; initial windows have no minimum hints.

Convergence requires exact observed/desired allocation equality, all seeded
windows tiled and retained, no transfer or focus request outstanding, truthful
native occupancy counts and the requested number of actually occupied logical
domains. Every measured call advances the revision with a fresh request and
observation identity, preserves allocations and leaves no pending transfer or
focus request. All 36 case repetitions passed these checks.

| Case | Unchanged median range (ms) | Highest repetition p95 (ms) |
|---|---:|---:|
| 10 windows, 1 output | 0.2544-0.2607 | 0.2730 |
| 10 windows, 2 outputs | 0.2726-0.2728 | 0.2835 |
| 10 windows, 4 outputs | 0.3324-0.3389 | 0.5079 |
| 50 windows, 1 output | 1.7851-1.8301 | 2.0011 |
| 50 windows, 2 outputs | 1.4949-1.5231 | 1.7173 |
| 50 windows, 4 outputs | 1.5136-1.5362 | 1.7282 |
| 200 windows, 1 output | 15.3029-16.4071 | 16.6517 |
| 200 windows, 2 outputs | 10.1319-10.7700 | 10.9186 |
| 200 windows, 4 outputs | 8.0114-8.4517 | 8.6152 |
| 50 windows, 2 outputs, 2 occupied workspaces/output | 1.4983-1.5227 | 1.7583 |
| 50 windows, 2 outputs, 10 occupied workspaces/output | 2.4054-2.5851 | 2.6435 |
| 50 windows, 2 outputs, 20 occupied workspaces/output | 4.5089-4.7365 | 4.9509 |

First reports are measured separately: 200 windows/1 output took
81.5147-85.6417 ms; 50 windows/2 outputs/20 workspaces took 3.1742-3.3923 ms.
The unchanged 50-to-200-window cost on one output grows about ninefold for
fourfold window growth. These synthetic current-v4 baselines are not
method-identical to historical F20, and neither timing nor ratios attribute
the cause. Full output: `../core-per-report-cost/results/baseline/release-matrix.csv`.

Profiling distinguishes `handle_report` descendants from harness setup,
report cloning and post-call oracle checks.

## Callgrind attribution

Valgrind 3.27.1 profiles a release-with-debuginfo build with collection disabled
at startup and toggled only around the non-inlined `report_scale::call`
wrapper. Each profile contains 15 steady calls (12 samples plus three warmups);
first-report and convergence work is excluded. The wrapper includes its small
post-call oracle, so the table uses the direct inclusive `handle_report` edge,
not the wrapper total. Raw profiles and inclusive annotations are retained in
`../core-per-report-cost/results/callgrind-before/`.

| Case | Inclusive instructions/report |
|---|---:|
| 50 windows, 1 output | 28,343,279 |
| 200 windows, 1 output | 271,703,791 |
| 50 windows, 2 outputs, 20 workspaces/output | 66,851,917 |

The 50-to-200-window cost grows 9.59-fold. Source inspection corroborates two
quadratic mechanisms:

- `session.rs::validate_topology` deep-clones a domain tree and rebuilds its
  leaf vector for each window, then repeats linear reciprocal binding scans.
- `session/world.rs::updated_focus_stack_on` and
  `updated_focus_history_on` resolve every retained focus entry by deep-cloning
  the tree, collecting all leaves and scanning all window links.

First remove these repeated clones/scans with borrowed trees and temporary
per-call membership indexes, preserving the exact validation and focus-history
results. Isolated focus queries must retain leaf-only matching even for
malformed trees; a first-node-by-id lookup is not equivalent when a group and
leaf repeat an id.

Occupied-workspace attribution remains incomplete. Recursive/inlined clone
annotations can exceed total instructions and are not independent costs.
No change to transactional staging or late-gate rollback is selected.

Targeted source review also found `Engine::reconcile_request` unconditionally
clones the entire engine once per swept domain, although its restore closure
uses that backup only after relocation into an absent target. Guarding this
existing backup by the relocation path is a local optimization candidate;
all actual relocation rollback and outer transactional staging stay intact.
Measure its isolated effect rather than attributing recursive clone sums.

## First local optimizations and remeasurement

- Topology validation now borrows trees and builds temporary per-domain leaf
  and link sets once. Focus-stack/history pruning similarly builds one
  membership index per batch. Isolated focus queries use a borrowed leaf-only
  walk, retaining matching behavior on malformed trees.
- `Engine::reconcile_request` takes its whole-engine rollback backup only when
  attempting actual relocation. The original relocation restore paths remain;
  retained reconciliation no longer clones every domain's sessions per domain.
- Ten tests cover scoped reciprocal bindings, global duplicate-leaf rejection,
  malformed trees, leaf-versus-group matching, stale focus pruning, retained
  ordering/duplicates and stack/history append differences.
- The example now prefers an exact case name before substring matching.
  Previously `w50-o2-ws2` also selected `ws20`; an exploratory control profile
  containing both cases is not used as a single-case result.

Direct remeasurement uses a release-with-debuginfo build, the same steady
Callgrind boundary with 15 calls per case, and a separate native `all 31 3`
run. All 36 native case repetitions pass the geometry, occupancy and
fresh-revision oracles.

| Case | Instructions/report before | Instructions/report now | Native median range now (ms) |
|---|---:|---:|---:|
| 50 windows, 1 output | 28,343,279 | 15,744,171 | 1.1170-1.1243 |
| 200 windows, 1 output | 271,703,791 | 70,208,136 | 4.5722-4.6379 |
| 50 windows, 2 outputs, 2 workspaces/output | Not isolated | 16,357,766 | 1.2232-1.2326 |
| 50 windows, 2 outputs, 20 workspaces/output | 66,851,917 | 32,377,701 | 2.2254-2.2376 |

The 50-to-200-window instruction ratio falls from 9.59 to 4.46. The current
200-window profile still shows hinted projection work prominently, and the
unchanged-domain sweep still runs. These are intermediate results, not
performance acceptance or a final budget recommendation. Evidence:
`../core-per-report-cost/results/local-after/`.

## Conservative sweep skip and linear hinted minimums

- The world sweep now skips the ordinary engine handle only after proving the
  retained, single-domain, all-tiled, unhinted case from live state. Exact
  membership, homing, focus, bounds/gaps and identity fences must hold; strict
  projection must succeed. No observation digest or persistent cache is added.
- The initial broad skip proposal was not accepted: unchanged convergence can
  still normalize focus bookkeeping and synchronize admission counters. The
  implemented skip shares convergence's focus/MRU/history/last-active tail and
  preserves the admission high-water update. Unsynchronized counters, hints,
  overlays, floats, minimize slots, fixed-size bookkeeping, pending/drag/divergence
  and all uncertain cases fall through. Not every unchanged domain skips.
- Eight regressions cover actual sweep hits, mixed-domain hint-change misses,
  normal and deliberately stale focus-bookkeeping equivalence, counter
  high-water retention after retirement, empty/nonempty focus and conservative
  invalidation guards. Direct engine callers retain their existing path.
- Hinted projection previously called `subtree_min` on descendants at every
  ancestor, becoming quadratic on deeply nested trees. It now computes both
  minimum extents once, bottom-up. Four regressions cover the old saturating
  sum/max/gap formula, exact nested hinted geometry and overconstraint order,
  linear resolver visits, and rejection before resolver calls on malformed
  trees. A 17-leaf skewed tree drops from 169 resolver visits to 34; the bound
  is two visits per leaf, independent of depth.
- Independent source review found no correctness blocker. The private skip
  helper receives the existing internal event type; it introduces no public
  interface, retained state, transaction or ownership restructuring.

## Final scale evidence and approved budget

Run `cargo run --release -p tiler-core --example report_scale -- all 31 3 all`.
The optional fourth argument selects `S1`, `S2`, `S3` or `all`; the default
remains `S1`, with the original default of 24 samples.

- S1 retains the unchanged-report oracle.
- S2 alternates a positive width/height minimum on one known tile. Every
  measured input differs and carries a meaningful hint, so the unhinted-only
  skip cannot hide this path. Allocations and membership must stay converged.
- S3 executes one freshly sequenced queued orientation toggle, requires exactly
  one successful matching settlement and changed positive tiled allocations,
  then echoes geometry in an untimed commandless report and checks full
  convergence before the next command.
- All shapes use fresh engines and three warmups. The final serial native run
  passes all 108 case/shape repetitions and 3,348 measured reports, including
  1,224 successful command-and-echo cycles counting warmups. First-report
  timings remain separate and are not a steady-state budget.

| Case | S1 median range (ms) | S2 median range (ms) | S3 median range (ms) | Highest S1/S2/S3 p95 (ms) |
|---|---:|---:|---:|---:|
| 50 windows, 1 output | 0.7847-0.7972 | 1.0602-1.0805 | 1.5923-1.6283 | 0.8401 / 1.1111 / 1.6652 |
| 200 windows, 1 output | 3.1562-3.1903 | 4.2819-4.3350 | 6.5049-6.6377 | 3.2129 / 4.3842 / 6.8097 |
| 50 windows, 2 outputs, 20 workspaces/output | 1.9048-1.9360 | 1.9246-1.9529 | 2.5404-2.5652 | 2.0544 / 2.0644 / 2.6111 |

Across all outputs and shapes, the largest p95 is 0.3706 ms for 10 windows,
2.6111 ms for 50 windows and 6.8097 ms for 200 windows.

Final Callgrind runs use the same release-with-debuginfo build, 12 explicit
samples plus three warmups, one exact case per profile, and the direct
inclusive `handle_report` edge. S1 collection toggles `report_scale::call`;
S2 uses `call_hint` and S3 uses `call_command`. Each profile contains 15
measured-boundary report calls; commandless S3 echoes are outside collection.

| Case | S1 instructions/report | S2 instructions/report | S3 instructions/report |
|---|---:|---:|---:|
| 50 windows, 1 output | 11,189,565 | 15,070,507 | Not profiled |
| 200 windows, 1 output | 48,077,321 | 65,149,991 | 98,565,130 |
| 50 windows, 2 outputs, 2 workspaces/output | 12,711,521 | Not profiled | Not profiled |
| 50 windows, 2 outputs, 20 workspaces/output | 28,821,301 | Not profiled | Not profiled |

S1's fourfold window growth now costs 4.30-fold instructions, down from
9.59-fold initially; current S2 growth is 4.32-fold. The collected S1
sweep-to-handle edge falls to about 670 instructions/report in the one-output
cases, corroborating that occupied regular domains bypass full reconciliation.
Projection/publication still runs; this is not a whole-report cache.

User 2026-11-17: the latency target is p95 per steady-state report in a release
build on a developer machine: 1 ms at 10 windows, 4 ms at 50 windows and 10 ms
at 200 windows. Check it manually with the native command above; the final
matrix meets all three tiers. First reports remain outside this target.

User 2026-11-17: CI uses deterministic Callgrind instructions per report, not
wall-clock assertions on shared runners. Keep the check proportional: a few
representative cases, roughly twice current cost as headroom, a growth bound
to catch returning superlinear work, and simple re-baselining.

`bash scripts/check-report-cost.sh` builds the release example with symbols
and profiles only S1 at 50 and 200 windows on one output. Each profile collects
four direct inclusive `handle_report` calls (three warmups and one sample);
setup, convergence and wrapper oracles are excluded. The script is the only
place to update the instruction ceilings and growth limit. It rejects missing
or malformed measurements and explains intentional-cost re-baselining on
regression failure. CI invokes it in one step in the existing Rust job.
The existing targeted hint-visit regression and manual S2/S3 matrix cover
changed inputs without adding another profiled CI shape.

Raw matrices, profiles, inclusive annotations and direct-edge summary:
`../core-per-report-cost/results/final-after/`.

## Verification before optimization

- After the final harness correction, current-tree checks pass: workspace
  formatting; scoped
  `tiler-core`, `tiler-protocol` and `omnitiler` all-target build and strict
  Clippy; 3,473 permitted Rust tests with zero failures or ignored tests.
- KWin typecheck and 1,378 tests pass; launcher checks 703/0, native-effect
  checks 204/0 and helper checks 58/0.
- An initial example-only strict-Clippy failure (`filter_map` with `bool::then`)
  was corrected; the final scoped all-target run passes.
- These checks establish the starting behavior, not performance acceptance.
  Other tracked changes were preserved; frozen fixtures are unchanged.
  Logs are retained in
  `../core-per-report-cost/results/baseline/`.

After the first local optimizations, all required checks pass again:
workspace format, scoped three-package all-target build and strict Clippy;
3,483 permitted Rust tests, zero failed/ignored; KWin typecheck and 1,378
tests; launcher 703/0, native-effect 204/0 and helpers 58/0. Full logs are in
`../core-per-report-cost/results/local-after/`. No dependency, contract or frozen
fixture edits are part of these optimizations.

After the final source and harness corrections, serial verification passes:
workspace formatting; scoped three-package all-target build and strict Clippy;
3,495 permitted Rust tests, zero failed/ignored; KWin typecheck and 1,378/0;
launcher 703/0, native-effect 204/0 and helpers 58/0. Earlier checks overlapping
edits are superseded. An overlapping KWin run timed out while discovering its
fixture; the serial rerun passes without a source or timeout change. Final
logs use the `verified-` prefix under `../core-per-report-cost/results/final-after/`.
The stable independent-review recheck confirms unchanged production diffs and
passes nine skip and 25 size-hint targeted tests. Unrelated tracked changes and
the separately updated backlog match their saved snapshots; frozen fixtures,
whitespace and added-line ASCII/privacy checks are clean.
The approved budget is promoted to the
[engine decision](../../decisions/engine.md#core-per-report-latency).

## Outcome and gate verification

- Complete: local superlinear scans and hinted descendant walks are removed,
  regular unchanged domains conservatively skip full reconciliation, and the
  approved proportional regression gate is wired into CI. Behavior, ordering,
  settlement, contract bytes and frozen fixtures are preserved.
- Direct gate rerun passes at 11,186,428.0 and 48,036,500.2 instructions/report,
  with 4.29-fold growth. Including a warm release build, the two profiles take
  8.4 seconds; the earlier core release rebuild took 30.4 seconds. A fresh
  shared-runner build duration has not been measured.
- Direct checks reject truncated/wrong-event profiles, both cost-ceiling
  regressions and excessive growth. Each regression failure includes the
  intentional-cost re-baselining instructions. Workflow YAML selects the one
  expected gate step; shell syntax passes.
- Final serial checks after gate integration pass: formatting, scoped all-target
  build and strict Clippy; 3,495 permitted Rust tests with zero failed/ignored;
  KWin typecheck and 1,378/0; launcher 703/0, native-effect 204/0 and helpers
  58/0. No production or harness change followed the accepted native matrix
  and independent source review; neither was repeated.
- Gate logs and failure-path evidence:
  `../core-per-report-cost/results/gate/`.
- Final preservation checks confirm unrelated tracked diffs match their saved
  snapshot, the existing backlog update is retained, frozen fixtures are
  untouched, and added lines pass privacy, ASCII and whitespace checks.

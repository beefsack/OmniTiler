# V4 phase 2: KDE scaled metrics

## Goal and scope

- Status: offline acceptance complete; native scale/rendering acceptance open.
- Recheck ordinary KDE tiling and presentation across output reconfiguration
  under the approved [per-output metric contract](../porting/world-contract.md#scale-and-rounding).
- KDE coordinates are logical pixels: report `scale:1`, never native device
  pixel ratio. Preserve checked, once-only core gap/style quantization and native
  coordinate normalization. Do not add another scale owner or conversion path.
- Scope: focused offline regressions for the existing producer/core route and
  presentation reflow, plus separately verified removal of an unused v3 builder.
- Non-goals: physical reconnect/rehome, other platform producers, state-target
  changes, a new rendering surface, or native acceptance.

## Established implementation

- `world-report.ts::encodeV4ScreenRow` already emits `scale:1`.
  `WorldEngine::scaled_gaps` and `scale_float_once` already quantize gaps and
  presentation styles with checked halves-up rounding; underlay resolves its
  negative sentinel before scaling. The report digest includes scale and bounds.
- KWin 6.8.1 (`1b798c05c19b1124edb1e16623191fcd2c128fd0`):
  `Workspace::applyOutputConfiguration` calls `updateOutputs`, whose unconditional
  `outputsChanged` forwards to script `screensChanged`. The entry already
  subscribes. A scale/mode change does not need another script signal or a DPR
  field, including a reconfiguration that preserves logical bounds.
- Native anchored rendering already invalidates on output scale changes. Native
  paint consumes logical geometry; no second style multiplication is needed.
- Shared policy still has unscaled resize steps/minima and fixed group-edge
  zones (`policy.rs`, `session/ops/resize.rs`, `cosmic_v1.rs`). They are correct
  for KDE's scale of 1; scale-not-1 support remains a separate implementation
  gap, not evidence of complete cross-host scaling. No policy decision changes.

## Acceptance and bounded work

1. Extend the existing genuine-engine ordinary-frame carrier: output DPR is
   ignored as a wire scale, `screensChanged` causes fresh capture, logical bounds
   changes reflow with the same logical gaps, and unchanged bounds do not churn.
2. Add a real report/reply presentation regression for logical topology changes
   at scale 1: ordinary tile frames and underlay geometry reflow; anchored border
   width/radius/outsets remain logical-sized. Use existing test infrastructure.
3. Remove the unused v3 inner-report builder in a separate increment only after
   confirming no production caller and retaining live lifecycle coverage.
4. Run scoped Rust formatting/tests/build/clippy and adapter typecheck/tests/build.
   Review changes for semantic coverage and unchanged immutable recordings.

## Verification and outcome

- Offline reflow acceptance passes without changing production metrics: the
  genuine entry/core carrier captures `screensChanged` at unchanged logical
  bounds with `scale:1`, no extra handshake or geometry writes, then reflows
  exactly from `[8,8,1904,1024]` to `[8,8,1264,664]` and back. This models signal
  delivery; actual KWin signal emission is source-checked, not exercised here.
- The real v4 report/reply regression reflows a tiled pair at scale 1, preserves
  border width/radius/outsets `3/6/[3,3,3,3]`, and recomputes found underlay outer
  geometry from `[2,2,796,556]` to `[2,2,636,436]` after settled frame facts.
  Independent source review found no blocking issue; the test witnesses now
  carry their report observation sequence. This is not native causal proof.
- Current-tree checks pass through `devenv shell --`:
  `cargo fmt --all -- --check`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture --nocapture`;
  `cargo build --locked -p tiler-core -p tiler-protocol -p omnitiler`;
  `cargo clippy -q -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `npm --prefix kwin run typecheck`; `npm --prefix kwin test` (1147 passed,
  zero skipped); `env OMNITILER_SCOPE_CLASS= npm --prefix kwin run build`.
  Rust results have zero failures/ignored tests and only the named exclusion.
  Private-bus logs contain actual storage/queue/take/presentation outcomes with
  no skip diagnostics. Immutable wire and stage5 recordings are unchanged.
- Separate cleanup removes the unused `WorldInnerReportInput` and
  `buildWorldInnerReportJson` v3 builder. Repository-wide references were only
  its definition and one dead-envelope test; all seven active lifecycle
  observation tests remain. The dead test and orphan helper/imports are gone:
  275 lines removed, no replacement path. All gates above passed again after
  removal; the adapter count decreased by exactly that one obsolete test.
- A named `cargo test -p omnitiler --test stage5-fixture-route -- --nocapture`
  run also passes both enabled production-route proofs and the witness join
  (3 passed, zero ignored). No production scale conversion or new signal/harness
  was necessary for the KDE slice.
- Native-only remainder: actual compositor scale/mode notification, ordinary
  frame convergence and physical border/underlay paint after the change. Smallest
  check: two fixture windows on one output, observe frames and loaded-effect
  presentation, change display scale once, verify logical gaps/styles and native
  convergence, then restore the display setting. No native test has run here.

## Current-scale native check not run

- The proposed two-output, different-scale check did not reach fixture staging.
  The production lifecycle's desktop command exposes the roster, not
  sticky-inclusive occupancy or fixture workspace placement. The existing
  baseline helper collects broader foreign-window facts rather than an
  occupancy-only result; no new capture was taken.
- Per-output frames, cross-output convergence and border/underlay paint remain
  unverified. No display setting changed. The unscoped non-trace bundle was
  rebuilt; controller and Planner status remained down.

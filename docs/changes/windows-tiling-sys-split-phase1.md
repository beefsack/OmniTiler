# Windows tiling-system split, phase 1

## Goal and scope

Reduce `crates/tiler-windows/src/tiling_sys.rs` toward the source
[file-size principle](../principles.md#file-size) through behavior-preserving
moves. Preserve names, tests, native behavior and external entrypoint paths;
allow only necessary visibility/module declarations and `cargo fmt` changes.
No logic changes or live testing.

## Approach and acceptance

- Follow the [split map](../research/windows-port/tiling-sys-split-map.md).
- Reuse existing path-included child modules. Move self-contained test leaves
  before shared fixtures and production orchestration.
- Each extraction is a separate pure-move commit, with unchanged body evidence
  and locked build/test/strict all-target clippy for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`, workspace format
  check and `git diff --check` green before committing.
- Preserve all original tests: baseline 2,095 passed, zero failed, one ignored.
  Current rebased baseline: 2,106 / 0 / 1. Moves must preserve counts; only
  verified additions in pulled upstream commits may increase the baseline.

## Accepted evidence

- Baseline: 61,917 parent lines; 26,470 lines in 28 inline test modules.
- Directional-cross tests: parent reduced to 59,560 lines; new child has
  2,333 lines. Same module name, fixture paths and 19 tests. Body comparison
  permits only rustfmt whitespace and trailing-comma changes.
- All required gates passed; test totals unchanged.

| Commit | Extraction | Parent lines afterward | Child lines |
| --- | --- | ---: | ---: |
| `4d1c5a1` | Directional-cross | 59,560 | 2,333 |
| `2ff5018` | Pending-drop | 56,698 | 2,852 |
| `66b73e6` | Fixed-predicate | 54,042 | 2,652 |
| `b6da4fd` | D23 minimize | 50,446 | 3,595 |
| `7c31a66` | Float/unfloat | 48,233 | 2,207 |
| `08736a2` | Workspace migration | 46,182 | 2,047 |

- Every extraction passed the required gates with 2,095 / 0 / 1 before
  upstream integration. Original/new body comparisons found only rustfmt
  changes; workspace migration also has one formatter-collapsed closure.
- D23 was moved intact to preserve its test namespace and shared helpers.
  Its new file still exceeds the test soft cap; subdivision remains mapped.
- Rebase incorporated `7da6d1e` shared window policy and macOS classification
  without extraction conflicts. All required commands pass on `08736a2`;
  aggregate tests are now 2,106 / 0 / 1. The 11-test increase is entirely
  new shared-core window-policy tests; every other suite count is unchanged,
  including Windows at 1,206 passed and one ignored. The 2,106 / 0 / 1
  rebased baseline replaces the initial aggregate count.

## Remaining work

Next: output-send, cross-drag, workspace/navigation, rendering/preview,
admission/exit, gesture/dispatch, remaining small tests, then the shared fixture
in map order. D23 subdivision needs a whole-test/helper-sharing boundary;
production moves need import/re-export review and must preserve oversized
individual functions intact. Parent reduction so far: 15,735 lines (25.4%).

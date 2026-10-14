# Windows tiling-system split, phase 1

## Goal and scope

Reduce `crates/tiler-windows/src/tiling_sys.rs` toward the source
[file-size principle](../../principles.md#file-size) through behavior-preserving
moves. Preserve names, tests, native behavior and external entrypoint paths;
allow only necessary visibility/module declarations and `cargo fmt` changes.
No logic changes or live testing.

## Approach and acceptance

- Follow the [split map](../../research/windows-port/tiling-sys-split-map.md).
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
| `00f6272` | Workspace migration | 46,182 | 2,047 |
| `da2de31` | Output-send | 44,571 | 1,606 |
| `b3bb3bb` | Cross-drag | 43,870 | 699 |
| `1e742fa` | Workspace navigation | 41,938 | 527 / 843 / 559 |
| `7fca7ae` | Render/preview | 39,770 | 775 / 788 / 595 |
| `361db20` | Admission/exit | 38,316 | 913 / 282 / 66 / 184 |
| `97f0ac2` | Gesture/dispatch | 36,708 | 476 / 190 / 342 / 588 |
| `8fa7549` | Small adapter tests | 36,133 | 16 / 200 / 163 / 36 / 160 |
| `53a4574` | Shared adapter fixture | 35,537 | 594 |

- The first six extractions passed the required gates with 2,095 / 0 / 1
  before upstream integration; all later extractions passed with 2,106 / 0 / 1.
  Original/new body comparisons found only rustfmt changes, including
  formatter-collapsed single-expression closure/match-arm blocks. Module
  namespaces, test names, attributes and shared-fixture access are preserved.
- D23 was moved intact to preserve its test namespace and shared helpers.
  Its new file still exceeds the test soft cap; subdivision remains mapped.
- Rebase incorporated `7da6d1e` shared window policy and macOS classification
  without extraction conflicts. All required commands pass after integration;
  aggregate tests are now 2,106 / 0 / 1. The 11-test increase is entirely
  new shared-core window-policy tests; every other suite count is unchanged,
  including Windows at 1,206 passed and one ignored. The 2,106 / 0 / 1
  rebased baseline replaces the initial aggregate count.
- Subsequent upstream commits `dd15c12` (documentation) and `1903f92` (macOS
  logging) did not affect the four-package test baseline. Workspace format
  checking passed after integration.
- Final required build/test/fmt/clippy/diff-check gates passed on the shared
  fixture extraction, with 2,106 passed, zero failed and one ignored. All 28
  inline test modules are extracted to 28 sibling files totaling 26,288 lines.
  Parent: 61,917 -> 35,537 lines (-26,380, 42.6%). Its production prefix before
  the original inline test section is byte-identical to baseline. No live tests.

## Remaining work

Phase 1 is complete at the test-extraction boundary. Next: D23 subdivision
design, then production map orders 14-30 (shell/monitor/observe;
hints/seams/placement/minmax; trace/input; render/retained; focus/underlay;
fullscreen/admission; rows/reconcile/geometry; keyboard/dispatch;
directional/output operations; restart/assignments; selection/send;
CLI/workspace/migration ticks; history/output sync; drag/gesture; settings;
main loop; public commands). D23 needs a whole-test/helper-sharing boundary
that preserves namespace and names. Production moves need import/re-export
review and must preserve oversized individual functions intact.

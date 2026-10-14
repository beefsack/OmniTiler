# Windows tiling-system split, phase 2

## Goal and scope

Continue the [split map](../research/windows-port/tiling-sys-split-map.md)
after phase 1: subdivide the D23 tests and extract production responsibilities
in orders 14-30. Preserve behavior, test names and public entrypoint paths.
Only whole-item moves, necessary visibility/imports/module declarations and
rustfmt changes are allowed. Oversized individual functions remain intact.

## Approach and acceptance

- One responsibility extraction per commit, following the map order; main
  loop and public commands last. Keep shared loop state in the parent initially.
- Compare moved bodies and inspect the minimal color-moved diff.
- Each extraction requires locked build, test and strict all-target clippy for
  `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`,
  workspace format checking and `git diff --check`.
- Baseline: 35,537 parent lines; 2,106 tests passed, zero failed, one ignored.
  Test-count increases require evidence of additions in upstream commits.
- No live testing. Record file sizes and soft-cap exceptions after each move.

## Accepted evidence

- D23: 3,595 -> 2,061 / 1,535 test lines. The last 13 whole tests move
  byte-for-byte into a same-namespace `include!`; 20 helpers stay in the first
  file and all 27 test names are preserved. Parent remains 35,537 lines.
- Required build/test/fmt/clippy/diff gates pass: 2,106 / 0 / 1.
- Shell/monitor/observe: parent 35,537 -> 34,884 lines; children 69 / 106 /
  511 lines. Bodies compare identically after removing necessary visibility
  changes and one rustfmt signature reflow. Original public paths are preserved
  through parent re-exports; shared state stays in the parent. Required gates
  pass with the same 2,106 / 0 / 1 baseline.
- Hints/seams/placement/minmax: parent 34,884 -> 33,818 lines; children
  138 / 495 / 177 / 314 lines. All cfg branches travel with their items;
  comparison permits only necessary visibility/imports and two formatted
  signatures. Required gates pass on final formatted source: 2,106 / 0 / 1.
- Trace/owner-work/preview/input: parent 33,818 -> 33,138 lines; children
  93 / 250 / 139 / 248 lines. Bodies differ only by necessary visibility,
  imports and four formatted signatures. Shared fields remain in the parent;
  crate-visible snapshot planner and enum also remain there to preserve paths
  in all builds. Required gates pass: 2,106 / 0 / 1.

## Remaining work

Production map orders 17-30, starting with render/retained.

## Commit records

| Commit | Extraction | Parent lines afterward | Child lines |
| --- | --- | ---: | ---: |
| `114d8ae` | D23 subdivision | 35,537 | 2,061 / 1,535 |
| `06b8e28` | Shell/monitor/observe | 34,884 | 69 / 106 / 511 |
| `40540a0` | Hints/seams/placement/minmax | 33,818 | 138 / 495 / 177 / 314 |

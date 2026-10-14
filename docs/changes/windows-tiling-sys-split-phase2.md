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

## Remaining work

Production map orders 14-30, starting with shell/monitor/observe.

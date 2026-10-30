# Fullscreen directional focus (R-MAX-02)

## Goal and scope

- Implement User 2026-10-29: directional focus from a fullscreen window skips
  same-output siblings and uses existing adjacent-output directional semantics;
  without an adjacent output, retain focus.
- Shared core policy consumed by all adapters. Explicit click, focus-by-window,
  workspace switch, directional move/send and maximized policy stay unchanged.
- No live testing or native acceptance claim.

## Acceptance and approach

1. Inspect shared Session fullscreen state, directional focus and adapter fences;
   stop for any unresolved product boundary or conflicting adapter policy.
2. Add failing-first core regressions for crossing despite a local sibling,
   no adjacent output, ordinary focus and explicit focus-by-window.
3. Apply the smallest shared focus-only change; add a KDE regression only if
   KDE owns a separate relevant path.
4. Review the diff and run sequential core/Windows/protocol tests, strict
   non-macOS workspace clippy, KDE tests/typecheck, formatting, portable and
   diff checks. Record evidence and archive on completion.

## Implementation and review

- Initial working tree clean. Existing User 2026-10-29 decision implemented;
  no new durable product decision.
- `Session::propose_focus` returns `Unchanged` for a focused fullscreen tile;
  the existing Engine fallback crosses horizontally. The cross proposal skips
  only the fullscreen source's local-edge exhaustion check, retaining adjacency,
  capability, observation, projection, target ranking and commit safeguards.
- Shared `WorldCommand::FocusDirectional` skips the same-output float lattice
  for eligible fullscreen floats and uses the existing float-origin remembered
  target cross proposal; without a horizontal adjacent output it retains.
- Source fullscreen includes both-flags observations. Other fullscreen windows
  do not change the source rule or existing destination selection. Existing
  automatic-float/floating-workspace eligibility and non-fullscreen behavior
  remain unchanged. No move/send or adapter source changes.
- Added 15 core regressions across Session, Engine and WorldCommand, covering
  tiled/floating crossing despite local candidates, no-adjacent retention,
  vertical retention, ordinary controls, both flags and explicit focus-by-window.
  Updated two prior fullscreen-local-focus expectations to the approved rule.
- Source/diff review found no core correctness issue. Tightened two no-op test
  rejection oracles to require `unchanged`; added a float no-adjacent case.
  KDE has no separate focus policy path, so no KDE-specific test was added.

## Verification

- Failing first: `cargo test -p tiler-core --lib fullscreen_focus_cross` before
  implementation: 9 failed, 5 passed, exposing local sibling/float focus. After
  implementation and oracle additions: 15 passed, 0 failed. These runs and all
  gates below used `devenv shell --impure --`; gates ran sequentially.
- `cargo test -p tiler-core -p tiler-windows -p tiler-protocol`: 2,324 passed,
  0 failed, 0 ignored across 57 test binaries; core library 1,062 passed,
  Windows library 329 passed.
- `cargo clippy --workspace --exclude tiler-macos --all-targets -- -D warnings`:
  exit 0, no code warnings.
- `npm test --prefix kwin`: 856 passed, 0 failed, 0 skipped, 129 suites.
- `npm run typecheck --prefix kwin`, `cargo fmt --all -- --check`,
  `just check-portable` and `git diff --check`: exit 0. Portable check reports
  zero normal core dependencies and no platform leaks.
- Environment emitted the existing devenv executable/lock-input version
  mismatch warning; no dependency changes. No live tests or native acceptance.

## Outcome and adapter limits

- Shared core implementation verified offline. KDE command-only focus and
  Windows direction-only commands consume it without adapter changes. Full
  cross-platform native delivery is not established; the following existing
  adapter limitations remain unresolved, not passing acceptance claims.
- Windows `tiling_sys/world_native.rs::focus_write_vetoed` vetoes a focus write
  when another readable captionless fullscreen foreground remains active (or
  the foreground is unreadable). Directional effects do not use the selected
  arrival exception. Core can select the adjacent target while native actuation
  refuses it. The gate has no managed-owned/app-native distinction. Preserved
  this safety gate; any narrow explicit-directional departure authorization
  needs separate adapter work and review, not a blanket bypass.
- macOS `world_command_map.rs::operation_command` maps `FocusStep` to a local
  `CoreCommand::Focus`, wrapped as `WorldCommand::Operation` in
  `runtime_thread.rs`. Fullscreen tiles now retain instead of taking a local
  sibling; this route still cannot cross, and floating exceptions still refuse
  `NotTiled`. Existing macOS hotkey cross-output integration was explicitly
  deferred in the cross-output edge-focus decision; no new integration here.
- Next: resolve Windows directional-departure safety policy and macOS hotkey
  integration scope before claiming R-MAX-02 parity on every native adapter.

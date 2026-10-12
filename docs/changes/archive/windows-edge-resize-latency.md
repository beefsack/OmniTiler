# Windows edge-resize latency

## Goal and acceptance

- Investigate the 2026-10-12 edge drag-resize usability complaint against the
  2026-10-02 accepted baseline. Reduce redundant owner work without changing
  final layouts, live border pixels, overlay placement, or game-safety fences.
- Scope: Windows adapter border rasterization and meaningful offline
  regressions. Shared core and other adapters are outside this change.
- Accept only byte-identical BGRA output and identical owned-surface FNV-1a
  checksums, including square/rounded, degenerate, and narrow surfaces.
- Native offline build/test/fmt/strict Clippy and whitespace gates must pass.
  Physical latency acceptance requires a separate user measurement.

## Hot path and evidence

Locators refer to base `e506350` plus the existing Windows follow-ups and this
raster change. Product traces inspected locally; resource identities omitted.

| Phase | Source | Work |
| --- | --- | --- |
| Input | `crates/tiler-windows/src/tiling_sys.rs:321-390` | Native app handles WM_SIZING/mouse motion; owner WinEvent callback queues START/END/location wakes, not an Engine command per mouse move. The 100 ms message wait is an idle timeout, not a refresh-rate limit. |
| Held sizing | `tiling_sys.rs:20626-20638,20756-20762,20907-20957` | Open gesture wakes refresh border/underlay/preview; tiling is paused. No neighbour writes or Engine resize while held. |
| Border | `tiling_sys.rs:2204-2467`; `active_border_sys.rs:215-317,1258-1353` | Fresh foreground observation/style/DPI; every size change allocates a full-window DIB, rasterizes/hashes it, uploads via UpdateLayeredWindow, then two owned-overlay SetWindowPos calls. Equal geometry has existing live no-op/reassert checks. |
| Other overlays | `tiling_sys.rs:2650-2713,3174-3218` | Resize alone does not trigger underlay observation/Engine resolution. Preview latches dead on size change and later returns before mover observation. |
| Release | `tiling_sys.rs:18496-19070,6212-6587`; `crates/tiler-core/src/session/ops/resize.rs`; `crates/tiler-core/src/engine.rs` | Observe domain, assemble hints, classify final edge/boundary, Engine share adjustment/projection, changed-member geometry writes, full readback. Existing settle-only layout semantics. |
| Hints | `tiling_sys.rs:625-629,831-878,18874-18896` | WM_GETMINMAXINFO sends at release, not throughout sizing: 10 ms per query, 40 ms operation budget. Fixed-size observation added in `a41fcb5`/`e506350` participates in row assembly. |
| Incidental work | `tiling_sys.rs:1096-1105,19508-19550,20192,5959-6024` | Settings mtime poll each pump; trace JSON/file append overhead. Fullscreen-metadata retirement sweep is reconcile-only, outside held sizing and successful resize settlement. Overlay latches affect row policy, not held-event native queries. |

- Of three recent dev/trace runs, one contains four applied pointer resizes and
  one unchanged resize refusal; another has no resize, the third mostly floating
  gestures. Do not treat all three as resize measurements.
- In the resize-bearing trace (3,706 lines), held-dispatch ticks
  199/202/206/210/214 contain 6/9/6/8/8 border redraws respectively, before
  settlement at 200/203/207/211/215. A dispatch tick is not a pump counter or
  time unit. Location events can trigger repeated full raster work between
  dispatches; the log does not establish events per second.
- Applied settlements write 3/2/2/3 member frames, each with zero readback
  mismatches. Their three-member hint-query durations are 0/0/0/6 ms with no
  timeouts. The largest hint sample in this run is 26 ms on reconcile, not
  resize settlement. No evidence here implicates hints in held edge lag.
- Run volume includes 271 observe, 328 plan, 156 write, 82 border-redraw and
  76 border-move lines. Trace increases formatting/I/O work; repaint logs also
  occur without trace. Repaint/pump timestamps are absent, so neither actual
  repaint durations nor physical input latency can be reconstructed.
- The exhaustive border raster/size-change repaint path dates to `b5374ce`,
  before the accepted 2026-10-02 baseline; no newly introduced raster algorithm
  was found. Radius default changed in `c3bcb98`, but the same coverage algorithm
  remains. Newer hints/fixed-size/latch/retirement work does not add a per-move
  Engine or min/max-query loop. A historical usability regression is unproven;
  substantial existing area-scaled dev-build cost is independently reproduced.

## Delivered fix and offline evidence, 2026-10-12

- Only `crates/tiler-windows/src/active_border_sys.rs` changes in production.
  Bulk-clear the backing store, evaluate possible ring bands, and advance the
  exact FNV-1a checksum across transparent zero-byte spans using wrapping prime
  powers. Default-style coverage/hash work is perimeter-scaled; bulk clear and
  layered presentation still scale with area. No visual throttling, geometry,
  logging, overlay placement, hint caching, or core-policy change.
- Two oracle regressions compare all bytes and checksums against the old
  exhaustive painter: 12,000 small dimension/style/color combinations, larger
  typical/thick surfaces, dirty buffers and untouched extra buffer tails.
  Include nonpositive dimensions/widths and negative/oversized rounded radii.
- Ignored pure raster benchmark checks old/new bytes and checksum on every
  sample. Same buffers, default width 3/radius 6, three repetitions; excludes DIB
  allocation, UpdateLayeredWindow, SetWindowPos, logging and presentation latency:

  | Synthetic surface | Dev old/new | Release old/new |
  | --- | ---: | ---: |
  | 800x600 | 22.60 / 1.46 ms | 3.25 / 0.25 ms |
  | 1920x1080 | 98.21 / 3.38 ms | 15.76 / 0.95 ms |

- Native MSVC Rust 1.99.0: locked build/test of tiler-core, tiler-protocol,
  tiler-kwin-effect-ffi and tiler-windows PASS (1,489 passed, zero failed,
  one ignored benchmark; Windows lib 367, tiling integration 115).
  `cargo fmt --all -- --check`, strict locked allowlisted Clippy
  `--all-targets -- -D warnings`, and `git diff --check` PASS. Commands use
  `mise exec --` per the Windows runbook. Release raster oracle tests and
  explicit dev/release benchmark also PASS.
- Re-run the pure benchmark with
  `mise exec -- cargo test --locked -p tiler-windows --lib paint_ring_bench_old_vs_new -- --ignored --nocapture`
  and add `--release` after `test` for the optimized profile.

## User measurement plan

- Follow [live Windows testing](../../live-windows-testing.md). Use ordinary
  non-elevated PS7, the same scoped Notepad/Paint windows, settings and comparable
  starting sizes. Stop/restore between runs and verify owner exit, hook/overlay
  release, visibility and settings restoration. Keep artifact/owner/display
  identities in local evidence only.
- Perform several slow and fast edge drags in each configuration: dev trace-on,
  dev trace-off, release trace-off. Report grabbed-edge smoothness separately
  from mouse-release-to-neighbour-settle delay; check final layout and border.
  A recording is optional and user-owned. Logs alone cannot measure key/photon
  or mouse/photon latency. The current tile recipe always builds dev.
- Dev run (add `--trace` only for the trace-on comparison):

  ```powershell
  mise exec -- just --justfile windows.justfile tile --user-start --scope-exe notepad.exe --scope-exe mspaint.exe
  # Perform physical drags, then stop/restore.
  mise exec -- just --justfile windows.justfile stop
  ```

- Release run using existing Explorer-broker helpers; it does not overwrite
  the dev payload and does not use the tile recipe, which would rebuild dev:

  ```powershell
  mise exec -- cargo build --release --locked -p tiler-windows
  . .\scripts\windows-dev.ps1
  $dir = (Resolve-Path -LiteralPath ".\target\release").Path
  $exe = Join-Path $dir "tiler-windows.exe"
  $argv = "tile --user-start --scope-exe notepad.exe --scope-exe mspaint.exe"
  Start-ExplorerGui $exe $argv $dir
  $ready = Assert-OwnerReady $exe "release-tile"
  $ready | ConvertTo-Json -Depth 8
  Get-FileHash -LiteralPath $exe -Algorithm SHA256
  # Record profile=release, revision/diff, argv and ready/hash output locally.
  # Perform the same physical drags, then verified stop/independent restore.
  Stop-PayloadOwner $exe
  ```

- Standard dev launches produce provenance sidecars automatically; this direct
  release helper launch prints ready/hash evidence and needs the local profile/
  revision/argv record above. Release is needed to separate unoptimized raster
  overhead from remaining presentation/application costs. If lag persists,
  next investigation is bounded repaint/presentation timing, not a change to
  live neighbour-resize semantics.

## Physical follow-up, 2026-10-12

- User scoped Notepad/Calculator/Paint, dev still with trace: edge resizing is
  much better after the raster fix, but noticeably less smooth than the
  no-owner high-refresh baseline. Partial improvement, not full acceptance.
- Remaining input-thread coupling, presentation/queries and trace work are
  investigated in the [follow-up](windows-resize-hook-latency.md), with
  trace-only busy-span diagnostics and a trace/profile comparison plan.

## Outcome

- Raster cost fixed and verified offline; exact pixels/checksums retained.
  Physical improvement is user-reported above; residual-lag attribution and
  full physical acceptance remain pending.
  Linux handback and applicable CI remain pending. Prior 2026-10-02 acceptance
  remains historical evidence, not acceptance of this performance fix.

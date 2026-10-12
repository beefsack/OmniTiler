# Windows resize input-thread latency

## Goal and acceptance

- Explain residual held edge-resize lag after the border raster optimization.
  Reduce redundant work without changing layout, overlay following, input
  consumption, lifetime/scope checks or fullscreen/game safety.
- Scope: Windows owner trace no-op suppression and bounded trace-only timing.
  Keep hook installation/thread ownership and all visual semantics unchanged.
- Preserve first hide/reason-change/visible-to-hidden diagnostics; suppress
  already-hidden identical repeats. Measure non-wait owner work across early
  exits and border refresh cost without logging from hook callbacks.
- Native offline common-definition-of-done gates and targeted diagnostic
  regressions must pass. Physical input latency needs a user measurement.

## Accepted evidence and approach

- User physical result 2026-10-12, scoped Notepad/Calculator/Paint, dev with
  trace: border raster fix makes edge resizing much better, but still laggy
  against the smooth no-owner high-refresh baseline. Partial improvement,
  not full performance acceptance.
- WH_MOUSE_LL and WH_KEYBOARD_LL install on the owner message-pump thread.
  Every mouse move traverses the cheap mouse callback even without Win-drag;
  border queries/presentation, WinEvents, Engine, settings and trace I/O share
  that thread. Busy owner spans can hold up system input delivery.
- New trace has five held resize blocks with 118/14/77/22/43 border redraws.
  Repeated idle-underlay lines count more pump iterations than changed-size
  redraws. Wake batches already coalesce to one refresh set, not one refresh
  per queued WinEvent. No existing wall-clock wake/presentation timing.
- After the first resize latch, preview returns before native observation or
  Engine calls; idle underlay returns before observation. Remaining held work
  includes one border observation, suspension reads, DIB allocation, layered
  upload/z-order placement or unchanged-surface live probes, settings polling
  and trace I/O. Pure raster benchmarks are workload proxies, not live bounds.
- Match border/underlay hide logging to existing change-only preview behavior.
  Add trace-only monotonic owner-work samples with a border-refresh subspan;
  document exclusions and logging overhead. No pump-rate cap or cache weakening.
- Dedicated hook-thread isolation could address input head-of-line blocking,
  but remains an unselected architecture option. Revisit only after separating
  trace/dev costs and collecting actual busy spans. Gaming shortcut containment
  is a distinct unresolved problem, not proven fixed by thread isolation.

## Related decided behavior

- User 2026-10-12: originally captionless owned-fullscreen metadata remains
  fail-closed without a restoration witness. Win+F11 owned toggle exit remains
  the release path; user will observe untiled windows during dogfood. This is
  selected behavior, not a remaining proof-policy decision.

## Ranked contributors and measured limits

Locators below refer to base `e506350` plus the Windows follow-ups and this
diagnostic delivery. Product resource identities and local log paths omitted.

| Rank | Contributor | Source / evidence |
| --- | --- | --- |
| 1 | Input head-of-line blocking on the installer thread | `crates/tiler-windows/src/tiling_sys.rs:20099-20129` places the keyboard/mouse hooks on the owner pump. `win_mouse.rs:824-942,948-968` and `snapkey.rs:4640,5239-5266` install global LL hooks and callbacks. Every mouse move must traverse the callback, including passed native sizing input. Long owner spans can delay delivery even though the callback itself is cheap. |
| 2 | Border presentation and queries on that thread | `active_border_sys.rs:215-317,997-1046,1258-1353`; `tiling_sys.rs:2349-2612`. Changed size allocates DC/DIB, paints, uploads the full layered surface, then performs two owned-overlay SetWindowPos calls. Fresh border observation includes process/exe/lifetime checks, styles, two DWM queries and DPI. Equal size avoids repaint but still probes visibility/frame/z-order via EnumWindows. |
| 3 | Trace file I/O and formatting | `tiling_sys.rs:1096-1105,2323-2340,2615-2635`. Old trace repeated identical hidden lines; 1,077 idle-underlay plus 138 hidden-border lines in a 2,195-line run. Each line opens/appends/closes a file on the installer thread. Actual time per append is unmeasured. |
| 4 | Dev-profile amplification and incidental reads | Standard tile recipe in `scripts/windows-dev.ps1:125` builds unoptimized plus debuginfo. Settings metadata polls, foreground suspension checks and chord reads share each pump; monitor enumeration is on the two-second slow path. Raster fix benchmark at synthetic 1920x1080 is 3.38 ms dev vs 0.95 ms release, excluding queries/allocation/upload/logging. It is a workload proxy, not a live per-event lower bound. |

- WinEvents use WINEVENT_OUTOFCONTEXT; four narrow hooks include explicit
  LOCATIONCHANGE (`tiling_sys.rs:20001-20040`). Wake batches already coalesce
  into one refresh set; own-overlay events do not trigger it. A 100 ms wait is
  only the idle timeout. Dispatch tick values are not pump counts or timestamps.
- New dev/trace run: 2,195 lines, 429 border events (276 redraws, 138 hidden,
  11 moved, three shown, one unchanged), 1,077 underlay events, all hidden/idle.
  Held ticks 64/68/73/77/79 have 118/14/77/22/43 redraws. More idle-underlay
  lines than redraws means more pump iterations with unchanged geometry, not
  several underlay calls per held iteration.
- Four applied settlements at 65/69/78/80 each write one frame, with no
  mismatches. Two-member hint query times are 5/3/0/0 ms, no timeouts. The fifth
  resize refuses unchanged. WM_GETMINMAXINFO is not sent throughout the hold;
  only START classification uses the bounded WM_NCHITTEST query during it.
- Keyboard classification durations are 2-38 us; zero mask sends. These do
  not measure time waiting for delivery or downstream hooks. Mouse statistics
  confirm five native downs, zero consumed downs/ups. Mouse Snap prevention
  uses a session SPI setting, not a second mouse hook.
- After resize latches preview-dead, there is no repeated preview observation
  or Engine resolution. Idle underlay also returns before observation. Retained
  overlay latches and stale-fullscreen sweep do not create per-move hint/Engine
  work. Current evidence cannot quantify location events/sec or live busy time.

## Delivered changes and gates, 2026-10-12

- Only production source change: `crates/tiler-windows/src/tiling_sys.rs`.
  Already-hidden identical border/underlay reasons now stay silent even in
  trace, matching existing preview behavior. First hides, reason changes and
  visible-to-hidden transitions remain logged; geometry/fences are unchanged.
- Trace-only `OwnerWorkGuard` (`tiling_sys.rs:1107-1261,20167-20195,20805-20814`)
  logs at most one `owner-work` sample per active wake, including held/suspend
  exits, using monotonic relative start, sequence, batch counts, held flag,
  `busy_us` and summed `border_refresh_us`. Completely quiet polls stay silent.
  No per-refresh timer or diagnostic write runs with trace disabled.
- `busy_us` begins after pump_wait, excludes its wait/callback delivery, the
  pre-pump stop probe and its own sample append. It includes other owner work
  and existing logging after the pump; border subspan includes observation and
  presentation together. It is wall duration, not CPU time or input latency.
  Wake counts combine non-overlay WinEvent kinds, not raw mouse moves. These
  exclusions prevent overstating input blockage or total busy fraction.
- Three offline regressions: hidden repeat suppression/retained transitions,
  trace-disabled and quiet silence, one closed sample with accumulated border
  time on scope exit and real early return. No live native window effects.
- Native MSVC Rust 1.99.0 locked four-package build/test PASS: 1,492 passed,
  zero failed, one ignored raster benchmark; Windows lib 370, tiling integration
  115. All-workspace fmt, strict locked allowlisted all-target Clippy with
  warnings denied and whitespace check PASS. Linux handback/CI pending.

## Options still unselected

- Dedicated LL-hook thread: removes rendering/Engine/file-I/O spans from the
  installer pump. Costs a second message loop, cross-thread publication of
  origin/remap/gate state, bounded queue wakeup, shutdown/unhook ordering and
  keyboard/mouse/mask sequence tests. Existing thread-local state cannot simply
  be shared. This is an architectural unit, not a callback micro-fix.
- Visual throttling or smaller/segmented overlay carriers: potentially reduces
  full-surface upload and z-order cost but changes follow/render architecture;
  neither selected here. Existing live reassert/identity checks stay intact.
- Recommendation: compare trace/profile combinations first. If release without
  trace still delays native sizing, use busy-span evidence to consider LL-hook
  isolation next. This does not establish a Win+G/F11 containment remedy; the
  [gaming-coexistence alternative](../windows-gaming-coexistence.md#parked-choices-and-recommendation)
  and its physical/game cost gate remain separate.

## Exact user comparison plan

- Follow [live Windows testing](../../live-windows-testing.md), ordinary
  non-elevated PS7, same scoped applications/settings/starting sizes. Stop and
  verify owner/hook/overlay exit, reveal and settings restoration between each
  run. Capture source/diff/profile/hash/owner/display identity locally only.
- Use the existing no-owner observation as baseline; repeat comparable slow
  and fast edge drags in the four combinations below. Report edge smoothness
  separately from release-to-neighbour-settle delay and final-layout/border
  correctness. The standard tile recipe always builds dev.

  ```powershell
  $scope = @("--scope-exe", "notepad.exe", "--scope-exe", "CalculatorApp.exe",
    "--scope-exe", "ApplicationFrameHost.exe", "--scope-exe", "mspaint.exe",
    "--scope-host-child", "ApplicationFrameHost.exe=CalculatorApp.exe")
  mise exec -- just --justfile windows.justfile stop
  # Dev, trace off: perform edge drags, then stop.
  mise exec -- just --justfile windows.justfile tile --user-start @scope
  mise exec -- just --justfile windows.justfile stop
  # Dev, trace on: repeat the same drags, then stop.
  mise exec -- just --justfile windows.justfile tile --user-start --trace @scope
  mise exec -- just --justfile windows.justfile stop
  ```

- Release uses existing Explorer helpers directly, preserving the dev payload.
  Each helper launch prints ready evidence; unlike the normal recipe it does
  not create a provenance sidecar. Record profile/revision/diff/argv/hash/ready
  locally. Do not use the tile recipe to start release: it rebuilds dev.

  ```powershell
  mise exec -- cargo build --release --locked -p tiler-windows
  . .\scripts\windows-dev.ps1
  $dir = (Resolve-Path -LiteralPath ".\target\release").Path
  $exe = Join-Path $dir "tiler-windows.exe"
  Get-FileHash -LiteralPath $exe -Algorithm SHA256
  # Release, trace off.
  $argv = "tile --user-start " + ($scope -join " ")
  Start-ExplorerGui $exe $argv $dir
  $ready = Assert-OwnerReady $exe "release-tile"
  $ready | ConvertTo-Json -Depth 8
  # Perform the same drags, then verified stop/restore.
  Stop-PayloadOwner $exe
  # Release, trace on: collect owner-work timing for comparison.
  $argv = "tile --user-start --trace " + ($scope -join " ")
  Start-ExplorerGui $exe $argv $dir
  $ready = Assert-OwnerReady $exe "release-trace-tile"
  $ready | ConvertTo-Json -Depth 8
  # Perform the same drags, then verified stop/restore.
  Stop-PayloadOwner $exe
  ```

- For trace runs, summarize held `owner-work` samples: busy and border-refresh
  median/p95/max, sample intervals and non-overlay wake counts per interval.
  START/END counts separate segments; tick-at-start is contextual, not elapsed
  time. Timing adds a diagnostic append, so judge smoothness primarily from
  trace-off runs and do not equate these samples with mouse-to-photon timing.

## Outcome

- Clear repeated trace work removed; missing timing evidence now collectable.
  No hook-thread/input-consumption/visual-policy change. Captionless metadata
  policy is recorded as decided. Physical residual-lag attribution, Linux
  handback and applicable CI remain pending. Next: user trace/profile comparison.

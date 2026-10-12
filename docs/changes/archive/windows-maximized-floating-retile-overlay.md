# Windows maximized floating-to-tiled overlay

- Goal: item 11 / Q3 R-MAX-03 reserves a tile slot while retaining native
  maximize, independently of the R-MAX-06 born-maximized setting.
- Scope: Windows adapter and offline regressions; existing Engine projection
  and overlay-write exclusion. D5 fullscreen exit and fixed D6 exceptions keep
  their existing policy; B9 explicit unfloat is a separate route.
- Acceptance: no retile maximize clear or overlay geometry write; sibling
  allocation, exact-slot native-restore projection, repeated observations,
  lifetime reuse and both born-setting values covered offline. Native pending.
- Approach: lifetime-bound retile overlay latch alongside the born overlay
  latch; reuse reserved-slot seeding and native maximized-write exclusion.
- Status: delivered offline from base `e506350`; ordinary retile/restore
  physically user-accepted 2026-10-12 below; remaining native legs pending.
- Outcome: `retile_keep_overlay` binds to the exact floating-first non-fixed
  lifetime; admission clear planning, clearing and slot assembly honor it.
  Prune/drop remove the latch with membership. Born setting, fixed exceptions,
  hydrated fixed overrides and first-fullscreen-exit policy stay independent.
- Accepted evidence: real Engine slot/sibling allocation, overlay write
  exclusion, exact restore projection, repeat stability and lifetime reuse;
  floating-first admission tested initially under each born-setting value and
  across live-setting flips. Independent review accepted the final code/tests.
- Shared core/protocol/KDE code, dependencies and settings unchanged.
- Tentative 2026-10-12, pending user review: keep separate born/retile latches
  so the born-setting adoption cannot change the workspace-retile route.
- Verification: locked four-package build/test, all-package format, strict
  allowlisted Clippy and diff whitespace; native journeys remain user-owned.

## Final offline gates

- `mise exec -- rustc -vV`: Rust 1.99.0, `x86_64-pc-windows-msvc`.
- Locked build/test of `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`
  and `tiler-windows`: pass, 1461 tests, 0 failed (Windows lib 340,
  tiling integration 114), including existing directional-navigation changes.
- `mise exec -- cargo fmt --all -- --check`: pass.
- Strict locked four-package Clippy `--all-targets -- -D warnings`: pass.
- `git diff --check`: pass. Linux/CI and native/application acceptance pending.

## User-run native plan for items 11 and 15

- Follow [live Windows testing](../../live-windows-testing.md). Preflight:
  source/diff, executable SHA-256, exact owner PID/start/session/integrity,
  display work/full rectangles/DPI and the baseline frames/visibility/maximize
  of disposable Notepad A, Paint B and Calculator C. Keep other same-app
  windows out of the desktop test scope; preserve unsaved work.
- The explicit executable scope below excludes Windows Terminal. The single
  session Terminal stays open/visible: never close, kill, hide, click/type
  test chords into it or synthesize any keys. Execute shell commands through
  the command runner; physical test chords go only to the mouse-focused test
  apps. Explorer-broker launch keeps the owner outside the Terminal tree.
- Before window tests, revalidate the independent stop/restore baseline and
  out-of-hook access. First owner launch/stop below exercises graceful release;
  stop if scope, owner identity or recovery is uncertain. Do not force-crash.
- From the repository root, exact launch and read-only owner/artifact checks:

  ```powershell
  mise exec -- just --justfile windows.justfile tile --user-start --trace --seconds 600 --scope-exe notepad.exe --scope-exe mspaint.exe --scope-exe CalculatorApp.exe --scope-exe ApplicationFrameHost.exe --scope-host-child ApplicationFrameHost.exe=CalculatorApp.exe
  mise exec -- .\target\windows-dev\tiler-windows.exe ready
  Get-FileHash -LiteralPath .\target\windows-dev\tiler-windows.exe -Algorithm SHA256
  mise exec -- just --justfile windows.justfile stop
  ```

- Verify hooks/overlays released, all test apps visible and stop/restore
  successful. Repeat the identical scoped launch for each bounded test leg;
  use the tray's existing settings/workspace controls, not unscoped launch.
- Item 11 staging: record the original default-workspace mode and born setting;
  use Settings to save default workspace tiling off, then stop the owner.
  Natively maximize Notepad A while stopped, leave Paint B normal, and relaunch
  with the identical explicit scope. This ensures A is first seen maximized
  on a floating workspace, rather than maximized after normal admission.
  Enable workspace tiling through the tray: A must stay
  maximized without a clear/toggle; B takes its share and A reserves a slot.
  Restore A with its native titlebar control: actual frame must match the
  reserved allocation, B keeps its share. Repeat maximize/restore, then idle:
  no unsolicited toggle or fighting. Repeat fresh floating-first admissions
  under `Tile on launch` and `Keep maximized overlay`; outcomes must agree.
  Stop/maximize/relaunch to stage each fresh admission. Return the default
  workspace mode to tiled before the item-15 leg.
  Record planned allocation and actual restored frame/flags from the trace.
- Related item 11: hidden-workspace retile/return must preserve maximize;
  later user-owned session-restored maximized admission must not loop. Born
  tiled admission remains setting-controlled. D6 automatic fixed Calculator
  remains floating on workspace enable; an existing explicit fixed override
  retains its tiled slot. Native recovery must reveal hidden test members.
- Item 15: tiled A/B; mouse-focus B, Win+G floats it; native-maximize B;
  mouse-focus B, press Win+G once. Trace order must be clear issued/result,
  observed-cleared, then `unfloat-applied`; real maximize clears, B fresh-tiles
  from restored frame at ordinary admission, retains focus, A reflows. Repeat
  normal unfloat as control. Fixed automatic C (when its native hints satisfy
  the configured predicate) follows clear/fresh tile and keeps the explicit
  override through later observation and graceful owner restart.
- Refusal controls: sticky-maximized and project-fullscreen floats must keep
  their existing refusal with no admission. A naturally refused/unobserved
  clear must keep float intent; a later explicit press may retry. Do not induce
  client failure or fullscreen races; record those legs TBD if unavailable.
  Games/protected/elevated windows are outside this app-only plan; stop if any
  enters scope. Game/DPI/multi-output and forced-loss checks remain separate.
- End each leg with `mise exec -- just --justfile windows.justfile stop`.
  Confirm exact owner exit, hook/overlay release, hidden-member reveal and
  session-setting restoration. If unresponsive, recheck identity, use only
  `mise exec -- .\target\windows-dev\tiler-windows.exe emergency-stop` then
  `mise exec -- .\target\windows-dev\tiler-windows.exe restore` via the
  proven recovery ladder. Stop on uncertain identity/restoration. Keep test
  apps open, restore their captured native states and original saved settings
  manually; tiling stop does
  not restore original geometry. Terminal stays visible/open throughout.
- Exact next action: perform the scoped native journeys and record actual
  frames/flags/focus/marker results; unsupported/unrun outcomes remain TBD.

## Native-session result - 2026-10-12

- R-MAX-03 NOT-RUN: the session used the exact-owner per-window float CLI for
  [B9 native evidence](windows-maximized-intentional-unfloat.md#agent-observed-native-evidence---2026-10-12),
  but did not exercise the tray-only workspace-tiling toggle. Native retile,
  reserved-slot restore and setting independence remain TBD; B9 is not evidence
  for this distinct route. Born-maximized both-values and fixed-hint spots were
  also not run.
- Border spot: launch reported radius 6 and the active-border overlay was
  visible with matching reported geometry. Corner rounding itself was not
  visually verified; no visible-radius acceptance claimed.
- Graceful stop/restore, owner absence, byte-identical saved-settings restore,
  closed test windows and unchanged non-test inventory verified. Terminal
   remained visible/unminimized. No product defect observed in exercised checks.

## User-accepted physical evidence - 2026-10-12

- Scoped normal owner, single output, Notepad/Paint, physical keyboard/mouse;
  base `e506350` plus combined directional, retile-overlay and B9 Windows changes.
- Floating workspace -> maximize -> retile preserves maximize; native
  unmaximize lands in the reserved slot. User accepts item 11.
- Observation only: choosing workspace tiling toggles from the tray does not
  restore the previously focused window afterwards. User notes this may suit
  keyboard accessibility of the tray; no behavior change requested.
- Hidden retile, session-restore no-loop, exact setting-qualified retile
  repetitions, fixed-size, game/DPI/multi-output remain pending. The separate
  born-maximized both-values Paint launch is accepted in its own record.

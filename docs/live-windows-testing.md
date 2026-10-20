# Live Windows Testing Guide

Read this guide before planning or running live Windows work. It is the
repository's safety and operational contract; it does not grant mutation
authorization. The user selected writing it before the first Windows session
  (2026-09-30, option A). Owned-window graceful/forced-loss restore must be
  proven on the physical host before broader use; input and broader app
  acceptance remain pending.

For native tools, permissions and Sandbox limitations, use the
[Windows development environment](windows-dev-environment.md). Product scope
remains in [Windows decisions](decisions.md#scope-and-platform-goals).

## Safety Boundary

- The user authorizes each experiment class separately: input hooks,
  window movement/hiding/restyling, desktop overlays, registry/policy changes
  and intentional process loss. Record the permitted environment, resources,
  operations and end condition before execution. KWin authorization does not
  carry over; a build, successful preflight or this guide grants no live scope.
- Agent tests use identified, owned disposable test windows only. Ordinary
  application acceptance is user-run dogfooding (user decision 2026-10-01). A
  global hook is a session-wide effect even when testing only one window;
  approval must bound the intercepted chords and test duration.
- Preserve unrelated windows, applications, displays, settings and processes.
  No broad cleanup, process-name kills, autostart, boot changes, logoff/restart
  or security exclusions. The user performs physical input, observations and
  session boundaries; agents do not synthesize those acceptance results.
- Never press Win+T during tests: it can open Windows Terminal, which may
  join an existing process the operator depends on. Avoid Win+G (Game Bar)
  and Win+F11 (Xbox full screen experience). Scope runs so Terminal is never
  managed; close only windows whose origin the run proved. Capture window
  preimages before moving test windows; restart Explorer only by exact shell
  PID.
- Games/anti-cheat, elevated or protected windows and the secure desktop are
  outside these experiments. Run at ordinary medium integrity, not elevated.
  Stop the experiment if those classes enter its scope; do not probe takeover.

## Required Preflight

- Read the approved experiment scope and inspect its executable entry points,
  build scripts and test targets for live effects. Building is not permission
  to launch a hook, overlay or window-control process.
- Record source revision plus relevant working-tree changes, artifact path
  and SHA-256, launch arguments, user/session and integrity. Bind the launched
  process to its executable and PID/start identity; a PID or filename alone
  is not ownership proof. Recheck identity before stop/kill or restoration.
- Capture the relevant display baseline: Windows identities, primary display,
  coordinates, scale and work areas. Identify the owned test windows and their
  process/instance, geometry, visibility and styles affected by the experiment.
  Capture only the state needed to restore the approved resources.
- Prove an accessible graceful-stop and exact-identity kill path first, using
  a non-hooking/non-hiding test process. Keep user mouse/Task Manager access
  and an out-of-hook recovery route available. Disable project dev autostart
  for crash probes; do not alter unrelated startup entries.
- Before a hiding/restyling experiment, prove the independent restore path
  with a bounded owned-window test. It must work after the primary process
  exits and restore only verified experiment-owned changes. Before broader
  hook use, prove release on disable/exit in the approved initial hook probe.
  Missing recovery commands or uncertain window identity block progression;
  proposed `tiler-windows` commands are not evidence that they exist or work.

## Recovery Ladder

Current CLI: `stop` requests verified owner exit; `emergency-stop` forces only
that exact verified owner. Both leave recovery state for independent `restore`.
`just stop` performs graceful stop then restore, resolving the current dev
payload pointer. Dev/test actors use Explorer's desktop broker, not the
protected Terminal tree.

1. Request graceful stop of the verified experiment owner. Check that hooks
   and overlays are released and affected owned windows return to baseline.
2. If unresponsive, recheck executable/PID/start/session identity and terminate
   only that exact authorized test process. Never kill by broad name or guess.
3. Run the independently proven restore path. Revalidate window/process
   identity before restoring; reused HWNDs or PIDs must not authorize writes
   to another window. Process exit alone does not restore hidden windows.
4. If agent control is unavailable or identity cannot be established, stop
   agent actions and ask the user to use the out-of-hook recovery route,
   mouse/Task Manager or Ctrl+Alt+Del secure attention. An ordinary hook cannot
   replace that security path. Do not autonomously log off/reboot or claim
   that secure attention itself restores windows.

Verify the relevant baseline after recovery. On uncertain ownership or failed
restoration, preserve evidence/residue and report the exact remaining state;
do not broaden cleanup or repeatedly restart the experiment.

## Input-Hook Rules

- Keep the installing thread's message loop running. Callbacks must return
  promptly: no blocking work, waits, synchronous logging or debugger pauses
  while the process has a live hook. Do not record raw keystrokes, typed text
  or application content; log bounded hook/action lifecycle summaries only.
- Microsoft documents that `LowLevelHooksTimeout` expiry can silently remove
  a low-level keyboard hook on Windows 7+. Windows 10 version 1709+ caps the
  timeout at 1000 ms. Do not change the timeout registry setting. Silent
  removal is neither a reliable recovery mechanism nor evidence of release;
  verify disable/exit behavior and physical delivery separately.
- Consume only the approved test chords. Product Meta/Win bindings take over
  Snap by default with a visible off setting; physical Snap/Start, key-down/up
  and disable/reversal evidence remain required. Sandbox redirected input or injected events do
  not prove the physical desktop's shortcut behavior.

## Agent Live-Test Practice

- Immediately before every injected keystroke (including Esc, Enter, and
  typing), verify the foreground HWND/PID belongs to the intended owned test
  window, menu, or dialog; otherwise abort that leg without injecting. Input
  reaching the operator's terminal can interrupt the agent session. Dismiss
  menus without global keystrokes: click a verified owned test window or send
  an owned-window cancellation such as `WM_CANCELMODE`, then verify the menu
  is gone. Never leave a menu, dialog, or overlay open at a step boundary.
- Scope the tiler to the permitted test apps so it never manages terminals or
  other windows, for example:
  `mise exec -- just dev trace --scope-exe notepad.exe --scope-exe CalculatorApp.exe --scope-exe ApplicationFrameHost.exe --scope-exe mspaint.exe --scope-host-child ApplicationFrameHost.exe=CalculatorApp.exe`.
  Recovery and final cleanup: `mise exec -- just stop`, then confirm no owner,
  restored Mouse Snap/session settings, revealed hidden windows and a clean
  ledger.
- Desktop control runs through scripted PowerShell helpers (`Add-Type`
  P/Invoke SendInput/SetCursorPos, taskbar auto-hide changes via documented
  `SHAppBarMessage(ABM_SETSTATE)`, `System.Windows.Automation` dialogs,
  GetGuiResources handle checks); no special desktop-control tooling is
  required. Helper-only state changes stay inside the approved scope with a
  captured preimage and restore; the product pager itself only reads the
  switch (ABM_GETSTATE). This grants no new mutation authorization.
  ABM_SETSTATE: [Microsoft SHAppBarMessage](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage).
- Reusable bounded-leg helpers are in
  [windows-live-lib.ps1](../scripts/windows-live-lib.ps1) (dot-source only;
  sourcing performs no live action). `mise exec -- just live-lib-check` and
  `mise exec -- just live-lib-mock` run offline parser/seam and fault-injection
  checks. Drivers must call leg cleanup and final cleanup from `finally`;
  the library does not launch a journey or replace its recovery procedure.
- Never fall back to clicks: one pre-guard fallback click reached the operator
  terminal without injected keys. Park the leg instead.
- A restorable preimage is an actual settings byte copy plus taskbar auto-hide
  and Mouse Snap/SPI state, not hashes or visibility alone.
- Helper lifetimes must cover the whole journey; mid-leg expiry invalidates
  that leg.
- Use system-aware/PMv2 helpers for exact geometry: unaware windows show a
  one-pixel host-side offset at non-100% scale (host constraint; baseline
  `9fa32420`), not adapter error.
- Injected Win+M bypasses the owner entirely; shortcut chords stay physical
  and user-owned.
- Exhaustion fixtures use few large-declared-minimum helpers: a default-policy
  500-arrival roster never exhausted, so shrink victims never prove fresh
  no-space.
- The normal owner ignores injected keyboard input. Agent journeys use the
  exact-owner test CLI controls (`workspace --float`/`--sticky`/
  `--fullscreen`, select/send), tray callbacks or injected mouse input;
  keyboard shortcut journeys remain physical and user-owned. Keyboard-resize
  and OS-owned chords such as Win+M stay physical for the same reason;
  synthetic input cannot drive them. Never weaken the filter for tests.
- The agent-owned identity-tagged `tiler-test-window.exe` is retained only for
  the proofs explicitly bound to it until ported: the non-hooking lifecycle
  `run --hide` proof and the documented private cloak proof (`cloak HWND
  --tag TAG --seconds N --stop-file PATH --log PATH`, `uncloak`,
  `cloak-observe`). New Windows agent live legs use the live fixture, scoped with
  `--scope-exe tiler-live-fixture.exe` (see Fixture-First Live Tests below).
  Do not cite private proofs as fixture coverage.
- Trace runs write run logs and provenance sidecars to
  `%LOCALAPPDATA%\omnitiler\session-1\`. Input queue age, owner work and
  render/drag-preview activity are summarized at most once per second, with
  immediate throttled outlier lines.
- Fixture conventions: numbered workspace operations are one-based and `0`
  selects the trailing empty workspace; opaque workspace tokens are creation
  identities, not ordinals; `float-mismatch` reports successful floating
  settlement with a frame delta. Compare native rectangles in one coordinate
  convention (do not mix DPI-virtualized and physical values).

## Fixture-First Live Tests

- Agent live legs test fixture windows only, scoped fixture-only. Start the
  fixture (`tiler-live-fixture serve`, pipe announcement), then scope the
  tiler before any mutation, e.g.
  `mise exec -- just tile --user-start --scope-exe tiler-live-fixture.exe ...`
  so only the fixture executable is managed; foreign windows stay untouched
  by construction. Full protocol, flags, fields and limits are in the
  [fixture README](../crates/tiler-live-fixture/README.md).
- Foreign (non-fixture) applications are only for application-specific
  investigations after first trying the fixture, with the approved scope
  naming them explicitly. Rationale: fixture windows are predictable,
  controllable and observable (caller ids, observed predicates, per-field
  outcomes), and fixture-only scope protects the operator desktop.
- The agent-owned `tiler-test-window.exe` helper is retained only for the
  proofs explicitly bound to it until ported (the `run --hide` lifecycle proof
  and the documented private cloak proof named above);
  it is not the default for new agent legs. Do not cite private proofs
  as fixture coverage.
- The fixture transport compiles on both native targets in CI. Windows passed
  the fixture build, tests and clippy in native run 39393279249; macOS carries
  2 open test failures (see the fixture README platform status). Live window
  behavior on either host is verify-on-first-use with no accepted evidence yet.
- Short first-use checklist: isolate (private run dir, fixture-only scope,
  user hand-over per leg); prove identity (announced pipe name plus PID,
  fixture caller ids, no Terminal/operator windows in scope); read back
  (`get` observed predicates, never accepted-setter echoes); clean up
  (`shutdown`, remove only the owned run dir -- the pipe endpoint is
  kernel-namespace, no file to delete -- `just stop`, verify restored baseline
  and clean ledger).

## Sandbox Registry And Policy Experiments (deferred to Phase 4)

- Registry/policy mutations are allowed only inside Windows Sandbox and only
  for the separately approved experiment. Sandbox use stays deferred to
  Phase 4 clean-runtime and guest-only experiments; historical 2026-09-30
  preflight state is not current proof. Confirm the command/process and
  target registry belong to the guest, not the host. If Sandbox is unavailable,
  defer; a separate account on the daily host is not this isolation boundary.
- Record the exact key/value preimage, including absence, type and contents.
  Bound the write, read back the result, then restore the exact preimage and
  verify both readback and the relevant behavior. Do not delete whole keys
  or overwrite drift from another owner to force restoration.
- Win+L policy can disable locking more broadly than one chord. Guest policy
  or locking-API results do not prove Win+L reaches the guest or that a
  replacement lock path works. Keep host locking unchanged; do not test
  secure-desktop takeover. Discarding a stuck guest contains the experiment,
  but is not proof that exact restoration succeeded.
- Use only the approved read-only payload mapping; no writable mapping of
  host settings, home or checkout. Preserve needed evidence before closing
  the disposable guest. Do not assume guest display/input/integrity behavior
  matches the physical host; use the runbook's isolation limits.

## Live Evidence

- Record authorization, date/OS build, physical host versus Sandbox, source
  and artifact identity, relevant baselines, bounded operations, stop/crash
  outcome and restoration readbacks. Keep resource identifiers in the scoped
  test record; omit titles, application content, secrets and raw input logs.
- An API return, exit status, missing error or visual appearance alone is not
  feature evidence. Distinguish requested actions, observed effects, exact
  restoration and uncertainty; retain the machine responses that gate them.
- Physical shortcut/display/game acceptance remains user-owned. Record what
  was actually observed; Sandbox, synthetic input and hosted CI are not
  substitutes. Game/elevation/secure-desktop journeys need a later separately
  approved procedure, not an expansion of this initial protocol.
- Stop and report on authority, identity, baseline, diagnostic, unexpected
  effect or restoration ambiguity. Revise the protocol from accepted Phase 1
  findings before relying on newly discovered behavior.
- Use a unique evidence directory per journey; reused screenshot filenames
  overwrite prior journey evidence.
- Independently verify screenshot/appearance conclusions; one pager miniature
  'missing' reading was an analysis error.
- External evidence groups stay on the run host, never in the repository;
  records here summarize them.
- Sequential per-window visibility confirmation serializes each app response
  latency (workspace switches observed around 0.5-1.3 s); posting all effects
  then confirming together under one event-driven deadline reduced this
  (around 0.1 s). Treat the figures as observed, not guaranteed.
- Process CPU-time sampling quantizes coarsely (around 7.8 ms/sample); treat
  near-zero idle-cost readings as granularity-limited, not proven zero.

Hook facts: [Microsoft LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc),
checked 2026-09-30; other setup/API evidence is linked from the runbook.

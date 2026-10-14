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

- Scope the tiler to the permitted test apps so it never manages terminals or
  other windows, for example:
  `mise exec -- just dev trace --scope-exe notepad.exe --scope-exe CalculatorApp.exe --scope-exe ApplicationFrameHost.exe --scope-exe mspaint.exe --scope-host-child ApplicationFrameHost.exe=CalculatorApp.exe`.
  Recovery and final cleanup: `mise exec -- just stop`, then confirm no owner,
  restored Mouse Snap/session settings, revealed hidden windows and a clean
  ledger.
- The normal owner ignores injected keyboard input. Agent journeys use the
  exact-owner test CLI controls (`workspace --float`/`--sticky`/
  `--fullscreen`, select/send), tray callbacks or injected mouse input;
  keyboard shortcut journeys remain physical and user-owned. Never bypass the
  injected-keyboard filter in product code.
- The agent-owned, identity-tagged `tiler-test-window.exe` is the standard
  disposable helper (accepted 2026-10-14). The non-hooking lifecycle
  `run --hide` proof accepts only this helper, not ordinary applications.
- Trace runs write run logs and provenance sidecars to
  `%LOCALAPPDATA%\omnitiler\session-1\`. Input queue age, owner work and
  render/drag-preview activity are summarized at most once per second, with
  immediate throttled outlier lines.
- Fixture conventions: numbered workspace operations are one-based and `0`
  selects the trailing empty workspace; opaque workspace tokens are creation
  identities, not ordinals; `float-mismatch` reports successful floating
  settlement with a frame delta. Compare native rectangles in one coordinate
  convention (do not mix DPI-virtualized and physical values).

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

Hook facts: [Microsoft LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc),
checked 2026-09-30; other setup/API evidence is linked from the runbook.

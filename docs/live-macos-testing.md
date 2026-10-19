# Live macOS Testing Guide

Read this guide before planning or running live macOS work. It is the
repository's safety and operational contract; it does not grant mutation
authorization. No build, successful preflight, or prior authorization on
another platform grants live macOS scope. Each experiment class needs its
own separate user authorization.

For native tools, permissions, signing identity, and sandbox limitations,
use the [macOS development environment](macos-dev-environment.md). Product
scope remains in [macOS decisions](decisions.md#scope-and-platform-goals).
Tier 2 (public plus scoped private APIs, SIP left enabled) is the only
selected route; Dock injection and SIP reduction are out of scope.

## Safety Boundary

- The user authorizes each experiment class separately: AX observation and
  window movement/resizing, event-tap or hotkey interception, desktop
  overlays and borders, native-Space moves and focus changes, TCC consent
  and reset flows, and intentional process loss. Record the permitted
  environment, resources, operations, and end condition before execution in
  ephemeral operational notes; repository records contain work and evidence,
  never session permissions or controls.
  KWin and Windows authorization do not carry over.
- Agent tests use identified, owned disposable test windows and Spaces
  only. Ordinary application acceptance is user-run dogfooding. An event
  tap is a session-wide effect even when testing one window; approval must
  bound the intercepted chords, tap placement, and test duration.
- Preserve unrelated windows, applications, Spaces, displays, settings,
  and processes. No broad cleanup, process-name kills, login-item or
  autostart changes, logoff/restart, or security-policy exclusions. The
  user performs physical input, observations, and session boundaries;
  agents do not synthesize those acceptance results.
- Never disable SIP or Gatekeeper, never inject into Dock or another
  process, and never write synthetic input to a window that is not an
  identified owned test window. Games and anti-cheat contexts, elevated or
  otherwise protected windows, fullscreen native Spaces under test by the
  user, and the lock screen are outside these experiments. Run as an
  ordinary user session, not elevated. Stop the experiment if those classes
  enter its scope; do not probe takeover.
- The terminal hosting the controlling agent session is absolutely
  protected: never move, resize, hide, minimize, close, quit, Space-move,
  or send keys or events to it, even when it appears to be an owned
  window. If the hosting terminal cannot be distinguished with certainty,
  stop and ask the user before any window-affecting step.
- Keep raw before/after records (PID, window ID, layer, onscreen, bounds) for
  every protected window, not just counts. CG bounds sampled while Mission
  Control is shown reflect its scaled thumbnails, not real moves: re-sample
  after Mission Control has closed before declaring drift, and stop on drift
  that persists.

## Required Preflight

- Read the approved experiment scope and inspect its executable entry
  points, build and signing steps, bundle identity, and test targets for
  live effects. Building or signing is not permission to launch a tap,
  overlay, or window-control process.
- Record source revision plus relevant working-tree changes, artifact path
  and SHA-256, bundle ID, signing identity and designated requirement,
  launch arguments, and graphical session. Bind the launched process to
   its executable, PID, and start identity; a PID, window ID, or filename
   alone is not ownership proof. Recheck identity before stop, kill, or
   restoration.
   For a directly spawned owned helper, the unreaped `std::process::Child`
   plus its PID and per-run nonce returned over that child's own pipe bind
   the run; a LaunchServices launch-date token is not required. Verify Child
   liveness and a fresh CG owner-PID/window join at each action. A separate
   command without that Child must freshly match the helper's nonce title
   (CG, or bounded AX when trusted), as well as its executable and PID;
   caller-to-record nonce equality alone does not prevent PID reuse.
- Capture the relevant baseline: display topology and separate-Spaces
  mode, Space membership of affected displays, and for each owned test
  window its owning PID, AX identity join (`_AXUIElementGetWindow`), CG
  window ID, geometry, visibility, Space, and focus state. Record exact
  preimages (including absence) of every state the experiment may change:
  window geometry, Space assignment, focus, overlay presence, and only the
  owned settings changed. Capture only the state needed to restore the
  approved resources.
- Prove an accessible graceful-stop and exact-identity kill path first,
  using a non-tapping, non-hiding test process. Keep user mouse, keyboard,
  Force Quit, and Activity Monitor access available, plus an out-of-tap
  recovery route. Disable project login-item or autostart entries for
  crash probes; do not alter unrelated startup entries.
- Before a hiding, Space-move, or restyling experiment, prove the
  independent restore path with a bounded owned-window test. It must work
  after the primary process exits and restore only verified
  experiment-owned changes, including geometry carried in the preimage.
  Before broader tap use, prove release on disable and exit in the
  approved initial tap probe. Missing recovery commands or uncertain
  window, process, or Space identity block progression; proposed probe
  commands are not evidence that they exist or work.
- Confirm the stable signing identity before any TCC-dependent run: fixed
  bundle ID, fixed install path, and one persistent signing identity,
  with `codesign` display and designated-requirement output recorded. A
  rebuilt ad-hoc binary may lose TCC grants; path stability alone is not
  proof.
- Launch TCC-dependent probes through LaunchServices, attributed to the signed
  bundle, rather than executing its binary from the shell. Direct execution
  inherits the terminal's responsible-process attribution: `ax_trusted:false`
  there does not establish that the bundle lacks consent. Use
  `mise exec -- just preflight-app` and `mise exec -- just launch -- <args>`.
  The underlying route is `open -n --stdout <file> --stderr <file>
  target/macos/OmniTilerProbe.app --args <args>`. `open -W` failed for this
  bundle; the launch wrapper polls output and process quiescence with a bounded
  deadline. It cannot observe the app's exit status: inspect the JSON outcome,
  not the wrapper status alone. After rebuilding, require `ax_trusted:true`
  through this route before AX effects; a lost grant is a stop condition.

## Recovery Ladder

1. Request graceful stop of the verified experiment owner. Check that taps
   and overlays are released and affected owned windows return to
   baseline geometry, Space, visibility, and focus.
2. If unresponsive, recheck executable, bundle ID, PID, start, and session
   identity and terminate only that exact authorized test process. Never
   kill by broad name or guess.
3. Run the independently proven restore path. Revalidate window, process,
   and Space identity before restoring; reused window IDs or PIDs must not
   authorize writes to another window. Process exit alone does not restore
   hidden, parked, or Space-moved windows.
4. If agent control is unavailable or identity cannot be established, stop
   agent actions and ask the user to use the out-of-tap recovery route
   (physical keyboard and mouse, Force Quit or Activity Monitor, alternate
   Space or display). Do not autonomously log off, restart, or claim that
   an OS attention path itself restores windows.

Verify the relevant baseline after recovery. On uncertain ownership or
failed restoration, preserve evidence and residue and report the exact
remaining state; do not broaden cleanup or repeatedly restart the
experiment.

## Event-Tap And Hotkey Rules

- Keep tap callbacks prompt: no blocking work, waits, synchronous logging,
  or debugger pauses while a tap is live. A disabled or timed-out tap is
  neither a reliable recovery mechanism nor evidence of release; verify
  disable and exit behavior plus physical delivery separately.
- Consume only the approved test chords. The event tap is the sole hotkey
  route; the Carbon registration/delivery diagnostic is retired. Use an
  intercepting tap only where suppression is proven needed and separately
  approved, with its consent requirements recorded per tap mode.
- Do not record raw keystrokes, typed text, window titles beyond what
  identity proof requires, or application content; log bounded
  tap-and-action lifecycle summaries only.
- Respect Secure Input and lock states: stop interception experiments when
  Secure Input is active rather than attempting to work around the
  blackout. Never synthesize system chords outside the approved owned
   test scope.

## User-Run Physical Checks

- User direction 2026-10-16: where automated live testing is hard, fiddly or
  unreliable, prefer user-run physical checks with a short procedure, bounded
  lifetime, explicit expected readbacks and cleanup. Do not invest in repeated
  synthetic substitutes for a physical-input or shell discriminator.
- A deliberately stalled callback is allowed only in the explicit bounded
  diagnostic fixture (`input-session --physical-stall --stall-once-ms`), never
  the product path. A real timeout notification, verified re-enable and
  subsequent suppression remain the strict fixture oracle. The physical run of
  2026-10-16 disabled the tap without any notification; health-check re-enable
  recovered it and is the accepted product route, so do not repeat the stall
  fixture expecting a notification without a changed hypothesis.
- User procedures must be copy-pasteable in any shell: no POSIX-only variable
  assignments, functions or subshell syntax; inline literal values instead.
- Agent live legs keep the display awake with a bounded `caffeinate -d` child,
  reaped afterwards; check the session lock state before each leg and stop on lock.
- Launch physical probes through the signed bundle. Use the launcher's `--stream`
  option for real-time press instructions and a wrapper timeout longer than the
  probe budget. Ctrl+C stops this launch wrapper, not the LaunchServices child;
  wait for its bounded teardown before starting another probe. Inspect terminal
  JSON rather than inferring success from wrapper exit status.
- Current [physical procedure](changes/archive/macos-dogfood-hardening.md#user-run-physical-script)
  covers timeout recovery, manual Space transfers and the same-application
  focus/gap/logging dogfood recheck. Its Carbon delivery step is retired: the
  diagnostic code and `input-register` command have been removed; retain the
  recorded 0/3 physical-delivery result, but do not run that step.

## TCC, Signing, And Permission Experiments

- Request only the TCC services the approved experiment needs
  (Accessibility first; Input Monitoring only where the chosen tap mode
  proves it needs it; no Screen Recording capture for baseline tiling).
  Consent shortfalls are stop conditions, not prompts to widen scope.
- `tccutil reset` commands are scoped to the experiment's own bundle ID
  only; never reset a service for all apps and never touch the TCC
  database directly. After a scoped reset the user relaunches and regrants
  in System Settings; agents do not perform the grant.
- Keep the stable-identity rule across runs: same bundle ID, install path,
  and signing identity. Verify grant survival across real rebuilds where
  the experiment depends on it. Never store certificate secrets in the
  repository.
- SIP and Gatekeeper stay on. Treat unsigned or ad-hoc dev-build friction
  as dev-only; it is not a shipping state and never justifies a bypass.

## Live Evidence

### Scoped screenshots

- When screen capture is available, use `screencapture -x -R X,Y,W,H -t png
  CAPTURE.png` for a tightly bounded owned-window or indicator region, or
  `screencapture -x -l WINDOW_ID -t png CAPTURE.png` for an owned window. A
  window-only capture may omit a separate border overlay; use a region covering
  the target and border for that check. Never capture the controlling terminal's
  contents or the whole desktop unnecessarily.
- Immediately before each capture, revalidate the owned target's process/window
  identity, liveness and current frame. A cached rectangle is not capture scope:
  after the target exits, that region may expose unrelated terminal content.
- Store captures in an approved temporary directory, inspect a few images per
  leg, and verify real application UI is visible rather than wallpaper or blank
  pixels. Use `sips` to inspect dimensions or downscale large captures if needed.
  Keep indicator crops tight: nearby menu extras can contain unrelated content.
- Pair each image with native focus, target/overlay frames and indicator state;
  pixels alone do not prove identity or effects. Delete capture files after
  inspection, never commit them, and retain the title-free native readbacks.
  If capture fails, shows a consent picker or produces blank content, stop that
  visual leg and record a user-assisted check instead. Do not trigger or work
  around permission changes. A retry is appropriate only after a separately
  confirmed capture-permission change, not as a repeated blind attempt.

### Native readbacks and acceptance

- Record the date, supported release family when relevant, physical versus VM
  test conditions (without the development host's OS version),
  source and artifact identity (including bundle ID and signing
  requirement), relevant baselines and preimages, bounded operations,
  stop and crash outcome, and restoration readbacks. Keep resource
  identifiers in the scoped test record; omit titles, application content,
  secrets, raw input logs, session-control details, and machine-specific
  paths or hardware details.
- An API return, exit status, missing error, or visual appearance alone is
  not feature evidence. Distinguish requested actions, observed effects
  (with AX, CG, and Space-membership readback), exact restoration, and
  uncertainty; retain the machine responses that gate them.
- Owned AppKit helpers must complete `finishLaunching()` and pump AppKit events
  as well as their run loop. CG-visible windows alone do not prove an AX
  hierarchy or functional activation. To test AX destruction notifications,
  close the owned window while its app remains alive; process termination alone
  did not deliver that notification. A closed window may retain an offscreen CG
  entry until app exit. Distinguish notification delivery, on-screen absence,
  lingering records, and final process/window cleanup.
- Physical shortcut, display, game, and shell-acceptance checks
  (Mission Control, Dock, Cmd-Tab, focus behavior) remain user-owned.
  Record what was actually observed; synthetic input, hosted CI, and
  upstream source text are not substitutes. Fullscreen, elevation, and
  anti-cheat journeys need a later separately approved procedure, not an
  expansion of this initial protocol.
- Stop and report on authority, identity, baseline, diagnostic, unexpected
  effect, or restoration ambiguity. Revise the protocol from accepted
  Phase 0 findings before relying on newly discovered behavior.

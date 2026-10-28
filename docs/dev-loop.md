# Development Loop

Cross-platform entry point: the root `justfile` requires just >= 1.56 and
conditionally imports `just/linux.just`, `just/windows.just`, and
`just/macos.just`; only the current OS recipes load.
`just dev [verbose|trace]` is the foreground entry point on each desktop. Linux
dispatches on `XDG_CURRENT_DESKTOP`: KDE runs `dev-kde`, GNOME reports
unsupported, anything else fails with the detected value. Windows runs the
native dev session (`just stop` recovers via the current payload pointer;
`just lifecycle-smoke [trace]` is the previous non-tiling loop; `just tile`
still requires explicit `--user-start`). macOS builds the signed development
bundle and launches its existing preview via `scripts/macos-dev.sh`;
`just dev-stop` uses its exact-owner stop path.

Current-source status: shared core and Windows use v2. The KDE/macOS recipes
remain present, but their consumers need the [v2 ports](porting/world-adapter-guide.md)
before current-core build/runtime acceptance. The platform workflow and earlier
native evidence below are not a claim that those ports are complete.

## Windows

Run from the repository root with mise-managed tools:

```powershell
mise exec -- just dev
mise exec -- just dev verbose
mise exec -- just dev trace --scope-exe notepad.exe
mise exec -- just stop
```

- `dev` builds, refuses an existing owner or recovery ledger, copies the
  owner and helper into a unique directory under `target/windows-dev`, and
  launches normal `tile --user-start` through Explorer's desktop broker.
  The owner stays outside the terminal process tree. Readiness and provenance
  are printed before the per-run log streams to the terminal.
- `verbose` uses default logging; `trace` adds `--trace`. Arguments after the
  mode pass through to tiling. No scope arguments means normal whole-desktop
  tiling, including the owner's hooks, overlays and tray.
- Ctrl+C or owner exit runs verified stop, independent restore and a clean
  ledger/request check inside PowerShell `finally`. Clean interruption prints
  `dev interrupted: verified stop+restore, owner exited and ledger clean`.
  Windows just may itself return a nonzero interruption status; the explicit
  cleanup result, not that status alone, establishes teardown.
- Startup interruption uses the unique payload's ledger identity, including a
  bounded wait for publication. Failure or unconfirmed exit is reported
  explicitly and retains the recovery pointer for `just stop`; it never
  claims a clean exit or stops an owner belonging to another payload.
- `just lifecycle-smoke [trace]` retains the old non-tiling ten-minute
  lifecycle smoke. `just tile --user-start [--trace]` retains detached normal
  tiling; `just settings` launches settings. See the
  [Windows recovery guide](live-windows-testing.md#recovery-ladder).

## Linux (KDE)

Run from inside the devenv shell (`just` is provided by `devenv.nix`).
These recipes never stop or mask units, never resolve the Planner through
`/nix/store`, and never create a `result` symlink. Preconditions fail closed.

```sh
just dev         # foreground: refuse unless DOWN, dev-on, tail labeled logs, Ctrl-C tears down via dev-off
just dev verbose # same bounded operational diagnostics, named explicitly for troubleshooting
just dev trace   # additionally build KWin trace detail and log bounded structural Planner request/reply JSON
just dev --scope-class OmniTilerTestWindow # fixture-only management
just dev-on      # disable packaged script, start worktree Planner + KWin bundle
just dev-on --scope-class OmniTilerTestWindow # detached scoped bring-up with the same filter
just dev-status  # read-only: name owner, isScriptLoaded, receipt, unit state
just reload      # rebuild and swap only the recorded worktree Planner
just dev-off     # unload exact script, stop recorded Planner, re-enable packaged script
```

Agent-driven fixture runs should stay scoped: `just dev --scope-class
OmniTilerTestWindow [--scope-class ...]` (or the `dev-on` equivalent).
Each flag carries an identity (or comma-joined allowlist) and repeated flags
accumulate, so the filter is exactly the listed identities, repeatably.
Only windows whose `resourceClass` is listed are managed (on Wayland this
is the `app_id`; the fixture uses `OmniTilerTestWindow` for both).
Scoped-out windows never enter the world report, so the core never sees
them: no tiling, no commands, no focus or stacking effects touch foreign
windows. Empty (default) keeps the unchanged whole-world behavior. Any
malformed entry fails closed before any mutation, and the allowlist bakes
into the dev bundle build. An explicit scope on an already-up session is
rejected outright (the live scope is not provable); changing scope requires
`just dev-off` first.

- Build and launch from the same source revision. When reusing Cargo outputs
  across checkouts, verify source/build provenance and the launched executable;
  rebuild affected packages if provenance is uncertain. A binary hash difference
  alone does not prove stale source.
- `just reload` does not reload the KWin script. After any `kwin/src` change,
  run `just dev-off` then `just dev` (or `just dev-on`); otherwise a live
  check exercises the old script against the new Planner.

- `dev` is a small foreground composition of the existing recipes. It
  probes the same two strict facts as `dev-on` (verified worktree Planner
  owning the D-Bus name, strictly parsed `isScriptLoaded`) and refuses
  without mutating unless both are down. UP and either SPLIT direction
  refuse with a clear error: this session never adopts pre-existing state.
  Use `just dev-status` to inspect, `just dev-off` to teardown pre-existing
  state, `just dev-on` for detached bring-up/recovery, and `just reload`
  for a stale `(deleted)` Planner. A stale owner exe refuses the same way.
  Malformed `isScriptLoaded` fails closed before mutation. On DOWN it runs
  `just dev-on`; on bring-up failure it exits non-zero with no logs tailed
  and no extra teardown (rollback stays owned by `dev-on`). On success it
  starts the worktree tray (`target/debug/omnitiler tray`) as the
  only tray owner in this session: when `com.omnitiler.Tray` is
  already owned, it logs the name-taken owner plainly, preserves the
  installed tray, and runs without a worktree tray instead of claiming one.
  Only a D-Bus owner that verifies as this worktree `$BIN tray` (exe,
  cmdline, `/proc/<pid>/stat` start identity) is recorded at
  `$STATE_DIR/tray-pid`, `tray-exe`, `tray-start`, and `tray-log` and tailed
  with the `[tray]` prefix; anything else is never killed. It then arms
  `just dev-off` for `INT`/`TERM`/`EXIT`, then tails the live Planner
  log from `$STATE_DIR/planner-log` prefixed `[planner]`, the tray stderr
  log prefixed `[tray]` when owned, and the KWin
   journal plugin lines (`journalctl --user -f _PID=<kwin-pid>` from the
    receipt `.pid`, filtered to `omnitiler:`) prefixed `[kwin]`.
   The labeled stream is also captured at `$STATE_DIR/dev-log`'s path; the
   durable file persists after teardown.
   `Ctrl-C` stops the tails, stops only the verified owned tray pid
   (re-verified first; a changed identity is refused, never signalled, and
   no generic `jobs` cleanup ever signals it), and runs the existing
   fail-closed receipt-bound
   `dev-off`; a `dev-off` or tray-stop failure exits non-zero loudly. Missing log,
   receipt, KWin pid, `tail`, or `journalctl` fails closed through the same
   teardown trap rather than tailing silently. An unverified controller teardown
   never retries unload; a verified Planner is still stopped before recovery is
   directed through logout/login. Detached `dev-on`/`dev-off` never start,
   stop, or record the tray.
- `dev-on` disables the packaged KWin script, verifies `isScriptLoaded`
  `false`, requires `com.omnitiler.Planner` to be unowned (it does not
  stop units for you), builds, launches exactly
  `target/debug/omnitiler planner-service` detached with
  `setsid nohup ... </dev/null &`, then proves identity from D-Bus rather
  than `$!`. `$!` is a launch hint only and never authoritative (setsid may
  fork when it is a process-group leader). After a bounded wait the owner
  PID is derived from `GetNameOwner` plus `GetConnectionUnixProcessID` and
  accepted only when `/proc/<pid>/exe` is the worktree `$BIN` or its exact
  kernel-generated `$BIN (deleted)` form, normalizes outside `/nix/store`,
  cmdline contains `planner-service`, and the
  `/proc/<pid>/stat` start identity is captured. That verified PID/exe/start
  is recorded. On failure only an already positively verified worktree
  planner is terminated, never an unverified PID or intermediate bash.
  `dev-on` then runs `scripts/start-test.sh start` with a dynamically
  derived `CONTROLLER_OWNERSHIP_FILE` under `$XDG_RUNTIME_DIR` and prints the
  verified Planner PID plus the script ID read from the controller receipt.
  Every failure after the packaged script was disabled runs fail-closed
  transactional rollback: terminate only the positively verified worktree
  planner (re-verified first), unload the KWin script only if this run
  successfully loaded it, re-enable the packaged script, remove only its own
  created receipt dir, and remove its state pointers/dir. Rollback never
  unloads a script it did not load and never touches a planner it did not
  positively verify; each failed rollback step prints a precise loud error.
  Success disarms rollback. Health requires both a verified worktree Planner
  and `isScriptLoaded=true`: when both are up, `dev-on` makes no changes; when
  both are down, it performs the normal bring-up; when only the verified
  Planner is up, it loads one controller against that Planner without starting,
  stopping, or reconfiguring either half, writes a fresh immutable receipt, and
  updates the state pointer only after re-verifying the same D-Bus owner and
  start identity; when only the controller is up, it refuses rather than
  loading a duplicate. `start-test.sh` keeps its duplicate plugin guard.
- `reload` rebuilds and safely swaps only the recorded worktree Planner. It
  never reloads or unloads the KWin script and requires the recorded
  PID/exe and current process state. The replacement is launched detached
  with `setsid nohup ... &` where `$!` is a hint only; the new PID is
  derived from the D-Bus owner (`GetNameOwner` plus
  `GetConnectionUnixProcessID`) under the same bounded wait and accepted
  only with the worktree exe or its exact kernel-generated ` (deleted)` form,
  no `/nix/store` after normalization, `planner-service` cmdline, and captured
  start identity. On failure only an already positively
  verified replacement is terminated, never an unverified PID.
- `dev-off` independently checks both halves. A loaded controller still
  requires its exact receipt and `start-test.sh stop`; an already-unloaded
  controller is never stopped with its stale ID. It terminates only a recorded
  worktree Planner that passes the existing identity checks even if controller
  teardown fails, skips an absent or unverified Planner without killing it, and
  re-enables the packaged script only after verified controller teardown.
- `dev-status` is read-only: name owner PID plus `/proc/<pid>/exe`,
  `isScriptLoaded`, recorded script ID, and installed unit state. It also
  reports `dev mode: UP`, `DOWN`, `SPLIT`, or `UNKNOWN`; `UP` requires both a
  verified worktree Planner and loaded controller.

Single engine: the worktree KWin bundle (`kwin/src/entry.ts` via
`startWorldAdapterEntry`, owner `kwin-world-adapter`) is the only
observer/actuator. Authenticated `DiscoverWorld` binds its activation to the
serving core generation; `DescribeWorld` sends complete foreground/hidden
observations and returns the full desired window/workspace snapshot
(`com.omnitiler.Planner` / `/com/omnitiler/Planner` /
`com.omnitiler.Planner1`). Rust retains the Engine and owns workspace policy;
the script applies authorized differences against fresh host observation.
Unknown scopes hold. Stale replies cause observation-only catch-up, never
command replay. Planner unavailability narrowly refuses policy changes; no
script policy fallback exists. Request and reply budgets are each 4 MiB.
There is no journal readiness wait or second runtime: bring-up is proven by
the strict `isScriptLoaded` envelope plus receipt-bound exact `Script<ID>`
introspection, and teardown is receipt-bound exact unload.

Current world-route diagnostics:

- `omnitiler:world:discovered gen=<generation>`: discovery only, not applied state.
- `omnitiler:world:dispatch req=<request> obs=<watermark> cmd=<operation|observe>`.
- `omnitiler:world:applied rev=<revision> geom=<N> member=<N> views=<N> mk=<N> rm=<N> fx=<N> held=<N>`: snapshot accepted and partial actuator counts, not atomic native completion.
  `member` counts native membership writes, not managed windows; use the observed
  roster to establish how many windows are managed.
- `omnitiler:world:stale reason=<token> recovery=catchup`: zero stale setters,
  followed by an observation-only report.
- `omnitiler:world:rejected kind=<token> msg=<fixed-check> recovery=on-next-event`:
  a bounded validation reason, never a window payload.
- Planner `omnitiler:world-summary direction=ingress|egress ...`: bounded
  request/outcome correlation without raw payloads; egress `msg=<fixed-check>`
  distinguishes validation failures without exposing content.

The removed legacy domain path's operation-specific `DescribePlan` journal
forms are intentionally not documented below; archived records that cite them
are historical evidence, not current implementation proof. Use the
[full-state live checklist](changes/archive/core-full-state-replies.md#remaining-work-and-user-owned-live-checks)
for the migrated path; live runs remain separately authorized.

## Manual Fallback

This avoids a system rebuild. It uses the existing explicit `/Scripting`
loader. Tray systemd user delivery remains unchanged.

### Loop

```sh
devenv shell --impure -- bash scripts/dogfood-install.sh disable
# Verify KWin unloaded the packaged script before continuing.
busctl --user --json=short call org.kde.KWin /Scripting org.kde.kwin.Scripting \
  isScriptLoaded s omnitiler-kwin

# The service must be unowned before starting the worktree Planner.
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
  org.freedesktop.DBus GetNameOwner s com.omnitiler.Planner
# Only if the owner is verified as omnitiler-planner.service:
# systemctl --user stop omnitiler-planner.service
# Then repeat GetNameOwner and require it to fail before proceeding.

devenv shell --impure -- cargo build
PLANNER_OUT="$(mktemp /tmp/omnitiler-planner-dev.XXXXXX.log)"
setsid nohup "$PWD/target/debug/omnitiler" planner-service >"$PLANNER_OUT" 2>&1 </dev/null &
# $! is a launch hint only, never authoritative (setsid may fork). Derive the
# owner PID from D-Bus and verify it before trusting it.
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s com.omnitiler.Planner
busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "<owner-name-from-above>"
readlink "/proc/<owner-pid>/exe"
tr '\0' ' ' < "/proc/<owner-pid>/cmdline"
busctl --user --no-pager status com.omnitiler.Planner
devenv shell --impure -- bash scripts/start-test.sh start
# Retain the printed CONTROLLER_OWNERSHIP_FILE receipt path and script ID.
# Every later stop must bind both: CONTROLLER_OWNERSHIP_FILE=<receipt> bash
# scripts/start-test.sh stop <script-id>.
```

1. Disable packaged KWin script.
2. Verify `isScriptLoaded` is `false`; reconfiguration can settle asynchronously.
3. Require `com.omnitiler.Planner` to be unowned. If it is owned, confirm
   its PID belongs to `omnitiler-planner.service`, stop only that unit,
   and recheck. If it is unmanaged or still owns the name, stop and report its
   PID and parent. Do not mask units: the D-Bus descriptor `Exec=` fallback can
   still activate the installed Planner.
4. Build and start only `target/debug/omnitiler planner-service` from
   this worktree. Record its output; derive the Planner PID from the D-Bus
   owner (`GetNameOwner` plus `GetConnectionUnixProcessID`), never from `$!`,
   and require `/proc/<pid>/exe` to be that worktree path or its exact
   kernel-generated ` (deleted)` form (never `/nix/store` after normalization),
   cmdline to contain `planner-service`, and the start
   identity to be captured.
5. `start-test.sh start` builds the KWin bundle, loads it once (duplicate
   plugin guard fails closed if already loaded), introspects the exact
   `Script<ID>`, runs it, and proves `isScriptLoaded=true` bound to the
   unchanged KWin PID/start identity. Retain its exact script ID and the
   `CONTROLLER_OWNERSHIP_FILE` receipt; the receipt is immutable and is the
   only teardown handle.

### Verify

- Observe `com.omnitiler.Planner` owned by the verified worktree Planner
  (`GetNameOwner` plus `GetConnectionUnixProcessID`, exe/cmdline/start checks).
- Confirm `isScriptLoaded s omnitiler-kwin` decodes to the strict
  `{"type":"b","data":[true]}` envelope.
- Confirm `just dev-status` reports `dev mode: UP`.
- Confirm the current world-route diagnostics above appear under the recorded
   KWin PID (`journalctl --user --no-pager _PID=<kwin-pid>`); loaded state alone never
  proves callbacks.

### Restore

```sh
CONTROLLER_OWNERSHIP_FILE=<receipt-from-start> devenv shell --impure -- bash scripts/start-test.sh stop <returned-script-id>
# Terminate only the recorded worktree Planner PID (re-verify exe/cmdline/start first).
devenv shell --impure -- bash scripts/dogfood-install.sh enable
```

`start-test.sh stop` requires both the nonce-owned receipt and the exact
script ID; a mismatched receipt, a missing receipt, or an already-unloaded
plugin fails closed without touching another script. Stopping/unloading does
not roll back window geometries the single world engine already
applied.

## Manual Single-Engine Journey (3-6 Windows)

Preconditions: `just dev-status` reports `dev mode: UP`. Record the KWin PID
from the receipt (`jq -r .pid <receipt>`) and open a journal cursor
(`journalctl --user --show-cursor`). Use 3 Wayland-native normal resizable
clients (for example 3 Konsole windows) on one output/workspace, then grow to
4-6 during the journey. All geometry changes arrive as one complete
`DescribeWorld` snapshot applied under per-property authority; explicit focus
changes arrive as one-shot effects after the required arrival/view proof.

1. Add (3 -> 4 windows): open a fourth client.
   Expected visible outcome: all four windows retile at once to cover the
   work area with no overlap; the new window is included in the layout.
2. Close (4 -> 3 windows): close one client.
   Expected visible outcome: the three survivors expand at once to cover the
   work area; no stale gap is left behind.
3. Focus change (pointer/activation, no shortcut): click another window or
   Alt-Tab to it.
   Expected visible outcome: focus follows the activated window; geometries
   do not change (membership-driven resync only refreshes the baseline).
4. Directional focus: press `Meta+H` / `Meta+J` / `Meta+K` / `Meta+L` (or
   `Meta+Left/Down/Up/Right`) with 3+ windows tiled.
   Expected visible outcome: focus moves one neighbor in that direction;
   geometries do not change.
5. Directional move: focus one window, press `Meta+Shift+H` / `Meta+Shift+J` /
   `Meta+Shift+K` / `Meta+Shift+L` (or `Meta+Shift+arrows`).
   Expected visible outcome: the focused window swaps order with its neighbor
   in that direction, all geometries are rewritten in canonical order, and
   focus is restored to the moved window.
6. Resize: focus one window, press `Meta+Alt+H/J/K/L` to grow toward that
   direction (outwards mode) or `Meta+Alt+Shift+H/J/K/L` to shrink from that
    direction (inwards mode). Grow also has arrow aliases
    `Meta+Alt+Left/Down/Up/Right`, and shrink has
    `Meta+Alt+Shift+Left/Down/Up/Right`. Plasma's default directional window
    switching occupies the grow-arrow chords until their conflicts are resolved.
    Repeat presses step further
   (`press_index` increments while focus/direction/mode/fingerprint are
   unchanged). Expected visible outcome: the focused edge moves one step,
   neighbors adjust to keep the work area covered, focus stays on the resized
   window. Grow to 5-6 windows and repeat 4-6 to confirm convergence without
   queues or retries.
7. Intentional float, sticky, and maximize are static-only. The controller
   request floats an
   eligible active normal window at a centered 60% work-area rectangle and
   removes it from the planner tree; toggling again is fresh admission. The
   float request carries no rectangle, so the Session selects the durable
   retained placement for a window that has floated before instead of
   recomputing the center; the unfloat request carries the window's live frame
   rect so a user moved/resized float is retained for the next float.
   Fullscreen and maximized targets refuse. `Meta+G` floats, `Meta+Shift+G`
   toggles sticky float, and `Meta+M` toggles maximize. Do not run these as a
   live journey without separate authorization. Grid View owns `Meta+G` and
   Krohnkite Monocle owns `Meta+M` in the observed session, so KGlobalAccel
   serial dispatch shadows the later project registrations until the user
   resolves each conflict in System Settings.

Physical shortcuts are required; `invokeShortcut` bypasses the xkb layer and
cannot prove delivery. KGlobalAccel records persist after unload and do not
prove live callbacks.

## Journal Line Forms

The only production KDE journal shapes are the current world-route
diagnostics above plus the world-route automatic fixed-float diagnostics and
the owner/tray records below. The removed legacy domain path's
`omnitiler:plan:*` operation tokens are intentionally undocumented here;
per-operation detail on the current path uses the same `omnitiler:world:`
prefix (see `kwin/src/world-entry.ts`, `kwin/src/world-apply.ts`,
`kwin/src/world-native.ts`, `kwin/src/world-drag.ts` and the Planner
`omnitiler:world-summary` / `omnitiler:intent-summary` emitters).

Filter by the recorded KWin PID only: `journalctl --user --no-pager _PID=<kwin-pid>`
(never `journalctl --system`). All emitted production diagnostics carry the
fixed `omnitiler:` prefix. `just dev` and `just dev verbose` retain
bounded lifecycle, terminal, refusal, and rejection evidence. `just dev trace`
additionally enables redacted per-window and hook detail plus the Planner's
bounded structural request/reply JSON. It does not capture raw native D-Bus
payloads. No line carries a caption or title. Most window-bearing trace lines carry
the stable `resource_class` application identifier as well as the opaque id; it
is not a caption and cannot contain document or page content. The timing probe
below carries only the opaque id and frame geometry.

Startup context (exactly one line per successful world entry start, using the
existing owner/generation provenance plus the compiled-in source revision;
`<source-rev>` is the installed build's bounded git revision, else
`local-dev`):

- `omnitiler:world:ready <state> source=<source-rev>`

### KDE open-animation timing probe (trace only)

- Enable through `just dev trace` (the launcher selects `npm run build:trace
  --prefix kwin`). This uses the existing `KWIN_TRACE_ENABLED` compile switch,
  `__OMNITILER_TRACE__="1"`; no runtime setting is added. Ordinary and installed
  builds explicitly disable it and strip probe work with `--minify-syntax`.
- Production `DescribeWorld` route only; no jump fix, admission delay change,
  extra capture, native write or timer. Trace builds alone attach a read/log-only
  `frameGeometryChanged` one-shot before each traced write. It disconnects on
  first signal, replacement, eviction, refused/thrown write or adapter stop.
- Format (one line per event, geometry in KWin frame coordinates):

  ```text
  omnitiler:world:trace win=<id> ev=<event> t=<wall-ms> [src=<source>] [req=<x,y,w,h>] [seen=<x,y,w,h|null> match=<0|1>]
  ```

| Event | Meaning / extra fields |
| --- | --- |
| `added` | Existing workspace `windowAdded` handler received this id |
| `debounce-start` / `debounce-fire` | Actual admission timer arm / callback |
| `send` / `reply` | `DescribeWorld` send / callback arrival; same `src`, only ids in that sent snapshot |
| `write` | Frame setter requested, not completion; `src`, `req` |
| `observed` | First subsequent frame signal; `src`, `req`, `seen`, exact `match=1` or mismatch/unreadable `match=0` |

- `t` is `Date.now()` wall-clock milliseconds, not monotonic time. Compare
  intervals only within one capture; an NTP/manual clock step can distort them.
  Cross-check journal monotonic receipt intervals; rerun negative or implausible
  intervals. Receipt time includes logging delay, and neither timestamp proves
  compositor presentation or animation completion.
- A window joining an already-armed debounce has no new `debounce-start`.
  Follow-up cycles can repeat debounce/send/reply events. Match flights by `src`;
  a reply can be malformed/stale and never write. A signal can fire synchronously
  inside the setter. No `observed` is expected without a write/signal, after a
  refused/thrown setter, or when already at the requested frame. Tracking is
  bounded to 16 pending ids and 16 source snapshots; use one window per capture.

#### User-run single-window capture

1. Follow the [live KWin guide](live-kwin-testing.md), including fresh baseline,
   helper accounting and evaluated readiness. Preserve the current open effect
   and animation speed; do not change them during a capture. Prepare the
   [fixture server and `run` helper](../crates/tiler-live-fixture/README.md#smoke-two-terminals-from-the-repo-root-after-an-env-restart)
   without spawning any windows. Prebuild the fixture to avoid a build pause at
   spawn. Run these commands from the repository root inside devenv.
2. In the capture terminal, retain a fresh journal cursor before starting the
   controller, then start the scoped trace controller in another terminal:

   ```sh
   CAPTURE_DIR="$(mktemp -d)"
   CURSOR="$(journalctl --user -n 1 -o json --no-pager | jq -er '.__CURSOR')"
   just dev-status
   ```

   ```sh
   just dev trace --scope-class OmniTilerTestWindow
   ```

   Stop on refusal or any ownership/readiness surprise; do not restart around
   an inert instance. Obtain the recorded KWin PID from the controller receipt
   and start the unfiltered stream in the capture terminal:

   ```sh
   RECEIPT="$(cat "$XDG_RUNTIME_DIR/omnitiler-dev/controller-receipt-path")"
   KWIN_PID="$(jq -er '.pid' "$RECEIPT")"
   journalctl --user --no-pager -f --after-cursor="$CURSOR" _PID="$KWIN_PID" -o short-monotonic | tee "$CAPTURE_DIR/kwin-journal.log"
   ```

3. Confirm ready/discovered and a matching dispatch/applied pair after the
   cursor. Start screen recording with the existing desktop recorder, capturing
   the entire target output, pointer and a visible capture terminal. Record the
   frame rate. Keep other windows stationary; do not change workspace, drag,
   resize or press tiling shortcuts. In the prepared fixture command terminal,
   create exactly one normal resizable window:

   ```sh
   run '{"v":1,"request_id":"timing-open","op":"spawn","windows":[{"id":"timing-one","size_px":[640,480]}]}'
   ```

4. Wait until the window settles, then stop recording and journal following
   with Ctrl+C. Save the video alongside the journal. Identify the single new
   `win` from `ev=added`; retain all its lines and surrounding dispatch/applied,
   stale/error and script-engine messages. Record video frame offsets for first
   visibility, open-effect end and tile jump, plus `added`-to-fire,
   send-to-reply and write-to-observed deltas. Note `match` and missing events;
   do not infer native completion from setter acceptance. The visible terminal
   is only a coarse video/log alignment cue, not a common presentation clock.
5. Close only `timing-one` with the fixture helper, shut down its server, and
   Ctrl+C the trace controller for its verified teardown. Follow the live guide's
   baseline comparison and exact cleanup. Check for script errors in the retained
   unfiltered current-PID stream. Keep artifacts outside the repository; repeat
   as a separate one-window capture if clock intervals are implausible.

World-route automatic fixed-float diagnostics:

- `omnitiler:world:focus-skipped kind=effect reason=<automatic-active|floating-active|sticky-active> window=<id> target=<id> id=<effect>`: automatic admission focus is suppressed over a focused float; explicit command effects are exempt.
- `omnitiler:world:automatic-focus-attempt req=<req> rev=<rev> target=<id> reason=planned-focus` and matching `automatic-focus-result ... outcome=<setter-accepted|setter-refused|threw> active=<true|false>` distinguish setter acceptance from native activation. Eligible automatic fixed-float targets never take admission activation.
- `omnitiler:world:automatic-keep-above window=<id> target=<target> outcome=<outcome>` records stacking readback/refusal; below-wins and external-clear release lines name the reason. Final state alone does not prove zero writes.

Intentional-float/fixed-window tile-override owner restart (ordinary output; one startup read and settled
membership writes, not per-frame snapshots):

- `[planner] omnitiler:intent-summary direction=egress op=<read|write> correlation=<correlation|-> outcome=<outcome> stored=<N|-> returned=<N|-> tile_stored=<N|-> tile_returned=<N|-> reason=<fixed-token|->`

`stored` on the write terminal is a separate storage acknowledgement,
not an atomic native/store commit. Read hydration occurs
before planning; membership only leaves recovered float frames untouched.
`tile_stored`/`tile_returned` count explicit fixed-window tile overrides;
they authorize normal tile placement, never saved positions or float writes.
Missing state is empty. Degraded reasons include `unreadable`, `corrupt`,
`namespace-mismatch` and `namespace-unavailable`; transport terminals include
`timeout`, `transport`, `malformed` and `oversize`. Reads settle empty on those
failures; failed writes retain local intent and the next settled membership
update sends the full set. There is no hold/retry scheduler. The brief startup
bootstrap defers command requests until the read settles or its deadline,
without queuing/replaying user commands. Summaries contain no native IDs,
application content, bus owners, geometry, paths or raw payloads. See the
[Q3 record](changes/archive/kde-intentional-float-restart.md).

Rust owner-monitor and tray startup diagnostics use fixed, redacted records:

- `omnitiler:route-diag component=planner stage=owner event=signal outcome=<malformed-signal|invalid-args>` (one malformed signal skipped; a later valid signal is processed)
- `omnitiler:route-diag component=tray-endpoint stage=owner event=startup outcome=query-failed` (startup KWin owner unknown; later authenticated live-owner publish can recover)
- `omnitiler:route-diag component=tray-endpoint stage=watcher event=query outcome=query-failed` (startup or later query failed; the watcher retry remains active)
- `omnitiler:route-diag component=tray-endpoint stage=watcher event=register outcome=registered` (live-confirmed watcher recovery)

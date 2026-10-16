# Development Loop

Cross-platform entry point: the root `justfile` requires just >= 1.56 and
conditionally imports `just/linux.just`, `just/windows.just`, and
`just/macos.just`; only the current OS recipes load.
`just dev [verbose|trace]` is the foreground entry point on each desktop. Linux
dispatches on `XDG_CURRENT_DESKTOP`: KDE runs `dev-kde`, GNOME reports
unsupported, anything else fails with the detected value. Windows runs the
native dev session (`just stop` recovers via the current payload pointer;
`just lifecycle-smoke [trace]` is the previous non-tiling loop; `just tile`
still requires explicit `--user-start`). macOS reports unsupported.

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
just dev-on      # disable packaged script, start worktree Planner + KWin bundle
just dev-status  # read-only: name owner, isScriptLoaded, receipt, unit state
just reload      # rebuild and swap only the recorded worktree Planner
just dev-off     # unload exact script, stop recorded Planner, re-enable packaged script
```

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
- `omnitiler:world:stale reason=<token> recovery=catchup`: zero stale setters,
  followed by an observation-only report.
- Planner `omnitiler:world-summary direction=ingress|egress ...`: bounded
  request/outcome correlation without raw payloads.

The operation-specific `DescribePlan` journal forms retained below describe
the legacy domain path and earlier acceptance evidence, not the current KDE
transport. Use the [full-state live checklist](changes/archive/core-full-state-replies.md#remaining-work-and-user-owned-live-checks)
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
- Confirm the single-engine journal shapes below appear under the recorded
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
not roll back window geometries the single DescribePlan engine already
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

Output move/send (item 5, offline verified; two-output native journey pending):

- Multiple readable adjacent outputs now select by window-centre projection,
  else span overlap, then left/top; reverse multiplicity is accepted.
  `omnitiler:plan:output-send-refused-ambiguous` is a retained refusal
  token for unreadable mover selection evidence, not candidate multiplicity;
  zero transfer writes. Unreadable topology reports
  `event=output-send outcome=scope-invalid reason=topology-unreadable`.
- `omnitiler:plan:busy-refused kind=output-send`: existing plan/send
  flight holds the route; no second dispatch.
- `omnitiler:route-diag component=cosmic-send stage=entry ... event=output-send`:
  `outcome=no-target` is a no-op; `refused`, `scope-invalid`,
  `native-refused` and `native-failed` describe refusals/failures.
  `gate=floating-boundary outcome=native-moved` is membership-only;
  `follow=followed|stayed|arrival-unconfirmed|focus-unconfirmed` reports the
  separate focus/visibility proof. Floating frames are not placement plans.
- Tiled output send reuses send diagnostics with
  `stage=arrival event=transfer outcome=transferred`, then `event=arrival outcome=arrived`;
  follow reports `state-confirmed`, stay `stay-confirmed`. Output and sole
  desktop membership must read back on the exact live mover; no desktop switch
  runs because the destination workspace is already current on its output.
- R4 move diagnostics/arrival fences now cover Up/Down as well as Left/Right.
  Full output rectangles select adjacency; carried placement remains work-area
  based. Failed/stale/timeout transfers force both-domain refresh without replay.

Workspace send follow/stay (item 2, offline verified; native journey pending):

- `omnitiler:route-diag component=cosmic-send stage=follow ... event=follow outcome=stay-confirmed`:
  fresh transfer arrived, source still selected; core-bound source MRU focus
  applied, or null desired focus required no setter. No desktop switch.
- The same stage emits `event=stay-pre|stay-focused` with existing redacted
  equality/count fields; `focus-unconfirmed`, `hooks-unavailable` and
  `arrival-unconfirmed` remain truthful terminal focus/arrival failures.
- `stage=entry ... outcome=native-moved follow=followed|stayed|arrival-unconfirmed|switch-unconfirmed|focus-unconfirmed|native-unavailable gate=floating-boundary`:
  membership-only boundary write succeeded; follow token reports the separate
  switch/focus result. `native-failed` or `native-refused` reports
  `follow=not-reached`, never a preservation/success claim.
- Relative target is frozen once from the scoped ring; `req_ord=-1` means
  relative/no numbered ordinal, `0` is append/trailing. Stay source-view drift
  before a reply produces `stale-revision` with zero writes; drift after the
  membership write produces `arrival-unconfirmed`, no hidden-source focus.
- `omnitiler:workspace:workspace-send-relative-absent:<reason>`:
  target resolution refused (`empty-ring`, `current-unknown`,
  `current-out-of-ring`, `no-active-output`, `unknown-output`, `target-removed`).

Workspace previous/relative diagnostics (opaque stable workspace IDs and
session-local output keys; native selection attempt completion is not visual
acceptance):

- `omnitiler:workspace:workspace-previous-recorded:<id>`: a changed
  current view was observed; `<id>` is the new current view, previous is the
  preceding observed ID. Native/send-follow/hotplug use the same observation.
- `omnitiler:workspace:workspace-previous-discarded:<key>`: disconnected
  output history and its observation baseline discarded.
- `omnitiler:workspace:workspace-previous-invalidated:<removed|out-of-scope>`.
- `omnitiler:workspace:workspace-previous-completed:<id>` and
  `omnitiler:workspace:workspace-relative-completed:<id>`: native
  selection seam invoked; recording follows observation, not setter return.
- `omnitiler:workspace:workspace-previous-absent:<no-active-output|unknown-output|no-history>`.
- `omnitiler:workspace:workspace-relative-absent:<no-active-output|unknown-output|empty-ring|current-unknown|current-out-of-ring|target-removed>`.
- `omnitiler:workspace:workspace-previous-no-op:already-there` and
  `omnitiler:workspace:workspace-relative-no-op:already-there`.

Whole-workspace output migration (R-WS-12, follow-only, current
offline-delivered baseline verified; native journey pending):

- `omnitiler:route-diag component=workspace-migrate stage=entry correlation=<correlation> generation=<generation> revision=0 diag_seq=-1 event=workspace-migrate outcome=<outcome> follow=not-reached gate=pre-commit phase=entry reason=<reason> req_ord=-1 inflight_stage=<stage>`:
  entry outcomes `invalid-direction`, `disabled`, `busy-send`,
  `busy-plan`, `mode-shared`, `mode-invalid`, `per-output-disabled`,
  `per-output-unreadable`, `no-target` (quiet no-op,
  `reason=no-adjacent-output`), `refused` (`reason=scope-unreadable`).
  A busy entry also emits `omnitiler:plan:busy-refused
  kind=workspace-migrate`; an unreadable scope also emits
  `omnitiler:plan:workspace-migrate-refused-scope`.
- `omnitiler:route-diag component=workspace-migrate
  route=migrate-workspace stage=<stage> correlation=<correlation>
  generation=<generation> revision=<revision> diag_seq=<seq>
  event=<event> outcome=<outcome>`: pre-commit refusals carry
  `follow=not-reached gate=pre-commit phase=request reason=<token>`
  with reasons `disabled`, `direction-invalid`,
  `transfer-unavailable`, `sequence-invalid`, `scope-invalid`,
  `mode-shared`, `mode-invalid`, `per-output-disabled`,
  `per-output-unreadable`, `same-output`, `workspace-mismatch`,
  `target-visible`, `desktop-cap`, `overlay-present`,
  `focus-mismatch`, `mode-unreadable`, `mode-mismatch`,
  `correlation-invalid`, `payload-invalid`, `request-over-cap`,
  `timeout`, `in-flight`.
- The `overlay-present` refusal above is the current offline-delivered
  baseline and is still implemented. User decision D8 2026-10-08 selects
  fullscreen+maximized carry instead; implementation pending, so no carry
  trace is claimed here.
- `omnitiler:workspace:workspace-migrate-refused:<shared-mode|unknown-output|unknown-workspace|out-of-scope|target-current|duplicate-target>`,
  `omnitiler:workspace:workspace-previous-invalidated:migrated`,
  `omnitiler:workspace:workspace-migrate-undisplaced`, and
  `omnitiler:workspace:workspace-migrate-completed` (map commit only,
  never a native-arrival claim).
- Terminals settle `release` (the settled refresh reconciles both
  domains from native observation) or `arrival`: `arrived` only after
  verified per-member arrival, views, and focus; `follow` reports
  `state-confirmed`, `focus-unconfirmed`, `arrival-unconfirmed`, or
  `hooks-unavailable`. A written-target/unwritten-source split settles
  `source-empty` (partial); other uncertain terminals include
  `member-closed`, `write-failed`, `switch-unconfirmed`,
  `output-unconfirmed`, `stale-revision`, `stale-scope`, `timeout`,
  and `arrival-timeout`. The Engine `planned` reply is a retained
  rekey, never a native-completion claim.

Filter by the recorded KWin PID only: `journalctl --user --no-pager _PID=<kwin-pid>`
(never `journalctl --system`). All emitted production diagnostics carry the
fixed `omnitiler:` prefix. `just dev` and `just dev verbose` retain
bounded lifecycle, terminal, refusal, and rejection evidence. `just dev trace`
additionally enables redacted per-window and hook detail plus the Planner's
bounded structural request/reply JSON. It does not capture raw native D-Bus
payloads. No line carries a caption or title. Window-bearing trace lines carry
the stable `resource_class` application identifier as well as the opaque id; it
is not a caption and cannot contain document or page content.

Native effect endpoint transitions (one initial state and, after initial
failure, one recovery on a later activation/reconfigure; no retry spam):

- `omnitiler:active-border:endpoint stage=failed service=<0|1> object=<0|1>` (first failed registration, including partial success before rollback)
- `omnitiler:active-border:endpoint available=<0|1>` (initial availability, then recovery if initially unavailable)
- `omnitiler:drag-oracle:endpoint stage=failed service=<0|1> object=<0|1>` (first failed registration, including partial success before rollback)
- `omnitiler:drag-oracle:endpoint available=<0|1>` (initial availability, then recovery if initially unavailable)
- `omnitiler:drag-oracle:press-spy available=<0|1>` (only if input was initially unavailable; late installation reports `1` once)

The name and object scalars describe that registration attempt only; `available=1`
requires both to succeed. These lines contain no native identities or payloads.

Startup context (exactly one line per successful plan entry start, using the
existing owner/generation provenance plus the compiled-in source revision;
`<source-rev>` is the installed build's bounded git revision, else
`local-dev`):

- `omnitiler:plan:ready owner=<owner> generation=<generation> source=<source-rev>`

Same-axis setting reload (ordinary output, only when the validated value changes
on `Options.configChanged`; affects subsequent move requests, no tree rebuild):

- `omnitiler:plan:config-reloaded stage=same-axis-move mode=<group-with-neighbor|swap-with-neighbor>`

Fixed-size predicate reload (same trigger; subsequent admissions only, no
reclassification of existing windows):

- `omnitiler:plan:config-reloaded stage=fixed-size-predicate predicate=<both-axes-fixed|either-axis-fixed>`

An unknown protocol `fixed_size_predicate` returns
`snapshot-invalid/fixed-predicate-invalid`; invalid KDE config falls back to
`both-axes-fixed` before dispatch.

Fixed-size automatic admission (normal output, only new classifications or
committed automatic membership, not per-frame polling):

- `[kwin] omnitiler:plan:fixed-size-classification op=<op> correlation=<correlation> evaluated=<N> classified=<N> reason=fixed-equal phase=classify`
- `[planner] omnitiler:fixed-size-admission op=<op> correlation=<correlation> evaluated=<N> admitted=<N> reason=fixed-equal`

The KDE line identifies classification before dispatch, not application.
The Planner line reports committed automatic membership; it does not prove
native placement or focus. Correlate the existing `planned-applied`/failure
terminal separately. These summaries carry no window/native IDs, geometry,
resource classes, captions or payloads. Admission adds no geometry, focus,
stacking or keep-above writes to the automatically floating client.

Intentional-float/fixed-window tile-override owner restart (ordinary output; one startup read and settled
membership writes, not per-frame snapshots):

- `[kwin] omnitiler:plan:intent-read correlation=<generation>-i<seq> outcome=<ok|degraded|rejected> stored=<N> returned=<N> tile_stored=<N> tile_returned=<N> reason=<fixed-token|->`
- `[kwin] omnitiler:plan:intent-write correlation=<generation>-i<seq> outcome=<stored|rejected|unavailable> stored=<N|-> tile_stored=<N|-> reason=<fixed-token|->`
- `[planner] omnitiler:intent-summary direction=egress op=<read|write> correlation=<correlation|-> outcome=<outcome> stored=<N|-> returned=<N|-> tile_stored=<N|-> tile_returned=<N|-> reason=<fixed-token|->`
- `[kwin] omnitiler:plan:intent-bootstrap-deferred kind=<operation>`

`stored` on the write terminal is a separate storage acknowledgement, not
`planned-applied` and not an atomic native/store commit. Read hydration occurs
before planning; membership only leaves recovered float frames untouched.
`tile_stored`/`tile_returned` count explicit fixed-window tile overrides;
they authorize normal tile placement, never saved positions or float writes.
Missing state is empty. Degraded reasons include `unreadable`, `corrupt`,
`namespace-mismatch` and `namespace-unavailable`; transport terminals include
`timeout`, `transport`, `malformed` and `oversize`. Reads settle empty on those
failures; failed writes retain local intent and the next settled membership
update sends the full set. There is no hold/retry scheduler. The brief startup
bootstrap defers plan/send requests until the read settles or its deadline,
without queuing/replaying user commands. Summaries contain no native IDs,
application content, bus owners, geometry, paths or raw payloads. See the
[Q3 record](changes/archive/kde-intentional-float-restart.md).

When a complete startup signal attachment fails, the entry remains inert and
retries on a later `windowAdded` or `Options.configChanged` event (one attempt
per event, no timer or attempt cap). Only transitions are logged:

- `omnitiler:plan:entry-attach stage=failed cause=<added|removed|activated|geometry|scope|maximize|enable-refused> recovery=retry-on-native-event`
- `omnitiler:plan:entry-attach stage=recovered`

Per dispatched `DescribePlan` flight, ordinary output retains one terminal
verdict line. Trace also records its route entry:

- `omnitiler:plan:cmd=<correlation> kind=<op> windows=<N> outcome=dispatch`
- `omnitiler:plan:cmd=<correlation> kind=<op> windows=<N> outcome=<outcome>`

The `outcome=dispatch` form is trace-only; the terminal form is ordinary.

where `<op>` is one of `admit|remove|move|focus|resize|reconcile|pointer-resize|toggle-float|toggle-orientation|drag-drop`,
`<correlation>` is `<generation>-p<seq>` (production: `plan-1-p<seq>`), `<N>`
is the observed window count, and terminal outcomes are `planned-applied`,
`rejected`, or the local fail-closed values `timer-failed`, `dbus-failed`,
`timeout`, `service-fault`, `correlation-mismatch`, `stale-dropped`,
`precondition-mismatch`, `stale-scope`, `write-failed`. Example pair:

- `omnitiler:plan:cmd=plan-1-p3 kind=admit windows=4 outcome=dispatch`
- `omnitiler:plan:cmd=plan-1-p3 kind=admit windows=4 outcome=planned-applied`

Per Rust rejection (one `rejected` kind line following the `outcome=rejected`
command line). `snapshot-invalid` includes its bounded Planner detail so the
invalid request cause is directly observable:

- `omnitiler:plan:rejected kind=<kind>`
- `omnitiler:plan:rejected kind=snapshot-invalid detail=<detail>`
- `omnitiler:plan:rejected kind=snapshot-invalid detail=window-out-of-bounds window=<id> resource_class=<class> rect=<rect> bounds=<rect>`

The third form is emitted when the adapter can identify the invalid carried
rectangle. It uses the same stable opaque id, resource class, and integer
geometry already emitted for applied writes.

Trace only: per applied geometry command (every member exactly one line, non-focus ops;
`<id>` is the stable opaque normalized window id, `<class>` is KWin's bounded
non-sensitive resource class, and `<rect>` is `x,y,w,h`):

- `omnitiler:plan:write window=<id> resource_class=<class> disposition=<written|skip-fullscreen|skip-maximized|skip-floating|skip-already-equal|write-failed|float-written|float-write-failed> rect=<rect>`

`write-failed` and `float-write-failed` remain ordinary failure evidence.

Per initially fullscreen window (once on hold and once on its first
non-fullscreen observation; `<id>` is the same opaque normalized window id):

- `omnitiler:plan:initial-fullscreen-held window=<id>`
- `omnitiler:plan:initial-fullscreen-released window=<id>`

Held windows are planner-only floating exceptions with no tile slot and no
geometry write. A subsequent fullscreen of a tiled window still retains its
slot and reports `skip-fullscreen` when a plan reflows siblings.

Per work-area/scope change (dedicated pair, never the generic reconcile line):

- `omnitiler:plan:scope-transition old=<old-rect> new=<new-rect>`
- `omnitiler:plan:work-area-reprojection selected=retained`

Per slice-2 pointer echo fence transition (one fixed token each):

- `omnitiler:plan:echo-fence-armed`
- `omnitiler:plan:echo-fence-consumed`
- `omnitiler:plan:echo-fence-cleared-equality`
- `omnitiler:plan:echo-fence-mismatched`

Per maximize-at-admission clear (first exit of a held born-fullscreen window
only; one attempt per live window identity, never a retry). Q3 first-seen
maximized admission, including R-MAX-03 floating-to-tiled, reserves a slot and
preserves maximize, so it emits no clear/echo lines; an applied
overlay reports `skip-maximized` with its reserved rectangle:

- `omnitiler:plan:maximize-admission-clear window=<id> resource_class=<class> outcome=<issued|invoked|missing|threw|observed-cleared|observed-maximized|observed-absent>`
- `omnitiler:plan:maximize-admission-echo-armed`
- `omnitiler:plan:maximize-admission-echo-consumed`
- `omnitiler:plan:maximize-admission-echo-mismatched`
- `omnitiler:plan:maximize-admission-echo-cleared-no-signal`

Explicit native state writes arm before their native call, consume only the
same Window object, and clear immediately when no synchronous echo arrives:

- `omnitiler:plan:maximize-toggle window=<id> resource_class=<class> target=<maximized|restored> outcome=<issued|invoked|missing|threw>`
- `omnitiler:plan:maximize-toggle-echo-armed|consumed|mismatched|cleared-no-signal`
- `omnitiler:plan:sticky-toggle window=<id> resource_class=<class> target=<all-desktops|current-desktop> outcome=<issued|invoked|missing|threw>`
- `omnitiler:plan:sticky-echo-armed|consumed|mismatched|cleared-no-signal`

`observed-cleared` confirms that the immediate re-observation saw restore.
`observed-maximized` records that KWin still reported maximize after the one
call, including an application that immediately reasserted it; it is then the
ordinary post-admission maximize-isolation case, with no further clear.
`missing` and `threw` distinguish an unavailable method from a native-call
exception.

Per refusal/rejection path (exact distinct tokens, one per cause; every
shortcut and pointer-route refusal carries its own fixed token):

- `omnitiler:plan:focus-refused-disabled` (shortcut focus while the adapter is disabled)
- `omnitiler:plan:focus-refused-invalid-direction`
- `omnitiler:plan:focus-refused-observe`
- `omnitiler:plan:focus-refused-floating` (directional focus on an intentionally floating active window)
- `omnitiler:plan:move-refused-disabled`
- `omnitiler:plan:move-refused-invalid-direction`
- `omnitiler:plan:move-refused-observe`
- `omnitiler:plan:move-refused-floating` (directional move on an intentionally floating active window)
- `omnitiler:plan:move-refused-fullscreen` (directional move refused on a fullscreen focused window)
- `omnitiler:plan:move-refused-maximize` (directional move refused on a maximized focused window; fullscreen wins when both)
- `omnitiler:plan:resize-refused-disabled`
- `omnitiler:plan:resize-refused-invalid-direction`
- `omnitiler:plan:resize-refused-invalid-mode`
- `omnitiler:plan:resize-refused-observe`
- `omnitiler:plan:resize-refused-floating` (directional resize on an intentionally floating active window)
- `omnitiler:plan:resize-refused-fullscreen` (directional resize refused on a fullscreen focused window)
- `omnitiler:plan:resize-refused-maximize` (directional resize refused on a maximized focused window; fullscreen wins when both)
- `omnitiler:plan:toggle-orient-refused-disabled`
- `omnitiler:plan:toggle-orient-refused-observe` (no valid observation or tiled focused target)
- `omnitiler:plan:toggle-orient-refused-floating` (floating or excluded active subject)
- `omnitiler:plan:toggle-orient-refused-workspace-floating`
- `omnitiler:plan:toggle-orient-refused-fullscreen` (focused overlay only, like resize)
- `omnitiler:plan:toggle-orient-refused-maximize` (focused overlay only; fullscreen wins when both)
- `omnitiler:plan:pointer-refused-disabled`
- `omnitiler:plan:pointer-refused-identity`
- `omnitiler:plan:pointer-refused-direction`
- `omnitiler:plan:pointer-refused-boundary`
- `omnitiler:plan:pointer-refused-observe`
- `omnitiler:plan:pointer-refused-absent`
- `omnitiler:plan:pointer-refused-fullscreen` (pointer-resize target refused while fullscreen)
- `omnitiler:plan:pointer-refused-maximize` (pointer-resize target refused while maximized; fullscreen wins when both)
- `omnitiler:plan:maximize-signal-unavailable` (logged once if an eligible window lacks `maximizedChanged`; tiling continues using fresh `maximizeMode` reads)
- `omnitiler:plan:maximize-refused-signal` (maximize subscription infrastructure unavailable)
- `omnitiler:plan:float-refused-disabled` (toggle-float while the adapter is disabled)
- `omnitiler:plan:float-refused-observe` (toggle-float target not found in the observation)
- `omnitiler:plan:float-refused-not-tiled` (toggle-float on an active-excluded or otherwise non-tiled target)
- `omnitiler:plan:float-refused-fullscreen` (toggle-float on a fullscreen focused window)
- `omnitiler:plan:float-refused-maximize` (toggle-float on a maximized focused window; fullscreen wins when both)
- `omnitiler:plan:sticky-refused-disabled|observe`
- `omnitiler:plan:sticky-refused-fullscreen|maximize|untracked|attempted window=<id> resource_class=<class>`
- `omnitiler:plan:maximize-refused-disabled|observe`
- `omnitiler:plan:maximize-refused-fullscreen|attempted window=<id> resource_class=<class>`
- `omnitiler:plan:drag-drop-refused-disabled|identity|coords|observe|absent|fullscreen|maximize|floating|cross-domain` (one per tiled move-drop refusal cause; `coords` covers a missing or out-of-range finish pointer capture)
- `omnitiler:plan:busy-refused kind=<focus|move|resize|toggle-float|toggle-orientation|toggle-sticky|toggle-maximize|drag-drop|drag-preview>` (shortcuts refuse busy; a move drop refuses during an R4 flight, otherwise defers behind an ordinary flight; preview backs off without queuing)
- `omnitiler:plan:reconcile-accepted windows=<count> cause=stable-drift recovery=accept-client-rect` (bounded reassertions exhausted; exact per-window geometry accepted)
 - `omnitiler:plan:stale-replan` (pre-write stale reply replanned once against fresh complete observation)
- `omnitiler:plan:shortcut-failed action=<action> sequence=<sequence>` (per failed shortcut registration)
- `omnitiler:plan:shortcut-dispatch-shadowed action=<action> sequence=<sequence> holder_component=<component> holder_action=<action>`

Additional ordinary lifecycle terminals and transitions:

- `omnitiler:plan:drag-reconcile-settled correlation=<drag-N> outcome=unavailable plan=none` (no observed domain or proven workspace/output removal from a complete topology list; never an applied claim)
- `omnitiler:plan:highlight-attach stage=failed reason=bridge-unavailable` (group bridge startup failed; logged once until recovery)
- `omnitiler:plan:highlight-attach stage=recovered` (later Plan-applied/config event reattached the group bridge; logged once)

Rust owner-monitor and tray startup diagnostics use fixed, redacted records:

- `omnitiler:route-diag component=planner stage=owner event=signal outcome=<malformed-signal|invalid-args>` (one malformed signal skipped; a later valid signal is processed)
- `omnitiler:route-diag component=tray-endpoint stage=owner event=startup outcome=query-failed` (startup KWin owner unknown; later authenticated live-owner publish can recover)
- `omnitiler:route-diag component=tray-endpoint stage=watcher event=query outcome=query-failed` (startup or later query failed; the watcher retry remains active)
- `omnitiler:route-diag component=tray-endpoint stage=watcher event=register outcome=registered` (live-confirmed watcher recovery)

Intentional-float Planner snapshot-invalid details:

- `toggle-float-op-invalid`
- `toggle-float-window-invalid`
- `float-rect-invalid`

Parent-orientation Planner snapshot-invalid details:

- `toggle-orient-op-invalid`
- `toggle-orient-window-invalid`

`kind=toggle-orientation` uses ordinary plan diagnostics and terminal handling.
A lone root leaf returns Rust `unchanged` without setters; floating subjects
return `not-tiled`. Sibling fullscreen/maximized members retain native state
and report existing `skip-fullscreen`/`skip-maximized` write dispositions.

Trace only: per window excluded before admission (one line per unchanged exclusion state for
an identified window; `<id>` is the normalized window id, or `unknown` when
KWin cannot provide one. `<class>` is KWin's non-sensitive resource class, or
`unknown` when unavailable):

- `omnitiler:plan:observe-excluded reason=<active-normal-window|normal-window|output-missing|output-mismatch|desktop-mismatch|frame-rect-missing|frame-rect-coordinate-invalid|frame-rect-size-invalid|frame-rect-coordinate-out-of-range|frame-rect-size-out-of-range> window=<id> resource_class=<class>`

Trace only: normal drag pull and verdict lines. Ordinary output retains the
failure tokens below plus the `drag-drop-refused-*` tokens above; the pointer
route's adapter emits the exact `pointer-refused-*` token above per cause:

- `omnitiler:route-diag:drag-pull action=dispatch`
- `omnitiler:route-diag:drag-verdict cancelled=<true|false> correlation=<drag-N> reason=<reason>`
- `omnitiler:route-diag:drag-drop-dispatched correlation=<drag-N> accepted=<true|false>` (tiled move finish routed to `kind=drag-drop`; `accepted=true` means the intent entered or deferred into the single flight, not that geometry applied; `false` means refused or failed before entering it, with a cause token and drag-rejection marker)
- `omnitiler:plan:drag-preview-backed-off correlation=<drag-N> reason=busy` (a sampled read-only preview yielded to the command flight; no queued request)
- `omnitiler:plan:drag-preview-settled correlation=<drag-N> outcome=<applied|refused|stale> reason=<token>` (only `applied` supplies a slot; refusal hides it)
- `omnitiler:route-diag:drag-preview-shown correlation=<drag-N>` / `drag-preview-cleared correlation=<drag-N> reason=<finish|refused|terminal>` (edge-only native setter/clear requests; no visual-delivery claim)
- `omnitiler:plan:drag-drop-cross-output correlation=<drag-N> source=cross-output dest=destination` / `drag-drop-cross-applied correlation=<drag-N> plan=<plan-id>` / `drag-drop-cross-refused correlation=<drag-N> reason=<token>` (cross-output destination dispatch, actual applied plan or terminal failure; no raw output or workspace IDs)
- `omnitiler:route-diag:drag-drop-thrown correlation=<drag-N>` (synchronous routing exception, fail-closed)
- `omnitiler:route-diag:drag-call-missing` (pull with no call binding)
- `omnitiler:route-diag:drag-call-thrown` (pull whose D-Bus call threw)
- `omnitiler:route-diag:drag-reply-invalid` (any malformed or unparseable reply)
- `omnitiler:route-diag:drag-route-missing` (non-cancelled verdict with no pointer route installed)
- `omnitiler:route-diag:drag-context-invalid`
- `omnitiler:route-diag:drag-scope-invalid`
- `omnitiler:route-diag:drag-unknown-window`
- `omnitiler:route-diag:drag-ref-mismatch`
- `omnitiler:route-diag:drag-start-missing`
- `omnitiler:route-diag:drag-move-ignored`
- `omnitiler:route-diag:drag-start-invalid`
- `omnitiler:route-diag:drag-edge-invalid`

Drag-oracle entry startup refusal (exactly one token per refused
`startDragOraclePullEntry` start, one per cause; the entry returns null):

- `omnitiler:route-diag:drag-entry-workspace-missing`
- `omnitiler:route-diag:drag-entry-call-missing`
- `omnitiler:route-diag:drag-entry-call-thrown`
- `omnitiler:route-diag:drag-entry-list-missing`
- `omnitiler:route-diag:drag-entry-list-thrown`
- `omnitiler:route-diag:drag-entry-list-invalid`
- `omnitiler:route-diag:drag-entry-finished-invalid` (a connectable finished signal that failed to attach)
- `omnitiler:route-diag:drag-entry-no-windows` (empty window list)
- `omnitiler:route-diag:drag-entry-no-finished` (windows exist but none expose a connectable finished signal)
- `omnitiler:route-diag:drag-entry-added-invalid` (missing or non-connectable windowAdded signal)
- `omnitiler:route-diag:drag-entry-added-connect-failed` (windowAdded attach returned null or threw)

Tiled move-drop lifecycle (shipped offline, live check pending): the finish context captures
`workspace.cursorPos` synchronously at FINISH before the async oracle pull;
the verdict carries no pointer and the route never reads the cursor at
reply. The entry calls `adapter.requestDragDrop` with that captured point
plus the verdict window identity through the shared single flight (deferred
when busy, never concurrent). Stepped tiled moves sample the pointer into
read-only Planner `drag-preview` requests on fresh, size-hinted destination
observations; one sample is outstanding at a time and a later step coalesces.
Both preview and final drop bind the observed mover as the logical focused
window for drag placement, even if KWin's active window is another tile.
The preview's `drag-N` correlation is local to Started; the final verdict's
`drag-N` comes from the native oracle and may have a different number. The
completed preview prior transfers across those two correlations.
While KWin reports an interactive move or resize, ordinary reconciliation
holds the gesture window even across pauses longer than the old missing-Finished
bound. A missing Finished releases that hold on a subsequent observation only
after KWin reports the gesture ended; the old Started-keyed move/resize timeout
tokens are retired.
The pointer selects the preview domain, with a read-only prospective mover
insertion when native reassignment has not yet arrived. The preview result
and its exact hover prior are scoped to the Started identity/revision;
Finish clears the independent native overlay and fences late replies while
preserving the last completed prior for the final drop. The native setter
accepts a translucent filled target rect above windows; center, refusal and
cancellation leave it hidden. A valid Started source-output mismatch dispatches
to the destination and successful application forces complete source-domain
reconciliation; the destination restore marker cannot fight the source.
Without a Started binding, retained `appliedById` evidence still guards
cross-domain direct callers. If native output has not yet reassigned at Finish,
the adapter uses the pointer's destination snapshot, sends the mover to that
output, sets its target desktop, and checks fresh complete destination
membership before applying planned geometry; refusal takes the existing
destination marker path. See the offline verification and remaining-size
rationale in `changes/archive/cross-output-drag-preview.md`.

Map each journey step above to one Plan command: add -> `kind=admit`, close ->
`kind=remove`, directional focus -> `kind=focus`, directional move ->
`kind=move`, resize -> `kind=resize`, intentional float toggle ->
`kind=toggle-float`, tiled move drop -> `kind=drag-drop` (ordinary terminal
plus trace dispatch, same fences as other ops). Ordinary output emits its
terminal outcome; trace also emits its dispatch line. Pointer focus change
alone emits no command line. A
rejection still emits its ordinary terminal and rejection-kind lines, then
recovers on the next fresh observation; the adapter never disables itself after
a reply.

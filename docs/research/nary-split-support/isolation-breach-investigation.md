# Nested-KWin Isolation Breach Investigation

## Scope and labels

- **Established fact:** this investigation is read-only. E4 remains parked; no compositor, probe, launcher, or D-Bus mutation was invoked.
- **Established fact:** repository citations use `path:line`; attempt citations use bare evidence filenames and their file lines. The supplied hash timeline is cited as "objective timeline" because it is a required input rather than a retained attempt artifact.
- **Established fact:** `Established fact` is direct repository, artifact, journal, or supplied-timeline evidence. `Inference` follows from that evidence. `Assumption` is a proposed future control or evidence requirement, not a claim about the observed writer.

## Outcome

- **Established fact:** host `~/.config/kwinrc` was original SHA-256 `905375112bbf8d9c8b882f687bd71eb1cb8eeb69a31ed657585889b9320e2fe8` at `13:47:39.187236714 +1000`, changed to `99167e972b9131f461e184b7cdabe899faa45767a2323e345ac29b4821885cd4` with mtime `14:00:45.572732578 +1000`, and the attempt classified that as `hard-stop-host-hash-changed`. `host-kwinrc-before.sha256:1`, `host-kwinrc-before.mtime:1`, `host-kwinrc-after.sha256:1`, `host-kwinrc-after.mtime:1`, `isolation-hash-status.txt:1`
- **Established fact:** the objective timeline records the original SHA at `13:47:39`, the divergent SHA at `14:00:45`, the original SHA again at `14:00:48`, and stability at `14:06:16`.
- **Established fact:** divergent `kwinrc` bytes were not retained. The retained evidence has only before and after SHA-256 values and mtimes, not an atomic byte copy, byte count, diff, or a writer identity. `run.sh:49-52`, `run.sh:83-109`
- **Inference:** the definitive result is an isolation breach or concurrent host-configuration write during the attempt window, but the cause is undetermined. The evidence cannot characterize the divergent content, identify the writer, or establish that the nested compositor caused it rather than a concurrent host process.
- **Established fact:** the nested compositor was launched at `2026-08-21T14:00:42,524518562+10:00`, returned READY as launcher PID `294663` and nested PID `294667`, and cleanup completed between `14:00:46,504559958+10:00` and `14:00:46,616734623+10:00`. `attempt-start.txt:1`, `launcher.out:1`, `launcher.pid:1`, `nested.pid:1`, `attempt-before-cleanup.txt:1`, `cleanup-result.txt:1`, `attempt-end.txt:1`
- **Inference:** the supplied return to the original SHA at `14:00:48`, after the recorded nested-process cleanup, is consistent with a transient write but does not identify its source or mechanism.
- **Assumption:** cause can only be settled by a future, separately authorized attempt that atomically retains every observed host `kwinrc` version (bytes, SHA-256, byte count, mtime), records a kernel/audit writer PID and executable for each write, and correlates that record with the nested PID, host KWin PID, process environment, and private-bus traffic. A hash-only before/after comparison cannot settle the cause.

## Actual Attempt Sequence

- **Established fact:** the preserved invocation was `bash scripts/nested-kwin-spike.sh <nested-workdir>`. `launch-command.txt:1`
- **Established fact:** the attempt first recorded the host hash and mtime, recorded the launch command, launched the nested compositor, captured the launcher status, read the three generated identity files, issued `loadScript`, held for three seconds, cleaned up owned PIDs, then recorded final host hash and mtime and private-config evidence. `run.sh:49-109`
- **Established fact:** the launcher creates a fresh work directory, then private `config`, `cache`, `data`, `state`, and `runtime` directories; it sets runtime mode `0700`; it launches KWin in `dbus-run-session`; and it waits for private-bus KWin introspection before returning the nested PID. `scripts/nested-kwin-spike.sh:21-45`
- **Established fact:** the private runtime directory was mode `700`, owned by the invoking user, and the private `kwinrc` existed with mode `600`, size `311`, and mtime `14:00:45.688873990 +1000`. `private-runtime-proof.txt:1`, `private-kwinrc-proof.txt:1`
- **Established fact:** the retained private `kwinrc` contains a nested desktop, nested output UUID, a four-child horizontal tiling record, and Xwayland scale. `kwinrc:1-11`
- **Established fact:** the probe did not run. The retained load call exited `0` and emitted `i 0`; the parser wrote `invalid-load-reply:i:0:`, no probe marker was found, and marker extraction exited `1`. `load-script-status.txt:1`, `load-script.out:1`, `probe-blocker.txt:1`, `probe-output.txt:1`, `probe-output-status.txt:1`

## Strict Loader Parser Defect

- **Established fact:** the preserved attempt invokes plain `busctl --address="$BUS" call ... loadScript s "$E/probe.js"`, then whitespace-splits its output into `LOAD_TYPE`, `SCRIPT_ID`, and `LOAD_EXTRA`. It permits execution only if the type token is `u`, the ID is decimal digits, and no extra token exists. `run.sh:62-73`
- **Established fact:** the actual reply was the signed-integer D-Bus signature `i 0`; therefore the `u` predicate rejected the valid zero script ID before it could call `org.kde.kwin.Script.run`. `load-script.out:1`, `run.sh:66-73`
- **Established fact:** the repository's documented strict contract says `/Scripting` `loadScript(s)` and `loadScript(ss)` return `i`, with valid JSON form `{"type":"i","data":[ID]}` and `ID` in `0..2147483647`. `docs/live-kwin-testing.md:180-189`
- **Inference:** the defect is not strict validation itself. It is a signature mismatch: the attempt parser expects unsigned `u`, while both its own retained reply and the repository contract use signed `i`.
- **Established fact:** the production launcher uses `busctl --json=short` and validates the documented JSON envelope before addressing `/Scripting/Script<ID>`. `scripts/start-test.sh:436-452`
- **Assumption:** a future attempt should use that JSON-envelope contract or an equivalent parser that accepts exactly signed `i` and the documented non-negative range, while retaining raw stdout before parsing. No parser change is made by this investigation.

## D-Bus-to-Host Hypothesis

- **Established fact:** the launcher starts KWin under `dbus-run-session`, writes the resulting `DBUS_SESSION_BUS_ADDRESS` to `bus.txt`, and uses `busctl --address="$(cat "$BUS")"` for readiness. `scripts/nested-kwin-spike.sh:30-39`
- **Established fact:** the retained private-bus address is an attempt-private address (details dropped); the attempt's later `loadScript` call uses that same private address. `bus.txt:1`, `run.sh:58-73`
- **Established fact:** the launcher deliberately connects the nested Wayland backend to the host's absolute `/run/user/<uid>/wayland-0` socket. `scripts/nested-kwin-spike.sh:11-16`, `scripts/nested-kwin-spike.sh:30`
- **Established fact:** the following exact read-only user-journal commands were run, each with the required `14:00:40` through `14:00:50` window, and each returned `-- No entries --`:

```sh
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=kwin_wayland
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=kwin
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=plasmashell
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=kded
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=dbus-daemon
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=dbus-broker
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _COMM=dbus-run-sess
journalctl --user --since '2026-08-21 14:00:40' --until '2026-08-21 14:00:50' -o short-iso --no-pager _SYSTEMD_USER_UNIT=dbus.service
```

- **Inference:** the D-Bus-to-host hypothesis is **undetermined**, not confirmed or refuted. The evidence proves use of an attempt-private session-bus address for the explicit KWin calls, but it has no D-Bus message trace, bus peer list, service-activation record, host writer PID, or relevant journal record. Empty filtered journals do not disprove an unlogged D-Bus path.
- **Established fact:** the private state file contains entries for host-specific application desktop IDs including `com.mitchellh.ghostty.desktop`, `org.kde.dolphin.desktop`, and `org.kde.konsole.desktop`. `kglobalshortcutsstaterc:177-187`
- **Inference:** that state is evidence of data ingress into the private tree, but it does not establish whether its source was session D-Bus, a host filesystem fallback, a service default, or another inherited channel. It cannot confirm the D-Bus-to-host hypothesis.

## Isolation Escape Inventory

- **Established fact:** the script explicitly overrides `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR`, and `KDEHOME` only inside the `dbus-run-session` child. `scripts/nested-kwin-spike.sh:4-8`, `scripts/nested-kwin-spike.sh:28-30`
- **Inference:** these explicit homes protect the primary per-user config, data, cache, state, runtime, and KDE home paths for the child, but the script does not use a sanitized environment. Any unmentioned parent variable remains an inheritance opportunity.
- **Established fact:** the script does not explicitly set or clear `HOME`, `XDG_CONFIG_DIRS`, `XDG_DATA_DIRS`, `DISPLAY`, `XAUTHORITY`, `DBUS_SYSTEM_BUS_ADDRESS`, `DBUS_STARTER_ADDRESS`, `SSH_AUTH_SOCK`, `PULSE_SERVER`, `KDE_*` variables other than `KDEHOME`, `QT_*` variables, graphics cache variables, `PATH`, or dynamic-loader variables. `scripts/nested-kwin-spike.sh:21-30`
- **Inference:** `HOME` and inherited XDG search-path variables are possible host config/data fallback channels; inherited display, agent, media, system-bus, KDE/Qt, graphics-cache, path, and loader variables are possible host IPC, state, executable-selection, or process-behavior channels. Their actual values and KWin's use of each were not captured, so none is an established cause.
- **Established fact:** the script's only explicit host runtime reference is `WAYLAND_DISPLAY="/run/user/$4/wayland-0"`, passed to `--wayland-display`. `scripts/nested-kwin-spike.sh:11-16`, `scripts/nested-kwin-spike.sh:30`
- **Inference:** that host Wayland socket is an intentional and necessary isolation boundary crossing. It is a direct host-compositor interaction channel and must be considered independently from session D-Bus.
- **Established fact:** the actual private session-bus socket was placed under `/tmp`, not under `$WORKDIR/runtime`; the script neither selects a per-run bus directory nor verifies the bus-socket ownership and mode. `bus.txt:1`, `scripts/nested-kwin-spike.sh:26-30`
- **Inference:** the session bus is private by address but not demonstrably contained in the evidence tree. This is a same-UID session-bus exposure and retention gap, not evidence that the host session bus was used.
- **Established fact:** the work directory must be absent, but the script accepts any caller-selected path and verifies neither canonical path ancestry, owner, nor modes for the work directory, config, data, cache, and state directories. Only `runtime` is chmodded `0700`. `scripts/nested-kwin-spike.sh:21-30`
- **Inference:** a hostile or shared parent directory, symlinked ancestor, permissive umask, or same-user process can weaken filesystem containment outside `runtime`. No such condition is established for this attempt.
- **Established fact:** `KWIN_BIN` is an inherited override and the script executes it without pinning or validating it. `scripts/nested-kwin-spike.sh:22-23`, `scripts/nested-kwin-spike.sh:30`
- **Inference:** executable substitution is an integrity escape opportunity for a caller-controlled environment, not evidence of this breach because the artifact does not retain `KWIN_BIN`.
- **Established fact:** the launcher has no copy command and no explicit read of `~/.config/kwinrc`; it creates fresh private directories and starts KWin. `scripts/nested-kwin-spike.sh:26-30`
- **Established fact:** the attempt postflight only proves that a private `kwinrc` exists; it does not record a host/private byte comparison, provenance, or copy operation. `run.sh:89-93`, `private-kwinrc-proof.txt:1`
- **Inference:** no literal host-config copy is established. The private `kwinrc`, shortcut config, and shortcut state are copies or generated state only in the generic sense that they are private files; whether any content was copied from host configuration is undetermined.

## User Ruling and Accepted Residuals

- **User ruling:** the forensic question changes nothing. The host write self-reverted within three seconds and the current host `kwinrc` content is byte-identical to its original content; divergent bytes were never captured, only their SHA-256 divergence.
- **User ruling:** writer attribution would require `fanotify` with a global mark, an `auditd` watch rule, or `bpftrace`; each requires root. `docs/../../decisions/kde.md#live-test-evidence-boundary` prohibits `sudo` without separate approval. The writer and mechanism are **UNDETERMINED** and will remain undetermined.
- **User ruling:** host `/Scripting` `loadScript`/`unloadScript` is already authorized, and host dogfooding proved that path end-to-end. No nesting is needed to exercise that authorized path.
- **Accepted unresolved risk:** a child might escape `DBUS_SESSION_BUS_ADDRESS`, reach host KWin, and reconfigure it. This D-Bus fallback is neither confirmed nor refuted. The missing divergent bytes and prohibited attribution mean these are unresolved-and-accepted risks, not benign-by-proof.
- **Established fact:** `scripts/nested-kwin-spike.sh` remains unhardened. Its line 30 `dbus-run-session -- /bin/sh -c ...` has no `env -i`, inherits the full parent environment, and exports `WAYLAND_DISPLAY="/run/user/$UID/wayland-0"`, the absolute host Wayland socket.
- **User ruling:** if nesting is ever needed, cheap containment options are a host `kwinrc` backup/restore or `bubblewrap`. `bubblewrap` is neither in `devenv.nix` nor on `PATH`.

## Proposed Guide Hardening - SUPERSEDED/DROPPED

The following proposals are retained as readable history only. They are
SUPERSEDED/DROPPED by the user ruling above and are not approved work.

- **Assumption (proposal):** amend `docs/live-kwin-testing.md` to require a fail-closed sanitized child environment: start from `env -i`, add only required execution variables and explicit safe system search paths, set every XDG home, and explicitly unset host session, starter, display, agent, media, loader, KDE, Qt, and graphics-cache variables unless an individually documented need exists.
- **Assumption (proposal):** amend the nested-launch contract to record the complete child environment, canonical workdir ancestry/ownership/modes, selected KWin executable and hash, private-bus address/socket metadata, and parent Wayland socket metadata before the compositor is allowed to mutate.
- **Assumption (proposal):** amend the contract to make the private D-Bus socket reside in an attempt-owned `0700` directory, then retain bus peer/service-activation evidence sufficient to distinguish the private session bus from host and system buses.
- **Assumption (proposal):** treat the absolute host Wayland socket as an explicit permitted host boundary crossing, not as proof of full isolation; require a stated threat model and a fresh host-writer baseline around it.
- **Assumption (proposal):** strengthen postflight to retain atomic before, intermediate-on-change, and after host `kwinrc` bytes with SHA-256, byte counts, mtimes, and writer attribution. A hash change remains a HARD STOP and must preserve the divergent bytes before cleanup or reporting.
- **Assumption (proposal):** require a parser gate before any future E4 attempt: retain raw `loadScript` output and accept only the documented signed `i` JSON envelope with a non-negative 32-bit ID, then introspect the returned script object before `run`.

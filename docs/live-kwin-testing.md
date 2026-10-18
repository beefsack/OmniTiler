# Live KWin/Plasma Testing Guide

Read this guide before planning or running live KWin/Plasma work. It is the
authoritative safety and operational contract for this repository; it does not
grant authorization beyond [Current Decisions](decisions.md#live-kwinplasma-boundary).

## Safety Boundary

- Preserve real windows, desktops, outputs, Krohnkite state, and unrelated
  configuration. Touch only identified project resources. Never use broad
  cleanup.
- Reversible, project-scoped live host tests may run under this guide and
  `docs/decisions.md` when exact restoration is verifiable. The user performs
  physical or manual observations and every session boundary.
- Stop on an ownership, parser, diagnostic, baseline, or restoration surprise.
- Do not use `sudo`, system plugin paths, Home Manager-managed files, external
  dotfiles, unrelated environment entries, unrelated host mutation,
  irreversible cleanup, or ambiguous-residue deletion.

## Nested KWin Isolation

- A nested `kwin_wayland` run must use fresh private `XDG_CONFIG_HOME`,
  `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR`, and
  `KDEHOME`, all scoped to its run directory. The private runtime directory
  must be user-owned mode `0700`.
- Use a private `dbus-run-session`, the proven `--socket nested-kwin-spike
  --width 640 --height 480 --no-global-shortcuts --no-kactivities
  --no-lockscreen` launcher shape, and never `--windowed`.
- A private runtime directory requires the parent Wayland display as an absolute
  path passed through `--wayland-display`. Never use the host runtime directory
  or remove host sockets.
- Before and after a bounded nested run, record the host `~/.config/kwinrc`
  SHA-256 and nanosecond mtime as a read-only diagnostic. Drift is a warning,
  never a veto and never restored or mutated: exact manifest-bound cleanup
  proceeds only after independent checkpoint isolation proof (exact private
  `HOME`/`XDG_*`/`KDEHOME`/bus/socket plus canonical wrapper identity,
  PID/start-tick, and workdir, with no host config path retained in the
  checkpoint environ/cmdline/known FDs/maps). Checkpoint-only: the proof
  validates retained state at that instant and does not prove against host
  config opened then closed (or mapped then unmapped) before the checkpoint.
  Confirm that the nested run created and used its own private `kwinrc`.

## KWin Script Lifecycle

- KWin executes the generated non-module ES2017 IIFE only. Do not ship ESM,
  Node imports, source maps, optional catch bindings, or manually edited output.
- `/Scripting` returns a signed `i` script ID from `loadScript(s)` or
  `loadScript(ss)`. `-1` is the failure sentinel; retain and strictly parse the
  raw result, accept only a non-negative 32-bit ID, introspect that exact script
  object, and run only it. `0` is a valid first ID and maps to
  `/Scripting/Script0` only when returned. Never guess `Script0`, call global
  `Scripting.start`, or use a KPackage as a test path.
- The current public host `/Scripting` API has no loaded-script enumeration,
  Script-object plugin/source identity, duplicate count, running-state property,
  or handler-absence proof. Do not unload a pre-existing production script for a
  host pilot unless an authoritative exact in-memory source/object restore route
  exists. A boolean `isScriptLoaded` reply and checkout hash do not provide one.
- `scripts/start-test.sh` is the manual lifecycle interface. `start` builds,
  loads, runs, and waits for current-PID readiness diagnostics; `status` and
  `diagnostics` are read-only; `stop` unloads only the exact project script.
  `reconcile-shortcuts --apply` is a shortcut mutation and always needs explicit
  authorization.
- `scripts/live-test.sh run` is a bounded convenience path, not proof of a
  product journey. After static checks it captures the exact baseline,
  establishes and tears down only the inert checkout provenance carrier through
  `start-test.sh`, and verifies restoration. It never disables, enables,
  reconfigures, loads, runs, or follows the controller; it never writes
  shortcut records, creates desktops/windows, or launches a nested compositor.
  The carrier is an operational lifecycle binding only, not direct
  evaluated-memory source proof.

## Diagnostics And Ownership

- KWin runs as `plasma-kwin_wayland.service`; use `journalctl --user` and filter
  by the recorded KWin PID. `journalctl --system` is not a valid KWin capture.
- Production diagnostics use the `omnitiler:` prefix. Keep the separate
  `QT_CATEGORY=kwin_scripting` error check; neither substitutes for direct
  authoritative state evidence.
- A journal cursor is opaque: strip the `-- cursor: ` display prefix before
  passing it to `--after-cursor`. Prove a new capture contract with a true
  positive marker before relying on it.
- KGlobalAccel collection must enumerate all components and exact action tuples.
  Persisted records after unload do not prove a callback. Never unregister or
  restore anything except exact verified project-owned action records.
- Do not use `scripts/custom-tile-acceptance.sh` for current live testing:
  its persisted `[Tiling]` preflight conflicts with the accepted baseline.
  The harness is queued for removal. Historical readiness flags establish
  neither current journey readiness nor feature acceptance.

## Custom Tile Safety Findings

- Never run `remove()` then `split()` in one structural run. A removal can
  invalidate the prior tree and crash KWin.
- Never use a fixed timer as a `deleteLater` barrier. Re-resolve every tile
  handle, including the root, after removal; use homogeneous structural batches
  and a freshly decoded root after each structural call.
- Never probe persistent user scope. A scripted collapse can also crash KWin;
  treat it as crash-class, not cleanup-class behavior.
- `createDesktop` can persist a default tiling group after its two-second save
  timer. Cleanup must target only exact verified owned keys, never unrelated
  stale groups.

## Live Evidence

- A transport reply, process exit status, missing error, or visual appearance
  alone is not feature evidence. Record the relevant authoritative state before
  and after a mutation and preserve raw machine responses that gate it.
- Use verified Wayland-native normal resizable test clients. Validate focus,
  active window, desktop/output scope, and tile association before insertion.
- `invokeShortcut` bypasses the xkb layer and cannot prove physical shortcut
  delivery. Manual journeys require physical input and matching state evidence.
- The trailing-empty workspace proof is a separate executable procedure at
  [live oscillation verification](live-oscillation-verification.md).
- Before startup, preserve the actual runtime float-intent store, including
  absence, plus kwinrc, project configuration, exact shortcut tuples and loaded
  effects. Record complete protected-window frames, states, memberships,
  outputs and views. See the store implementation rather than guessing its path.
- Enable correlated complete request/reply capture before each leg and prove
  capture with a positive marker. At the first unexpected refusal or baseline
  mismatch, stop the leg, retain its gating evidence and restore before retrying.
- Prefer existing desktops. Use disposable kwrite, kcalc or kolourpaint clients;
  use a genuine fixed-size Qt client when fixed-size behavior is required.
  Fixture choice grants no additional mutation authorization.
- Preserve normal desktop retirement/appending as documented lifecycle churn,
  not a restoration target; see the live-boundary decision.

## Preserved Residue

- The unidentified prior `omnitiler-advisory-*` runtime-directory
  residue is preserved untouched. Do not search for, enumerate, inspect,
  identify heuristically, modify, or delete it. No stale POC2/POC3 harness
  or checkpoint retry is authorized; recovery requires explicit user
  authorization and exact identity or hash verification.
- Grants and prohibitions stay authoritative in
  [Current Decisions](decisions.md#live-kwinplasma-boundary); this guide holds
  procedures, preflight, and residue handling only.

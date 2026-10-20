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
  loads, runs, and records the plugin load and receipt-bound `Script<ID>`
  introspection under the captured KWin identity; it performs no journal
  readiness wait. Those checks cannot detect the numeric-path collision below.
  `status`,
  `desktops`, `reconcile-shortcuts` (without `--apply`),
  `snapshot-shortcuts`, and `snapshot-kglobalaccel` are read-only; `stop`
  unloads only the exact project script. `reconcile-shortcuts --apply` is a
  shortcut mutation and always needs explicit authorization.
- A load/introspection/run receipt plus `isScriptLoaded=true` does not bind
  the named plugin to the object at the returned numeric path. KWin allocates
  IDs from its current script count without renumbering surviving objects;
  unloading an older script while a later helper survives can reuse that
  helper's path. The duplicate object registration is unchecked, and `run`
  can target the already-running helper and silently do nothing. A fresh
  D-Bus `run` delays its reply until evaluation, but an already-running or
  starting target returns immediately; evaluation errors can also return
  normally. See [KWin lifecycle source at the pinned revision](https://invent.kde.org/plasma/kwin/-/blob/1f543de908/src/scripting/scripting.cpp).
- Retain each helper's plugin, returned ID and source receipt. Tear down
  helpers in reverse load order and verify both the named plugin is unloaded
  and its exact script object is gone before removing an older controller
  or loading its replacement. Never carry a later helper across controller
  unload/reload, guess an ID, or clean an unidentified occupant. Stop on any
  accounting mismatch. Require the evaluated-readiness gate below regardless
  of successful lifecycle receipts.
- Do not use `scripts/live-test.sh run` for this procedure: its carrier setup
  calls the absent `start-test.sh provenance` subcommand. It cannot establish
  the claimed carrier lifecycle or controller readiness. Use the manual
  lifecycle interface with helper accounting and the readiness gate below;
  historical wrapper readiness flags are not acceptance evidence.
- The carrier (`kwin/src/provenance-entry.ts`: one
  `omnitiler:provenance-ready` line, no workspace, shortcut, timer, or D-Bus
  access) proves only its own evaluation via that single line. A carrier
  load receipt is lifecycle binding evidence and never controller-readiness
  evidence; every controller claim additionally requires the gate below.

## Evaluated Readiness Gate (Controller)

After every controller `start` (manual `start-test.sh`, `just dev-on`,
recovery), and before any journey, restart, or persistence claim, prove
evaluation under the recorded KWin PID using a journal cursor captured before
startup:

- `omnitiler:world:ready`, `omnitiler:world:discovered`, and a matched
  accepted `dispatch`/`applied` pair appear after the cursor. Absence
  within the bounded window means the instance is inert: stop, retain the
  gating evidence, and diagnose before any retry. Never re-run, restart
  around it, or claim a pass from a silent instance.
- When correlated D-Bus capture runs, the first `ReadFloatIntent`
  (float-seed) and `DiscoverWorld` calls must appear. Zero calls with a
  successful run receipt is the inert-instance signature from the retained
  silent-restart leg (loaded, introspected, and run with an unchanged
  bundle, yet no seed, no world call, no journal output).

## Diagnostics And Ownership

- KWin runs as `plasma-kwin_wayland.service`; use `journalctl --user` and filter
  by the recorded KWin PID. `journalctl --system` is not a valid KWin capture.
  Check evaluation errors on the unfiltered current-PID stream: keep the
  separate `QT_CATEGORY=kwin_scripting` error check without the `omnitiler:`
  filter, since a filtered capture cannot see script-engine failures. Neither
  check substitutes for direct authoritative state evidence.
- Production diagnostics use the `omnitiler:` prefix.
- A journal cursor is opaque: strip the `-- cursor: ` display prefix before
  passing it to `--after-cursor`. Prove a new capture contract with a true
  positive marker before relying on it.
- KGlobalAccel collection must enumerate all components and exact action tuples.
  Persisted records after unload do not prove a callback. Never unregister or
  restore anything except exact verified project-owned action records.
- The obsolete custom-tile acceptance harness has been removed: its persisted
  `[Tiling]` preflight conflicted with the accepted baseline. Historical
  readiness flags establish neither current journey readiness nor acceptance.

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
- Never call no-argument `org.kde.KWin.queryWindowInfo`: it starts an interactive
  window picker and can leave a crosshair after the caller times out; Escape
  dismisses it. Prior occurrences in the archived
  [POC3](changes/archive/poc3-disposable-rust-actuation.md) and
  [direct-geometry migration](changes/archive/rust-engine-direct-geometry-migration.md)
  records are not passive inventory precedents. For fixture-only tests, use
  `list` to discover caller IDs and `get` for observed state, retaining unknown
  and unsupported values; these do not establish foreign-window state.
- Use verified Wayland-native normal resizable test clients. Validate focus,
  active window, desktop/output scope, and tile association before insertion.
  For agent-driven runs, stay fixture-scoped (`just dev --scope-class
  OmniTilerTestWindow [--scope-class ...]` or the `dev-on` equivalent) so
  only listed-`resourceClass` fixture windows are managed; foreign windows
  stay untouched by construction.
- `invokeShortcut` bypasses the xkb layer and cannot prove physical shortcut
  delivery. Manual journeys require physical input and matching state evidence.
- The trailing-empty workspace proof is a separate executable procedure at
  [live oscillation verification](live-oscillation-verification.md).
- Before startup, preserve the actual runtime float-intent store, including
  absence, plus kwinrc, project configuration, exact shortcut tuples and loaded
  effects. Record complete protected-window frames, states, memberships,
  outputs and views. See the store implementation rather than guessing its path.
- Capture these inventories freshly for every leg. Shortcut/effect counts,
  active effects alone, and a prior leg's baseline cannot establish restoration:
  retain all shortcut component tuples, loaded and active effects, store/config
  bytes and modes, pointer, and complete protected-window state. Compare the
  actual inventories afterward and retain the capture commands and nonempty
  correlated reply bodies, not placeholder extracts.
- Enable correlated complete request/reply capture before each leg and prove
  capture with a positive marker. At the first unexpected refusal or baseline
  mismatch, stop the leg, retain its gating evidence and restore before retrying.
- Prefer existing desktops. Use disposable kwrite, kcalc or kolourpaint clients;
  use a genuine fixed-size Qt client when fixed-size behavior is required.
  Fixture choice grants no additional mutation authorization.
- Preserve normal desktop retirement/appending as documented lifecycle churn,
  not a restoration target; see the live-boundary decision.

## Fixture-First Live Tests

- Agent live legs test fixture windows only, scoped fixture-only. Start the
  fixture (`tiler-live-fixture serve`), then scope the tiler to its identity
  before any mutation: `just dev-on --scope-class OmniTilerTestWindow`
  (teardown with `just dev-off`); agent-driven runs stay scoped so only
  listed-`resourceClass` fixture windows are managed. Full protocol, flags,
  fields and limits are in the [fixture README](../crates/tiler-live-fixture/README.md).
- Foreign (non-fixture) applications are only for application-specific
  investigations after first trying the fixture, with the approved scope
  naming them explicitly. Rationale: fixture windows are predictable,
  controllable and observable (caller ids, observed predicates, per-field
  outcomes), and fixture-only scope protects the operator desktop by
  construction: unrelated windows stay untouched even on a mistaken command.

## Preserved Residue

- The unidentified prior `omnitiler-advisory-*` runtime-directory
  residue is preserved untouched. Do not search for, enumerate, inspect,
  identify heuristically, modify, or delete it. No stale POC2/POC3 harness
  or checkpoint retry is authorized; recovery requires explicit user
  authorization and exact identity or hash verification.
- Grants and prohibitions stay authoritative in
  [Current Decisions](decisions.md#live-kwinplasma-boundary); this guide holds
  procedures, preflight, and residue handling only.

# Live KWin/Plasma Testing Guide

Read this guide before planning or running live KWin/Plasma work.

## Testing Posture (User 2026-11-15)

This section is the default and wins over anything below or in the
[fixture README](../crates/tiler-live-fixture/README.md); the rest describes
how, not extra gates.

- Test in the real desktop session. Optimise for outcomes: efficient,
  effective, pragmatic. Add a gate only if it prevents real harm to user data
  or the session; a check that only adds procedure is dropped.
- Standing authorization: fixture-scoped live testing in the real session is
  authorized. User windows may be tiled or moved; no restoration is required.
- Hard limits: never close or kill user windows; no display configuration
  changes; no writes to the user's stored tiler settings.
- Physical input through `ydotool`/`ydotoold` is allowed; stop a daemon the
  run started.
- Nested KWin only for loading ABI-bound native effects and deliberate
  forced-crash probes. A crash found in the real session is fixed and
  retested in the real session.
- No separate verdict-audit agent, no exhaustive process inventory, no
  readiness re-checks between legs. Verdicts cite raw log lines.
- Teardown: copy dev logs, `just dev-off`, stop what the run started.

## Fixture-First Live Tests

First live run (from the repo root inside the devenv shell):

1. Start the fixture in a terminal: `cargo run --locked -p tiler-live-fixture -- serve`
2. Start the tiler scoped to fixture windows: `just dev-on --scope-class OmniTilerTestWindow`
3. Drive windows with `tiler-live-fixture send` (or `scripts/kde-scenarios.mjs`).
4. Teardown: copy dev logs, run `just dev-off`, and stop anything the run started (fixture server, any `ydotool` daemon).

- Append the test workspace beyond the whole existing roster (`>n` by construction, where `n` is the highest occupied workspace ordinal; no routine occupancy inventory needed). Otherwise inventory occupancy fresh (including sticky windows) and use workspaces `>n`.
- Foreign windows stay unmanaged by the `--scope-class` filter; sticky foreign windows on an appended workspace remain unmanaged. Foreign apps are only for app-specific investigations after trying the fixture, with approved scope naming them.
- Full fixture protocol, flags, and `observed`/`unsupported` semantics: [fixture README](../crates/tiler-live-fixture/README.md).

## Live Tooling (User 2026-11-17)

- Keep reusable KWin script lifecycle, fixture launch/state, input and teardown
  operations in a small importable repository helper library with single-purpose
  commands. Quick checks are short scripts importing it; use
  `scripts/kde-scenarios.mjs` for multi-step legs.
- External one-off scripts remain allowed for isolated, quick or exploratory
  checks, but import repository helpers rather than copying them. Teardown,
  key release and lock receipts are written and tested once.
- When a one-off is reused or outgrows a quick check, promote its reusable parts
  into the helpers; keep one-off procedure logic outside the repository.
- Extract only what current live legs need, with tests, from the existing runner
  and prior helper scripts. Build incrementally, not through an upfront port.
- Evidence guidance, applied case by case: keep disposable working bundles and
  logs in the change's gitignored `docs/changes/<change>/results/`, not temporary
  system directories. Never commit those working bundles.
- Persist only decisive, small, non-sensitive evidence likely to be revisited:
  a few redacted lines in the change note or a small adjacent file, naming the
  producing command/script and commit. Leave ephemeral troubleshooting local;
  summarize it briefly when useful. Weigh value against overhead.

## Safety And Hygiene

- Platform target: latest-stable KDE Plasma.
- Stop conditions: stop on a KWin crash or crash-class surprise, a filter failure causing repeated non-fixture actuation, or a real product/contract decision. A crash found in the real session is fixed and retested in the real session (do not reroute to nested KWin).
- Harm prevention: no `sudo`, no Home Manager or system paths, no broad or unowned cleanup, no unrelated configuration, and never no-arg `org.kde.KWin.queryWindowInfo` (starts an interactive picker; Escape dismisses it).
- Residue: preserve unidentified pre-existing runtime residue (such as `omnitiler-advisory-*`) untouched; do not heuristically modify or delete unowned residue.
- Evidence boundary: [Current Decisions](./decisions/kde.md#live-test-evidence-boundary).

## KWin Script Lifecycle

- KWin executes the generated non-module ES2017 IIFE only (no ESM, Node imports, source maps, optional catch bindings, or manual edits).
- `/Scripting` returns a signed `i` script ID from `loadScript(s)` / `loadScript(ss)`. `-1` is the failure sentinel; `0` maps to `/Scripting/Script0` only when returned. Accept only a non-negative 32-bit ID, introspect that exact object, and run only it. Never guess `Script0`, call global `Scripting.start`, or use a KPackage as a test path.
- Numeric-path reuse hazard: KWin allocates IDs from its current script count without renumbering surviving objects ([KWin scripting source](https://invent.kde.org/plasma/kwin/-/blob/1f543de908/src/scripting/scripting.cpp)). Unloading an older script while a later helper survives can reuse that helper's path so `run` targets the already-running helper and silently does nothing. Unload helpers before reloading a controller, and never guess an ID or clean an unidentified occupant.
- Let `just dev-on` / `just dev-off` manage packaged-script handoff (`dev-on` disables the packaged script and `dev-off` re-enables it after exact controller teardown); do not manually unload unrelated scripts.
- Manual lifecycle interface (`scripts/start-test.sh`): `start` builds, loads, and runs the script without a journal readiness wait; `stop` unloads only the exact project script.
- Inspection subcommands (`status`, `desktops`, `reconcile-shortcuts`, `snapshot-shortcuts`, `snapshot-kglobalaccel`) are read-only; `reconcile-shortcuts --apply` writes stored shortcuts and is unnecessary for normal fixture runs.
- `just dev` / `just dev-on` builds and starts the live stack (unscoped by default); `just dev-status` is read-only; `just dev-off` tears down. A command timeout does not prove startup or mutation did not occur.

## Diagnostics And Journal

- KWin runs as `plasma-kwin_wayland.service`; read `journalctl --user` filtered by the KWin D-Bus-owner PID (`GetNameOwner` on `org.kde.KWin`, then `GetConnectionUnixProcessID`). `MainPID` can be a launcher, and `journalctl --system` is invalid for KWin.
- Production logs use the `omnitiler:` prefix. Check `QT_CATEGORY=kwin_scripting` without the `omnitiler:` filter to catch script-engine evaluation errors.
- For a startup log capture, take a pre-start cursor from the unfiltered user journal (an empty PID-filtered journal can return an older cursor) and strip the `-- cursor: ` prefix before passing it to `--after-cursor`.
- Cheap readiness evidence (not a gate): `omnitiler:world:ready`, `omnitiler:world:discovered`, and a matched `dispatch`/`applied` pair after the cursor. A successful `run` receipt with no journal output can indicate an inert instance.
- KGlobalAccel: service `org.kde.kglobalaccel`, root interface `org.kde.KGlobalAccel` at `/kglobalaccel`, and component interface `org.kde.kglobalaccel.Component` at enumerated object paths; persisted action records alone do not prove a callback, and never unregister or restore anything except exact verified project-owned action records.

## Custom Tile Safety Findings

- Never run `remove()` then `split()` in one structural run; removal can invalidate the prior tree and crash KWin.
- Never use a fixed timer as a `deleteLater` barrier. Re-resolve every tile handle (including root) after removal; use homogeneous structural batches and a freshly decoded root after each structural call.
- Avoid probing persistent user scope; a scripted collapse can crash KWin.
- `createDesktop` can persist a default tiling group after its two-second save timer. Default tiling groups and normal desktop retirement/appending are normal lifecycle churn, not restoration failures; cleanup must target only exact verified owned keys.

## Live Evidence

- Transport replies, exit status, or visual appearance alone do not prove feature behavior. Native claims require joined fixture identity (each sentinel and PID matched one-to-one to a native KWin UUID/frame) plus relevant native before/after state; missing, duplicate, or ambiguous joins leave that claim unverified.
- Use `tiler-live-fixture` (`list`/`get` for observed state, retaining `unknown`/`unsupported`), or disposable Qt clients (`kwrite`, `kcalc`, `kolourpaint`, or a genuine fixed-size Qt client) when needed.
- Protocol claims use `scripts/kde-scenarios.mjs` correlated logs plus native readback of owned state. Do not run `dbus-monitor` (over-captures foreign traffic).
- `ReadWorldPresentation` strips operation lanes and native write tokens; use full correlated `Describe` replies to check pending operations.
- Wayland fixture `visible:null` is unknown, and KWin `hidden=false` alone does not prove a window is shown on the current desktop/activity. Membership exclusion is not hide/reveal evidence; leave unsupported outcomes TBD.
- `invokeShortcut` bypasses the xkb layer and cannot prove physical shortcut delivery; use physical input (for example via `ydotool`) with matching state evidence.
- Related procedures and optional tools (not routine gates): [live oscillation verification](live-oscillation-verification.md), `kwin/tests/baseline-check/`, `kwin/tests/fixture-attribution/`, and `kwin/tests/visibility-probe/`.

## Nested KWin Isolation

Applies only to loading ABI-bound native effects and deliberate forced-crash probes.

- Use fresh private `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR` (user-owned mode `0700`), and `KDEHOME`, all scoped to the run directory.
- Launch via private `dbus-run-session` with `--socket nested-kwin-spike --width 640 --height 480 --no-global-shortcuts --no-kactivities --no-lockscreen`, and never `--windowed`.
- Pass the parent Wayland display as an absolute path via `--wayland-display`. Never use the host runtime directory or remove host sockets.

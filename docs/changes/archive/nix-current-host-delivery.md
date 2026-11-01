# Nix Current-Host Delivery

## Goal

Provide a Nix-first consumer contract for the KWin script, native effect/KCM,
and optional tray without changing the external consumer repository.

## Scope And Non-Goals

- Scope: reproducible flake packages, caller-pkgs package factories, default
  NixOS and Home Manager modules, explicit source filesets, and the existing
  namespaced user-local dogfood boundary.
- Direct `packages.native-effect` is a convenience package built from this
  flake's pinned nixpkgs input. A direct external consumer must make this
  repository's nixpkgs input follow the host nixpkgs for native ABI alignment.
- `lib.mkNativeEffect { pkgs = hostPkgs; }` and the NixOS module's default
  factory path use caller package sets and are host-pkgs safe.
- Non-goals: live current-host installation, KWin/session load or reload,
  session-boundary delivery, login/autostart validation, or Nix generation
  activation and rollback claims.

## Acceptance

- Static outputs expose the script, native effect/KCM, and tray packages for
  the declared Linux systems, the package factories, and both module entry
  points.
- Module boundaries keep script enablement, native package ownership, and tray
  autostart ownership within their documented scopes.
- Runtime and session acceptance remains pending the live gates listed below.

## Approach And Dependencies

- Keep `devenv.nix` as the development build baseline and use the repository
  flake for Nix consumption.
- Use explicit source filesets; factory and module native-effect builds use the
  matching KWin development package from the selected caller package set.
  Direct convenience-package builds use this flake's selected package set and
  require the consumer nixpkgs follow for host ABI alignment.
- The direct convenience native package depends on a consumer nixpkgs follow;
  factory and module paths depend on the consumer's caller `pkgs` and matching
  KWin ABI. `flake.lock` pins this repository's evaluation input.
- Retain the namespaced, reversible dogfood path as a non-coexistent
  alternative to a Nix-managed copy of the same plugin IDs.

## Material Decisions

- Nix-first current-host delivery is selected; the external consumer repository
  is not inspected or modified.
- The NixOS module owns the script/native-effect system packages and writes
  only `[Plugins] omnitiler-kwinEnabled=true`; it does not enable the
  native border or mutate shortcuts.
- The Home Manager module owns only the optional immutable tray XDG autostart
  file, with store-backed `Exec` and `TryExec`, and no activation hook.
- The native effect is always rebuilt for the package set and KWin ABI used by
  its selected build; it is not a portable binary.

## Accepted Static Evidence And Outcome

- `nix flake check --all-systems --no-update-lock-file` passed for the declared
  `aarch64-linux` and `x86_64-linux` systems, including module-boundary and
  package derivation evaluation.
- `bash scripts/dogfood-install.test.sh` passed 482 checks with no failures.
- Static delivery is accepted for the declared package, factory, module,
  source-fileset, ownership, and dogfood rollback contracts. No live result is
  claimed.

## Bounded Live Attempt

- This was only an attempted current-session, user-local dogfood staging/root
  check. It did not exercise NixOS/Home Manager delivery or activation, and
  this record update does not add or schedule that activation.
- The attempt made no mutations. Raw before/after evidence required by the
  live-testing contract was not retained, so this attempt did not advance
  acceptance of KWin identity, config, plugin staging/discovery/load, PID/proc
  state, or restoration.
- Static repository code and configuration define the persistent key as
  `[Plugins] omnitiler-active-borderEnabled` and the native settings
  group as `[Effect-omnitiler-active-border]`; no live `kwinrc` values
  are claimed by this record.
- Any future native-effect build must use consumer `hostPkgs` and explicitly
  supply and verify the `kwin` package matching the running KWin, together
  with that package's `kwin.dev`; `mkNativeEffect` accepts `kwin`, so
  `hostPkgs` alone is insufficient ABI proof. The active devenv package
  selection was not an acceptable ABI candidate.
- The attempt stopped fail-closed because authoritative current-session
  discovery evidence was unavailable. No ABI build result, native discovery,
  KCM discovery/load, load/unload,
  config/default preservation after change, hot reconfigure, or applied
  restoration was accepted. The separately user-owned physical visual border
  and KCM usability check remains a future manual gate. The required
  logout/login or new-session boundary is also a user-owned future session
  gate; neither was performed, authorized, or awaited here.

## Accepted Nix-Only Resolution And Session Activation

- After the user-owned restart, the local shadow path
  `~/.local/share/kwin/scripts/omnitiler-kwin` was absent and default
  KPackage resolution uniquely selected the active-generation package
  `omnitiler-kwin-0.1.0` (`share/kwin/scripts/omnitiler-kwin`).
  Its `metadata.json` SHA-256 was
  `ceb49666a22cd18afa8ab5381eb997df1608dbcfc1bd8049d45823757474903f` and
  `contents/code/main.js` SHA-256 was
  `37688bc5df45ab82f0407fa788322aca364dd13f4c6fc10788f3eda09bbf5f58`,
  matching the active generation-linked producer artifact.
- KWin PID/start and D-Bus ownership remained stable during read-only checks;
  the plugin key was true, `isScriptLoaded` was true, and post-start journal
  evidence referenced the active-generation script. `/Scripting` contained
  `Script0` only, with no `Script1`; project shortcut and config records remained
  present without mutation.
- This accepts Nix-only package resolution and session activation. It does not
  prove evaluated-memory byte identity, Custom Tile or physical behavior,
  tray/effect behavior, external NixOS/Home Manager activation, or Nix
  generation update/rollback behavior.

## Pending Live Gaps

- Obtain a separately permitted source of authoritative current-session
  plugin-discovery evidence. For the future host-compatible user-local
  dogfood action, invoke `lib.mkNativeEffect` with consumer `hostPkgs`,
  explicitly supply and verify the matching running-KWin `kwin` package and
  its `kwin.dev`, verify the output's two exact files
  `$out/lib/qt-6/plugins/kwin/effects/plugins/omnitiler-active-border.so`
  and
  `$out/lib/qt-6/plugins/kwin/effects/configs/omnitiler-active-border_config.so`,
  and copy those files into the documented user-local dogfood root with
  preimage and rollback checks. This Nix-output handoff is separate from
  `scripts/dogfood-install.sh`, which does not perform it. Any later host
  action uses only the general live-KWin safety protocol.
- Validate native discovery/load, KCM discovery/load, load/unload,
  config/default preservation after change, hot reconfigure, applied
  restoration, and the remaining manual visual border/KCM usability gates.
- Existing separate residual scope remains outside this attempt: external
  NixOS/Home Manager consumption, clean install/update, Nix generation
  rollback activation, and session lifecycle behavior remain in the existing
  backlog item; watcher ordering and tray login/autostart remain separate live
  gates.

## Next Action

Obtain separate authorization for live KWin acceptance of COSMIC directional
bindings. No Custom Tile, physical, tray/effect, external NixOS/Home Manager,
or Nix update/rollback behavior is accepted by this resolution evidence.

## Moved Evidence (from docs/decisions.md)

- Current-host Nix integration, KWin/session load or reload, watcher ordering,
  login/autostart behavior, and update/rollback activation across Nix
  generations are pending live evidence, not passed by static evaluation or
  shell tests.

## Backlog detail moved 2026-11-01

### Original backlog lines 2006-2009

- P1 | External NixOS/Home Manager delivery validation | Clean external
  install, update, rollback, and host-matching KWin ABI still unproven
  on an independent host. Remains P1 in the 0.1 gate (user 2026-10-10).
  [change](nix-current-host-delivery.md)

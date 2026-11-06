# KDE support and delivery

### KDE support floor
- Decision: KDE 0.1 supports KWin 6.6+ including tiling, Settings and native effect; stock Ubuntu 26.04 LTS is supported. Plasma 5, Debian 13/KWin 6.3.6, KWin 6.4/6.5 unsupported. KWin 6.6 has one current desktop on every output, so per-output modes unavailable: saved/requested per-output modes resolve to shared with honest diagnostic; migration refuses unsupported. KWin 6.7+ retains per-output behavior. Detect scripting capability at narrow workspace seam; native builds require KWin >=6.6 and ECM >=6.24.
- Why: not recorded.
- Assumes / revisit if: test relied-on APIs at 6.6 floor and latest stable Plasma to catch upgrade breakage. New source evidence uses KWin v6.8.1 matching live tests; existing v6.7.4 citations stay revision-bound.
- Status: active, 2026-10-29
- Evidence: [KDE port source policy](../porting/kde-port-notes.md), [support floor](../changes/archive/kwin-6.6-support.md), [workspace discriminator](../spec/reference-outcomes.md#kwin-66-shared-workspace-fallback).

### Native effect packaging and version pin
- Decision: core package (KWin script, Planner, tray, both Settings KCMs) is independent of optional ABI-bound native-effect package. KWin updates remain possible without companion; missing/failed/removed/mismatched effect leaves tiling working and KWin stable, while border/drag preview degrade until rebuild. V4 K1 has no OmniTiler active border, using KWin's own focus indication; the native anchored-descriptor consumer port belongs to the focus-feedback slice. No script-side absolute geometry is permitted and K1 changes no C++ presentation DTO. A loaded effect must stay honestly hidden without stale paint or fail-closed noise, independently of tiling. Debian effect package pins exact compiled-against `libkwin6` distro build including revision and is rebuilt per KWin update; remove companion if matching rebuild unavailable. RPM/Arch dependencies remain loose. Keep exact effect pin; apt upgrade and full-upgrade hold KWin back until rebuilt effect installed. Core owns Settings KCMs; script KPackage still needs host-built native KCM companion for Configure, effect need not be enabled.
- Why: not recorded.
- Assumes / revisit if: exact pin relaxation not selected; investigate next-patch compatibility, OBS rebuild latency and pin options. Missing/failed/removed effect cases must-pass 0.1.
- Status: active, 2026-10-13
- Evidence: [KWin 6.6 support](../changes/archive/kwin-6.6-support.md), [solver evidence](../changes/archive/ubuntu-apt-solver-acceptance.md).

### Offline packaging and release delivery
- Decision: package `omnitiler` core and `omnitiler-native-effect` effect only; both KCMs in core, recipes under `packaging/`. Release archives include tagged-SHA JS bundle and vendored Rust crates; distro builds work without Node/network. User units install inertly, tray explicitly enabled. Release OBS route 3: maintainer-run scripted `osc commit` per release using release archive, `bump-version.sh`, checksum sidecar and pinned `download_url` + `verify_file`; archive is single source-delivery mechanism shared by Ubuntu 26.04/26.10, Tumbleweed, Fedora; neon excluded from 0.1. Post-release webhook rebuild trigger optional/unset; `OBS_TOKEN` unset until automation implemented/tested. Route 1 bot automation or SCM bridge post-0.1; dedicated bot only if automation later selected. Script work deferred until near release.
- Why: not recorded.
- Assumes / revisit if: offline preparation is not OBS delivery; webhook cannot update stable package pinned source/checksum. Third-party notices required before 0.1 binaries.
- Status: active, 2026-10-18
- Evidence: [offline packaging](../changes/archive/release-0.1-offline-packaging.md), [OBS handoff](../research/distribution-package-feasibility/obs-release-handoff.md#decision-disposition-user-2026-10-18).

### Nix host delivery
- Decision: Nix-first current-host delivery exports packages `default`, `kwin-script`, `native-effect`, `tray` and factories `mkKwinScript`, `mkNativeEffect`, `mkTray`, plus default NixOS/Home Manager modules. Convenience native-effect package uses repository pinned nixpkgs; external consumers must make repository nixpkgs follow host nixpkgs. `lib.mkNativeEffect` uses caller `pkgs` and matching `kdePackages.kwin.dev`; NixOS module uses caller pkgs. Native effect is not portable prebuilt; each build targets Nix-managed Plasma/KWin set. NixOS module owns system packages and writes only `[Plugins] omnitiler-kwinEnabled=true`; no border enable or shortcut changes. Home Manager owns optional tray unit and on-demand Planner activation; neither writes user `kwinrc` authority.
- Why: not recorded.
- Assumes / revisit if: do not inspect/change external consumer repository. Explicit flake source filesets exclude build/generated/unrelated files. Current build baseline remains `devenv.nix`.
- Status: active, 2026-09-20
- Evidence: [Nix packaging](../changes/archive/release-0.1-offline-packaging.md).

### Native development delivery
- Decision: development iteration uses packaged baseline plus namespaced reversible user-local dogfood override. It must not coexist with Nix-managed same plugin IDs, preserve exact normal-path restoration, and avoid system/unrelated state. Explicit one-time `just dev-native-setup`/`just dev-native-remove` stages checkout effect via project-owned env script with robust quoting/exact-content ownership, no `kwinrc` writes/receipt state. Dogfood and dev path share exact env script and cannot coexist. `just dev` preflights both; transiently loads invocation-owned effects with KWin owner guards, preserves preloaded effects in reverse-order teardown, no hot reload; unload verification never proves library unmapped. Setup recreates native build cache only when source/build dir differs, and replaces exact generated env export only if previous stage no longer exists; unfamiliar content, symlinks and other stage refused. Cache cleanup refuses unsafe source overlap/symlinked builds. Session delivery needs logout/login after setup/rebuild.
- Why: not recorded.
- Assumes / revisit if: native setup remains exact-identity and restoration bounded; no durable receipt or host configuration writes.
- Status: active, 2026-09-20
- Evidence: [native development](../changes/archive/release-0.1-offline-packaging.md).

### Host KWin development authority
- Decision: current-system host KWin derivation is native-development authority. `scripts/nix-host-kwin-build.sh` resolves running `kwin_wayland` to exact derivation/matching dev output, realizes exact output, builds in `nix develop <host-drv>` with explicit `/nix/store` Cargo/rustc and `-DKWin_DIR=<resolved-dev>/lib/cmake/KWin`; legacy pinned CMake dirs never drive or leak. `just build-native-effect` and dogfood `effect-install` use builder with identity-keyed build dirs and fail closed without provenance. Former shared dev pin is portable/interim only, not authority.
- Why: not recorded.
- Assumes / revisit if: preserve exact host provenance; no fallback to unrelated KWin development package.
- Status: active, 2026-09-20
- Evidence: [native development](../changes/archive/release-0.1-offline-packaging.md).

### Planner activation and loss
- Decision: package `omnitiler planner-service` as session D-Bus `com.omnitiler.Planner`; immutable descriptor names systemd service, Home Manager puts package in D-Bus path and owns matching user `Type=dbus` unit with exact BusName/immutable ExecStart. No shell, autostart target, durable PID/receipt or second process mode. Home Manager option defaults true on module import; service inert until D-Bus activation; false omits descriptor/unit. `Restart=no`: name loss terminates old Planner session/transaction, no systemd restart/rebind loop; later idle command may request fresh activation. Session teardown/name loss ends process without durable recovery state.
- Why: not recorded.
- Assumes / revisit if: confirmed loss may start one bounded fresh session from current eligible windows only; no durable layout snapshot/journal, old in-flight transaction terminal, old-generation reply rejected, no interrupted command replay/uncertain mutation recovery/false send commit. If Planner survives sleep, retain in-memory layouts.
- Status: active, 2026-09-16
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md).

### Planner request binding and authorization
- Decision: selected Rust KWin command resolves Planner name first. If absent, one bounded `StartServiceByName(..., 0)`, accepts only `PrimaryOwner`/`AlreadyOwner`, then resolves/pins unique owner before planner method. Activation/identity/owner/stale/loss/timeout failure refuses Rust route with no Legacy fallback; pending plan never rebinds after restarted owner. Planner same-UID authorization uses D-Bus `GetConnectionUnixUser` for caller unique name; malformed/failed/unknown/different UID rejects. Executable/PID/start-tick/boot, systemd MainPID/parentage and wrapper/cgroup identity do not participate. Current world/float-intent methods additionally require live KWin unique owner; authorization failures are bounded in-band rejections.
- Why: not recorded.
- Assumes / revisit if: public KWin scripting cannot attest initially resolved same-UID Planner binary.
- Status: active, 2026-09-16
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md).

### Planner and KWin restart adoption
- Decision: if KWin restarts while Planner survives, retain topology for domains sharing observed window IDs. Fresh per-window `internalId` UUIDs with a complete nonempty reconcile and no shared IDs fresh-adopt that domain instead of rebuilding arbitrary ID order. Empty observation retains ordinary retirement/revision behavior; mismatched owner/generation rejects. Other domains and Planner binding remain intact.
- Why: not recorded.
- Assumes / revisit if: world discovery/generation supersedes removed fixed `plan-1` script-generation mechanism; a loss-confirmed new Planner session starts from current eligible windows only, no durable snapshot/journal or old internal history. Old in-flight transaction is terminal; old-generation reply rejected; no interrupted-command replay or uncertain native mutation recovery.
- Status: active, 2026-09-28
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md).

### KDE initial layout fitting
- Decision: when first startup domain lacks usable retained session, Rust may use versioned best-effort near-layout fitting to minimize unnecessary initial movement; keep it simple, comprehensible, deterministic, with Rust structural inference and KWin TS native observation. Same attempt applies only to a post-confirmed-loss fresh session using current eligible windows; it may change grouping and does not reconstruct old topology.
- Why: minimize unnecessary initial window movement.
- Assumes / revisit if: no park/unmanaged fallback or broader activation lifecycle is selected. General historical-layout reconstruction remains unselected.
- Status: active, 2026-09-16
- Evidence: [Planner route](../changes/archive/core-full-state-replies.md).

### KDE fullscreen presentation and host integration
- Decision: hide active border for fullscreen or any native maximize axis; group visual also hides while maximized. Applet popup active window hides native border; restore on ordinary focus. Seed committed maximize mode on effect load/add; any maximize axis/fullscreen suppresses both visuals; native transition signals authoritative, never guess requested state, poll, timer or infer geometry. One effect owns active border, group overlay and drag oracle; no second drag effect/factory/metadata/KCM. Never clip/reshape/change window texture; Plasma 6.5+ decoration-driven rounded corners. KDE grouped underlay trigger is Meta+Shift held or matching focused native interactive move; Meta/resize alone do not trigger. No polling/timed flash/new hook. KDE Stage 6 cursor facts sample fresh `cursorPos` and advisory `windowAt` list head on relevant event/command/gesture captures; no polling or pure-motion full captures when cursor settings disabled. Border live system accent default and explicit override stay independent of underlay/preview fills and core pager-focused-window color. Cursor warp unsupported and never reported applied.
- Why: not recorded.
- Assumes / revisit if: physical rendering/input acceptance remains separately pending; native painting is not established by offline selector coverage.
- Status: active, 2026-10-28
- Evidence: [KDE port](../changes/kde-world-v2-port.md#stage-6-host-integration-outcome-2026-10-28), [group underlay](../changes/group-underlay-move-trigger.md).

### KDE gesture transport
- Decision: one native effect lifecycle owns immutable gesture identity/start facts/phases/finish; broker folds script cursor/hit observations into existing `GestureIngress`/`GestureSettled`. The passive `InputEventSpy` observes configured modifier-resize presses without grabbing or consuming input and carries a usable press with final geometry through the existing separate read-only session D-Bus endpoint; no endpoint rewrite is selected. No script lifecycle minting, delayed-witness adoption, or window/frame/age heuristics. Esc reports `Cancelled` only when delivery to synchronous interactive finish is causally fenced; other finishes report `Released`. Earlier shortcut/effect filters can consume Esc; indistinguishable Xwayland `_NET_WM_MOVERESIZE` cancel reports `Released`. Unavailable capture omits unsupported ingress, never invents terminal; cursor warp unsupported and never reported applied.
- Why: not recorded.
- Assumes / revisit if: transport precedes Stage 5 producing-gesture/native-invalidation freshness enforcement; identity exposed now, enforcement belongs to next package. COSMIC-style Esc-as-drop remains possible later, not implemented.
- Status: active, 2026-10-26
- Evidence: [gesture transport](../changes/kde-world-v2-port.md#gesture-transport-accepted-offline-recovery-2026-10-26).

### Deferred Activities support
- Decision: inactive-Activity windows are retained and never tiled, focused or moved by core except with their workspace (migration/swap), when host removes their display, or to finish a transfer pending before hide. Report host-hidden windows through existing `WindowFact.minimized=true`; no new core state. All-desktop maps to sticky; subset-of-desktops floats and never rehomes. Activities support is later port work, not implemented.
- Why: not recorded.
- Assumes / revisit if: KDE evidence remains pending and tentative; port may propose evidence-backed alternative, any core change needs user approval. Do not infer Activities semantics from macOS multi-Space/accessibility variants.
- Status: tentative, 2026-10-24
- Evidence: [port deferred investigations](../changes/kde-world-v2-port.md).

### Deferred asynchronous configure investigation
- Decision: first reproduce KDE/Wayland pending leg natively and check KWin pending/completion signal. Only if reproduced may port propose neutral settling representation; any core change needs user approval. Keep existing convergence without speculative core signal.
- Why: not recorded.
- Assumes / revisit if: distinct from Windows resize lane: later staged frame can supersede immediate readback for learning minima, but later facts are not native completion. Windows minimum-learning policy is owned by [core settlement](engine.md#core-derived-native-settlement); no settling-protocol redesign without reproduced failure and readable host completion signal. macOS and future remote adapters share broker, startup-adoption and async-settlement concerns; multi-Space and accessibility variants apply only where stated and do not prove Activities semantics. Implementation/status evidence is archived separately.
- Status: active, 2026-10-24
- Evidence: [port deferred investigations](../changes/kde-world-v2-port.md), [evidence disposition](../changes/archive/decisions-evidence-2026-10-31.md#targeted-baseline-evidence-dispositions).

### Deferred connector and cleanup review
- Decision: retain core-only ordered baseline; native shared connector is deferred to separation review after rearchitecture. Logical receipts do not prove native hide/reveal. Next narrow checker first rejects missing readback, pending operations, omissions/fences, coverage defects and skipped intermediate hide/reveal; build no connector here. Code cleanup/abstraction review is a separate follow-up.
- Why: not recorded.
- Assumes / revisit if: cleanup review does not defer known correctness defects or restore retired platform policy route. Generic affected-port concerns apply to macOS/future remote adapters, not automatic proof of Activities semantics. Baseline revision and withdrawn-draft history are archived as evidence.
- Status: active, 2026-10-24
- Evidence: [port deferred investigations](../changes/kde-world-v2-port.md), [evidence disposition](../changes/archive/decisions-evidence-2026-10-31.md#targeted-baseline-evidence-dispositions).

### Live-test evidence boundary
- Decision: durable default (user 2026-11-06): n is the max occupied workspace ordinal across the relevant inventory, inventoried fresh each run and never hardcoded; occupancy counts non-fixture native windows too (occupancy only, never a full protected-window inventory); run tests on workspaces >n with the fixture and exact platform filter so only fixture windows are managed. All-desktop/sticky membership is recorded as observed, never fabricated; unsupported outcomes stay TBD. Incidental stray effects on existing windows are accepted; no routine baseline capture/compare, exact-restoration proof, protected-window inventory, or stop-on-baseline surprise. Desktop ID/order/count changes and default tiling groups produced by normal OmniTiler lifecycle are observations, not restoration failures. Session-delivered package claims require a session boundary; physical/manual outcomes remain separate from offline and agent-observed evidence. Stop on KWin crash/crash-class surprise, repeated non-fixture actuation from filter failure, or a real product/contract decision. The live KWin/Plasma procedure is maintained at [live testing guide](../live-kwin-testing.md); operational controls and authorization are not copied into this decision register.
- Why: not recorded.
- Assumes / revisit if: test evidence must identify which boundary and observation actually occurred; readiness checks alone do not establish native or physical acceptance.
- Status: active, 2026-10-14
- Evidence: [dual-output procedure](../changes/kde-dual-output-live.md#approved-test-procedure-2026-10-14), [live testing guide](../live-kwin-testing.md).

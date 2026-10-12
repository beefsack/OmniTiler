# KWin 6.6 support

Status: offline delivery complete, 2026-10-12. Live and full dependency-solver
acceptance remain pending below.

## Goal and scope

- Support KWin 6.6 for KDE 0.1, including tiling, Settings and the native
  effect. KWin 6.7+ retains per-output workspace behavior.
- On 6.6, the single current desktop is current on every output. Per-output
  mode is unavailable; a requested per-output mode degrades to shared mode
  with an honest diagnostic. No Plasma 5 or KWin 6.3-6.5 support.
- Build core plus Settings and the effect companion from one exact source
  archive on Ubuntu 26.04 release and updates, and Ubuntu 26.10.

## Approach and acceptance

1. Audit every scripting API and native header against KWin 6.6.4, and verify
   Ubuntu Rust availability before changing compatibility behavior.
2. Feature-detect per-output desktops in the KDE adapter, cover absent methods
   with offline fixtures, and lower the tested ECM floor to 6.24.
3. Extend Debian packaging with the effect companion, pinned to the exact KWin
   package build. Add an effective guard against future 6.7-only API use.
4. Run Rust, JavaScript, native, shell, portable Windows and applicable Nix gates;
   build both Debian packages in each Ubuntu target from the same archive.
5. Promote the support decision, update installation and specification records,
   and retain user-owned live acceptance checks.

## Evidence and decisions

- Initial static audit against KWin v6.6.4 found the known missing
  `currentDesktopForScreen` / `setCurrentDesktopForScreen` methods and the
  additional migration-only `options.perOutputVirtualDesktops` property.
  Other consumed workspace/window/output/desktop globals and native effect
  symbols are present. The current-desktop signal's older arity is compatible
  with the existing callback.
- Ubuntu 26.04 stock Rust 1.93.1 exceeds the observed Rust 1.88 source floor;
  no alternate toolchain package is needed. Ubuntu 26.10 image is available.
- Independent review of `fac8f4f` and `ed37e31` found no defects. The 6.7+
  getter/setter paths retain their existing behavior; the shared-write
  diagnostic now reports only successful writes. Legacy tests cover observation,
  shared switching, cross-output send/follow/stay and move-with-follow through
  the real Engine bridge. Migration refuses unsupported per-output operation
  before reading the absent options property.
- Debian core contains both Settings KCMs, the script, planner/tray and inert
  activation units; it requires KWin >= 6.6 without an exact pin. The companion
  contains only the effect and exactly depends on both core and the installed
  compiled-against `libkwin6` distro version. Removing the companion preserves
  Settings/Revert. A KWin package update requires an effect rebuild or companion
  removal before upgrading; full apt upgrade-solver acceptance is still pending.
- Fixed the core-only profile to exclude the effect binary package using
  `Build-Profiles: <!noeffect>`, not just its build dependencies. Without that
  exclusion the profile would still try to package an absent effect with an
  unresolved KWin version substvar. A real no-header core build passed.
- RPM's ECM requirement changed from 6.26 to 6.24 to match the tested native
  CMake floor; no RPM dependency-policy change. Arch and Nix recipes were
  unchanged. OBS Ubuntu provisioning is not a trivial recipe change; the
  distro-agnostic source service remains usable once provisioning and the
  release source/checksum handoff are completed.
- Durable support and package policy is recorded in
  [decisions](../../decisions.md#kde-support-floor), with installation guidance
  and a minimal [workspace row](../../spec/reference-outcomes.md#kwin-66-shared-workspace-fallback).

## Accepted offline verification

- One immutable `omnitiler-0.1.0.tar.gz` source snapshot, based on `ed37e31`
  with the final Debian/RPM packaging changes, was used for all Ubuntu builds
  and the Fedora 43 rebuild. It contains `VERSION=0.1.0`,
  `SOURCE_REV=ed37e31583c505dd2eae77a6d26cb6e4128a82d6`, the prebuilt
  revision-stamped JS bundle and locked vendored Rust crates. SHA-256:
  `7e41978cf9cd3b414107f08ad80e89945f2e78ccd27c8af8201041bf5bf08849`.
  Fresh-Cargo-home offline archive self-check passed. This is a verification
  snapshot, not a published release or a claim of identical DEB/RPM payloads.
- Disposable-container `dpkg-buildpackage -us -uc` builds passed:

  | Image / target | KWin distro build | Rust | ECM | Result |
  | --- | --- | --- | --- | --- |
  | Ubuntu 26.04 release | `4:6.6.4-0ubuntu1` | 1.93.1 | 6.24 | Core + effect |
  | Ubuntu 26.04 updates | `4:6.6.6-0ubuntu0.1` | 1.93.1 | 6.24 | Core + effect |
  | Ubuntu 26.10 | `4:6.7.5-0ubuntu1` | 1.97.1 | 6.29 | Core + effect |
  | Ubuntu 26.04 `noeffect` | No `kwin-dev` installed | 1.93.1 | 6.24 | Core only, both KCMs |

  Images: `ubuntu:26.04@sha256:f144425ff09be612d6d9ad965196e9cdc23dae1f42110a8a11a3e9a8198759f7`
  and `ubuntu:26.10@sha256:ee126c2fa0249079a7e24ae3a3d29b04783ef93ab33c868751c9a1289d3fffff`.
  Release KWin packages were explicitly selected rather than using updates
  candidates. Package inspection confirmed the two core KCMs, effect-only
  companion, exact dependency including distro revision, and factory IIDs
  `EffectPluginFactory6.6.4`, `6.6.6` and `6.7.5` respectively. Payload
  install/remove probes passed with `--force-depends`; these do not establish
  full dependency solver acceptance or effect loading in a compositor.
- Fedora 43 `rpmbuild -bb` passed from the same archive and byte-identical
  current spec, producing core/effect RPMs plus debug packages. Arch's
  unchanged recipe parsed with `makepkg --printsrcinfo`; a new full Arch build
  was not repeated (the release URL is unpublished). Earlier full split-package
  evidence remains in [core Settings delivery](release-0.1-core-settings.md).
- Full Rust workspace tests, format and strict all-targets Clippy passed.
  Windows portable allowlist build/test/Clippy passed on Linux for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`; core portability
  checks passed. This does not replace native Windows runner evidence.
- Full KWin JS: 1,327 tests in 193 suites passed; typecheck passed. Native
  CTest: 33 effect/full-tree and 26 Settings-only tests passed.
- All 12 CI headless shell suites passed, including Debian packaging (30
  checks), archive/KPackage, isolated dev/install fixtures, tray and the KWin
  API guard (8 checks). The guard test invokes the guard on the actual source
  tree and rejects unseamed 6.7 symbols, unaudited API symbols and cast-hidden
  reads. CI's `scripts/*.test.sh` glob runs it; its `npm test` glob also runs
  the legacy compatibility fixtures. No extra gate plumbing is needed.
- All applicable `checks.x86_64-linux` outputs passed: `module-boundary`,
  `native-effect`, `native-settings`, `native-effect-tests`,
  `native-settings-tests`; `packages.tray` and `packages.kwin-script` built.
  Delivery install checks and module assertions passed. ARM checks were not
  run. Whitespace check passed.
- An isolated verification copy's Nix output symlink initially caused two
  dev-fixture suites to refuse; removing that generated link restored both
  passes. No product workaround or source change was required.

## User-owned live checks

- Kubuntu 26.04 VM: install matching core/effect packages; verify Settings,
  automatic tiling, focus/move, shared desktop switching and send/follow with
  two outputs, including background desktop layouts.
- Check a saved per-output request visibly reports shared fallback, and that
  Settings Apply/Revert remains useful without unsupported migration.
- Verify active border, group underlay and drop preview; fullscreen/gaming
  non-interference; missing/failed/removed effect leaves tiling running.
- Upgrade KWin with a matching rebuilt effect; check dependency solver and
  effect loading. Revert before package removal.
- KWin 6.7+: smoke independent per-output desktops, shared mode, switching,
  send/follow, Settings migration and the three native visuals.

## Remaining acceptance

- Full apt dependency solving, install/upgrade/removal with matching rebuilt
  effects, including allowing KWin upgrades after companion removal.
- The live journeys above and the existing KDE 0.1 reliability/gaming gate.
- Release publication, OBS accounts/targets and automated stable source/checksum
  handoff, licensing/vendor audit, and independent NixOS/Home Manager delivery
  validation remain tracked in [backlog](../../backlog.md).

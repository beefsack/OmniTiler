# Installation (tentative 0.1 offline preparation)

The packaging evidence below predates the completed core/Windows v2 boundary.
KDE's current consumers still require the [v2 port](porting/kde-port-notes.md);
this record does not claim current-source KDE build or runtime acceptance.

Status: packaging recipes exist but no OBS account, project, token, or
published repository exists. Container builds completed for the recipes
(evidence below); OBS source-handoff automation remains blocked, and
full runtime dependency resolution and live-session checks remain pending.
Package names and repository contents marked "verified Oct 2026"
were checked against the live distro repositories from disposable
containers.

Build evidence: disposable Docker containers built source archives containing
the revision-stamped JS bundle and vendored Rust crates. Earlier scratch-tag
builds used fresh Cargo homes with `--network none`; the current Ubuntu and
Fedora 43 snapshot builds used offline Cargo after distro dependency preparation.

- Tumbleweed and Fedora 43/44: `rpmbuild -bb` produced core and native RPMs.
  Generated KWin dependencies were unversioned names and
  `libkwin.so.6()(64bit)`, with no exact KWin package-version requirement.
- Arch: `makepkg` produced both split packages; `pacman -U` resolved
  dependencies. `makepkg --printsrcinfo` matched `.SRCINFO`; `namcap`
  found no PKGBUILD errors. Shipping-package dependency warnings remain;
  the generated debug package also has symlink diagnostics.
- Ubuntu 26.04 release (KWin `4:6.6.4-0ubuntu1`), 26.04 updates
  (`4:6.6.6-0ubuntu0.1`) and 26.10 (`4:6.7.5-0ubuntu1`): the same exact
  source archive produced both core-with-Settings and effect-only `.deb`
  packages with `dpkg-buildpackage -us -uc`. Each effect exactly depends on
  its compiled-against `libkwin6` build and has the matching factory IID.
  The `noeffect` profile also built core with both KCMs without `kwin-dev`.
  Vendored manifest backups survived cleaning; the tray unit is not
  automatically enabled. [Current evidence](changes/archive/kwin-6.6-support.md).
- Fedora 43 also rebuilt both RPMs from that same current snapshot with the
  revised ECM 6.24 requirement. Arch's unchanged recipe parsed successfully;
  the full Arch build evidence above predates this compatibility delivery.
- RPM/Ubuntu install probes used `--nodeps`/`--force-depends`, so they
  establish payload installation and binary execution, not full dependency
  solver acceptance. No desktop, D-Bus activation or compositor was started.

All core artifacts contain both KCMs with no `libkwin` dependency; each
effect companion contains only the ABI-bound effect. Settings-only Ubuntu
builds use ECM/KF6 6.24 without `kwin-dev`. Detailed evidence is in the
[archived change record](changes/archive/release-0.1-core-settings.md). Package binaries are
verification artifacts, not published releases; only the source archive's
reproducibility is claimed, not identical RPM/DEB payloads.

## Supported KDE versions

KDE 0.1 supports KWin 6.6 and later, including tiling, Settings and the native
effect. On 6.6 one current desktop applies to every output; a saved per-output
workspace request degrades to shared with a diagnostic. Per-output workspaces
require KWin 6.7+. Plasma 5, Debian 13's KWin 6.3.6, and KWin 6.4/6.5 remain
unsupported. [Support decision](decisions.md#kde-support-floor).

## Recipe layout (tentative)

First-party licensing is MIT (copyright Michael Alexander), except the native
effect plugin sources, GPL-2.0-or-later. The linked Rust staticlib and both
Settings KCMs remain MIT. Cargo/npm, KPlugin and distro/Nix metadata match;
packages include first-party license notices and Debian has per-path DEP-5
stanzas. Complete the tool-based audit, vendored attribution inventory and
per-distro native closure checks before publishing
([licensing research](research/licensing.md)).

| Path | Purpose |
| --- | --- |
| `packaging/rpm/omnitiler.spec` | One spec, two binaries: `omnitiler` (core with Settings) and `omnitiler-native-effect` (effect only). Conditionals cover openSUSE Tumbleweed and Fedora 43/44. |
| `packaging/arch/PKGBUILD`, `.SRCINFO`, `omnitiler*.install` | Split package (`omnitiler` with Settings, `omnitiler-native-effect` effect only) for AUR-only consumption. No OBS pacman repo: Arch scope is AUR. |
| `packaging/debian/` | Two-package `debian/` source dir for Ubuntu 26.04/26.10: core with Settings and an exactly KWin-pinned effect companion. Optional `noeffect` build profile. |
| `packaging/obs/_service` | Pins the immutable GitHub Release tarball per version and its SHA-256; never a raw git checkout. |
| `packaging/systemd/` | `omnitiler-tray.service` and `omnitiler-planner.service` user units, mirroring `home-manager-module.nix`. Never auto-enabled by any recipe. |
| `packaging/bump-version.sh` | Rewrites the pinned version across all recipes for a release (`--version X.Y.Z [--sha256 ...]`). |
| `.obs/workflows.yml` | Single `tag_push` workflow running `trigger_services`. Inert until OBS provisioning exists. |

## What gets installed

Core (`omnitiler`): `/usr/bin/omnitiler`, the KWin script
at `/usr/share/kwin/scripts/omnitiler-kwin/` (exactly
`metadata.json`, `contents/code/main.js`, `contents/config/main.xml`,
`contents/ui/config.ui`; the bundle is prebuilt, recipes never run npm),
`/usr/share/dbus-1/services/com.omnitiler.Planner.service`,
`/usr/lib/systemd/user/omnitiler-{tray,planner}.service`, the
hicolor SVG icon, and both native settings pages (KWin-independent,
Qt/KF6 only) under the Qt6 plugin dir -
`kwin/effects/configs/omnitiler-active-border_config.so` and
`kwin/scripts/configs/omnitiler-kwin_config.so` (the Configure page
referenced by the KWin script).

Native effect (`omnitiler-native-effect`, where offered): exactly
one plugin under the Qt6 plugin dir -
`kwin/effects/plugins/omnitiler-active-border.so`. JSON metadata is
embedded in the `.so` files at compile time (`K_PLUGIN_CLASS_WITH_JSON` /
`KWIN_EFFECT_FACTORY`), so no `.json` ships alongside; this matches the
flake `installCheck` file sets (effect-only vs settings-only).

First-party notices ship in distro/Nix package license directories (Debian
uses its standard copyright file). The standalone `.kwinscript` archive has
five files: the four script files above plus root `LICENSE`.

## Enable, tray, and planner

Recipes install files only. To use them:

1. Enable the script: `kwriteconfig6 --file kwinrc --group Plugins --key
   omnitiler-kwinEnabled true`, then `qdbus org.kde.KWin /KWin
   reconfigure`.
2. Tray (optional): `systemctl --user enable --now
   omnitiler-tray.service`. The planner needs no enabling; D-Bus
   activates `omnitiler-planner.service` on demand.

## Revert before removing (all managers)

"Revert" means the Settings page buttons, not disabling the script.
Settings (with Revert) lives in core: removing the effect companion alone
leaves Settings and Revert available. Press Revert before removing core:
Revert Shortcuts (restores KDE defaults for cleared shortcut bindings) and
per-row Revert to KDE default (removes the local host key so the KDE
default takes effect again). Removal does not
restore host keys or shortcut overrides. If you already removed
core without reverting, reinstall core and press Revert.
Disabling the script (setting `omnitiler-kwinEnabled=false` and
reconfiguring KWin) is a separate step and does not restore host settings.

Each recipe repeats this in its idiomatic post-removal message (RPM
`%postun` echo on final erase only, pacman `post_remove`, Debian `postrm`
on remove/purge).

## KWin upgrades: policy and limitation

Core does not exactly pin KWin; Debian core requires KWin >= 6.6. Debian's
effect companion depends on the exact `libkwin6` package version it was built
against, including the distro revision. It must be rebuilt for every KWin
package update. An exact dependency can hold an upgrade, and does not prove
the running compositor has restarted into the installed version.

On Ubuntu, the package manager may hold back a KWin update until a rebuilt
`omnitiler-native-effect` is published. Core (`omnitiler`) tiling and Settings
keep working without the effect. You can wait for and install the rebuilt
effect, or remove only `omnitiler-native-effect` to take the KWin update and
reinstall the rebuilt effect later. Both `apt upgrade` and `apt full-upgrade`
held KWin back in [solver acceptance](changes/archive/ubuntu-apt-solver-acceptance.md).

RPM and Arch retain their unversioned KWin dependencies. Their loader check
rejects different-version effects; same-version distro ABI patches still
require rebuilding. For all recipes:

- After every KWin/Plasma upgrade, update the native-effect package to the
  rebuild against the new headers before relying on the effect. Core tiling
  and Settings keep working without it.
- Split layout note (0.1, pre-release): core ships both KCM plugins and the
  companion ships the effect plugin only. RPM keeps the companion's
  exact-version core requirement (`Requires: %{name} =
  %{version}-%{release}`) and Arch keeps
  `depends=("omnitiler=$pkgver-$pkgrel" ...)` on the companion to
  keep installs paired; neither is claimed to migrate file ownership across
  updates (no predecessor was ever published). Removing the effect
  companion alone always leaves Settings/Revert in core.
- KWin 6.7.5's [loader source](https://github.com/KDE/kwin/blob/v6.7.5/src/effect/effectloader.cpp)
  checks the plugin IID before `loader.instance()` and returns null on a
  mismatch. Its [factory header](https://github.com/KDE/kwin/blob/v6.7.5/src/effect/effect.h)
  includes `KWIN_PLUGIN_VERSION_STRING` in that IID; distro builds confirmed
  this metadata. A different-version effect is refused before instantiation.
  KCMs run in the settings process. Missing/failed/removed-effect live
  acceptance remains a separate user-owned 0.1 gate.
- Residual risk (concrete): same-IID C++ ABI drift is not loader-guarded -
  upstream KWin states effect ABIs are not stable. Only rebuilding covers
  it (tracked via the `cmake(KWin)` BuildRequires). The existing versioned
  IID already covers version changes; another version comparison would not
  detect same-version distro ABI patches. Such patches require a rebuild.
  RPM auto-generated ELF SONAME `Requires` are plain SONAME tokens with no
  versions (confirmed in the real TW and F43 builds:
  `libkwin.so.6()(64bit)`), so they do not block upgrades within the
  SONAME; no dependency filtering is applied. See also the OBS rebuild
  timing note below.

## Without the companion: what still works

Tiling, workspaces, shortcuts, Settings (including Revert), and the planner
keep working on core alone. What is missing without the ABI-matched
companion: only the active-border effect itself (the effect reconfigure
from Settings tolerates the absent effect and keeps Apply enabled via
retry-on-next-save). The script's `X-KDE-ConfigModule` resolves to the
core-shipped native script KCM; the Nix flake ships the same split
(`packages.native-settings` always alongside `packages.native-effect` in
the NixOS module).

## Per-distro notes

- openSUSE Tumbleweed (verified Oct 2026: `kwin6-devel` 6.7.5 with
  `cmake(KWin)`, `kf6-extra-cmake-modules` 6.30.0, `kf6-*-devel` 6.30.0,
  `qt6-base-devel`, `rust`/`cargo` 1.98.1, `systemd-rpm-macros`,
  `kf6-kconfig`/`kf6-kcmutils` shipping the config/shell helpers). First
  OBS target; rolling KWin means rebuild per snapshot.
- Fedora 43/44 (verified Oct 2026 via updates repo: `kwin-devel` 6.7.5
  with `cmake(KWin)`, `extra-cmake-modules` 6.30.0, `kf6-*-devel` 6.30.0,
  `qt6-qtbase-devel`, `rust`/`cargo` 1.98.1, `systemd-rpm-macros`). Base
  repos alone may carry older ECM/KDE stacks, so the OBS target must
  resolve the updates repository; the spec's `>= 6.24.0` ECM floor fails
  closed otherwise. Same spec as Tumbleweed via `%suse_version`/`%fedora`
  conditionals.
- Arch, AUR only (verified Oct 2026: `kwin` 6.7.5 ships headers, ECM
  6.30.0, unprefixed `kconfig`/`kcmutils`/etc., `rust` 1.99 shipping cargo).
  Split PKGBUILD; local AUR-shaped builds see the user's own KWin. There is
  no OBS pacman repo and none is planned.
- Ubuntu 26.04 LTS: stock `kwin-dev` 6.6.4 release / 6.6.6 updates,
  `rustc`/`cargo` 1.93.1, `extra-cmake-modules` and KF6 6.24, Qt6 6.10.
  Both packages build with the native tree's ECM 6.24 / KWin 6.6 floors;
  core recommends the effect companion. The single current desktop applies
  to every output; per-output workspace modes degrade to shared. Core's
  Settings-only build does not need KWin headers, and
  `DEB_BUILD_PROFILES=noeffect` builds only core without `kwin-dev`.
  Runtime KF dependencies resolve through `${shlibs:Depends}`; Settings
  helpers are hard dependencies (`libkf6config-bin`, `libkf6kcmutils-bin`).
- Ubuntu 26.10 (release scheduled 2026-10-15): the same Debian recipe built
  both packages against KWin 6.7.5, ECM 6.29 and stock Rust 1.97.1 in the
  available image. Per-output workspaces are supported. Each Ubuntu release
  and KWin package update needs its own matching effect rebuild.
- KDE neon: no OBS target provisioning exists (upstream
  openSUSE/open-build-service#19317); do not promise it.

## OBS wiring (provisioning-gated, nothing created)

**Blocked automation leg:** the workflow cannot upload sources: `trigger_services` only re-runs
`_service` on the package sources already committed in OBS, so every
release needs its sources updated there first. Until that happens the
pinned checksum placeholder makes the source service fail closed; it is not
a usable release pipeline even if the token is supplied. Keep OBS_TOKEN
unset until the source handoff is resolved and tested.

1. Maintainer creates the OBS account, one stable project, and the
   `omnitiler` package (unpublished test repo first), uploading
   the spec, `debian/` files, and `_service` manually. (No PKGBUILD: Arch
   is AUR-only.)
2. With OBS_TOKEN still unset, push the release tag. GitHub publishes the
   tarball and checksum; its OBS job reports an inert skip.
3. For a manual source-service trial after that release, run
   `packaging/bump-version.sh --version X.Y.Z --sha256 <64-hex-digest>` in
   a packaging checkout, then commit the bumped spec, Debian files and
   `_service` to the OBS package using `osc`. This is a manual fallback,
   not completed tag-to-stable automation. Source services download and
   verify the release archive before the network-free build runs.
4. Once an automated source handoff is implemented and tested, configure
   GitHub secret OBS_TOKEN (`<id>:<secret>`) and variable OBS_TRIGGER_URL.
   The post-release webhook is tag-only and inert without its secret.
   A repo-level webhook would race asset creation and must not be added.

Two investigated mechanisms did not close the handoff: `branch_package`
creates tag-suffixed packages rather than updating the chosen stable
package; `verify_file` accepts a literal checksum, not a checksum-sidecar
parameter. See the [OBS workflow guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-scm-ci-workflow-integration)
and [source-service guide](https://www.open-build-service.org/help/manuals/obs-user-guide/cha-obs-source-services).

Rebuild timing honesty: OBS rebuilds after it sees new KWin build
dependencies (default transitive rebuild), but DoD polling, queue, build,
and publish latency are unbounded here, so a same-day distro KWin release
can precede its rebuilt effect. RPM/Arch's loose dependency plus the
versioned-IID check allows version updates without an exact package pin;
Debian's exact pin requires a matching rebuild or companion removal first.
SONAME-breaking host changes and same-version ABI patches still need distro
transaction/rebuild verification; no solver simulation of those cases was run.

## Building locally (no OBS needed)

- RPM: `rpmbuild -bs packaging/rpm/omnitiler.spec` (needs the
  release tarball as `Source0`; full builds need the distro deps above).
- Arch: `makepkg --printsrcinfo` / `makepkg -s` from `packaging/arch/`
  with the release tarball URL reachable.
- Debian: place the release tarball beside the build directory, renamed to
  `omnitiler_0.1.0.orig.tar.gz` (adjust the version for later
  releases; the rename is required by the `3.0 (quilt)` source format),
  extract it, overlay `packaging/debian/` as `debian/` inside the extracted
  directory, then run `dpkg-buildpackage -us -uc` there.
- Version bump check: `packaging/bump-version.sh --version <current>`
  must be a no-op.

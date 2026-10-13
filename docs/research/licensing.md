# Licensing research: provenance, dependencies, candidates, stores, recommendation

Status: MIT project license applied by the 2026-10-11 decision, with a
GPL-2.0-or-later native-effect exception (section 8). License-tool verification,
the CI license gate and distribution-specific native closure audits remain pending.

Pins: `Cargo.lock` (`--locked` throughout); Nix `nixpkgs`
`54ba4bcec4043e72a4006d825e0d7aff5562008f`; devenv `nixpkgs`
`e554fab72f81915600f3f449b786fd9af40439a5`. Upstream reference pins come
from this repo's own research docs (section 5).

## 1. Provenance sweep

Before license application, a bounded search (not proof of absence) over
tracked `crates/`, `kwin/src`, `kwin/native-effect`, `test-fixtures/` for license headers, reuse
markers (`ported`, `vendored`, `adapted from`, ...), `Krohnkite`,
`Bismuth`, `Polonium`, `Karousel`, and COSMIC identifiers: no SPDX
headers, no license texts, no `vendor/` dir, no third-party WM code.
The only `vendored-sources` hit is Cargo's offline-build
source-replacement (`kwin/native-effect/CMakeLists.txt:100-105`); no
vendored sources are checked in.

- COSMIC analogue is behavioral, not a copy:
  `crates/tiler-core/src/directional.rs:1608-1672` redistributes unit
  fractions fail-closed, vs upstream integer-pixel geometry with
  rounding fix-up (upstream SPDX `GPL-3.0-only`, pin in
  `docs/research/active-window-border/grouped-window-options.md`).
  Upstream identifiers occur only in research docs, never shipped code.
- Win32 API use (`GetAsyncKeyState`, `NOTIFYICONDATAW` semantics) is
  documented-API use through `windows-sys`; no vendored Microsoft
  snippet (`tray_sys.rs:32-37` cites behavior only).
- Reference-WM findings stay fenced in research docs with reuse guards
  (`docs/research/windows-port/plan.md:486-500`); the one Krohnkite hit
  is a shortcut-holder name in a log string
  (`kwin/src/plan-adapter-entry.ts:5322`).
- Fixtures are first-party generated JSON; `kwin/src`,
  `kwin/native-effect` call KWin/Qt/KF6 APIs without embedding sources.

## 2. Rust dependency inventory

`cargo metadata --locked --format-version 1`: 93 packages = 88 external
crates-io + 5 workspace. License strings are declared `license` fields,
grouped verbatim; each package appears exactly once
(46 + 21 + 16 + 2 + 1 + 1 + 1 = 88; 87 unique names, `syn` twice).

### 2.1 Workspace crates (first-party, all `license = "MIT"`)

| Crate | Role |
| --- | --- |
| `tiler-core` | Portable planning core, no dependencies |
| `tiler-protocol` | Planner protocol over `tiler-core` (serde) |
| `tiler-kwin-effect-ffi` | Staticlib behind the KWin effect C ABI (serde) |
| `omnitiler` | Linux planner/service binary (zbus, rustix, serde) |
| `tiler-windows` | Windows binary (serde; `windows-sys`/`windows-link` on `cfg(windows)`) |

All five declarations are confirmed by `cargo metadata --locked` after license
application. The Rust staticlib stays MIT when linked into the GPL effect.

### 2.2 Screen: no dependency requiring copyleft

No external crate requires copyleft: every declared field offers an
MIT/Apache-2.0/Unicode-3.0 alternative (section 2.3). The only copyleft
token in the inventory is the unchosen `LGPL-2.1-or-later` alternative
in `r-efi 6.0.0` (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`); use under
MIT, LGPL alternative never triggered. No dev-only external dependency;
no build-dependencies in any manifest.

### 2.3 Grouped license table (exact versions)

Declared `MIT OR Apache-2.0` (46):

async-broadcast 0.7.2, async-recursion 1.1.1, async-trait 0.1.92,
bitflags 2.13.1, bumpalo 3.20.3, cfg-if 1.0.4, crossbeam-utils 0.8.22,
enumflags2 0.7.12, enumflags2_derive 0.7.12, errno 0.3.14,
futures-core 0.3.34, futures-io 0.3.34, futures-task 0.3.34,
futures-util 0.3.34, getrandom 0.4.3, hashbrown 0.17.1, hermit-abi 0.5.2,
hex 0.4.3, itoa 1.0.18, js-sys 0.3.104, libc 0.2.189, once_cell 1.21.4,
ordered-stream 0.2.0, piper 0.2.5, proc-macro-crate 3.5.0,
proc-macro2 1.0.107, quote 1.0.47, rustversion 1.0.23, serde 1.0.229,
serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.151,
serde_repr 0.1.21, signal-hook-registry 1.4.8, syn 2.0.119, syn 3.0.4,
tempfile 3.27.0, toml_datetime 1.1.1+spec-1.1.0,
toml_edit 0.25.13+spec-1.1.0, toml_parser 1.1.3+spec-1.1.0,
wasm-bindgen 0.2.127, wasm-bindgen-macro 0.2.127,
wasm-bindgen-macro-support 0.2.127, wasm-bindgen-shared 0.2.127,
windows-link 0.2.1, windows-sys 0.61.2.

Declared `Apache-2.0 OR MIT` (21):

async-channel 2.5.0, async-executor 1.14.0, async-io 2.6.0,
async-lock 3.4.2, async-process 2.5.0, async-signal 0.2.14,
async-task 4.7.1, atomic-waker 1.1.2, autocfg 1.5.1, blocking 1.7.0,
concurrent-queue 2.5.0, equivalent 1.0.2, event-listener 5.4.2,
event-listener-strategy 0.5.4, fastrand 2.5.0, futures-lite 2.6.1,
indexmap 2.14.0, parking 2.2.1, pin-project-lite 0.2.17,
polling 3.11.0, uuid 1.26.0.

Declared `MIT` (16):

endi 1.1.1, memoffset 0.9.1, slab 0.4.12, tracing 0.1.44,
tracing-attributes 0.1.31, tracing-core 0.1.36, uds_windows 1.2.1,
winnow 1.0.4, zbus 5.19.0, zbus_macros 5.19.0, zbus_names 4.3.4,
zcheapstr 1.1.0, zmij 1.0.23, zvariant 5.15.0, zvariant_derive 5.15.0,
zvariant_utils 4.2.0.

| Package | Declared expression | Reading |
| --- | --- | --- |
| rustix 1.1.4 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Use under MIT; LLVM rider needs no decision unless relied on |
| linux-raw-sys 0.12.1 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Same; Linux rustix backend |
| memchr 2.8.3 | `Unlicense OR MIT` | Use under MIT; `Unlicense` not allowed |
| unicode-ident 1.0.24 | `(MIT OR Apache-2.0) AND Unicode-3.0` | Needs `Unicode-3.0` too; build-time proc-macro, still audit-visible |
| r-efi 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | Use under MIT; LGPL alternative never triggered; UEFI-only |

`allow = ["Apache-2.0", "MIT", "Unicode-3.0"]` satisfies every expression
via the MIT/Apache/Unicode alternative. Anything offering only other
terms fails the check by default.

### 2.4 Target split (dependency edges, not compiled output)

`cargo tree -e normal` lists normal-dependency edges per target
(build- and dev-edges excluded) - but tree presence is NOT proof a crate
is compiled or distributed: proc-macro crates appear as normal edges yet
run only at build time as code generators, and target-gated crates
resolve without compiling. Per-manifest gates:
Linux-only (`cfg(unix)` path: async-signal, libc, linux-raw-sys,
signal-hook-registry); Windows-only (`cfg(windows)`: getrandom,
memoffset, tempfile, uds_windows, windows-link, windows-sys, incl. the
direct `tiler-windows` dep on `windows-sys 0.61`); other-platform edges
(hermit-abi, js-sys/wasm-bindgen set, r-efi) and build/feature-dependent
entries (autocfg, rustversion, bumpalo, futures-task/util) resolve in metadata
but are absent from these normal-edge views. Build-edge exclusion does not
establish absence from compilation.

## 3. npm inventory

`kwin/package-lock.json` (`lockfileVersion` 3): all 50 entries
`dev: true`; only the built `main.js` ships, never `node_modules`.
esbuild family 0.28.2 MIT (27), typescript family 7.0.2 Apache-2.0 (21),
`@types/node` 26.2.0 MIT, `undici-types` 8.3.0 MIT. No copyleft.
Direct devDeps: `@types/node`, esbuild, typescript; `engines: node >= 24`.
`license_finder` verification pending (section 9).

## 4. Native link inventory

Effect plugin links own staticlib + `Threads`, `${CMAKE_DL_LIBS}`,
`KWin::kwin`, `KF6::ConfigCore/ConfigGui/ColorScheme`, `Qt6::Core/Gui/DBus`
(`CMakeLists.txt:149-160`). KCMs link only KF6
(CoreAddons/ConfigCore/ConfigGui/I18n/KCMUtils/WidgetsAddons) +
`Qt6::Core/DBus/Widgets` (`:182-192`, `:207-217`). ECM is build-time
only. Nix pin evaluates kwin 6.7.4, qtbase 6.11.1, kconfig 6.29.0.
`${CMAKE_DL_LIBS}` on Linux is `dl`
(`cmake-4.4/Modules/Platform/Linux.cmake: set(CMAKE_DL_LIBS "dl")`):
glibc `libdl.so.2`, LGPL-2.1-or-later, no new license obligation beyond
the C runtime row below. `Threads` is pthreads from the same glibc.

### 4.1 KWin: GPL compatibility required; headers GPL-2.0-or-later

The effect includes `<effect/effect.h>`, `<effect/effectwindow.h>`
(`activewindowborder.h:6-7`), links `KWin::kwin`, loads in-process. Both
headers at pinned tag v6.7.5 carry `SPDX-License-Identifier:
GPL-2.0-or-later`
([effect.h](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/effect/effect.h),
[effectwindow.h](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/effect/effectwindow.h)).
The combined effect binary distributes under GPL-compatible terms
(GPLv2 s2 / GPLv3 s5(c)); GPLv2 s9 permits the GPLv3 election
([GPLv2 s9](https://www.gnu.org/licenses/old-licenses/gpl-2.0.html)).
This does not require our original effect source to be GPL: MIT source can
be linked into a GPL-covered binary while retaining its MIT grant. The
2026-10-11 decision instead chooses GPL-2.0-or-later for the effect sources
to make the boundary explicit and permit KWin code reuse. It does not
relicense the MIT Rust staticlib or the Settings KCMs.
KDE policy confirms the file-level regime
([Licensing Policy](https://community.kde.org/Policies/Licensing_Policy)).
Transitive runtime closure unmapped: treat GPL compatibility as required.
The QML/JS script bundle links no native code.

### 4.2 Qt6/KF6: direct native license table (actual sources)

Each row is the exact SPDX from the upstream file at the pinned tag
(KF6 v6.29.0, Qt qtbase v6.11.1), confirmed identical in the read-only
Nix store `-dev` headers. Use the LGPL alternative in every row.

| Linked target | Actual header / file | Exact SPDX (use LGPL alt) |
| --- | --- | --- |
| `KF6::ConfigCore` (`KSharedConfig`, `KConfigGroup`) | [ksharedconfig.h](https://raw.githubusercontent.com/KDE/kconfig/v6.29.0/src/core/ksharedconfig.h) | `LGPL-2.0-or-later` |
| `KF6::ConfigGui` (`KConfigGui`, `KConfigSkeleton`) | [kconfiggui.h](https://raw.githubusercontent.com/KDE/kconfig/v6.29.0/src/gui/kconfiggui.h) (`LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL`); [kconfigskeleton.h](https://raw.githubusercontent.com/KDE/kconfig/v6.29.0/src/gui/kconfigskeleton.h) (`LGPL-2.0-or-later`) | LGPL (either file) |
| `KF6::ColorScheme` (`KColorScheme`) | [kcolorscheme.h](https://raw.githubusercontent.com/KDE/kcolorscheme/v6.29.0/src/kcolorscheme.h) | `LGPL-2.0-or-later` |
| `KF6::CoreAddons` (`KPluginFactory`, `KPluginMetaData` macros) | [kpluginfactory.h](https://raw.githubusercontent.com/KDE/kcoreaddons/v6.29.0/src/lib/plugin/kpluginfactory.h), [kpluginmetadata.h](https://raw.githubusercontent.com/KDE/kcoreaddons/v6.29.0/src/lib/plugin/kpluginmetadata.h) | `LGPL-2.0-or-later` both |
| `KF6::I18n` (`KLocalizedString`) | [klocalizedstring.h](https://raw.githubusercontent.com/KDE/ki18n/v6.29.0/src/i18n/klocalizedstring.h) | `LGPL-2.0-or-later` |
| `KF6::KCMUtils` (`KCModule`) | [kcmodule.h](https://raw.githubusercontent.com/KDE/kcmutils/v6.29.0/src/kcmodule.h) | `LGPL-2.0-or-later` |
| `KF6::WidgetsAddons` (`KColorButton` et al) | [kcolorbutton.h](https://raw.githubusercontent.com/KDE/kwidgetsaddons/v6.29.0/src/kcolorbutton.h) | `LGPL-2.0-or-later` |
| `Qt6::Core/Gui/DBus/Widgets` | [qobject.h](https://raw.githubusercontent.com/qt/qtbase/v6.11.1/src/corelib/kernel/qobject.h) (same header SPDX on Gui/DBus/Widgets) | `LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only`; use under `LGPL-3.0-only` |

Our Qt modules (Core, Gui, DBus, Widgets) are absent from Qt's GPL-only
module list, so LGPLv3 applies
([Qt Licensing](https://doc.qt.io/qt-6/licensing.html)). KCM macros ride
the LGPL CoreAddons API above. Full distro-specific transitive closure
(e.g. `libKF6ColorScheme` also NEEDED-pulls `KF6GuiAddons`,
`libKF6KCMUtils` pulls `KF6XmlGui/ConfigWidgets/Qt6Qml/Quick`;
`libQt6Core` pulls `icu/glib/zlib/systemd/double-conversion/b2/pcre2`)
stays pending, but the direct link inventory in this table is complete.
Spot `readelf -d` on the read-only Nix store `.so` files confirms
the expected direct NEEDED (`Qt6Core/Gui/DBus/Widgets`, `KF6*`,
`libstdc++`, `libm`, `libgcc_s`, `libc`, `libdl`). NEEDED entries identify
dynamic dependencies; they do not audit statically embedded code.

### 4.3 Tray, planner, script binaries; C/C++ runtime

No C++ tray. Linux tray/planner are `tray`/`planner-service` subcommands
of the `omnitiler` Rust binary (`src/main.rs`); KWin-side
`tray-publisher.ts` ships in the script bundle. D-Bus via pure-Rust
`zbus` 5.19 (`#[zbus::interface]`); no `libdbus` linked. Windows via
Win32/COM through `windows-sys`. No macOS binary. Native closure beyond
section 2 is unaudited follow-up, not a claim.

C/C++ runtime linked by every native `.so` (via `readelf -d NEEDED` on
the read-only Nix store artifacts: `libc.so.6`, `libm.so.6`,
`libgcc_s.so.1`, `libstdc++.so.6`, `libdl.so.2`):

| Runtime | License | Project effect |
| --- | --- | --- |
| glibc (`libc`, `libm`, `libdl`, pthreads) | `LGPL-2.1-or-later` ([LICENSES](https://raw.githubusercontent.com/bminor/glibc/master/LICENSES)) | Dynamic link; no project copyleft |
| libstdc++ / libgcc (`libstdc++.so.6`, `libgcc_s.so.1`) | `GPL-3.0-or-later WITH GCC-exception-3.1` ([GCC runtime exception](https://www.gnu.org/licenses/gcc-exception-3.1.html)) | Exception covers eligible compilation; does NOT force project GPL |
| LLVM/Clang runtime | Unknown (no LLVM runtime linked in the inspected NEEDED) | Pending only if ever linked |

The script `kwin/metadata.json` and both Settings KCM metadata files
(`activeborderconfig_module.json`, `scriptconfig_module.json`) declare MIT.
Only `kwin/native-effect/metadata.json` declares GPL-2.0-or-later.

### 4.4 Per-distro native closure (source-based, no installs)

Source scope: `packaging/debian/control`, `packaging/debian/rules`,
`packaging/debian/copyright`, `packaging/rpm/omnitiler.spec`,
`packaging/arch/PKGBUILD`, `packaging/arch/.SRCINFO`,
`packaging/obs/_service`, plus the link lines in section 4
(`kwin/native-effect/CMakeLists.txt`: effect 148-159, KCMs 181-191
and 206-216; `find_package` floors: ECM 6.24.0, KWin 6.6.0) and the
license facts in sections 4.1-4.3 and 6. No distro packages were
installed; no live builds were run. Anything not determinable from
these sources is marked pending. Transitive `.so` closure per distro
(`readelf -d NEEDED` on distro-built artifacts) is pending for all
four targets.

Recipe-to-link mapping used below:

- KWin effect link (`KWin::kwin`): Debian `kwin-dev` / `libkwin6`;
  RPM `cmake(KWin)` plus `kwin6` (SUSE family) / `kwin` (Fedora family);
  Arch `kwin` (headers ship in the main package, no `-dev` split).
- Qt6 link (`Qt6::Core/Gui/DBus/Widgets`): Debian `qt6-base-dev`;
  RPM `qt6-base-devel` (SUSE) / `qt6-qtbase-devel` (Fedora);
  Arch `qt6-base`.
- KF6 link (`ConfigCore/ConfigGui/ColorScheme/CoreAddons/I18n/KCMUtils/WidgetsAddons`):
  Debian `libkf6config-dev`, `libkf6coreaddons-dev`, `libkf6kcmutils-dev`,
  `libkf6i18n-dev`, `libkf6widgetsaddons-dev`, `libkf6colorscheme-dev`;
  RPM `kf6-kconfig-devel`, `kf6-kcoreaddons-devel`, `kf6-kcmutils-devel`,
  `kf6-ki18n-devel`, `kf6-kwidgetsaddons-devel`, `kf6-kcolorscheme-devel`;
  Arch `kconfig`, `kcoreaddons`, `kcmutils`, `ki18n`, `kwidgetsaddons`,
  `kcolorscheme`.
- C/C++ runtime (`libc/libm/libdl/pthreads` via `Threads` +
  `${CMAKE_DL_LIBS}`; `libstdc++/libgcc_s` via the C++ toolchain):
  Debian `build-essential` toolchain plus `libc`/`libstdc++` via
  `${shlibs:Depends}`; RPM `gcc-c++` plus RPM auto-requires;
  Arch `gcc` plus pacman dependencies. License facts from section 4.3.
- Build-only, not linked into the plugins: `extra-cmake-modules`
  (all targets), `cmake`, `cargo`/`rustc`/`rust`, `pkg-config`,
  `libdrm-dev`/`libdrm-devel`, `libepoxy-devel`, Qt Quick/Declarative
  devel (`qt6-quick-devel` / `qt6-qtdeclarative-devel`, pulled by KWin
  CMake config), Vulkan headers (`vulkan-headers`), `systemd-rpm-macros`,
  `hicolor-icon-theme`, `kf6-kwindowsystem-devel` (RPM only, no matching
  `KF6::WindowSystem` link line), D-Bus helpers (`libkf6config-bin`,
  `libkf6kcmutils-bin`, `kf6-kconfig`, `kf6-kcmutils`, `kcmshell6` /
  `kwriteconfig6` flows).

Declared version pins and floors (source-evidenced):

| Target | KWin floor/pin | ECM floor | Qt/KF6 floor |
| --- | --- | --- | --- |
| Debian (`packaging/debian/control`, `rules`) | `kwin-dev (>= 4:6.6)` build; core runtime `kwin-wayland (>= 4:6.6) \| kwin-x11 (>= 4:6.6)`; effect runtime exact `libkwin6 (= ${kwin:Version})` recorded at build time from the installed `libkwin6` including Debian revision; CMake `find_package(KWin 6.6.0 REQUIRED)` fails closed below 6.6 | `extra-cmake-modules (>= 6.24)` | None beyond the CMake `find_package` names; `noeffect` profile drops `kwin-dev`, colorscheme, libdrm, pkg-config for settings-only core builds |
| RPM (`packaging/rpm/omnitiler.spec`) | `cmake(KWin)` virtual (provider name split noted in recipe); runtime unversioned `kwin6` / `kwin`; effect `Requires` deliberately unversioned so upgrades are never held back; build is always the full native tree | `kf6-extra-cmake-modules >= 6.24.0` (SUSE) / `extra-cmake-modules >= 6.24.0` (Fedora) | None named; full-tree build keeps the 6.24.0 floor |
| Arch (`packaging/arch/PKGBUILD`, `.SRCINFO`) | `kwin` makedepends and runtime depends, both deliberately unversioned; effect `depends=("omnitiler=$pkgver-$pkgrel" kwin)` | `extra-cmake-modules` unversioned | All Qt/KF6 makedepends and core depends unversioned |
| OBS (`packaging/obs/_service`) | No separate dependency list; consumes the release tarball that the RPM recipe builds, so the RPM row applies; KWin upgrade policy and manual source update are documented in the recipe and `docs/installation.md` | Same as RPM | Same as RPM |

Per-distro linked-library license and compatibility. License column
reuses the section 4.1/4.2/4.3 source facts; it is not a per-distro
re-audit of upstream sources. Compatibility column is against the
section 8 decision (effect GPL-2.0-or-later, KCMs MIT).

| Distro target | Linked library (distro package) | License (source) | Compatible with effect GPL-2.0-or-later / KCM MIT |
| --- | --- | --- | --- |
| Debian | KWin (`kwin-dev` / `libkwin6`) | GPL-2.0-or-later (section 4.1 headers) | Yes: same license family for the effect; KCMs do not link KWin |
| Debian | KF6 ConfigCore/ConfigGui/ColorScheme/CoreAddons/I18n/KCMUtils/WidgetsAddons (`libkf6*-dev`) | LGPL (LGPL-2.0-or-later per file, ConfigGui either file LGPL; section 4.2, use LGPL alternative) | Yes: dynamic link, both effect and MIT KCMs |
| Debian | Qt6 Core/Gui/DBus/Widgets (`qt6-base-dev`) | LGPL-3.0-only alternative (section 4.2); absent from Qt GPL-only module list | Yes: effect via v3 election for the combination (section 6); MIT KCMs directly |
| Debian | glibc `libc/libm/libdl`/pthreads (`Threads`, `CMAKE_DL_LIBS`, `${shlibs:Depends}`) | LGPL-2.1-or-later (section 4.3) | Yes: dynamic link, no project copyleft |
| Debian | libstdc++/libgcc (`build-essential` toolchain, `${shlibs:Depends}`) | GPL-3.0-or-later WITH GCC-exception-3.1 (section 4.3) | Yes: exception covers eligible compilation, does not force project GPL |
| RPM (SUSE/Fedora) | KWin (`cmake(KWin)`; runtime `kwin6` / `kwin`) | GPL-2.0-or-later (section 4.1 headers) | Yes: same as Debian row |
| RPM (SUSE/Fedora) | KF6 set (`kf6-*-devel`) | LGPL as above (section 4.2) | Yes: dynamic link |
| RPM (SUSE/Fedora) | Qt6 Core/Gui/DBus/Widgets (`qt6-base-devel` / `qt6-qtbase-devel`) | LGPL-3.0-only alternative (section 4.2) | Yes: effect via v3 election (section 6); MIT KCMs directly |
| RPM (SUSE/Fedora) | glibc / pthreads / libdl (`gcc-c++`, RPM auto-requires) | LGPL-2.1-or-later (section 4.3) | Yes: dynamic link |
| RPM (SUSE/Fedora) | libstdc++/libgcc (`gcc-c++`, RPM auto-requires) | GPL-3.0 WITH GCC-exception-3.1 (section 4.3) | Yes: exception, same as Debian row |
| Arch | KWin (`kwin`) | GPL-2.0-or-later (section 4.1 headers) | Yes: same as above |
| Arch | KF6 set (`kconfig`, `kcoreaddons`, `kcmutils`, `ki18n`, `kwidgetsaddons`, `kcolorscheme`) | LGPL as above (section 4.2) | Yes: dynamic link |
| Arch | Qt6 Core/Gui/DBus/Widgets (`qt6-base`) | LGPL-3.0-only alternative (section 4.2) | Yes: effect via v3 election (section 6); MIT KCMs directly |
| Arch | glibc / pthreads / libdl (`gcc` toolchain, pacman deps) | LGPL-2.1-or-later (section 4.3) | Yes: dynamic link |
| Arch | libstdc++/libgcc (`gcc` toolchain, pacman deps) | GPL-3.0 WITH GCC-exception-3.1 (section 4.3) | Yes: exception |
| OBS | Same as RPM (service consumes the release tarball built by the RPM recipe) | Same as RPM rows | Yes, subject to the same pending items below |

Pending / unknown (not asserted):

- Distro-transitive closure for every target: distro-built `.so`
  NEEDED audit (e.g. KF6 GuiAddons/XmlGui/ConfigWidgets, Qt Qml/Quick,
  icu/glib/zlib/systemd/double-conversion/b2/pcre2 noted in section 4.2),
  distro patch deltas to upstream license headers, and static-embedding
  review beyond NEEDED entries. Section 4.2 direct-link table stays the
  complete audited set.
- Distro compiler-runtime version output (`libstdc++`/`libgcc_s`/`libc`
  SONAMEs per built artifact) and the KWin exact-build identity behind
  each unversioned RPM/Arch depends (Debian records it via
  `kwin:Version`; RPM/Arch deliberately do not pin).
- Non-linked recipe entries need no compatibility claim:
  `kf6-kwindowsystem-devel`, `libdrm`/`libepoxy`, Qt Quick/Declarative
  devel, Vulkan headers, ECM/CMake/Cargo/Rust, `hicolor-icon-theme`,
  config-bin helpers. Listed here only to keep the inventory honest.
- OBS provisioning itself (no account/project/package yet; checksum
  placeholder fails closed) is a release-process pending item, not a
  license finding.

## 5. Reference-WM reuse

Checked October 2026 against upstream license declarations. "Only/later
not established" = GPLv3 text without visible election; assume stricter.

| Project | License | Source |
| --- | --- | --- |
| cosmic-comp | GPL-3.0-only | [repo](https://github.com/pop-os/cosmic-comp) (`Cargo.toml`, `src/main.rs`) |
| niri | GPL-3.0-or-later | [repo](https://github.com/niri-wm/niri) (`Cargo.toml`) |
| PaperWM | GPL-3.0 (unestablished) | [repo](https://github.com/paperwm/PaperWM) (`LICENSE`, develop) |
| Karousel | GPL-3.0 (unestablished) | [repo](https://github.com/peterfajdiga/karousel) (`LICENSE`) |
| awesome | GPL-2.0-or-later | [repo](https://github.com/awesomeWM/awesome) (`awesome.c`) |
| sway / qtile / paneru | MIT | [sway](https://github.com/swaywm/sway/blob/master/LICENSE), [qtile](https://github.com/qtile/qtile), [paneru](https://github.com/karinushka/paneru) |
| Hyprland / i3 / xmonad | BSD-3-Clause | [Hyprland](https://github.com/hyprwm/Hyprland), [i3](https://github.com/i3/i3), [xmonad](https://github.com/xmonad/xmonad) |
| bspwm | BSD-2-Clause | [repo](https://github.com/baskerville/bspwm) |
| rift | Apache-2.0 | [LICENSE](https://raw.githubusercontent.com/acsandmann/rift/main/LICENSE) |

MIT/BSD reusable under any candidate with attribution. awesome
(GPL-2.0-or-later) reusable under v2 or any elected later version. niri
under GPL-3.0-or-later. cosmic-comp forces any combined release onto
GPL-3.0-only. No copies found: conditional guidance, not obligation.

## 6. Candidate consequences

GPL-2.0-or-later code combines with GPLv3-only code by electing GPLv3
for the combination
([FSF matrix note 3](https://gplv3.fsf.org/dd3-faq/),
[FSF upgrade answer](https://gplv3.fsf.org/wiki/index.php?printable=yes&title=FAQ_Update));
a combination containing GPL-3.0-only files then distributes as
GPL-3.0-only, while own files keep their election for other artifacts.

| Candidate | Effect binary | Reuse | LGPL | Win / Mac binaries |
| --- | --- | --- | --- | --- |
| GPL-3.0-or-later | Satisfies incl. GPL-3.0-only reuse | All rows | Compatible | Compatible; stores per section 7 |
| GPL-2.0-or-later | Satisfies (headers v2-compatible; v3 election open) | Absorbs GPL-3.0-only via v3 election (release then GPL-3.0-only) | Compatible via v3 election | Same store position as GPL-3.0-or-later |
| MPL-2.0 | Larger Work (s3.3) if files lack the Secondary-License block; distribute combination under GPL too ([guide](https://www.mozilla.org/en-US/MPL/2.0/combining-mpl-and-gpl/)) | No GPL absorption into MPL-only files | Linking fine | Compatible; header overhead, solves no store gate |
| MIT / Apache-2.0 | NOT alone sufficient: effect linked into KWin still distributes GPL-covered | MIT/BSD rows only | Compatible | Cleanest store optionality if GPL code stays out (boundary required) |

MIT suits any GPL version; Apache-2.0 suits GPLv3 only, not GPLv2
([ASF](https://www.apache.org/licenses/GPL-compatibility.html),
[GNU list](https://www.gnu.org/licenses/license-list.html)). GPL routes
need Corresponding Source + Installation Information discipline (v3 s6).

## 7. Store and platform feasibility

### 7.1 Reference-tiler distribution

| App | License | MAS | Distribution |
| --- | --- | --- | --- |
| Magnet | Proprietary | Present ([listing](https://apps.apple.com/au/app/magnet/id441258766?mt=12), [vendor](https://magnet.crowdcafe.com/)) | Mac App Store |
| Rectangle | MIT ([LICENSE](https://github.com/rxhanson/Rectangle/blob/main/LICENSE)) | Not found | Direct dmg + brew ([repo](https://github.com/rxhanson/Rectangle)) |
| Rectangle Pro | Proprietary | Not found in checked vendor sources | Direct trial/purchase ([vendor](https://rectangleapp.com/pro)) |
| Moom Classic | Proprietary | Present ([listing](https://apps.apple.com/us/app/moom-classic/id419330170?mt=12)) | Legacy listing, grandfathered unsandboxed (section 7.2) |
| Moom 4+ | Proprietary | Absent (vendor) | Direct-only ([vendor](https://manytricks.com/blog/?page_id=6326)) |
| Amethyst / yabai / AeroSpace / paneru / rift | MIT / MIT / MIT / MIT / Apache-2.0 | Not found | GitHub/brew direct ([Amethyst](https://github.com/ianyh/Amethyst), [yabai](https://github.com/asmvik/yabai), [AeroSpace](https://github.com/nikitabobko/AeroSpace), [paneru](https://github.com/karinushka/paneru), [rift](https://github.com/acsandmann/rift)); yabai extras need partial SIP disable |

"Not found" = absent from checked sources, not proof of absence.

### 7.2 Apple sandbox vs window control (technical gate)

Apple's sandbox doc lists as forbidden for sandboxed apps: "Use of
accessibility APIs in assistive apps" and "Sending Apple Events to
arbitrary apps"
([App Sandbox](https://developer.apple.com/documentation/security/protecting-user-data-with-app-sandbox)).
MAS requires sandboxing ([2.4.5(i)](https://developer.apple.com/app-store/review/guidelines/))
and public APIs only ([2.5.1](https://developer.apple.com/app-store/review/guidelines/)).
Moom's vendor states the AX pieces Moom needs "cannot be sandboxed," and
that Moom Classic stays in the store only as a grandfathered
pre-sandbox-rule app from 2011 ("allowed to remain ... even if they
weren't sandboxed, as long as there were no major changes")
([FAQ](https://manytricks.com/osticket/kb/faq.php?id=137)) - so Moom
Classic is NOT evidence a sandboxed tiler passes review today. Magnet's
listing is likewise an older listing with undisclosed architecture: no
claim about what AX surface it uses. MAS feasibility of full auto-tiling
is therefore UNPROVEN; a sandboxed MAS product, if feasible at all,
would be a constrained subset. (Sandbox = build-time entitlement
boundary; AX user consent in System Settings is the separate runtime TCC
layer - no claim here that TCC consent cures sandbox denial.)

### 7.3 GPL vs current Mac App Store terms

Current US terms ([Apple Media Services](https://www.apple.com/legal/internet-services/itunes/us/terms.html)):

- App licenses: every acquired App is governed by the Standard EULA
  "unless Apple or the App Provider provides an overriding custom
  license agreement ('Custom EULA')"; "Apple is a third-party
  beneficiary of the Standard EULA or Custom EULA ... and may therefore
  enforce such agreement."
- Standard EULA (a): "nontransferable license ... on any Apple-branded
  products that you own or control and as permitted by the Usage Rules";
  "You may not transfer, redistribute or sublicense"; "You may not copy
  ..., reverse-engineer, disassemble, attempt to derive the source code
  of, modify, or create derivative works" - carved out only "to the
  extent ... prohibited by applicable law or ... permitted by the
  licensing terms governing use of any open-sourced components included
  with the Licensed Application."
- Usage Rules, All Services: "only for personal, noncommercial purposes
  (except as set forth in the App Store Content section below or as
  otherwise specified by Apple)". The 5-account / 10-device
  (max 5 computers) limits in the same Usage Rules are Apple-Account /
  media-association caps ("use Content from up to five (5) different
  Apple Accounts on each device", "up to ten (10) devices ... signed in
  with your Apple Account"), not per-Mac-app install caps; they are not
  applied here as MAS distribution caps.

Tie to GPL (narrowed): the Standard-EULA redistribution / no-copy /
no-derivative-work bars map onto the "further restrictions" GPLv2 s6 /
GPLv3 s10 forbid; the open-source carve-out covers only the EULA's own
clause-(a) restrictions, not the Agreement-level Usage Rules. The
personal-noncommercial rule carries an App-Store-Content exception, so
no claim is made here that commercial/enterprise MAS use is barred
outright. A Custom EULA overrides the Standard EULA but Apple remains
the enforcing beneficiary. Historical FSF enforcement (GNU Go,
VLC: [analysis](https://www.fsf.org/blogs/licensing/more-about-the-app-store-gpl-enforcement),
[news](https://www.fsf.org/news/2010-05-app-store-compliance)) matches
this reading but predates current terms. Unknown: whether Apple would
accept a GPL Custom-EULA app in practice.

### 7.4 Microsoft Store and winget

ADA v8.11 defines FOSS as OSI-approved-licensed software; the developer
is "solely responsible" for FOSS compliance incl. source availability
(s4.c) and grants Microsoft agency rights "except for any material
subject to any FOSS licenses" (s4.b), with custom license or SALT
fallback (s4.i)
([ADA PDF](https://cdn-dynmedia-1.microsoft.com/is/content/microsoftcorp/microsoft/store/documents/legal/ada/fy26/MS.Store.ADAv8.11.US.EN.pdf)).
No blanket GPL ban. winget: GPL Inkscape installs via the msstore source
with its GPLv3-or-later grant in the manifest
([winget-cli#5619](https://github.com/microsoft/winget-cli/issues/5619)).

### 7.5 Direct distribution

Developer-ID signing and notarization permit direct distribution outside the
Mac App Store ([Apple notarization guidance](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution));
Accessibility consent remains necessary for AX window control. This route
imposes no store EULA, so the Usage-Rule conflict does not arise; Corresponding Source
(and Installation Information where applicable) is still owed. winget has
no license-class restriction beyond accurate manifests.

## 8. Approved decision (2026-10-11)

- Project default: MIT, copyright 2026 Michael Alexander, confirmed against
  git authorship. Root `LICENSE` supplies the notice; `LICENSES/` supplies
  MIT and GPL-2.0-or-later texts. This is not a claim of full REUSE coverage.
- GPL-2.0-or-later exception: `activewindowborder.cpp`, `activewindowborder.h`,
  `activeborderlogic.h`, `oraclepress.h` in `kwin/native-effect`, each with
  SPDX license/copyright headers, plus the effect's `metadata.json` License.
  These are effect implementation/policy inputs, not MIT KCM inputs.
- Everything else stays MIT: all Rust crates including the effect staticlib,
  shared POD FFI declarations and config inputs, KWin TypeScript script,
  Settings KCMs, tray/planner, scripts, packaging and docs. MIT code inside
  the GPL-covered effect binary retains its MIT source license.
- The source choice is deliberate, not forced by linking (section 4.1).
  Non-effect binaries retain licensing flexibility for stores; section 7.2
  still leaves full MAS tiler feasibility unproven.
- Cargo/npm, KPlugin, RPM/Arch/Debian and Nix package metadata now match;
  package payloads carry the applicable first-party notices. Per-distro
  native dependency and vendored attribution checks remain separate.

| Component | Outcome |
| --- | --- |
| Rust core / planner / effect staticlib | MIT; permissive dependency inventory |
| Native effect plugin | GPL-2.0-or-later by choice; combined binary GPL-covered |
| Settings KCMs / KWin script | MIT; KCM link line has Qt/KF6, no KWin GPL dependency |
| Windows binary | MIT; Store + winget viable |
| Future Mac binary | MIT default; no native inventory yet; direct distribution feasible, MAS feature feasibility unproven |

## 9. License tools and policy

`deny.toml` allows exactly `Apache-2.0`, `MIT`, `Unicode-3.0` (minimal set
for section 2.3). The cargo-deny audit and `licenses`/`bans`/`sources` CI gate
are delivered ([evidence](../changes/archive/licensing-audit-gate.md)).
Tools are already declared in
[devenv.nix](../../devenv.nix) (`cargo-deny 0.20.2`, `cargo-about
0.9.0`, `license_finder 7.2.1`, store-verified). Audit commands:

- `cargo deny --locked check licenses` (workspace root; `--locked`
  asserts `Cargo.lock` unchanged,
  [CLI](https://embarkstudios.github.io/cargo-deny/cli/common.html)).
- `(workdir kwin) license_finder report --format csv` (report formats per
  [README](https://github.com/pivotal/LicenseFinder) are text/csv/html/
  markdown - csv chosen for machine use; needs `node_modules` installed).

cargo-about generates the committed union `THIRD-PARTY-NOTICES` using
`about.toml`, `about.hbs` and `scripts/third-party-notices.sh`:

- `just third-party-notices`: fetch locked sources and manually regenerate.
- `cargo fetch --locked`, then `just check-third-party-notices`: offline
  freshness lint; CI follows this sequence and never updates the tracked file.
- Generation uses frozen local crate sources, both supported Linux targets,
  and the shipping normal-dependency closure. MIT is preferred for OR-licensed
  crates so actual harvested copyright texts are retained; synthesized
  copyright-free fallback texts fail the check. Supplemental top-level
  `COPYRIGHT*`/`NOTICE*` files from in-scope crates are appended verbatim.

## 10. Follow-ups and unknowns

Map transitive native
closures per distro/target and confirm complete library-source coverage
beyond the sampled headers, including vendored attribution. License selection,
texts, GPL source headers and manifest/package declarations are complete.
Rectangle Pro internals remain Unknown.
Unsettled: GPL Custom-EULA acceptance in practice; sandboxed full-tiler
feasibility; Installation-Information mapping for direct downloads.

## 11. Vendored attribution inventory (2026-10-12)

Bounded search (not proof of absence) over tracked sources for copied
third-party code that ships in a distributed artifact. The original inventory
predates the generated root `THIRD-PARTY-NOTICES` implementation below.

| Item | Ships? | Attribution needed? |
| --- | --- | --- |
| Checked-in `vendor/` tree | No: no `vendor/` dir exists; the only `vendored-sources` hit is Cargo offline-build source-replacement (`kwin/native-effect/CMakeLists.txt:100-105`), repeated in the packaging recipes | No |
| Third-party license headers / SPDX in shipped sources | None: the only SPDX headers are the four first-party GPL-2.0-or-later effect files (section 8); `crates/` carries no third-party headers (the sole `copied` hit is an English doc word in `crates/tiler-core/src/contract.rs:113`) | No |
| KWin script bundle (`contents/code/main.js`) | Built at build time by esbuild from first-party `kwin/src` (all imports relative; no npm runtime imports); git-ignored build output, never checked in | No |
| npm package code in `main.js` | None: direct devDeps are `esbuild 0.28.2` (bundler), `typescript 7.0.2` (compiler), `@types/node 26.2.0` (types only), all `dev: true` in `kwin/package-lock.json`; the artifact-smoke test asserts no `__esm`/`__commonJS` helpers and no `import`/`export`/`node:` markers in the bundle | No |
| Fonts / icons / assets in the bundle | None bundled: the only shipped icon is first-party `assets/icons/omnitiler.svg`, installed to hicolor by all three recipes | No |
| Build-time `vendor/` tree (cargo vendor output in the release tarball) | Created by the packaging recipes at build time, not checked in; upstream crate sources travel with their own license files, all inside the `deny.toml` allow set (section 2.3; `cargo deny check licenses` passes) | Covered by the Debian `copyright` vendor comment (line 8) plus each crate's own license files; no separate notice file |
| Package payload notices | First-party texts plus the same generated union file in both packages: RPM `%license`, Arch `/usr/share/licenses`, Debian `/usr/share/doc`; Nix tray/effect outputs under `share/licenses` | Generated file covers compiled Rust dependency texts and supplemental copyright notices; Nix payloads verified, actual distro package extraction pending |

Required attributions: first-party MIT/GPL texts via the payload row above;
Debian `copyright` lines 4-8 record the MIT default, the GPL effect files
and the vendor-tree comment.

User decision 2026-10-13 supersedes the tentative no-notice conclusion:
MIT/Apache crates compiled into distributed binaries require license texts;
sources/license files in a build-time vendor tree do not replace binary
artifact notice delivery. Before 0.1, add tool-generated third-party notices
to GitHub Release binaries and Debian/RPM/Arch packages. The accepted delivery
uses one committed generated union file, manual `just` regeneration and a CI
freshness linter. `cargo-about` was already declared in `devenv.nix`.
Keep the CI gate's `licenses`, `bans` and `sources` checks.
Generation verified 70 third-party crate/version entries, 25 harvested
license-text groups and two supplemental COPYRIGHT files (rustix/linux-raw-sys),
with no top-level upstream NOTICE files found in this closure. Selected texts
are MIT and Unicode-3.0; dual-license alternatives do not require duplicating
unselected Apache texts. Source-archive contract
tests and both Rust-bearing Nix payload builds pass; distro package extraction
and future GitHub binary-archive delivery remain unverified, as does hosted CI
execution.
[Evidence](../changes/archive/third-party-notices.md).
[Decision](../decisions.md#licensing), [queue](../backlog.md).

# Ubuntu native effect and KDE support-floor research

- Research date: 2026-10-11. Status: **research done, awaiting user decision**.
- Updated recommendation: **KWin 6.7 is the current full-product API floor**.
  The [support-floor follow-up](#support-floor-and-older-distributions)
  found that the script's tiling observation requires APIs absent from stock
  Ubuntu 26.04's KWin 6.6, even in shared mode. Supporting that target needs
  a script compatibility change and a workspace-behavior decision first.
- Native build recommendation remains ECM **6.24** and a host-matched
  effect-only Debian companion: both Ubuntu base 6.6.4 and updates 6.6.6
  compile. This is a component build route, not evidence that the full
  tiler works on those runtimes. No support/build-floor decision is adopted.
- A throwaway floor change was sufficient: the effect builds and installs;
  the effect-only build passes six offline checks against stock `kwin-dev`
  **6.6.6**. No API workaround or ECM bundle was needed. Runtime
  rendering/loading and OBS builds remain unverified.
- This updates the Ubuntu/Node feasibility assessment in [OBS research](obs.md).
  Existing [offline packaging evidence](../../changes/archive/release-0.1-offline-packaging.md)
  and [core-with-Settings evidence](../../changes/archive/release-0.1-core-settings.md)
  are retained; those builds were not repeated.

## Native package options and consequences

- All stock-Ubuntu options below remain subject to the separate script API
  blocker. ECM bundling or effect omission does not fix that blocker.

| Option | What users get on Ubuntu 26.04 | Effort (relative estimate) | Risks / consequences |
| --- | --- | --- | --- |
| **A. Stock KWin + ECM 6.24 floor (native-build recommendation)** | Core + Settings packages and native visuals can be built; working tiling additionally needs script compatibility | Low build-system change; medium companion packaging and release validation, plus script work | Effect/offline checks proven on 6.6.4 and 6.6.6; separate matching binaries; script API and runtime gates remain |
| B. Bundle pinned ECM 6.26 in source archive | Same features on the same stock KWin; no host ECM upgrade | Medium: archive inputs, local ECM prefix, recipe wiring, attribution and bundle test | No benefit over A demonstrated; maintain another pinned source dependency and module/template closure; does not solve KWin ABI matching |
| C. Keep Ubuntu core + Settings only for 0.1 | Existing package builds, without native border, underlay or drop preview | Lowest additional native packaging work | Still blocked by script API requirements; removing the effect does not make older-KWin tiling work |
| D. Require Kubuntu backports PPA | Currently the same KWin 6.6.6 stack as stock updates | Extra repository/provisioning work | PPA has no newer KWin or resolute ECM; does not fix the current floor; unnecessary third-party repository dependency |
| E. Add KDE neon as a separate target | Full features for neon users if separately built/validated; no stock Ubuntu 26.04 effect binary | High/uncertain: OBS provisioning, noble toolchain/recipe trial, rolling rebuilds | Version-sufficient KDE stack but Ubuntu 24.04 base, distinct ABI target, no verified public neon OBS DoD target |

- Material choice first: retain KWin 6.7 as the full-product floor or pursue
  older-KWin script compatibility. If the latter is selected, A is the
  demonstrated stock-Ubuntu native route; base-only 6.6.4 needs its own
  matching binary, not a 6.6.6 build. Neon remains separate context.
  Effort estimates above do not measure the script compatibility work.

## 1. ECM: actual requirements and safe floor

- [Native CMake](../../../kwin/native-effect/CMakeLists.txt) requests ECM 6.26
  with the effect enabled, 6.24 otherwise. `find_package(KWin REQUIRED)` has
  **no 6.7.5 minimum**. The Rust core does not use ECM; its package needs ECM
  only to build its Settings plugins.
- Direct ECM includes and their roles:

| Module / setting | Used for | Evidence about 6.26 |
| --- | --- | --- |
| `KDEInstallDirs` | KDE/Qt plugin install directories | Stock 6.24 configured and installed all three plugins correctly |
| `KDECMakeSettings` | KDE build defaults, including automoc | Stock 6.24 built the Qt/KF targets successfully |
| `ECMSetupQtPluginMacroNames` with `MACRO_NAMES` | Teach automoc the four `KWIN_EFFECT_FACTORY*` macros | [`MACRO_NAMES` dates to 6.18](https://api.kde.org/ecm/module/ECMSetupQtPluginMacroNames.html); 6.24 satisfies it |

- `kcoreaddons_add_plugin`, `kconfig_add_kcfg_files` and `ki18n_wrap_ui`
  come from the KF6 packages/macros, not a new ECM 6.26 feature. The project
  does not include `KDECompilerSettings` or an ECM 6.26-only module.
- Verified: changing only the effect branch's `find_package(ECM 6.26.0 ...)`
  to `6.24.0` in a throwaway copy produced the effect and both KCMs with the
  existing `-Wall -Wextra -Werror`. A separate offline effect-only build and
  checks also passed. **No observed requirement genuinely needs ECM 6.26.**
- Recommendation: one tested 6.24 floor for effect and Settings. Do not infer
  effect support below 6.24 merely from the macro's 6.18 introduction.
  The follow-up separately proves Settings on ECM 6.13. A later
  implementation should reconcile the RPM/Arch dependency floors and
  documentation with CMake, while keeping actual distro Qt/KF6/KWin inputs.
  This evidence is about Ubuntu's tested stack, not arbitrary older stacks.

## 2. Bundling ECM in the source archive

- ECM 6.26 remains an unbuilt fallback; the follow-up tested a private ECM
  6.24 bundle on Debian, clearing configure but not its later Rust/C++ gaps.
  The needed ECM module files
  carry `BSD-3-Clause`; redistribution requires retained copyright notices,
  conditions/disclaimer and the non-endorsement condition. Preserve upstream
  SPDX headers and license text and extend the vendored attribution / Debian
  copyright inventory. A full upstream source bundle also needs its complete
  per-file license inventory rather than assuming every ancillary file has
  the modules' license. Sources: [ECM upstream](https://github.com/KDE/extra-cmake-modules),
  [BSD-3-Clause text](https://spdx.org/licenses/BSD-3-Clause.html).
- A selective copy needs more than the three direct includes: their closure
  includes `QtVersionOption`, `ECMQueryQt`, `KDEInstallDirs6`,
  `KDEInstallDirsCommon`, `KDESetupPrefixScript`, prefix shell templates and
  `ECMUseFindModules`, plus a usable ECM package config/version. An initial
  static 11-file candidate omitted templates; **no minimal bundle is proven**.
  KWin's `FindLibdrm` and KF6 tools/macros remain distro dependencies.
- Safer fallback: add a checksum-pinned complete ECM 6.26 source payload to
  [the archive builder](../../../scripts/build-source-archive.sh), configure
  it locally with documentation/tests disabled, install to a private build
  prefix, and point the effect's `ECM_DIR` to that prefix's
  `share/ECM/cmake`. Raw upstream source cannot be selected as an installed
  package directly: [`ECMConfig.cmake` and `ECMConfigVersion.cmake` are generated](https://github.com/KDE/extra-cmake-modules/blob/v6.26.0/CMakeLists.txt).
  Its [declared CMake minimum is 3.29](https://github.com/KDE/extra-cmake-modules/blob/v6.26.0/CMakeLists.txt#L9);
  Ubuntu's tested CMake 4.2.3 clears that prerequisite.
- Preserve the archive's deterministic tag/source/checksum contract; fetch
  and validate the added source before the offline distro build. No global
  ECM replacement is necessary and ECM is not a runtime dependency.
- Maintenance cost: version/checksum refresh, upstream licensing inventory,
  archive identity/reproducibility checks, local-prefix recipe integration
  and offline tests on supported stacks. A selective bundle additionally
  requires tracking include/template edges. Lowering the tested floor avoids
  that additional ownership without changing effect code.

## 3. KWin headers, APIs and ABI

### Baselines and source compatibility

- Prior research correctly identified base `kwin-dev` **4:6.6.4-0ubuntu1**.
  Current stock `resolute-updates` supplies **4:6.6.6-0ubuntu0.1**, used in
  this experiment. Sources: [base package](https://packages.ubuntu.com/resolute/kwin-dev),
  [updates package](https://www.ubuntuupdates.org/package/core/resolute/universe/updates/kwin-dev).
  The follow-up also built the effect against release-only 6.6.4 successfully.
- The supplied reference
  [`8438567a741826da8b7536a8b10eb3af8fc8820d`](https://github.com/KDE/kwin/commit/8438567a741826da8b7536a8b10eb3af8fc8820d)
  is a translation-sync commit whose
  [`PROJECT_VERSION` is **6.7.4**](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/CMakeLists.txt),
  not 6.7.5. Keep the pinned-reference comparison distinct from the 6.7.5
  distro build evidence in [installation](../../installation.md).
- Stock `kwin-dev` supplies the internal headers actually consumed by
  [the effect header](../../../kwin/native-effect/activewindowborder.h) and
  [implementation](../../../kwin/native-effect/activewindowborder.cpp):

```text
effect/{effect,effecthandler,effectwindow}.h
scene/{imageitem,outlinedborderitem,workspacescene,windowitem}.h
core/inputdevice.h
input.h  input_event.h  input_event_spy.h
options.h  window.h  plugin.h
```

- Comparison against the exact reference: `outlinedborderitem.h`,
  `input_event.h`, `plugin.h`, `core/inputdevice.h` are identical; the other
  ten headers differ textually. A diff is not evidence of a used API break.
  The decisive result is that the current effect compiles and links against
  stock 6.6.6 without C++ edits. Used window movement/maximize/visibility,
  geometry/fullscreen, add/delete/close, stacking/focus and mouse signals
  are present, as are the configured resize-binding `options.h` accessors.
- No compile-breaking API difference was found in the consumed surface.
  Border/underlay scene behavior, move/resize signal timing, drop-preview
  visibility and rendering on Ubuntu are **unverified at runtime**. Unit
  logic checks do not establish compositor behavior.

### Source gates and mismatch behavior

- There are no KWin version `#if` guards in the effect. The existing
  [Nix factory](../../../flake.nix) selects the caller's `kwin.dev` and an
  explicit `KWin_DIR`; the
  [host builder](../../../scripts/nix-host-kwin-build.sh) resolves the exact
  host derivation. These are provenance gates, not a 6.7.5 source minimum.
- A later supported-version gate could use CMake's discovered `KWin_VERSION`
  and explicit compile definitions if a real API split appears. Stock
  `config-kwin.h` has a string `KWIN_PLUGIN_VERSION_STRING` and
  `PROJECT_VERSION_PATCH`, not a usable major/minor integer macro pair.
  A direct preprocessor numeric comparison of the version string is not
  viable. No conditional implementation is currently needed; neither a
  version range nor guards make a binary cross-version ABI-compatible.
- Verified metadata: the Ubuntu-built effect embeds
  `org.kde.kwin.EffectPluginFactory6.6.6`. Upstream loaders at
  [6.6.6](https://github.com/KDE/kwin/blob/v6.6.6/src/effect/effectloader.cpp)
  and [6.7.5](https://github.com/KDE/kwin/blob/v6.7.5/src/effect/effectloader.cpp)
  compare this IID to their expected version **before** `loader.instance()`.
  A different-version plugin is refused before effect instantiation; an
  ordinary Qt loader failure returns no factory and logs the loader error.
  Thus a 6.7.5 effect is not a shipping binary for stock Ubuntu 6.6.x.
- Residual risk: the same IID does not detect distro-patched ABI/layout
  changes that leave KWin's upstream version unchanged. Such mismatches can
  misbehave or crash; a loader error is not guaranteed. Rebuild for the
  actual distro development/runtime packages and verify upgrades.
  [Existing policy](../../installation.md#kwin-upgrades-policy-and-limitation)
  avoids holding KWin upgrades back and keeps core independent. OBS rebuild
  lag can temporarily remove the native visuals; it cannot provide atomic
  publication with an external KWin update.

## 4. Node 22 versus Node >=24

- [The source archive](../../../scripts/build-source-archive.sh) contains
  prebuilt `kwin/contents/code/main.js` with `SOURCE_REV` baked in and
  vendored Rust crates. [Debian rules](../../../packaging/debian/rules)
  validate and install that bundle, never invoke npm, and build Rust offline.
  KWin executes generated JavaScript, not Node. **Node 22 is not an Ubuntu
  package-build or runtime obstacle.**
- Archive production is a different stage: it still runs Node/npm.
  [`kwin/package.json`](../../../kwin/package.json) declares Node >=24.
  As an additional experiment, stock Node 22.22.1 / npm 9.2.0 completed
  `npm ci --no-audit --no-fund` and `build:installed` with exit 0, producing
  a 1,014,273-byte bundle containing an injected hexadecimal test revision,
  not the real source SHA; npm emitted `EBADENGINE`. This is not a
  supported-floor change or a complete archive producer test on Node 22.
  Engine-strict settings may reject it. Keep the
  declared archive-production toolchain and prebuilt-input contract.

## 5. Kubuntu backports and KDE neon

| Target snapshot | Verified package versions / contents | Consequence |
| --- | --- | --- |
| [Kubuntu backports, resolute](https://launchpad.net/~kubuntu-ppa/+archive/ubuntu/backports/+index?field.series_filter=resolute) | 18 published Plasma source packages, 6.6.6 point releases; [`kwin-dev` 4:6.6.6-0ubuntu0.1~ubuntu26.04~ppa1](https://www.ubuntuupdates.org/package/kubuntu-ppa_backports/resolute/main/base/kwin-dev); no resolute ECM publication | No newer upstream KWin than stock updates, no ECM 6.26; not a blocker-lifting route |
| [neon User noble apt index](https://archive.neon.kde.org/user/dists/noble/main/binary-amd64/Packages.gz) | `kwin-dev` 4:6.7.5-0zneon+24.04+noble+release+build109; ECM 6.30.0 build56; KF6 development packages 6.30.0; Qt base dev 6.11.1 build162 | KDE dependencies clear the current floor, but this is its own 24.04/noble-based ABI target |

- PPA absence of resolute ECM was checked against the series-filtered
  publication list and [ECM pool](https://ppa.launchpadcontent.net/kubuntu-ppa/backports/ubuntu/pool/main/e/extra-cmake-modules/),
  which contains older jammy-era ECM files. Do not equate OBS's Ubuntu
  `backports` pocket with this PPA.
- Neon [Release metadata](https://archive.neon.kde.org/user/dists/noble/Release)
  is dated 2026-10-06 09:15:37 UTC, `Suite: noble`, User Edition. The index
  retains older builds; versions above are the highest published amd64
  entries observed, not a measured freshness/SLA promise. The
  [neon FAQ](https://neon.kde.org/faq) also identifies the 24.04 base.
- Neon toolchain packages are absent from its overlay; they come from noble.
  Its complete Rust/CMake resolver/build was not tested. `kwin-dev` depends
  on neon `kf6-*` packages and exact-version `kwin-common`; a dedicated recipe
  must resolve that stack rather than reuse stock Ubuntu KWin packages.
- No public neon DoD target was found under the checked conventional OBS
  project names; unauthenticated OBS search cannot prove exhaustive absence.
  [OBS issue #19317](https://github.com/openSUSE/open-build-service/issues/19317)
  documents the external-resource admin-rights error and was closed with a
  referral to `admin@opensuse.org`, not a provisioned mirror. Provisioning
  remains unverified. Exact next step if neon is selected: request a User
  `noble/main` DoD target, obtain its actual OBS project/repository path and
  refresh expectations, then test the complete recipe on that target.

## 6. OBS Ubuntu:26.04 feasibility

- Verified: [`Ubuntu:26.04`](https://build.opensuse.org/project/show/Ubuntu:26.04)
  exists. Its [repositories](https://build.opensuse.org/repositories/Ubuntu:26.04)
  expose `standard`, `update`, `universe`, `universe-update`, `backports`,
  `universe-backports`, including x86_64. Its
  [project configuration](https://build.opensuse.org/projects/Ubuntu:26.04/prjconf)
  is Debian-format, with Ubuntu 2604 macros. Target existence does not prove
  an OmniTiler resolver/build/publish run.
- KWin, ECM and the needed KDE development packages require **universe**;
  `standard` alone is insufficient. For the updated target, place
  `universe-update` before `universe` and `update` before `standard`.
  [OBS PR #19085](https://github.com/openSUSE/open-build-service/pull/19085)
  explains first-repository-wins stacking and proposes highest-version
  multi-pocket merging; it remains open/unmerged at this research date.
  Verify actual selected versions in OBS buildinfo, not just path names.
- **Core + Settings:** existing Debian recipe uses ECM 6.24 and no `kwin-dev`;
  prior offline `.deb` build succeeded. Stock dependencies are present, so
  the standard OBS target is build-feasible without PPA/neon provisioning.
  The script's KWin 6.7 API requirement prevents equating that build with
  working stock-Ubuntu tiling. The
  actual OBS Debian backend, `cargo:native`/`rustc:native` resolution and
  install/upgrade solver acceptance have not been exercised.
- **Effect:** the checked-in CMake floor still rejects stock ECM. Option A
  removes the demonstrated configure blocker; an effect-only Debian package
  must then be added. Current [Debian control](../../../packaging/debian/control)
  declares only core and has no effect Build-Depends/binary stanza. Add
  `kwin-dev`, `libkf6colorscheme-dev`, `libdrm-dev`, `pkg-config` to the
  effect build inputs alongside the existing Qt/KF6/Rust inputs. Do not add
  effect dependencies to a core-only build. `FindLibdrm` is pulled by
  KWin's CMake config even without a direct project DRM call.
- [The OBS source service](../../../packaging/obs/_service) consumes the
  release archive plus a pinned checksum; it does not need a new
  `cargo_vendor` service or Node on the build worker. OBS accounts/project,
  source/checksum handoff and actual service execution remain the separate
  [release-handoff backlog](../../backlog.md) work. Neon/PPA DoD access is
  not a prerequisite for stock Ubuntu.

## Empirical evidence and reproduction summary

- Test condition: disposable Ubuntu 26.04 amd64 container using stock apt
  packages; isolated source copy. Only the copy's ECM effect floor changed.
- Installed versions: ECM 6.24.0-0ubuntu1; KWin dev 4:6.6.6-0ubuntu0.1;
  Qt base dev 6.10.2+dfsg-7; KF6 dev 6.24.0; Rust/Cargo 1.93.1ubuntu1;
  CMake 4.2.3-2ubuntu2; Node 22.22.1/npm 9.2.0 for the separate JS probe.
- Container preparation installed `build-essential cmake extra-cmake-modules
  kwin-dev qt6-base-dev libkf6config-dev libkf6coreaddons-dev
  libkf6kcmutils-dev libkf6i18n-dev libkf6widgetsaddons-dev
  libkf6colorscheme-dev libdrm-dev pkg-config ca-certificates cargo rustc`.
  Preparation may use the network; the final effect build ran with Docker
  `--network none` and a locked 88-crate vendor tree.

| Experiment | Observed result | What it establishes |
| --- | --- | --- |
| Original CMake, ECM 6.24 | Configure fails: ECM 6.26 requested, installed 6.24 rejected | The declared floor is the first blocker |
| Throwaway floor 6.24, full native build | Configure/build/install succeed; effect and both KCM `.so` files produced | No C++ API patch or new ECM needed on tested stock stack |
| Effect-only, vendored Rust, network disabled | Configure/build/install exit 0; only effect `.so` installed; IID ends in 6.6.6 | Offline effect companion source-build route |
| Effect-only CTest, offscreen | 6/6 pass: logic, group highlight, metadata/factory, unified lifecycle, drag-oracle Rust, group-highlight Rust | Existing non-compositor checks pass on this toolchain |
| Node 22 JS probe | npm warning, exit 0, injected test revision present | Bundle mechanism proof only; package recipes already need no Node |

- Equivalent commands inside the prepared container, with `SOURCE`, `BUILD`,
  `VENDOR`, `DEST` identifying disposable inputs/output and the floor change
  already applied to that source copy:

```sh
export CARGO_NET_OFFLINE=true
export OMNITILER_VENDOR_DIR="$VENDOR"
cmake -S "$SOURCE/kwin/native-effect" -B "$BUILD" \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON \
  -DOMNITILER_BUILD_EFFECT=ON -DOMNITILER_BUILD_SETTINGS=OFF \
  -DCMAKE_INSTALL_PREFIX=/usr
cmake --build "$BUILD"
DESTDIR="$DEST" cmake --install "$BUILD"
QT_QPA_PLATFORM=offscreen ctest --test-dir "$BUILD" --output-on-failure
```

- Installed effect path relative to the staging root:
  `usr/lib/x86_64-linux-gnu/qt6/plugins/kwin/effects/plugins/omnitiler-active-border.so`.
- Intermediate preparation failures were missing `libdrm-dev`/`pkg-config`
  at configure and missing CA certificates during online crate retrieval.
  Adding the missing container inputs resolved them; they were not KWin API
  failures. The later offline build removes crate-network dependence.
- Evidence limits: this was a source-copy experiment, not a new final release
  tarball or effect `.deb`/OBS build. No live compositor was started; no
  rendering, load, absent/failed/removed-effect, KWin upgrade or dependency
  solver acceptance is claimed. These initial results are extended below
  with base-only 6.6.4 and Debian diagnostics; a complete neon build remains
  untested.

## Support floor and older distributions

### Compatibility table

- Compile results describe source experiments or explicitly identified prior
  package evidence. Script gaps are static source findings, not live-session
  observations. A successful native build is not full-product acceptance.

| Distribution / version | KWin | ECM | Compile result | Script API gaps | Recommended support status |
| --- | --- | --- | --- | --- | --- |
| Ubuntu/Kubuntu 26.04 LTS, release pocket only | 6.6.4 | 6.24 | Effect build/install + 6/6 offline checks pass with throwaway 6.24 floor; matching headers/library | Required per-output desktop methods absent; tiling observation fails closed in every mode | Native-build capable, full product blocked pending script compatibility |
| Ubuntu/Kubuntu 26.04 LTS, updates | 6.6.6 | 6.24 | Prior core + Settings package and full native builds pass; effect-only 6/6 checks pass | Same missing methods as base | Same full-product blocker; updates do not supply 6.7 APIs |
| Ubuntu/Kubuntu 24.04 LTS, stock | 5.27.11 | 5.115 | Not built; stock KWin/Qt5/KF5 cannot satisfy current effect/KCM source | `client*` versus required `window*` API; desktop/output argument differences; no per-output desktop methods | Unsupported as-is; separate Plasma 5 port required |
| Debian 13 trixie stable | 6.3.6 | 6.13 | Settings pass at 6.13; stock-Rust core fails; newer-Rust core passes; effect still fails on missing border header after ECM/Rust workaround | Required per-output desktop methods absent | Unsupported as-is; toolchain, native API and script work required |
| Debian 12 bookworm, oldstable/LTS | 5.27.5 | 5.103 | Not built; Plasma 5 stack is incompatible as-is | Same Plasma 5 interface mismatch | Unsupported as-is; LTS security status does not change APIs |
| openSUSE Leap 16.0, base | 6.4.2 | 6.16 | Not built; source tag lacks required border header; ECM also predates `MACRO_NAMES` | Required per-output desktop methods absent | Unsupported as-is; native/script compatibility needed |
| Fedora 43, base only | 6.4.5 | 6.18 | Not built; source tag lacks required border header; macro prerequisite present but declared floors reject | Required per-output desktop methods absent | Unsupported base; use a separately validated newer KDE stack |
| Previously verified Fedora updates / Tumbleweed / Arch stacks | 6.7.5 | 6.30 | Prior offline core + native package builds passed ([evidence](../../installation.md)) | Audited required per-output methods present | Candidate 0.1 targets; remaining runtime/release gates still apply |

### Ubuntu and Debian LTS boundary

- **Ubuntu 26.04 is the current LTS**, released 2026-04-23; 24.04 is the
  previous LTS. Sources: [Canonical announcement](https://canonical.com/blog/canonical-releases-ubuntu-26-04-lts-resolute-raccoon),
  [26.04 release notes](https://documentation.ubuntu.com/release-notes/26.04/).
  Ubuntu Desktop itself uses GNOME; the KDE stack assessed here is Kubuntu
  or KDE packages from the Ubuntu archive. Stock noble ships
  [KWin 5.27.11](https://packages.ubuntu.com/noble/kwin-dev) and
  [ECM 5.115](https://packages.ubuntu.com/noble/extra-cmake-modules).
- **Debian 13 trixie is stable**, with KWin/Plasma 6.3.6. **Debian 12 is now
  oldstable under Debian LTS**, with Plasma/KWin 5.27.5. Regular bookworm
  support ended 2026-07-11, LTS runs through 2028-06-30 with reduced
  architecture/package coverage; KWin-specific LTS security coverage was
  not verified. Sources: [trixie release](https://www.debian.org/releases/trixie/),
  [bookworm lifecycle](https://www.debian.org/releases/bookworm/),
  [KWin package tracker](https://tracker.debian.org/pkg/kwin),
  [bookworm ECM tracker](https://tracker.debian.org/pkg/extra-cmake-modules).
- Plasma 5 support is **impossible with the current sources and stock stack**,
  not fundamentally impossible to implement. The script requires
  `windowList`, `activeWindow`, `windowAdded`/`windowRemoved`/`windowActivated`;
  5.27 exposes `clientList`, `activeClient` and `client*` signals instead.
  Its `removeDesktop(int)` and `sendClientToScreen(Window*, int)` also differ
  from current object arguments. The native tree requests Qt6/KF6 and KWin
  6 internal APIs; newer Rust or an ECM bundle cannot port those interfaces.
  Sources: [5.27 wrapper](https://github.com/KDE/kwin/blob/Plasma/5.27/src/scripting/workspace_wrapper.h),
  [current startup window-list gate](../../../kwin/src/plan-adapter-entry.ts),
  [native dependencies](../../../kwin/native-effect/CMakeLists.txt).

### Release-pocket Ubuntu build proof

- Prepared an isolated `ubuntu:26.04` builder using only the `resolute`
  release pocket, with release pin priority 1001 and a downgrade of packages
  baked into the image from updates. Merely deleting update sources initially
  left incompatible preinstalled library versions; the pinned downgrade
  resolved that builder-preparation issue. This was not a host-system change.
- Inventory confirmed `kwin-dev`, `kwin-common`, `libkwin6` all at
  **4:6.6.4-0ubuntu1**, ECM 6.24.0-0ubuntu1, Qt 6.10.2, KF6 6.24, Rust/Cargo
  1.93.1ubuntu1 and CMake 4.2.3-2ubuntu2. Headers and link-time KWin library
  matched; no 6.6.4/6.6.6 mixture was used.
- With only the source-copy effect ECM floor lowered to 6.24, offline
  effect-only configure/build/install passed and CTest passed **6/6**.
  The installed plugin's IID is
  `org.kde.kwin.EffectPluginFactory6.6.4`; its ELF linkage resolves to the
  matching `libkwin6`. Reproduction uses the earlier effect-only CMake/Cargo
  commands after the release-pocket builder preparation. Runtime loading,
  rendering, upgrade transactions and OBS builds were not exercised.

### Debian 13 build proof and actual toolchain constraints

- Stock `debian:trixie` inventory: KWin dev/library **4:6.3.6-1**,
  ECM **6.13.0-1**, Qt **6.8.2+dfsg-9+deb13u2**, KF6 **6.13**, CMake
  **3.31.6-2**, Rust/Cargo **1.85.1+dfsg1-1+deb13u1**. All source inputs
  came from a copy of commit `0bb4b14`, with the locked vendor tree.

| Build leg | Result / failure | Interpretation |
| --- | --- | --- |
| Core, stock Rust 1.85.1, `cargo build --release --locked --offline -p omnitiler` with vendor replacement | Exit 101: locked `zbus` 5.19.0, `zvariant` 5.15.0 and associated crates require Rust 1.87 | Edition-2024 support alone is insufficient |
| Settings-only, throwaway ECM floor 6.13 | Configure/build/install pass; both KCMs produced, no `libkwin` linkage | Qt 6.8.2 / KF6 6.13 / CMake 3.31.6 suffice for these KCM sources |
| Effect-only, throwaway ECM floor 6.13, stock ECM | Configure exit 1: `Unknown keywords ... MACRO_NAMES;KWIN_EFFECT_FACTORY;...` | Module exists, but this argument needs ECM >=6.18; lowering to 6.13 alone is invalid |
| Effect configure with private pinned ECM 6.24 | Configure passes | Full ECM source bundle/private prefix is feasible; it does not resolve later blockers |
| Effect with private ECM 6.24 and stock Rust | Build exit 2: `E0658` for let-chains and `unsigned_is_multiple_of` | Independent source-language floor beyond Rust 1.85 |
| Core diagnostic with nonstock Rust 1.98.1 / Cargo 1.98.0 | Offline locked build exit 0 | Newer Rust clears the observed core compiler blocker; not a stock-trixie build |
| Effect diagnostic with that Rust and private ECM, stock Debian C++/KWin | Rust staticlib passes, C++ build exit 2: `scene/outlinedborderitem.h: No such file or directory` | Real KWin API/header gap, not merely ECM/Rust version checks |

- ECM bundle used the complete
  [6.24.0 source tarball](https://download.kde.org/stable/frameworks/6.24/extra-cmake-modules-6.24.0.tar.xz),
  SHA-256 `8ef3f7e176588e099c02559d20ddf4fed0590f92c168f0bcc60a7e638ba1e6a3`.
  Configured with docs/tests disabled, installed to a private build prefix,
  selected through `-DECM_DIR=<prefix>/share/ECM/cmake`; no global replacement
  or C++/Rust source edits. The newer-Rust diagnostic used an existing
  toolchain read-only, retaining Debian's C++/Qt/KF6/KWin packages.
- **Observed Rust lower bound is 1.88**, not a declared/proven MSRV:
  `edition = "2024"` needs 1.85; locked D-Bus dependencies demand 1.87;
  [let-chains in `size_hints.rs`](../../../crates/tiler-core/src/size_hints.rs)
  require [Rust 1.88](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/),
  while integer `is_multiple_of` requires 1.87. No crate currently declares
  `rust-version`. Versions 1.88-1.92 were not tested; passing compilers here
  were 1.93.1 (Ubuntu) and 1.98.1 (Debian diagnostic).
- Current CMake requests >=3.19 and does not set explicit Qt/KF6 minimums.
  The Debian Settings experiment demonstrates their tested lower stack;
  it does not establish minimum versions for every native feature. ECM
  6.18 is the effect macro prerequisite, **6.24 is the tested successful
  effect build version**. CMake 3.31.6 was not a blocker, including when
  configuring/installing ECM 6.24 (upstream CMake minimum 3.27).
- The diagnostic stopped at the first missing include; further effect API
  differences are untested behind that gap. Shipping Debian
  13 would require more than lowering floors or supplying a modern compiler.

### Script API audit and the limiting floor

- Checked the script's actual runtime accesses against KWin's exposed
  `Q_PROPERTY`, `Q_INVOKABLE`, scriptable methods and signals, rather than
  counting names in type declarations. Wrapper baselines:
  [6.3.6](https://github.com/KDE/kwin/blob/v6.3.6/src/scripting/workspace_wrapper.h),
  [6.4.5](https://github.com/KDE/kwin/blob/v6.4.5/src/scripting/workspace_wrapper.h),
  [6.6.4](https://github.com/KDE/kwin/blob/v6.6.4/src/scripting/workspace_wrapper.h),
  [6.6.6](https://github.com/KDE/kwin/blob/v6.6.6/src/scripting/workspace_wrapper.h),
  [6.7.0](https://github.com/KDE/kwin/blob/v6.7.0/src/scripting/workspace_wrapper.h),
  [6.7.5](https://github.com/KDE/kwin/blob/v6.7.5/src/scripting/workspace_wrapper.h).

| Consumed interface | 6.3.x | 6.4.x | 6.6.x | 6.7.x | Significance |
| --- | --- | --- | --- | --- | --- |
| `windowList`, `activeWindow`, `screens`, `activeScreen`, `desktops`, `currentDesktop`, `clientArea`, desktop creation/removal, output-object `sendClientToScreen` | Present at 6.3.6 | Present at 6.4.5 | Present at 6.6.4/6.6.6 | Present | Basic object-based window/desktop/output interfaces are not the limiting gap |
| `windowAdded`, `windowRemoved`, `windowActivated`, `screensChanged`, `desktopsChanged` | Present | Present | Present | Present | Lifecycle/topology resync |
| `currentDesktopForScreen`, `setCurrentDesktopForScreen` | Absent | Absent | Absent | Present from 6.7.0 | Required by current observation/navigation; the full-product floor |
| `currentDesktopChanged` | One argument | One argument | One argument | Three arguments | Subscription re-reads state with a zero-argument callback; no static arity blocker found |
| Window geometry/membership/state properties and signals, output identity/geometry, desktop id | Audited at 6.3.6 | Detailed window audit not repeated | Detailed window audit not repeated | Audited current surface | Most basic APIs predate 6.7; this does not override missing workspace methods |
| `rootTile` and low-level Tile manipulation | `rootTile` absent | Present | Present | Present | Declared types, not current runtime calls; not the limiting tiling API |

- Per-output desktop APIs were introduced by
  [commit `fa6c4a42`](https://github.com/KDE/kwin/commit/fa6c4a42), first
  stable in 6.7.0, after the 6.6 line. `rootTile` arrived in 6.4 via
  [commit `eded5996`](https://github.com/KDE/kwin/commit/eded5996), but the
  product does not currently call that API; it must not inflate the floor.
- Concrete current-source chain:
  - [`observeNative`](../../../kwin/src/plan-adapter-entry.ts) reads
    `currentDesktopForScreen` at lines 2856-2859 and returns null if absent.
    `observeHiddenDomains` returns an empty list at lines 2231-2234.
    Neither gate branches on workspace mode.
  - `collectAllSnapshots` at lines 5429-5455 receives no foreground/background
    domains through those paths. Static consequence: automatic tiling cannot
    obtain its normal planner snapshots on unmodified KWin <=6.6.
  - [`WorkspaceNativeAdapter`](../../../kwin/src/workspace-native.ts)
    independently gates per-output reads/writes on those methods. Its
    `synchronizeShared` fallback can write `currentDesktop`, but does not
    fix the mode-independent observation gates. **Selecting shared mode
    alone is not a demonstrated workaround.**
  - Default is `per-output-local`; startup accepts the mode without automatic
    capability-based downgrade. No runtime polyfill was found. Capability
    checks fail closed; this is not evidence of a compositor crash.
- Window `frameGeometry`, `desktops`, state flags, size hints and relevant
  geometry/move/desktop/output signals were present in the 6.3.6
  [window header](https://github.com/KDE/kwin/blob/v6.3.6/src/window.h), with
  [output](https://github.com/KDE/kwin/blob/v6.3.6/src/core/output.h) and
  [desktop](https://github.com/KDE/kwin/blob/v6.3.6/src/virtualdesktops.h)
  properties. Optional/global helpers have guarded fallback paths; type-only
  Tile/window declarations are not proof of a required runtime dependency.
  Signal timing and JS exposure behavior were not run in a compositor.
- Independent native lower bound: required `OutlinedBorderItem` was added by
  [commit `3843ab61`](https://github.com/KDE/kwin/commit/3843ab61), first
  found in stable 6.5.0 and absent in 6.3.6, 6.4.2 and 6.4.5. Its presence
  alone does not prove full 6.5 native compatibility. KWin 6.0-6.2 were not
  exhaustively audited; the later required API introductions already exclude
  them from current full-product support.
- Context package sources: [Leap 16 OSS archive](https://download.opensuse.org/distribution/leap/16.0/repo/oss/)
  (`kwin6` 6.4.2, ECM 6.16) and Fedora 43 base
  [KWin](https://packages.fedoraproject.org/pkgs/kwin/kwin/fedora-43.html) /
  [ECM](https://packages.fedoraproject.org/pkgs/extra-cmake-modules/extra-cmake-modules/fedora-43.html)
  (6.4.5 / 6.18). Exact Qt/KF6/Rust inventories and builds on those two
  targets were not performed; base and updates must not be conflated.

### Neon package-reuse context

KDE neon User still publishes its Plasma/KWin 6.7.5 stack on an Ubuntu
24.04/noble base ([FAQ](https://neon.kde.org/faq), noble index above);
[early-October community discussion](https://discuss.kde.org/t/updated-isos-for-neon/50632)
reports that the 26.04 rebase had not started, rather than an official
schedule commitment. Ubuntu 26.04 binaries cannot be assumed installable
before the rebase: base-library and package dependencies differ, even though
neon's Qt/KF6 are newer. After a 26.04 rebase, core/KCM reuse may become
possible if actual dependencies and Qt/KF6 linkage validate; it is not
automatic. An Ubuntu 6.6.x-built effect would still be refused by neon's
6.7.x factory IID check and needs a build for neon's exact KWin. No neon
install/rebase test was performed.

## Decision and exact next action

- User chooses **KWin 6.7 as the current full-product floor**, or authorizes
  a separate older-KWin compatibility change and decides how workspace modes
  should behave without native per-output desktops. Recommendation: retain
  6.7 for the initial unchanged-product scope; no Plasma 5 port or Debian 13
  native port for 0.1. Exact 6.7.0 native builds were not run; prior 6.7.5
  builds and this source audit do not replace runtime acceptance.
- If stock Ubuntu 26.04 support is selected, resolve and verify the script
  observation/workspace compatibility first, then lower/reconcile the tested
  ECM floor and add a host-matched effect-only Debian companion. Use exact
  release-archive and OBS builds, including the correct universe/update
  ordering. Base-only 6.6.4 compile feasibility is now proven, but not runtime
  compatibility or binary reuse across KWin versions.
- Before shipping a selected target: validate its KDE session's border, group
  underlay/drop preview and missing/failed/removed-effect behavior, plus
  KWin upgrade/solver transactions and rebuild timing. Preserve the existing
  non-pinning, independent-core policy. Same-version ABI patches require
  matching rebuilds; no source/version gate eliminates that risk.

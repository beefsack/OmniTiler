# Ubuntu 26.04 native effect shipping research

- Research date: 2026-10-11. Status: **research done, awaiting user decision**.
- Recommendation: use stock Ubuntu 26.04 updates, lower the effect's ECM
  requirement to the already-used Settings floor of **6.24**, and add an
  effect-only Debian companion built against that target's KWin. This is a
  proposed follow-up, not an implemented build or packaging decision.
- A throwaway floor change was sufficient: the effect builds and installs;
  the effect-only build passes six offline checks against stock `kwin-dev`
  **6.6.6**. No API workaround or ECM bundle was needed. Runtime
  rendering/loading and OBS builds remain unverified.
- This updates the Ubuntu/Node feasibility assessment in [OBS research](obs.md).
  Existing [offline packaging evidence](../../changes/archive/release-0.1-offline-packaging.md)
  and [core-with-Settings evidence](../../changes/archive/release-0.1-core-settings.md)
  are retained; those builds were not repeated.

## Options and consequences

| Option | What users get on Ubuntu 26.04 | Effort (relative estimate) | Risks / consequences |
| --- | --- | --- | --- |
| **A. Stock updates + ECM 6.24 floor (recommended)** | Core + Settings and optional active border, group underlay, drop preview; effect rebuilt for stock KWin | Low build-system change; medium companion packaging and release validation | Compile/offline checks proven on 6.6.6, not runtime; rebuild after KWin changes; older base-only 6.6.4 needs a separate matching build or update |
| B. Bundle pinned ECM 6.26 in source archive | Same features on the same stock KWin; no host ECM upgrade | Medium: archive inputs, local ECM prefix, recipe wiring, attribution and bundle test | No benefit over A demonstrated; maintain another pinned source dependency and module/template closure; does not solve KWin ABI matching |
| C. Keep Ubuntu core + Settings only for 0.1 | Tiling and Settings/Revert; no native border, underlay or drop preview | Lowest additional work; existing Debian recipe | Explicit Ubuntu visual-feature omission despite a proven compile route; runtime core-only acceptance still required |
| D. Require Kubuntu backports PPA | Currently the same KWin 6.6.6 stack as stock updates | Extra repository/provisioning work | PPA has no newer KWin or resolute ECM; does not fix the current floor; unnecessary third-party repository dependency |
| E. Add KDE neon as a separate target | Full features for neon users if separately built/validated; no stock Ubuntu 26.04 effect binary | High/uncertain: OBS provisioning, noble toolchain/recipe trial, rolling rebuilds | Version-sufficient KDE stack but Ubuntu 24.04 base, distinct ABI target, no verified public neon OBS DoD target |

- Material choices: A versus B/C; whether 0.1 Ubuntu effect support targets
  normal updated installations only or also a separate base-only 6.6.4 build;
  whether neon provisioning is worth pursuing independently.
- Recommended scope: A for updated Ubuntu/Kubuntu 26.04, no PPA requirement,
  retain C as a fallback if runtime/release validation fails, and keep neon
  outside the initial shipping promise. Effort estimates are not measured
  delivery schedules.

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
  support below 6.24 merely from the macro's 6.18 introduction. A later
  implementation should reconcile the RPM/Arch dependency floors and
  documentation with CMake, while keeping actual distro Qt/KF6/KWin inputs.
  This evidence is about Ubuntu's tested stack, not arbitrary older stacks.

## 2. Bundling ECM in the source archive

- Feasible fallback, not empirically built here. The needed ECM module files
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
  Base-only 6.6.4 was not built here.
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
  the standard OBS target is feasible without PPA/neon provisioning. The
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
  solver acceptance is claimed. ECM bundling, base-only 6.6.4 and a complete
  neon build remain untested.

## Decision and exact next action

- User selects the route and Ubuntu base/updates coverage; recommendation is
  **A with updated stock Ubuntu 26.04**, neon deferred.
- If accepted: lower/reconcile the ECM floor, add an effect-only Debian
  companion, build the exact release archive offline into both Debian
  packages, then trial an unpublished OBS x86_64 target with the correct
  universe/update ordering and inspect buildinfo/package dependencies.
- Before shipping: validate an actual Ubuntu KDE session's border, group
  underlay/drop preview and missing/failed/removed-effect behavior, plus
  KWin upgrade/solver transactions and rebuild timing. Preserve the existing
  non-pinning, independent-core policy. Same-version ABI patches require
  matching rebuilds; no source/version gate eliminates that risk.

# Tentative 0.1 offline-preparation recipe. Not yet built on OBS; see
# docs/installation.md for status, blockers, and the KWin-upgrade policy.
# Package names below were verified against the live openSUSE Tumbleweed,
# Fedora 43, and Fedora 44 repositories in October 2026 except where noted.
#
# Source contract: Source0 is the immutable GitHub Release tarball produced
# by release.yml (scripts/build-source-archive.sh). It already contains the
# prebuilt KWin bundle (kwin/contents/code/main.js) and the vendored Rust
# crates (vendor/), so this recipe never runs npm and never touches the
# network: the KWin script is installed as files, and Cargo builds fully
# offline. A raw git checkout (tar_scm) would lack both and must not be
# substituted here.

Name:           omnitiler
Version:        0.1.0
Release:        1%{?dist}
Summary:        COSMIC-style automatic tiling for KWin (core)
License:        MIT
URL:            https://github.com/beefsack/OmniTiler
Source0:        https://github.com/beefsack/OmniTiler/releases/download/v%{version}/omnitiler-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  cmake
BuildRequires:  gcc-c++
BuildRequires:  systemd-rpm-macros
# KWin development files on both families. The cmake() virtual keeps the
# recipe independent of the kwin6-devel (openSUSE) / kwin-devel (Fedora)
# package-name split; both providers were verified to supply cmake(KWin).
BuildRequires:  cmake(KWin)
# ECM floor is the native tree's tested CMake requirement
# (kwin/native-effect/CMakeLists.txt: find_package(ECM 6.24.0 REQUIRED);
# the KWin-independent settings KCMs need only the same 6.24.0).
# This recipe always builds the full native tree, so the 6.24.0 floor stays.
%if 0%{?suse_version}
BuildRequires:  kf6-extra-cmake-modules >= 6.24.0
BuildRequires:  qt6-base-devel
# KWin's CMake config pulls Qt6Quick (not required by kwin6-devel itself).
BuildRequires:  qt6-quick-devel
%else
BuildRequires:  extra-cmake-modules >= 6.24.0
BuildRequires:  qt6-qtbase-devel
BuildRequires:  qt6-qtdeclarative-devel
%endif
BuildRequires:  kf6-kconfig-devel
BuildRequires:  kf6-kcoreaddons-devel
BuildRequires:  kf6-kcmutils-devel
BuildRequires:  kf6-ki18n-devel
BuildRequires:  kf6-kwindowsystem-devel
BuildRequires:  kf6-kwidgetsaddons-devel
BuildRequires:  kf6-kcolorscheme-devel
# KWin's CMake config needs epoxy and libdrm (not guaranteed via the KWin
# devel packages' own requires on all targets).
BuildRequires:  libepoxy-devel
BuildRequires:  libdrm-devel

%global kwin_plugindir %{_libdir}/qt6/plugins
# Runtime: the KWin script needs its host, the tray/enable flows shell out
# to kwriteconfig6/kreadconfig6 (kf6-kconfig) and kcmshell6 (kf6-kcmutils),
# and the icon follows the hicolor theme layout. All four are hard
# requirements: installing without them leaves documented flows broken.
# The native effect stays a weak Recommends: core tiling works without it.
%if 0%{?suse_version}
%global kwin_runtime kwin6
%else
%global kwin_runtime kwin
%endif
Requires:       %{kwin_runtime}
Requires:       kf6-kconfig
Requires:       kf6-kcmutils
Requires:       hicolor-icon-theme
Recommends:     %{name}-native-effect

%description
Core of OmniTiler: the KWin script (prebuilt, no Node needed at
packaging time), the Rust planner/tray binary with on-demand D-Bus plus
systemd user activation, the application icon, and both native settings
pages (the effect settings page and the native script settings page, the
Configure page referenced by the KWin script). The settings pages are
KWin-independent (Qt/KF6 only); the effect reconfigure tolerates an absent
effect.

Tiling and Settings work with this package alone. Only the active-border
effect itself needs the companion omnitiler-native-effect package
(ABI-matched to your KWin).

Revert before removing: open Settings and press Revert (Revert Shortcuts,
plus Revert to KDE default on any changed host row) to restore host keys
and shortcut overrides. Removal does not restore them; Settings lives in
this package, so press Revert before removing it. If you already removed
without reverting, reinstall this package and press Revert. Disabling the
script is separate (set omnitiler-kwinEnabled=false and reconfigure
KWin).

%package native-effect
Summary:        OmniTiler native KWin effect (active border)
License:        GPL-2.0-or-later
Requires:       %{name} = %{version}-%{release}
# Deliberately unversioned: this package must never hold a KWin upgrade
# back (a pinned Requires would block host security updates), and an exact
# Requires would still not prove the loaded .so matches the running KWin.
# Rebuilds track KWin via the cmake(KWin) BuildRequires above; see
# docs/installation.md for the upgrade-window limitation and the
# versioned factory IID and same-version ABI-patch limitation.
%if 0%{?suse_version}
Requires:       kwin6
%else
Requires:       kwin
%endif

%description native-effect
Optional ABI-bound companion: the native active-border effect only. The
settings pages (with Revert) live in the core package, so removing this
companion alone leaves Settings and Revert available. Tied to the KWin
development headers it was built against; after a KWin upgrade, update this
package before relying on the effect. Removing it leaves core tiling and
Settings working, minus the border effect.
The exact-version core requirement keeps effect/core installs paired. This
split layout (both KCM plugins in core, effect plugin here) is new and
pre-release: no predecessor was ever published, so no file-ownership
migration across updates is claimed.

Revert before removing: no Revert step is lost by removing this package
alone (Settings stays in core). Removal does not restore host keys or
shortcut overrides; press Revert in the core Settings before removing core.
If you already removed core without reverting, reinstall core and press
Revert.
Disabling the script or unloading the effect is separate.

%prep
%setup -q -n %{name}-%{version}
# Fail closed: the recipe version must match the immutable release tarball.
[ "$(cat VERSION)" = "%{version}" ] || exit 1
grep -Eq '^[0-9a-f]{40}([0-9a-f]{24})?$' SOURCE_REV || exit 1
grep -Fq "$(cat SOURCE_REV)" kwin/contents/code/main.js || exit 1
test -d vendor || exit 1
test -f THIRD-PARTY-NOTICES || exit 1

%build
# Offline Rust core build, exactly per scripts/build-source-archive.sh.
# (The --config directory value carries inner double quotes: cargo's CLI
# expression parser requires quoted strings.)
export CARGO_NET_OFFLINE=true
VENDOR_DIR="$PWD/vendor"
cargo --config 'source.crates-io.replace-with="vendored-sources"' \
      --config "source.vendored-sources.directory=\"$VENDOR_DIR\"" \
      build --release --locked --offline -p omnitiler
# Native effect against this target's own KWin headers. The CMake project
# reuses the same vendor tree via OMNITILER_VENDOR_DIR with
# CARGO_NET_OFFLINE=true (see kwin/native-effect/CMakeLists.txt), so no
# network either. Plain cmake here, not the distro cmake macro (spelled
# %%cmake in this comment because rpm expands even that): the macro hardcodes
# srcdir "." and builddir "build", which cannot address this project's
# kwin/native-effect subdirectory; the explicit dirs below are equivalent
# and distro-independent. PLUGINDIR is pinned explicitly because the
# flake's lib/qt-6/plugins layout is Nix-specific; distro Qt resolves
# qt6/plugins.
export OMNITILER_VENDOR_DIR="$PWD/vendor"
cmake -S kwin/native-effect -B build-native \
  -DCMAKE_INSTALL_PREFIX=%{_prefix} \
  -DCMAKE_INSTALL_LIBDIR=%{_lib} \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_TESTING=OFF \
  -DKDE_INSTALL_PLUGINDIR=%{kwin_plugindir}
cmake --build build-native

%install
# Core binary.
install -Dm755 target/release/omnitiler %{buildroot}%{_bindir}/omnitiler
# KWin script: install the exact prebuilt four-file set, no npm involved.
scriptdir=%{buildroot}%{_datadir}/kwin/scripts/omnitiler-kwin
install -Dm644 kwin/metadata.json "$scriptdir/metadata.json"
install -Dm644 kwin/contents/code/main.js "$scriptdir/contents/code/main.js"
install -Dm644 kwin/contents/config/main.xml "$scriptdir/contents/config/main.xml"
install -Dm644 kwin/contents/ui/config.ui "$scriptdir/contents/ui/config.ui"
# D-Bus activation descriptor (template carries @out@, Nix-style).
install -d %{buildroot}%{_datadir}/dbus-1/services
sed 's|@out@|%{_prefix}|' nix/com.omnitiler.Planner.service \
  > %{buildroot}%{_datadir}/dbus-1/services/com.omnitiler.Planner.service
chmod 644 %{buildroot}%{_datadir}/dbus-1/services/com.omnitiler.Planner.service
# Systemd user units (mirrors home-manager-module.nix; never auto-enabled
# by this package, the user opts in with systemctl --user enable).
install -Dm644 packaging/systemd/omnitiler-tray.service \
  %{buildroot}%{_userunitdir}/omnitiler-tray.service
install -Dm644 packaging/systemd/omnitiler-planner.service \
  %{buildroot}%{_userunitdir}/omnitiler-planner.service
# Icon.
install -Dm644 assets/icons/omnitiler.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/omnitiler.svg
# Native effect plus settings pages (three plugins; JSON metadata is
# embedded in the .so files at compile time, so no .json ships alongside).
# %files splits them: the two KCMs belong to core, the effect plugin to the
# companion.
DESTDIR=%{buildroot} cmake --install build-native

# Do not use systemd_user_post here: it applies distro presets and may
# enable the opt-in tray globally. systemd file triggers handle reloads;
# neither unit needs enabling for on-demand planner activation.

%preun
%systemd_user_preun omnitiler-tray.service omnitiler-planner.service

%postun
%systemd_user_postun omnitiler-tray.service omnitiler-planner.service
# Final erase only ($1=0): upgrades ($1=1) keep host settings untouched.
if [ $1 -eq 0 ]; then
  echo "omnitiler removed (including Settings). Press Revert in Settings before removing:"
  echo "that step restores host keys and shortcut overrides, removal does not."
  echo "If you already removed without reverting, reinstall this package and press Revert."
fi

%postun native-effect
if [ $1 -eq 0 ]; then
  echo "omnitiler-native-effect removed. Settings and Revert remain"
  echo "available in the core package: removal does not restore host keys or"
  echo "shortcut overrides, so press Revert in the core Settings if needed."
fi

%files
%license LICENSE
%license THIRD-PARTY-NOTICES
%{_bindir}/omnitiler
%{_datadir}/kwin/scripts/omnitiler-kwin/
%{_datadir}/dbus-1/services/com.omnitiler.Planner.service
%{_userunitdir}/omnitiler-tray.service
%{_userunitdir}/omnitiler-planner.service
%{_datadir}/icons/hicolor/scalable/apps/omnitiler.svg
# KWin-independent settings pages (Qt/KF6 only; JSON metadata is embedded in
# the .so files at compile time, so no .json ships alongside).
%{kwin_plugindir}/kwin/effects/configs/omnitiler-active-border_config.so
%{kwin_plugindir}/kwin/scripts/configs/omnitiler-kwin_config.so

%files native-effect
%license LICENSES/GPL-2.0-or-later.txt
%license LICENSE
%license THIRD-PARTY-NOTICES
%{kwin_plugindir}/kwin/effects/plugins/omnitiler-active-border.so

%changelog
* Sat Oct 10 2026 OmniTiler Contributors - 0.1.0-1
- Tentative 0.1 offline-preparation recipe: core plus optional native
  effect, offline Cargo/CMake against the release tarball, no KWin pin.

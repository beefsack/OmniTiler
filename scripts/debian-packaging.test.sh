#!/usr/bin/env bash
# Static guard for the Debian two-package recipe (offline, no containers).
# Checks the packaging contract that the Ubuntu container builds then prove
# on real stacks: core lower-bounds KWin with no exact pin, the effect
# companion exactly pins the libkwin6 distro build compiled against, the
# core-only profile drops KWin build dependencies, and the ECM/KWin floors
# stay reconciled with kwin/native-effect/CMakeLists.txt.
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CONTROL="$REPO_ROOT/packaging/debian/control"
RULES="$REPO_ROOT/packaging/debian/rules"
CHANGELOG="$REPO_ROOT/packaging/debian/changelog"
SPEC="$REPO_ROOT/packaging/rpm/omnitiler.spec"
CMAKE="$REPO_ROOT/kwin/native-effect/CMakeLists.txt"
PASS=0
FAIL=0

pass() {
  PASS=$((PASS + 1))
}

fail() {
  echo "FAIL [$1]: $2" >&2
  FAIL=$((FAIL + 1))
}

check_grep() {
  local name="$1" file="$2" pattern="$3"
  if grep -Eq -- "$pattern" "$file"; then
    pass
  else
    fail "$name" "pattern not found in $file: $pattern"
  fi
}

check_fixed() {
  local name="$1" file="$2" literal="$3"
  if grep -Fq -- "$literal" "$file"; then
    pass
  else
    fail "$name" "literal not found in $file: $literal"
  fi
}

check_absent() {
  local name="$1" file="$2" pattern="$3"
  if grep -Eq -- "$pattern" "$file"; then
    fail "$name" "forbidden pattern present in $file: $pattern"
  else
    pass
  fi
}

# ---- Part 1: two binary packages exist ----
check_grep "core-stanza" "$CONTROL" '^Package: omnitiler$'
check_grep "effect-stanza" "$CONTROL" '^Package: omnitiler-native-effect$'

# ---- Part 2: core lower-bounds KWin >= 6.6 with no exact pin ----
check_fixed "core-kwin-floor" "$CONTROL" 'kwin-wayland (>= 4:6.6) | kwin-x11 (>= 4:6.6)'
CORE_DEPS="$(grep -A3 '^Package: omnitiler$' "$CONTROL" | grep '^Depends:')"
case "$CORE_DEPS" in
  *'libkwin6 (= '*)
    fail "core-no-exact-pin" "core must not exactly pin libkwin6: $CORE_DEPS"
    ;;
  *)
    pass
    ;;
esac

# ---- Part 3: effect exactly pins core and the KWin distro build ----
check_fixed "effect-core-pin" "$CONTROL" 'omnitiler (= ${source:Version})'
check_fixed "effect-kwin-pin" "$CONTROL" 'libkwin6 (= ${kwin:Version})'
check_fixed "effect-build-profile" "$CONTROL" 'Build-Profiles: <!noeffect>'

# ---- Part 4: effect Build-Depends stay out of the core-only profile ----
for dep in 'kwin-dev (>= 4:6.6) <!noeffect>' 'libkf6colorscheme-dev <!noeffect>' \
           'libdrm-dev <!noeffect>' 'pkg-config <!noeffect>'; do
  check_fixed "profile-dep" "$CONTROL" "$dep"
done

# ---- Part 5: rules builds both trees and records the exact KWin build ----
check_grep "rules-settings-build" "$RULES" 'OMNITILER_BUILD_EFFECT=OFF'
check_grep "rules-effect-build" "$RULES" 'OMNITILER_BUILD_EFFECT=ON'
check_grep "rules-effect-nosettings" "$RULES" 'OMNITILER_BUILD_SETTINGS=OFF'
check_grep "rules-noeffect-build" "$RULES" 'filter noeffect'
check_grep "rules-kwin-substvar" "$RULES" "dpkg-query -W -f="
check_grep "rules-kwin-lib" "$RULES" 'libkwin6'
check_grep "rules-effect-install" "$RULES" 'debian/omnitiler-native-effect cmake --install build-effect'
check_grep "rules-effect-vendor" "$RULES" 'OMNITILER_VENDOR_DIR='

# ---- Part 6: per-package maintainer scripts, no bare postrm ----
if [[ -f "$REPO_ROOT/packaging/debian/omnitiler.postrm" ]]; then
  pass
else
  fail "core-postrm" "packaging/debian/omnitiler.postrm missing"
fi
if [[ -f "$REPO_ROOT/packaging/debian/omnitiler-native-effect.postrm" ]]; then
  pass
else
  fail "effect-postrm" "packaging/debian/omnitiler-native-effect.postrm missing"
fi
if [[ -e "$REPO_ROOT/packaging/debian/postrm" ]]; then
  fail "no-bare-postrm" "bare packaging/debian/postrm must not remain beside per-package scripts"
else
  pass
fi

# ---- Part 7: floors reconciled (no 6.26 drift), KWin 6.6 configure floor ----
check_grep "cmake-ecm-floor" "$CMAKE" 'find_package\(ECM 6\.24\.0 REQUIRED'
check_grep "cmake-kwin-floor" "$CMAKE" 'find_package\(KWin 6\.6\.0 REQUIRED\)'
check_absent "no-626-debian" "$CONTROL" '6\.26'
check_absent "no-626-rules" "$RULES" '6\.26'
check_absent "no-626-spec" "$SPEC" '6\.26'
check_fixed "spec-ecm-floor" "$SPEC" 'extra-cmake-modules >= 6.24.0'
check_fixed "control-ecm-floor" "$CONTROL" 'extra-cmake-modules (>= 6.24)'

# ---- Part 8: recipe/changelog version identity (bump-version contract) ----
SPEC_VERSION="$(grep -E '^Version:' "$SPEC" | head -n 1 | awk '{print $2}')"
CHANGELOG_VERSION="$(head -n 1 "$CHANGELOG" | sed -E 's/^[^(]*\(([^)]+)\).*/\1/' | cut -d- -f1)"
if [[ -n "${SPEC_VERSION:-}" && "$SPEC_VERSION" == "${CHANGELOG_VERSION:-}" ]]; then
  pass
else
  fail "version-identity" "spec $SPEC_VERSION vs changelog $CHANGELOG_VERSION"
fi

echo "debian-packaging: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]]

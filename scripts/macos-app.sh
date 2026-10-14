#!/usr/bin/env bash
# Stable signed .app recipe for the Phase 0 macOS probe.
# Fixed bundle ID (crates/tiler-macos/Info.plist) and FIXED output path
# (target/macos/OmniTilerProbe.app). Fails closed when the persistent named
# dev identity is unavailable: no ad-hoc fallback (`codesign -s -` is never
# used), no custom output path (an arbitrary rm -rf target is refused).
#
#   scripts/macos-app.sh --identity 'OmniTiler Mac Dev'
#   scripts/macos-app.sh --verify
#
# Steps: cargo build both binaries (mise toolchain, MACOSX_DEPLOYMENT_TARGET
# fixed at the macOS floor) -> assemble .app -> sign nested executables
# first, bundle last -> codesign verify/DR -> spctl diagnostic (not a grant).
# Cert creation is a later step; a missing identity is a hard error, not a
# fallback.
#
# Deployment-target note: the floor is enforced ONLY by this script's
# `MACOSX_DEPLOYMENT_TARGET` export at build time. A plain
# `cargo build -p tiler-macos` outside this script does NOT enforce the
# floor (rustc links the host SDK default); `verify` re-checks the built
# binaries' minos below, so an out-of-band build fails closed here instead
# of shipping a wrong-floor bundle.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$REPO_ROOT/crates/tiler-macos"
OUT="$REPO_ROOT/target/macos/OmniTilerProbe.app"
TARGET_DIR="$REPO_ROOT/target"
TARGET_MACOS_DIR="$REPO_ROOT/target/macos"
MARKER_REL="Contents/Resources/.omnitiler-probe-marker"
BUNDLE_ID="io.github.omnitiler.macos.probe"
DEPLOYMENT_TARGET="15.0"

IDENTITY=""
VERIFY_ONLY=0

usage() {
  echo "usage: $(basename -- "$0") [--identity 'NAME'] [--verify]" >&2
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --identity) IDENTITY="${2:-}"; shift 2 ;;
    --verify) VERIFY_ONLY=1; shift ;;
    -h|--help) usage ;;
    *) echo "error: unknown arg '$1'" >&2; usage ;;
  esac
done

fail() { echo "error: $*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || fail "macOS only (uname=$(uname -s))"
[[ "$(uname -m)" == "arm64" ]] || fail "Apple Silicon only (uname -m=$(uname -m))"
command -v mise >/dev/null 2>&1 || fail "mise not found; install per docs/macos-dev-environment.md"
command -v codesign >/dev/null 2>&1 || fail "codesign not found (CLT required)"
command -v security >/dev/null 2>&1 || fail "security not found (CLT required)"

# Fixed-output guard: only this exact path may be (re)created. Anything else
# is refused rather than risk deleting foreign data. The path itself must
# never be a symlink (a symlinked OUT would make `rm -rf` delete foreign
# content), and neither may its parents (`target`, `target/macos`): a
# symlinked parent would divert the same deletion. After assembly the bundle
# must contain no symlinks at all. An existing output must carry our dedicated
# owned-bundle marker; absent/mismatched marker means foreign data and refuses
# before any `rm -rf`.
case "$OUT" in
  "$REPO_ROOT"/target/macos/OmniTilerProbe.app) ;;
  *) fail "refusing non-fixed output path: $OUT" ;;
esac
[[ ! -L "$OUT" ]] || fail "refusing symlinked output path: $OUT"
[[ ! -L "$TARGET_DIR" ]] || fail "refusing symlinked parent: $TARGET_DIR"
[[ ! -L "$TARGET_MACOS_DIR" ]] || fail "refusing symlinked parent: $TARGET_MACOS_DIR"

require_owned_marker() {
  local marker="$OUT/$MARKER_REL"
  [[ -d "$OUT" ]] || fail "existing output is not a directory: $OUT (refusing foreign-file risk)"
  [[ -f "$marker" && ! -L "$marker" ]] \
    || fail "existing output lacks owned-bundle marker ($marker); refusing to delete foreign data"
  grep -Fqx "$BUNDLE_ID" "$marker" \
    || fail "existing output marker mismatch ($marker); refusing to delete foreign bundle"
}

verify_app() {
  local app="$1"
  [[ -d "$app" ]] || fail "app missing: $app"
  [[ ! -L "$app" ]] || fail "app path is a symlink: $app"
  # Owned-bundle marker: proves this directory was assembled by this script.
  local marker="$app/$MARKER_REL"
  [[ -f "$marker" && ! -L "$marker" ]] || fail "bundle lacks owned marker: $marker"
  grep -Fqx "$BUNDLE_ID" "$marker" || fail "bundle marker mismatch: $marker"
  # No symlinks anywhere inside the bundle: a planted link could divert
  # signing, verification, or a later launch at a foreign file.
  local links
  links="$(find "$app" -type l -print 2>/dev/null || true)"
  [[ -z "$links" ]] || fail "bundle contains symlinks (refusing foreign-file risk): $links"
  codesign --verify --deep --strict --verbose=2 "$app" \
    || fail "codesign verify failed for $app"
  echo "--- designated requirement ---"
  codesign -d -r - "$app" || fail "codesign designated-requirement read failed"
  echo "--- signature display ---"
  codesign --display --verbose=2 "$app" || fail "codesign display failed"
  echo "--- deployment-target floor ($DEPLOYMENT_TARGET) ---"
  check_minos_floor "$app/Contents/MacOS/tiler-macos"
  check_minos_floor "$app/Contents/MacOS/tiler-macos-probe-target"
  if command -v spctl >/dev/null 2>&1; then
    echo "--- spctl assessment (diagnostic, not a grant) ---"
    spctl -a -vvv -t exec "$app" || echo "note: spctl assessment non-zero (dev-signed; expected before notarization)"
  fi
  /usr/bin/shasum -a 256 "$app/Contents/MacOS/tiler-macos" || true
  echo "verify: OK $app"
}

# Enforce the macOS floor on a built Mach-O: the LC_BUILD_VERSION minos
# must equal the script floor. Plain `cargo build` (no
# MACOSX_DEPLOYMENT_TARGET export) links the host default and fails here.
check_minos_floor() {
  local bin="$1"
  [[ -f "$bin" && ! -L "$bin" ]] || fail "binary missing or symlinked: $bin"
  command -v otool >/dev/null 2>&1 || fail "otool not found (CLT required for floor check)"
  local minos
  minos="$(otool -l "$bin" | awk '/LC_BUILD_VERSION/{found=1} found && /minos/{print $2; exit}')"
  [[ -n "$minos" ]] || fail "could not read LC_BUILD_VERSION minos from $bin"
  [[ "$minos" == "$DEPLOYMENT_TARGET" ]] \
    || fail "deployment-target drift: $bin minos=$minos, want $DEPLOYMENT_TARGET (rebuild via this script, not plain cargo build)"
  echo "minos OK: $bin ($minos)"
}

if [[ "$VERIFY_ONLY" -eq 1 ]]; then
  verify_app "$OUT"
  exit 0
fi

[[ -n "$IDENTITY" ]] || fail "no --identity given (persistent named dev identity required; ad-hoc signing is refused)"
# Exact identity match by full quoted name, signed by hash: collect every
# `security find-identity` line whose full name equals the request exactly
# (never a substring), require exactly one, then sign with its SHA-1 hash.
# Trust is not required for a persistent self-signed dev identity. Without
# -v, security lists all matches, then repeats trusted ones in a second section.
# Zero matches fail closed; duplicates fail closed (ambiguous identity).
identities="$(security find-identity -p codesigning)" \
  || fail "security find-identity failed"
match_count=0
SIGN_HASH=""
while IFS= read -r line; do
  # Count only the first section, not the repeated valid-identity subset.
  [[ "$line" =~ ^[[:space:]]*Valid\ identities\ only[[:space:]]*$ ]] && break
  name="$(printf '%s\n' "$line" | sed -n 's/^.*"\(.*\)".*$/\1/p')"
  if [[ -n "$name" && "$name" == "$IDENTITY" ]]; then
    hash="$(printf '%s\n' "$line" | grep -oE '[0-9A-Fa-f]{40}' | head -n 1 || true)"
    [[ -n "$hash" ]] || fail "identity '$IDENTITY' matched but has no hash; refusing"
    match_count=$((match_count + 1))
    SIGN_HASH="$hash"
  fi
done <<<"$identities"
if [[ "$match_count" -eq 0 ]]; then
  echo "error: signing identity '$IDENTITY' not found in keychain (exact match required)" >&2
  printf '%s\n' "$identities" || true
  echo "hint: cert creation is a later step; refusing ad-hoc fallback" >&2
  exit 1
fi
if [[ "$match_count" -ne 1 ]]; then
  echo "error: signing identity '$IDENTITY' is ambiguous ($match_count exact matches); refusing" >&2
  printf '%s\n' "$identities" || true
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"
mise exec -- cargo build --locked -p tiler-macos --bins \
  || fail "cargo build -p tiler-macos --bins failed (MACOSX_DEPLOYMENT_TARGET=$DEPLOYMENT_TARGET)"
BIN_DIR="$REPO_ROOT/target/debug"
[[ -x "$BIN_DIR/tiler-macos" ]] || fail "missing binary $BIN_DIR/tiler-macos"
[[ -x "$BIN_DIR/tiler-macos-probe-target" ]] || fail "missing binary $BIN_DIR/tiler-macos-probe-target"

# Re-check symlink guards immediately before the destructive step, then
# require the owned marker for any existing output before `rm -rf`.
[[ ! -L "$TARGET_DIR" ]] || fail "refusing symlinked parent: $TARGET_DIR"
[[ ! -L "$TARGET_MACOS_DIR" ]] || fail "refusing symlinked parent: $TARGET_MACOS_DIR"
[[ ! -L "$OUT" ]] || fail "refusing symlinked output path: $OUT"
if [[ -e "$OUT" ]]; then
  require_owned_marker
fi
rm -rf -- "$OUT"
mkdir -p -- "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
[[ ! -L "$OUT" ]] || fail "output path became a symlink during assembly: $OUT"
# Source binaries must be plain files: never sign (or ship) a symlinked
# foreign executable from inside the expected target dir.
for src in "$BIN_DIR/tiler-macos" "$BIN_DIR/tiler-macos-probe-target"; do
  [[ -f "$src" && ! -L "$src" ]] || fail "binary source missing or symlinked: $src"
done
cp -f -- "$CRATE_DIR/Info.plist" "$OUT/Contents/Info.plist"
cp -f -- "$BIN_DIR/tiler-macos" "$OUT/Contents/MacOS/tiler-macos"
cp -f -- "$BIN_DIR/tiler-macos-probe-target" "$OUT/Contents/MacOS/tiler-macos-probe-target"
# BSD chmod has no -- separator; these fixed absolute operands cannot be options.
chmod +x "$OUT/Contents/MacOS/tiler-macos" "$OUT/Contents/MacOS/tiler-macos-probe-target"

# Bundle ID guard: Info.plist must carry the fixed ID.
grep -Fq "<string>$BUNDLE_ID</string>" "$OUT/Contents/Info.plist" \
  || fail "bundle ID drift: $BUNDLE_ID missing from Info.plist"

# Dedicated owned-bundle marker: written at assembly, required before any
# future `rm -rf` of this path and checked by `verify`.
printf '%s\n' "$BUNDLE_ID" > "$OUT/$MARKER_REL"

# Sign nested executables first, bundle last, by resolved hash (unique exact
# full-name match above; never a substring, never ambiguous).
codesign --force --options runtime --timestamp=none --sign "$SIGN_HASH" \
  "$OUT/Contents/MacOS/tiler-macos" \
  || fail "codesign failed for tiler-macos"
codesign --force --options runtime --timestamp=none --sign "$SIGN_HASH" \
  "$OUT/Contents/MacOS/tiler-macos-probe-target" \
  || fail "codesign failed for probe target"
codesign --force --options runtime --timestamp=none --sign "$SIGN_HASH" "$OUT" \
  || fail "codesign failed for bundle"

verify_app "$OUT"
echo "app: $OUT"

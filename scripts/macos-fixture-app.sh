#!/usr/bin/env bash
# Stable signed .app recipe for the development-only macOS live fixture.
# Fixed bundle ID (crates/tiler-live-fixture/Info.plist) and fixed output
# path (target/macos/OmniTilerLiveFixture.app). Fails closed when the
# persistent named dev identity is unavailable: no ad-hoc fallback
# (`codesign -s -` is never used), no custom output path (an arbitrary
# rm -rf target is refused).
#
#   scripts/macos-fixture-app.sh --identity 'OmniTiler Mac Dev'
#   scripts/macos-fixture-app.sh --verify
#
# Steps: cargo build the fixture binary (mise toolchain,
# MACOSX_DEPLOYMENT_TARGET fixed at the macOS floor, artifact path resolved
# from cargo JSON so CARGO_TARGET_DIR is honored and target/debug is never
# assumed) -> assemble the .app -> sign nested executable first, bundle
# last -> codesign verify/DR -> spctl diagnostic (not a grant). Cert
# creation is a later step; a missing identity is a hard error, not a
# fallback. No TCC usage strings are added by this script (no TCC/security
# changes).
#
# Deployment-target note: the floor is enforced ONLY by this script's
# `MACOSX_DEPLOYMENT_TARGET` export at build time. A plain
# `cargo build -p tiler-live-fixture` outside this script does NOT enforce
# the floor; `verify` re-checks the built binary minos below, so an
# out-of-band build fails closed here instead of shipping a wrong-floor
# bundle.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$REPO_ROOT/crates/tiler-live-fixture"
OUT="$REPO_ROOT/target/macos/OmniTilerLiveFixture.app"
TARGET_DIR="$REPO_ROOT/target"
TARGET_MACOS_DIR="$REPO_ROOT/target/macos"
MARKER_REL="Contents/Resources/.omnitiler-fixture-marker"
BUNDLE_ID="io.github.omnitiler.live-fixture"
BIN_NAME="tiler-live-fixture"
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

# Fixed-output guard: only the exact path below may be (re)created.
case "$OUT" in
  "$REPO_ROOT"/target/macos/OmniTilerLiveFixture.app) ;;
  *) fail "refusing non-fixed output path: $OUT" ;;
esac
[[ ! -L "$OUT" ]] || fail "refusing symlinked output path: $OUT"
[[ ! -L "$TARGET_DIR" ]] || fail "refusing symlinked parent: $TARGET_DIR"
[[ ! -L "$TARGET_MACOS_DIR" ]] || fail "refusing symlinked parent: $TARGET_MACOS_DIR"

require_owned_marker() {
  local app="$1" bundle="$2"
  local marker="$app/$MARKER_REL"
  [[ -d "$app" ]] || fail "existing output is not a directory: $app (refusing foreign-file risk)"
  [[ -f "$marker" && ! -L "$marker" ]] \
    || fail "existing output lacks owned-bundle marker ($marker); refusing to delete foreign data"
  grep -Fqx "$bundle" "$marker" \
    || fail "existing output marker mismatch ($marker); refusing to delete foreign bundle"
}

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

verify_app() {
  local app="$1" bundle="$2" bin="$3"
  [[ -d "$app" ]] || fail "app missing: $app"
  [[ ! -L "$app" ]] || fail "app path is a symlink: $app"
  local marker="$app/$MARKER_REL"
  [[ -f "$marker" && ! -L "$marker" ]] || fail "bundle lacks owned marker: $marker"
  grep -Fqx "$bundle" "$marker" || fail "bundle marker mismatch: $marker"
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
  check_minos_floor "$app/Contents/MacOS/$bin"
  if command -v spctl >/dev/null 2>&1; then
    echo "--- spctl assessment (diagnostic, not a grant) ---"
    spctl -a -vvv -t exec "$app" || echo "note: spctl assessment non-zero (dev-signed; expected before notarization)"
  fi
  /usr/bin/shasum -a 256 "$app/Contents/MacOS/$bin" || true
  echo "verify: OK $app"
}

# Resolve the built fixture binary from cargo JSON so CARGO_TARGET_DIR is
# honored; never assume target/debug. Caller must have built with
# --message-format=json; the first compiler-artifact executable named
# BIN_NAME wins. jq is mise-managed (same route as cargo below).
resolve_bin_from_json() {
  mise exec -- jq -r --arg want "$BIN_NAME" \
    'select(.reason == "compiler-artifact" and .target.name == $want and (.executable // "") != "") | .executable' \
    | head -n 1
}

if [[ "$VERIFY_ONLY" -eq 1 ]]; then
  verify_app "$OUT" "$BUNDLE_ID" "$BIN_NAME"
  exit 0
fi

[[ -n "$IDENTITY" ]] || fail "no --identity given (persistent named dev identity required; ad-hoc signing is refused)"
identities="$(security find-identity -p codesigning)" \
  || fail "security find-identity failed"
match_count=0
SIGN_HASH=""
while IFS= read -r line; do
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
BUILD_JSON="$(mise exec -- cargo build --locked --offline -p tiler-live-fixture --message-format=json)" \
  || fail "cargo build -p tiler-live-fixture failed (MACOSX_DEPLOYMENT_TARGET=$DEPLOYMENT_TARGET)"
BIN_SRC="$(printf '%s\n' "$BUILD_JSON" | resolve_bin_from_json)"
[[ -n "$BIN_SRC" ]] || fail "cargo JSON carried no $BIN_NAME artifact (build failed or format changed)"
[[ -f "$BIN_SRC" && ! -L "$BIN_SRC" ]] || fail "binary source missing or symlinked: $BIN_SRC"
[[ -x "$BIN_SRC" ]] || fail "binary source not executable: $BIN_SRC"

[[ ! -L "$TARGET_DIR" ]] || fail "refusing symlinked parent: $TARGET_DIR"
[[ ! -L "$TARGET_MACOS_DIR" ]] || fail "refusing symlinked parent: $TARGET_MACOS_DIR"
[[ ! -L "$OUT" ]] || fail "refusing symlinked output path: $OUT"
if [[ -e "$OUT" ]]; then
  require_owned_marker "$OUT" "$BUNDLE_ID"
fi
rm -rf -- "$OUT"
mkdir -p -- "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
[[ ! -L "$OUT" ]] || fail "output path became a symlink during assembly: $OUT"
cp -f -- "$CRATE_DIR/Info.plist" "$OUT/Contents/Info.plist"
cp -f -- "$BIN_SRC" "$OUT/Contents/MacOS/$BIN_NAME"
# BSD chmod has no -- separator; these fixed absolute operands cannot be options.
chmod +x "$OUT/Contents/MacOS/$BIN_NAME"

grep -Fq "<string>$BUNDLE_ID</string>" "$OUT/Contents/Info.plist" \
  || fail "bundle ID drift: $BUNDLE_ID missing from Info.plist"

printf '%s\n' "$BUNDLE_ID" > "$OUT/$MARKER_REL"

codesign --force --options runtime --timestamp=none --sign "$SIGN_HASH" \
  "$OUT/Contents/MacOS/$BIN_NAME" \
  || fail "codesign failed for $BIN_NAME"
codesign --force --options runtime --timestamp=none --sign "$SIGN_HASH" "$OUT" \
  || fail "codesign failed for bundle"

verify_app "$OUT" "$BUNDLE_ID" "$BIN_NAME"
echo "app: $OUT"

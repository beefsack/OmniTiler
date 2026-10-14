#!/usr/bin/env bash
# Builds the offline KDE 0.1 source archive from a tracked release tag.
#
# Archive contract (for distro/OBS packaging):
#   - File:   dist/omnitiler-<VERSION>.tar.gz
#             plus sidecar dist/omnitiler-<VERSION>.tar.gz.sha256
#             ("<digest>  <basename>", same shape as build-kpackage.sh).
#   - The archive extracts to exactly one top-level directory:
#             omnitiler-<VERSION>/
#   - Contents: the exact tracked source at tag v<VERSION> (via git archive)
#     plus only these generated additions:
#       kwin/contents/code/main.js   prebuilt KWin bundle with the tag commit
#                                    SHA baked in as OMNITILER_SOURCE_REV
#       vendor/                      cargo vendor output for fully offline Rust builds
#       VERSION                      <VERSION> (e.g. 0.1.0)
#       SOURCE_REV                   full commit SHA of the tag
#   - The tag must already carry the committed generated union notice at
#     root THIRD-PARTY-NOTICES (both shipping Rust roots, Linux
#     x86_64/aarch64); distro and Nix recipes install that same file, so
#     the build fails closed when the tag lacks it.
#   - The tarball is deterministic for a given tag (sorted entries, commit-time
#     mtimes, numeric 0:0 ownership, gzip -n), so rebuilds hash identically.
#   - Offline consumption (no network at distro build time):
#       core Rust:  cargo --locked --offline with source replacement, e.g.
#         cargo --config 'source.crates-io.replace-with="vendored-sources"' \
#               --config 'source.vendored-sources.directory="<abs path>/vendor"' \
#               build --locked --offline -p omnitiler
#       native CMake: kwin/native-effect/CMakeLists.txt applies the same
#         replacement automatically when OMNITILER_VENDOR_DIR points at
#         the archive vendor/ directory; also export CARGO_NET_OFFLINE=true.
#       KWin script: install kwin/contents/code/main.js directly, no npm needed.
#         scripts/build-kpackage.sh skips its npm build step when
#         OMNITILER_PREBUILT_BUNDLE=1.
#   - Build identity: the tag must be vX.Y.Z and match kwin/package.json plus
#     every workspace crate version. The baked SOURCE_REV is the tag commit
#     SHA, never the tag name: kwin/src/source-rev.ts only accepts 40/64
#     lowercase hex and falls back to local-dev otherwise.
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  printf 'usage: build-source-archive.sh --tag <vX.Y.Z> [--output-dir <dir>]\n' >&2
}

die() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

TAG=""
OUTPUT_DIR="$REPO_ROOT/dist"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      [[ $# -ge 2 ]] || { usage; exit 1; }
      TAG="$2"
      shift 2
      ;;
    --output-dir)
      [[ $# -ge 2 ]] || { usage; exit 1; }
      OUTPUT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 1
      ;;
  esac
done
[[ -n "$TAG" ]] || { usage; exit 1; }

[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] \
  || die "tag must look like vX.Y.Z (got '$TAG'); stable releases only"
VERSION="${TAG#v}"
TAG_REF="refs/tags/$TAG"

for tool in git tar gzip sha256sum; do
  command -v -- "$tool" >/dev/null 2>&1 || die "required tool '$tool' not found in PATH"
done

TAG_SHA="$(git -C "$REPO_ROOT" rev-parse --verify --quiet "$TAG_REF^{commit}" 2>/dev/null)" \
  || die "tag '$TAG' does not exist in this repository"
[[ "$TAG_SHA" =~ ^[0-9a-f]{40}([0-9a-f]{24})?$ ]] || die "tag commit SHA is malformed"

# Version identity is checked against the tagged source, never the worktree.
tagged_version() {
  git -C "$REPO_ROOT" show "$TAG_REF:$1" 2>/dev/null || die "tagged source has no $1"
}
NPM_VERSION="$(tagged_version kwin/package.json | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>{try{const v=JSON.parse(d).version;if(typeof v!=='string')process.exit(1);process.stdout.write(v)}catch{process.exit(1)}})")" \
  || die "tagged kwin/package.json has no usable version"
[[ "$NPM_VERSION" == "$VERSION" ]] \
  || die "tagged kwin/package.json version $NPM_VERSION does not match tag $TAG"
for member in tiler-core tiler-protocol omnitiler tiler-kwin-effect-ffi tiler-windows tiler-macos; do
  CRATE_VERSION="$(tagged_version "crates/$member/Cargo.toml" | grep -E '^version = "' | head -n 1 | sed -E 's/^version = "(.*)"$/\1/')"
  [[ "$CRATE_VERSION" == "$VERSION" ]] \
    || die "tagged crates/$member/Cargo.toml version '${CRATE_VERSION:-missing}' does not match tag $TAG"
done

mkdir -p -- "$OUTPUT_DIR"
OUTPUT_DIR="$(cd -- "$OUTPUT_DIR" && pwd -P)"

TOP_DIR="omnitiler-$VERSION"
ARCHIVE_NAME="$TOP_DIR.tar.gz"
ARCHIVE_OUTPUT="$OUTPUT_DIR/$ARCHIVE_NAME"
SIDECAR_OUTPUT="$OUTPUT_DIR/$ARCHIVE_NAME.sha256"

umask 022
TMP_ROOT="$(mktemp -d "$OUTPUT_DIR/.build-source-archive.XXXXXX")" || die "could not create temporary root"
PUBLICATION_ACTIVE=0
ARCHIVE_BACKUP="$TMP_ROOT/archive.previous"
SIDECAR_BACKUP="$TMP_ROOT/sidecar.previous"
had_archive=0
had_sidecar=0
cleanup() {
  local status=$?
  trap - EXIT HUP INT TERM
  if [[ "$PUBLICATION_ACTIVE" -eq 1 ]]; then
    if [[ "$had_archive" -eq 1 ]]; then
      cp -p -- "$ARCHIVE_BACKUP" "$ARCHIVE_OUTPUT" || status=1
    else
      rm -f -- "$ARCHIVE_OUTPUT" || status=1
    fi
    if [[ "$had_sidecar" -eq 1 ]]; then
      cp -p -- "$SIDECAR_BACKUP" "$SIDECAR_OUTPUT" || status=1
    else
      rm -f -- "$SIDECAR_OUTPUT" || status=1
    fi
  fi
  [[ -z "${TMP_ROOT:-}" ]] || rm -rf -- "$TMP_ROOT"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

for output in "$ARCHIVE_OUTPUT" "$SIDECAR_OUTPUT"; do
  [[ ! -e "$output" && ! -L "$output" ]] || [[ -f "$output" && ! -L "$output" ]] \
    || die "existing release output is not a regular non-symlink file: $output"
done
if [[ -e "$ARCHIVE_OUTPUT" || -L "$ARCHIVE_OUTPUT" ]]; then
  cp -p -- "$ARCHIVE_OUTPUT" "$ARCHIVE_BACKUP" || die "could not back up existing archive"
  had_archive=1
fi
if [[ -e "$SIDECAR_OUTPUT" || -L "$SIDECAR_OUTPUT" ]]; then
  cp -p -- "$SIDECAR_OUTPUT" "$SIDECAR_BACKUP" || die "could not back up existing sidecar"
  had_sidecar=1
fi

STAGE="$TMP_ROOT/build"
TREE="$STAGE/$TOP_DIR"
ARCHIVE_TMP="$TMP_ROOT/$ARCHIVE_NAME"
SIDECAR_TMP="$TMP_ROOT/$ARCHIVE_NAME.sha256"
mkdir -p -- "$STAGE"

# Tracked source only: git archive of the tag object, independent of worktree state.
git -C "$REPO_ROOT" archive --format=tar --prefix="$TOP_DIR/" "$TAG_REF" | tar -x -C "$STAGE" \
  || die "git archive of tag $TAG failed"
[[ -f "$TREE/kwin/package.json" && -f "$TREE/Cargo.lock" ]] \
  || die "tagged source is missing kwin/package.json or Cargo.lock"
[[ -f "$TREE/THIRD-PARTY-NOTICES" && ! -L "$TREE/THIRD-PARTY-NOTICES" ]] \
  || die "tagged source is missing root THIRD-PARTY-NOTICES (commit the generated union file before tagging)"

command -v -- npm >/dev/null 2>&1 || die "required tool 'npm' not found in PATH"
command -v -- cargo >/dev/null 2>&1 || die "required tool 'cargo' not found in PATH"

# Prebuilt KWin bundle with the tag commit SHA baked in as build identity.
npm ci --prefix "$TREE/kwin" --no-audit --no-fund || die "npm ci failed for tagged kwin source"
OMNITILER_SOURCE_REV="$TAG_SHA" npm --prefix "$TREE/kwin" run build:installed \
  || die "prebuilt KWin bundle build failed"
BUNDLE="$TREE/kwin/contents/code/main.js"
[[ -f "$BUNDLE" && ! -L "$BUNDLE" ]] || die "prebuilt KWin bundle missing after build"
grep -Fq -- "$TAG_SHA" "$BUNDLE" || die "prebuilt bundle does not contain baked source rev $TAG_SHA"
chmod 0644 -- "$BUNDLE"

# Vendored Rust crates for fully offline distro builds.
(cd -- "$TREE" && cargo vendor --locked vendor >/dev/null) || die "cargo vendor failed"
[[ -d "$TREE/vendor" ]] || die "cargo vendor produced no vendor directory"

# Prove the staged tree builds offline before packing: fresh CARGO_HOME with a
# crates-io replacement pointing at the staged vendor dir, no network.
VENDOR_ABS="$TREE/vendor"
CARGO_HOME_TMP="$TMP_ROOT/cargo-home"
TARGET_TMP="$TMP_ROOT/cargo-target"
mkdir -p -- "$CARGO_HOME_TMP"
cat > "$CARGO_HOME_TMP/config.toml" <<EOF
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "$VENDOR_ABS"
EOF
(cd -- "$TREE" && CARGO_HOME="$CARGO_HOME_TMP" CARGO_NET_OFFLINE=true \
  cargo build --locked --offline -p omnitiler -p tiler-kwin-effect-ffi \
  --target-dir "$TARGET_TMP") || die "offline Rust self-check build failed"

printf '%s\n' "$VERSION" > "$TREE/VERSION"
printf '%s\n' "$TAG_SHA" > "$TREE/SOURCE_REV"

# Drop build-time-only directories; they must never ship in the archive.
rm -rf -- "$TREE/kwin/node_modules" "$TREE/kwin/dist" "$TREE/dist" "$TREE/target"

# Deterministic pack: sorted entries, commit-time mtimes, numeric 0:0 ownership.
COMMIT_EPOCH="$(git -C "$REPO_ROOT" log -1 --format=%ct "$TAG_SHA")" \
  || die "could not read tag commit time"
[[ "$COMMIT_EPOCH" =~ ^[0-9]+$ ]] || die "tag commit time is malformed"
tar --sort=name --mtime="@$COMMIT_EPOCH" --owner=0 --group=0 --numeric-owner \
  -cf - -C "$STAGE" "$TOP_DIR" | gzip -n > "$ARCHIVE_TMP" \
  || die "archive creation failed"

digest="$(sha256sum "$ARCHIVE_TMP" | awk '{print $1}')" || die "could not calculate archive SHA-256"
[[ "$digest" =~ ^[[:xdigit:]]{64}$ ]] || die "archive SHA-256 is invalid"
printf '%s  %s\n' "$digest" "$ARCHIVE_NAME" > "$SIDECAR_TMP"
chmod 0644 -- "$ARCHIVE_TMP" "$SIDECAR_TMP"

PUBLICATION_ACTIVE=1
mv -f -- "$ARCHIVE_TMP" "$ARCHIVE_OUTPUT" || die "could not publish archive"
mv -f -- "$SIDECAR_TMP" "$SIDECAR_OUTPUT" || die "could not publish archive SHA-256"
PUBLICATION_ACTIVE=0
rm -rf -- "$TMP_ROOT"
[[ ! -e "$TMP_ROOT" ]] || die "could not remove temporary root"
TMP_ROOT=""

printf 'archive: %s\n' "$ARCHIVE_OUTPUT"
printf 'sha256: %s\n' "$digest"

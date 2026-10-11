#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
KWIN_DIR="$REPO_ROOT/kwin"
METADATA="$KWIN_DIR/metadata.json"
MEMBERS=(
  metadata.json
  contents/code/main.js
  contents/config/main.xml
  contents/ui/config.ui
)
# Root MIT notice ships as a top-level LICENSE member; its source lives at
# the repository root, not under kwin/.
LICENSE_MEMBER="LICENSE"
LICENSE_SRC="$REPO_ROOT/LICENSE"
ARCHIVE_MEMBERS=("$LICENSE_MEMBER" "${MEMBERS[@]}")

usage() {
  printf 'usage: build-kpackage.sh [--output-dir <dir>]\n' >&2
}

die() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_tool() {
  local var_name="$1" default_name="$2" value="${!1:-}"
  if [[ -n "$value" ]]; then
    if [[ "$value" == */* && "$value" != /* ]]; then
      value="$(pwd -P)/$value"
    fi
    if [[ "$value" != */* ]]; then
      value="$(command -v -- "$value")" || die "$var_name is set but was not found in PATH: ${!1}"
    fi
    [[ -x "$value" ]] || die "$var_name is set but is not an executable: $value"
    printf '%s\n' "$value"
    return
  fi
  value="$(command -v -- "$default_name")" || die "required tool '$default_name' not found in PATH"
  [[ -x "$value" ]] || die "required tool '$default_name' is not an executable file"
  printf '%s\n' "$value"
}

if [[ $# -eq 0 ]]; then
  OUTPUT_DIR="$REPO_ROOT/dist"
elif [[ $# -eq 2 && "$1" == "--output-dir" && -n "$2" && "$2" != --* ]]; then
  OUTPUT_DIR="$2"
else
  usage
  exit 1
fi

mkdir -p -- "$OUTPUT_DIR"
OUTPUT_DIR="$(cd -- "$OUTPUT_DIR" && pwd -P)"

NPM=""
if [[ "${OMNITILER_PREBUILT_BUNDLE:-0}" != "1" ]]; then
  NPM="$(require_tool NPM_BIN npm)"
fi
ZIP="$(require_tool ZIP_BIN zip)"
SHA256SUM="$(require_tool SHA256SUM_BIN sha256sum)"
KPACKAGETOOL6="$(require_tool KPACKAGETOOL6_BIN kpackagetool6)"
MV="$(command -v mv)" || die "required tool 'mv' not found in PATH"
NODE="$(command -v node)" || die "required tool 'node' not found in PATH"

PLUGIN_ID="$("$NODE" - "$METADATA" <<'NODE'
const fs = require('fs');

try {
  const id = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))?.KPlugin?.Id;
  if (typeof id !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(id)) {
    process.exit(1);
  }
  process.stdout.write(id);
} catch {
  process.exit(1);
}
NODE
)" || die "metadata.json must contain a safe KPlugin.Id"
ARTIFACT_NAME="$PLUGIN_ID.kwinscript"

umask 077
TMP_ROOT="$(mktemp -d "$OUTPUT_DIR/.build-kpackage.XXXXXX")" || die "could not create temporary root"
PUBLICATION_ACTIVE=0
cleanup() {
  local status=$?
  trap - EXIT HUP INT TERM
  if [[ "${PUBLICATION_ACTIVE:-0}" -eq 1 ]]; then
    restore_previous_outputs || {
      printf 'error: could not restore previous release output\n' >&2
      status=1
    }
  fi
  [[ -z "${TMP_ROOT:-}" ]] || rm -rf -- "$TMP_ROOT"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

STAGING="$TMP_ROOT/staging"
VALIDATION_ROOT="$TMP_ROOT/validation"
ARCHIVE_TMP="$TMP_ROOT/$ARTIFACT_NAME"
SIDECAR_TMP="$TMP_ROOT/$ARTIFACT_NAME.sha256"
ARCHIVE_OUTPUT="$OUTPUT_DIR/$ARTIFACT_NAME"
SIDECAR_OUTPUT="$OUTPUT_DIR/$ARTIFACT_NAME.sha256"
ARCHIVE_BACKUP="$TMP_ROOT/$ARTIFACT_NAME.previous"
SIDECAR_BACKUP="$TMP_ROOT/$ARTIFACT_NAME.sha256.previous"

had_archive=0
had_sidecar=0
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

restore_previous_outputs() {
  if [[ "$had_archive" -eq 1 ]]; then
    cp -p -- "$ARCHIVE_BACKUP" "$ARCHIVE_OUTPUT" || return 1
  else
    rm -f -- "$ARCHIVE_OUTPUT" || return 1
  fi
  if [[ "$had_sidecar" -eq 1 ]]; then
    cp -p -- "$SIDECAR_BACKUP" "$SIDECAR_OUTPUT" || return 1
  else
    rm -f -- "$SIDECAR_OUTPUT" || return 1
  fi
}

# Build the generated bundle before checking and copying the fixed source set,
# unless the caller supplies the prebuilt offline bundle (release source
# archives ship kwin/contents/code/main.js ready-made, so distro builds do
# not need the Node >=24 toolchain; any Node that parses metadata.json
# still suffices for the plugin-id check above).
if [[ "${OMNITILER_PREBUILT_BUNDLE:-0}" == "1" ]]; then
  [[ -f "$KWIN_DIR/contents/code/main.js" && ! -L "$KWIN_DIR/contents/code/main.js" ]] \
    || die "OMNITILER_PREBUILT_BUNDLE=1 but contents/code/main.js is not a regular non-symlink file"
else
  "$NPM" --prefix "$KWIN_DIR" run build || die "npm run build failed"
fi

for member in "${MEMBERS[@]}"; do
  source="$KWIN_DIR/$member"
  [[ -f "$source" && ! -L "$source" ]] || die "source is not a regular non-symlink file: $member"
done
[[ -f "$LICENSE_SRC" && ! -L "$LICENSE_SRC" ]] || die "source is not a regular non-symlink file: $LICENSE_MEMBER"

rm -rf -- "$STAGING"
mkdir -p -- "$STAGING/contents/code" "$STAGING/contents/config" "$STAGING/contents/ui"
for member in "${MEMBERS[@]}"; do
  cp -- "$KWIN_DIR/$member" "$STAGING/$member"
  chmod 0644 -- "$STAGING/$member"
done
cp -- "$LICENSE_SRC" "$STAGING/$LICENSE_MEMBER"
chmod 0644 -- "$STAGING/$LICENSE_MEMBER"

export LC_ALL=C
export LANG=C
export TZ=UTC0
touch -t 198001010000 -- "$STAGING/$LICENSE_MEMBER" "${MEMBERS[@]/#/$STAGING/}"
(
  cd -- "$STAGING"
  "$ZIP" -q -X -D "$ARCHIVE_TMP" "${ARCHIVE_MEMBERS[@]}"
) || die "archive creation failed"

digest="$("$SHA256SUM" "$ARCHIVE_TMP" | awk '{print $1}')" || die "could not calculate archive SHA-256"
[[ "$digest" =~ ^[[:xdigit:]]{64}$ ]] || die "archive SHA-256 is invalid"
printf '%s  %s\n' "$digest" "$ARTIFACT_NAME" > "$SIDECAR_TMP"
chmod 0644 -- "$ARCHIVE_TMP" "$SIDECAR_TMP"

mkdir -p -- "$VALIDATION_ROOT/home" "$VALIDATION_ROOT/config" "$VALIDATION_ROOT/cache" \
  "$VALIDATION_ROOT/data" "$VALIDATION_ROOT/state" "$VALIDATION_ROOT/runtime" "$VALIDATION_ROOT/packageroot"
chmod 0700 -- "$VALIDATION_ROOT/runtime"
HOME="$VALIDATION_ROOT/home" \
XDG_CONFIG_HOME="$VALIDATION_ROOT/config" \
XDG_CACHE_HOME="$VALIDATION_ROOT/cache" \
XDG_DATA_HOME="$VALIDATION_ROOT/data" \
XDG_STATE_HOME="$VALIDATION_ROOT/state" \
XDG_RUNTIME_DIR="$VALIDATION_ROOT/runtime" \
  "$KPACKAGETOOL6" --type KWin/Script --packageroot "$VALIDATION_ROOT/packageroot" --install "$ARCHIVE_TMP" \
  || die "kpackagetool6 validation failed"

PACKAGE_ROOT="$VALIDATION_ROOT/packageroot/$PLUGIN_ID"
[[ -d "$PACKAGE_ROOT" && ! -L "$PACKAGE_ROOT" ]] || die "kpackagetool6 did not install the expected package root"
EXPECTED_DIRECTORIES=(
  "$PACKAGE_ROOT"
  "$PACKAGE_ROOT/contents"
  "$PACKAGE_ROOT/contents/code"
  "$PACKAGE_ROOT/contents/config"
  "$PACKAGE_ROOT/contents/ui"
)
find "$VALIDATION_ROOT/packageroot" -mindepth 1 -type d -print0 > "$TMP_ROOT/installed-directories" \
  || die "could not inspect kpackagetool6 installation directories"
directory_count=0
while IFS= read -r -d '' installed_directory; do
  valid_directory=0
  for expected_directory in "${EXPECTED_DIRECTORIES[@]}"; do
    if [[ "$installed_directory" == "$expected_directory" ]]; then
      ((directory_count += 1))
      valid_directory=1
      break
    fi
  done
  [[ "$valid_directory" -eq 1 ]] \
    || die "kpackagetool6 installed an unexpected directory: $installed_directory"
done < "$TMP_ROOT/installed-directories"
[[ "$directory_count" -eq "${#EXPECTED_DIRECTORIES[@]}" ]] \
  || die "kpackagetool6 installed an incomplete package directory structure"
find "$VALIDATION_ROOT/packageroot" \( -type f -o -type l \) -print0 > "$TMP_ROOT/installed-paths" \
  || die "could not inspect kpackagetool6 installation"
while IFS= read -r -d '' installed_path; do
  [[ ! -L "$installed_path" ]] || die "kpackagetool6 installed an unexpected symlink"
  [[ "$installed_path" == "$PACKAGE_ROOT/"* ]] \
    || die "kpackagetool6 installed a file or symlink outside the expected package tree"
done < "$TMP_ROOT/installed-paths"
for member in "${ARCHIVE_MEMBERS[@]}"; do
  [[ -f "$PACKAGE_ROOT/$member" && ! -L "$PACKAGE_ROOT/$member" ]] \
    || die "kpackagetool6 installed an invalid package member: $member"
done
find "$PACKAGE_ROOT" -type f -print0 > "$TMP_ROOT/installed-members" \
  || die "could not inspect installed package members"
member_count=0
while IFS= read -r -d '' installed_path; do
  installed_member="${installed_path#"$PACKAGE_ROOT/"}"
  valid_member=0
  for member in "${ARCHIVE_MEMBERS[@]}"; do
    if [[ "$installed_member" == "$member" ]]; then
      ((member_count += 1))
      valid_member=1
      break
    fi
  done
  [[ "$valid_member" -eq 1 ]] || die "kpackagetool6 installed an unexpected package member: $installed_member"
done < "$TMP_ROOT/installed-members"
[[ "$member_count" -eq "${#ARCHIVE_MEMBERS[@]}" ]] || die "kpackagetool6 installed an incomplete package"

PUBLICATION_ACTIVE=1
if ! "$MV" -f -- "$ARCHIVE_TMP" "$ARCHIVE_OUTPUT"; then
  die "could not publish archive"
fi
if ! "$MV" -f -- "$SIDECAR_TMP" "$SIDECAR_OUTPUT"; then
  die "could not publish archive SHA-256"
fi
PUBLICATION_ACTIVE=0
rm -rf -- "$TMP_ROOT"
[[ ! -e "$TMP_ROOT" ]] || die "could not remove temporary root"
TMP_ROOT=""

#!/usr/bin/env bash
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
PRODUCTION_SCRIPT="$REPO_ROOT/scripts/build-kpackage.sh"
MEMBERS=(
  LICENSE
  metadata.json
  contents/code/main.js
  contents/config/main.xml
  contents/ui/config.ui
)
NATIVE_MEMBERS=(
  kwin/effects/plugins/omnitiler-active-border.so
  kwin/effects/configs/omnitiler-active-border_config.so
  kwin/scripts/configs/omnitiler-kwin_config.so
)
PLUGIN_ID="omnitiler-kwin"

WORK="$(mktemp -d)"
FIXTURE="$WORK/fixture"
FIXTURE_TMP="$FIXTURE/tmp"
FAKE_BIN="$WORK/bin"
FAIL=0

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

for tool in bash stat zip unzip zipinfo sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'FAIL: missing required tool: %s\n' "$tool" >&2
    FAIL=1
  fi
done
if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi

BASH_BIN="$(command -v bash)"
ZIP_BIN="$(command -v zip)"
UNZIP_BIN="$(command -v unzip)"
ZIPINFO_BIN="$(command -v zipinfo)"
SHA256SUM_BIN="$(command -v sha256sum)"
STAT_BIN="$(command -v stat)"

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  FAIL=1
}

assert_file() {
  [[ -f "$1" ]] || fail "missing file: $1"
}

assert_no_artifacts() {
  local output_dir="$1"
  local artifacts=()
  local path
  shopt -s nullglob
  artifacts=("$output_dir"/*.kwinscript "$output_dir"/*.kwinscript.sha256)
  shopt -u nullglob
  for path in "${artifacts[@]}"; do
    [[ ! -e "$path" ]] || fail "partial artifact remains after failure: $path"
  done
}

archive_in() {
  local output_dir="$1"
  local archives=()
  shopt -s nullglob
  archives=("$output_dir"/*.kwinscript)
  shopt -u nullglob
  [[ "${#archives[@]}" -eq 1 ]] || return 1
  printf '%s\n' "${archives[0]}"
}

assert_source_inputs() {
  local path source
  for path in "${MEMBERS[@]}"; do
    case "$path" in
      LICENSE) source="$FIXTURE/$path" ;;
      *) source="$FIXTURE/kwin/$path" ;;
    esac
    if [[ ! -f "$source" || -L "$source" ]]; then
      fail "fixture source is not a regular non-symlink file: $path"
    fi
  done
}

assert_archive_audit() {
  local archive="$1"
  local got=()
  local records=()
  local index mode
  mapfile -t got < <("$ZIPINFO_BIN" -1 "$archive")
  if [[ "${#got[@]}" -ne "${#MEMBERS[@]}" ]]; then
    fail "archive does not contain exactly five members"
    return
  fi
  for index in "${!MEMBERS[@]}"; do
    [[ "${got[$index]}" == "${MEMBERS[$index]}" ]] || fail "archive member order differs at index $index"
  done
  for native_member in "${NATIVE_MEMBERS[@]}"; do
    if printf '%s\n' "${got[@]}" | grep -Fxq -- "$native_member"; then
      fail "archive contains native member: $native_member"
    fi
  done

  mapfile -t records < <("$ZIPINFO_BIN" -l "$archive" | awk '/^[-dl][rwxstST-]{9}/')
  if [[ "${#records[@]}" -ne "${#MEMBERS[@]}" ]]; then
    fail "archive audit did not find five file records"
    return
  fi
  mode="${records[0]%% *}"
  for index in "${!records[@]}"; do
    [[ "${records[$index]%% *}" == "$mode" && "$mode" == -* ]] || fail "archive member is not a normalized regular file"
  done
  if "$ZIPINFO_BIN" -v "$archive" | awk '/length of extra field:/ && $0 !~ /0 bytes/ { exit 1 }'; then
    :
  else
    fail "archive contains ZIP extra fields"
  fi
  if [[ "$("$ZIPINFO_BIN" -v "$archive" | awk '/file last modified on \(DOS date\/time\):/ { if ($0 !~ /1980 Jan 1 00:00:00/) exit 1; count++ } END { print count + 0 }')" != 5 ]]; then
    fail "archive member timestamps are not exactly 1980-01-01 00:00:00"
  fi
}

assert_archive_payload() {
  local archive="$1" member expected
  for member in "${MEMBERS[@]}"; do
    case "$member" in
      LICENSE) expected="$FIXTURE/$member" ;;
      *) expected="$FIXTURE/kwin/$member" ;;
    esac
    if ! "$UNZIP_BIN" -p "$archive" "$member" | cmp -s - "$expected"; then
      fail "archive payload differs from the staged script member: $member"
    fi
  done
}

assert_sidecar() {
  local archive="$1"
  local sidecar="$archive.sha256"
  local digest expected
  assert_file "$sidecar"
  digest="$("$SHA256SUM_BIN" "$archive" | awk '{print $1}')"
  expected="$digest  ${archive##*/}"
  if ! cmp -s <(printf '%s\n' "$expected") "$sidecar"; then
    fail "SHA-256 sidecar is not exactly '<digest>  <archive-name>'"
  fi
}

assert_final_modes() {
  local archive="$1"
  [[ "$("$STAT_BIN" -c '%a' "$archive")" == 644 ]] || fail "archive mode is not 0644"
  [[ "$("$STAT_BIN" -c '%a' "$archive.sha256")" == 644 ]] || fail "sidecar mode is not 0644"
}

assert_pair_preserved() {
  local output_dir="$1" expected_archive="$2" expected_sidecar="$3" archive
  archive="$(archive_in "$output_dir")" || {
    fail "existing archive was not preserved"
    return
  }
  cmp -s "$archive" "$expected_archive" || fail "existing archive changed after failure"
  cmp -s "$archive.sha256" "$expected_sidecar" || fail "existing sidecar changed after failure"
}

seed_release_outputs() {
  local output_dir="$1" state="$2"
  local archive="$output_dir/$PLUGIN_ID.kwinscript"
  local sidecar="$archive.sha256"
  rm -f -- "$archive" "$sidecar"
  case "$state" in
    none)
      ;;
    archive)
      printf 'old archive\n' > "$archive"
      chmod 0644 -- "$archive"
      ;;
    sidecar)
      printf 'old sidecar\n' > "$sidecar"
      chmod 0600 -- "$sidecar"
      ;;
    pair)
      printf 'old archive\n' > "$archive"
      printf 'old sidecar\n' > "$sidecar"
      chmod 0644 -- "$archive"
      chmod 0600 -- "$sidecar"
      ;;
    *)
      fail "unknown prior output state: $state"
      ;;
  esac
}

capture_release_outputs() {
  local output_dir="$1" snapshot_dir="$2"
  local archive="$output_dir/$PLUGIN_ID.kwinscript"
  mkdir -p "$snapshot_dir"
  for path in "$archive" "$archive.sha256"; do
    [[ ! -e "$path" ]] || cp -p -- "$path" "$snapshot_dir/${path##*/}"
  done
}

assert_release_outputs_restored() {
  local output_dir="$1" snapshot_dir="$2"
  local archive="$output_dir/$PLUGIN_ID.kwinscript"
  for path in "$archive" "$archive.sha256"; do
    local expected="$snapshot_dir/${path##*/}"
    if [[ -e "$expected" ]]; then
      [[ -f "$path" && ! -L "$path" ]] || fail "prior output was not restored: $path"
      cmp -s "$path" "$expected" || fail "prior output contents changed: $path"
      [[ "$("$STAT_BIN" -c '%a' "$path")" == "$("$STAT_BIN" -c '%a' "$expected")" ]] || fail "prior output mode changed: $path"
    else
      [[ ! -e "$path" ]] || fail "new output remains where none existed: $path"
    fi
  done
}

make_fake_tools() {
  mkdir -p "$FAKE_BIN"
  cat > "$FAKE_BIN/npm" <<'EOF'
#!/usr/bin/env bash
set -u
if [[ -e "${FAKE_STATE:?}/build-fail" ]]; then
  exit 1
fi
prefix=""
while [[ "$#" -gt 0 ]]; do
  if [[ "$1" == "--prefix" ]]; then
    prefix="$2"
    shift 2
  else
    shift
  fi
done
[[ -n "$prefix" ]] || exit 2
rm -rf -- "$prefix/dist"
mkdir -p "$prefix/contents/code"
printf 'generated-by-fake-build\n' > "$prefix/contents/code/main.js"
EOF
  cat > "$FAKE_BIN/zip" <<'EOF'
#!/usr/bin/env bash
set -u
[[ ! -e "${FAKE_STATE:?}/archive-fail" ]] || exit 1
exec "${REAL_ZIP_BIN:?}" "$@"
EOF
  cat > "$FAKE_BIN/sha256sum" <<'EOF'
#!/usr/bin/env bash
exec "${REAL_SHA256SUM_BIN:?}" "$@"
EOF
  cat > "$FAKE_BIN/kpackagetool6" <<'EOF'
#!/usr/bin/env bash
set -u
[[ ! -e "${FAKE_STATE:?}/validation-fail" ]] || exit 1
arguments=("$@")
packageroot=""
archive=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --packageroot)
      packageroot="$2"
      shift 2
      ;;
    --install)
      archive="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
[[ -n "$packageroot" && -n "$archive" ]] || exit 2
for argument in "${arguments[@]}"; do
  if [[ "$argument" == /* && "$argument" != "${FIXTURE_ROOT:?}/"* ]]; then
    printf 'validation path escaped fixture root: %s\n' "$argument" >&2
    exit 2
  fi
done
printf '%s\n' "${arguments[@]}" > "${VALIDATION_ARGS:?}"
package="$packageroot/${FAKE_PLUGIN_ID:?}"
mkdir -p "$package"
"${REAL_UNZIP_BIN:?}" -qq "$archive" -d "$package"
if [[ -e "${FAKE_STATE:?}/install-outside-package" ]]; then
  mkdir -p "$packageroot/unexpected"
  printf 'outside\n' > "$packageroot/unexpected/file"
fi
if [[ -e "${FAKE_STATE:?}/install-empty-outside-package" ]]; then
  mkdir -p "$packageroot/unexpected"
fi
if [[ -e "${FAKE_STATE:?}/install-unexpected-member" ]]; then
  printf 'unexpected\n' > "$package/unexpected.txt"
fi
if [[ -e "${FAKE_STATE:?}/install-symlink" ]]; then
  ln -s metadata.json "$package/unexpected-link"
fi
EOF
  cat > "$FAKE_BIN/mv" <<'EOF'
#!/usr/bin/env bash
set -u
source="${@: -2:1}"
case "$source" in
  */.build-kpackage.*/*.kwinscript)
    [[ ! -e "${FAKE_STATE:?}/publish-archive-fail" ]] || exit 1
    if [[ -e "${FAKE_STATE:?}/interrupt-after-first-publish" ]]; then
      "${REAL_MV_BIN:?}" "$@"
      status=$?
      [[ "$status" -ne 0 ]] || kill -TERM "$PPID"
      exit "$status"
    fi
    ;;
  */.build-kpackage.*/*.kwinscript.sha256)
    [[ ! -e "${FAKE_STATE:?}/publish-sidecar-fail" ]] || exit 1
    ;;
esac
exec "${REAL_MV_BIN:?}" "$@"
EOF
  chmod +x "$FAKE_BIN/npm" "$FAKE_BIN/zip" "$FAKE_BIN/sha256sum" "$FAKE_BIN/kpackagetool6" "$FAKE_BIN/mv"
}

setup_fixture() {
  rm -rf "$FIXTURE"
  mkdir -p "$FIXTURE/scripts" "$FIXTURE/kwin/contents/code" "$FIXTURE/kwin/contents/config" "$FIXTURE/kwin/contents/ui" "$FIXTURE_TMP"
  cp "$REPO_ROOT/kwin/metadata.json" "$FIXTURE/kwin/metadata.json"
  cp "$REPO_ROOT/LICENSE" "$FIXTURE/LICENSE"
  cp "$REPO_ROOT/kwin/contents/config/main.xml" "$FIXTURE/kwin/contents/config/main.xml"
  cp "$REPO_ROOT/kwin/contents/ui/config.ui" "$FIXTURE/kwin/contents/ui/config.ui"
  printf 'stale-manual-bundle\n' > "$FIXTURE/kwin/contents/code/main.js"
  if [[ -f "$PRODUCTION_SCRIPT" ]]; then
    cp "$PRODUCTION_SCRIPT" "$FIXTURE/scripts/build-kpackage.sh"
  fi
  mkdir -p "$FIXTURE_TMP/state"
  : > "$FIXTURE_TMP/validation-args"
  assert_source_inputs
}

run_packaging() {
  local output_dir="$1"
  local timezone="$2"
  local locale="$3"
  local default_output="${4:-0}"
  local override_style="${5:-absolute}"
  local npm="$FAKE_BIN/npm" zip="$FAKE_BIN/zip" sha256sum="$FAKE_BIN/sha256sum"
  local kpackagetool6="$FAKE_BIN/kpackagetool6"
  local args=(--output-dir "$output_dir")
  if [[ "$default_output" -eq 1 ]]; then
    args=()
  fi
  case "$override_style" in
    absolute)
      ;;
    bare)
      npm="npm"
      zip="zip"
      sha256sum="sha256sum"
      kpackagetool6="kpackagetool6"
      ;;
    relative)
      npm="../bin/npm"
      zip="../bin/zip"
      sha256sum="../bin/sha256sum"
      kpackagetool6="../bin/kpackagetool6"
      ;;
    *)
      fail "unknown tool override style: $override_style"
      return 2
      ;;
  esac
  (
    cd "$FIXTURE" || exit 1
    env HOME="$FIXTURE_TMP/home" XDG_DATA_HOME="$FIXTURE_TMP/data" XDG_CONFIG_HOME="$FIXTURE_TMP/config" \
      XDG_CACHE_HOME="$FIXTURE_TMP/cache" TMPDIR="$FIXTURE_TMP/tmp" TZ="$timezone" LC_ALL="$locale" LANG="$locale" \
      PATH="$FAKE_BIN:$PATH" NPM_BIN="$npm" ZIP_BIN="$zip" SHA256SUM_BIN="$sha256sum" KPACKAGETOOL6_BIN="$kpackagetool6" \
      FAKE_STATE="$FIXTURE_TMP/state" REAL_ZIP_BIN="$ZIP_BIN" REAL_SHA256SUM_BIN="$SHA256SUM_BIN" REAL_MV_BIN="$(command -v mv)" \
      REAL_UNZIP_BIN="$UNZIP_BIN" FAKE_PLUGIN_ID="$PLUGIN_ID" FIXTURE_ROOT="$FIXTURE" VALIDATION_ARGS="$FIXTURE_TMP/validation-args" \
      "$BASH_BIN" scripts/build-kpackage.sh "${args[@]}"
  )
}

self_check() {
  local source="$WORK/self-check"
  local archive="$WORK/self-check.kwinscript"
  mkdir -p "$source/contents/code" "$source/contents/config" "$source/contents/ui"
  printf 'MIT notice\n' > "$source/LICENSE"
  printf '{}\n' > "$source/metadata.json"
  printf 'bundle\n' > "$source/contents/code/main.js"
  printf '<kcfg/>\n' > "$source/contents/config/main.xml"
  printf '<ui/>\n' > "$source/contents/ui/config.ui"
  touch -t 198001010000 -- "${MEMBERS[@]/#/$source/}"
  (
    cd "$source" || exit 1
    "$ZIP_BIN" -q -X "$archive" "${MEMBERS[@]}"
  ) || fail "self-check archive creation failed"
  assert_archive_audit "$archive"
  "$UNZIP_BIN" -p "$archive" contents/code/main.js | cmp -s - <(printf 'bundle\n') || fail "self-check archive extraction failed"
}

make_fake_tools
self_check
if [[ "$FAIL" -ne 0 ]]; then
  printf 'harness self-check failed\n' >&2
  exit 1
fi

if [[ ! -f "$PRODUCTION_SCRIPT" ]]; then
  printf 'FAIL: production command absent: %s (intended unit-01 boundary)\n' "$PRODUCTION_SCRIPT" >&2
  exit 1
fi

# The fake declared build replaces this stale source bundle before it is staged.
setup_fixture
OUTPUT_ONE="$FIXTURE_TMP/output-one"
mkdir -p "$OUTPUT_ONE"
if ! run_packaging "$OUTPUT_ONE" UTC0 C; then
  fail "baseline packaging command failed"
else
  ARCHIVE_ONE="$(archive_in "$OUTPUT_ONE")" || fail "baseline output does not contain one archive"
  if [[ -n "${ARCHIVE_ONE:-}" ]]; then
    assert_archive_audit "$ARCHIVE_ONE"
    assert_sidecar "$ARCHIVE_ONE"
    assert_archive_payload "$ARCHIVE_ONE"
    "$UNZIP_BIN" -p "$ARCHIVE_ONE" contents/code/main.js | cmp -s - <(printf 'generated-by-fake-build\n') || fail "archived bundle was not regenerated before staging"
    # Script-only artifact keeps the qualified native script KCM reference:
    # the Configure page resolves only when the companion ABI-matched
    # native-effect delivery is installed alongside this package.
    "$UNZIP_BIN" -p "$ARCHIVE_ONE" metadata.json | grep -Fq "kwin/scripts/configs/omnitiler-kwin_config" \
      || fail "archived metadata does not reference the native script KCM companion"
    "$UNZIP_BIN" -p "$ARCHIVE_ONE" metadata.json | grep -Fq '"License": "MIT"' \
      || fail "archived metadata does not declare the MIT license"
  fi
fi
[[ -s "$FIXTURE_TMP/validation-args" ]] || fail "isolated validation was not invoked"

# The default release root must survive a declared build that cleans kwin/dist.
setup_fixture
if ! run_packaging "" UTC0 C 1; then
  fail "default packaging command failed"
else
  ARCHIVE_DEFAULT="$(archive_in "$FIXTURE/dist")" || fail "default output does not contain one archive"
  if [[ -n "${ARCHIVE_DEFAULT:-}" ]]; then
    assert_archive_audit "$ARCHIVE_DEFAULT"
    assert_sidecar "$ARCHIVE_DEFAULT"
    assert_final_modes "$ARCHIVE_DEFAULT"
  fi
  [[ ! -e "$FIXTURE/kwin/dist" ]] || fail "declared build did not clean kwin/dist"
fi

# Locale and timezone changes must not change bytes, modes, timestamps, or extras.
setup_fixture
OUTPUT_TWO="$FIXTURE_TMP/output-two"
OUTPUT_THREE="$FIXTURE_TMP/output-three"
mkdir -p "$OUTPUT_TWO" "$OUTPUT_THREE"
if run_packaging "$OUTPUT_TWO" UTC0 C && run_packaging "$OUTPUT_THREE" UTC-8 C.utf8; then
  ARCHIVE_TWO="$(archive_in "$OUTPUT_TWO")" || fail "first deterministic output missing"
  ARCHIVE_THREE="$(archive_in "$OUTPUT_THREE")" || fail "second deterministic output missing"
  if [[ -n "${ARCHIVE_TWO:-}" && -n "${ARCHIVE_THREE:-}" ]]; then
    cmp -s "$ARCHIVE_TWO" "$ARCHIVE_THREE" || fail "independent archives differ across locale/timezone"
    assert_archive_audit "$ARCHIVE_TWO"
    assert_archive_audit "$ARCHIVE_THREE"
    assert_sidecar "$ARCHIVE_TWO"
    assert_sidecar "$ARCHIVE_THREE"
  fi
else
  fail "deterministic packaging command failed"
fi

# Absolute, bare, and relative command overrides must remain executable after staging changes directory.
setup_fixture
OUTPUT_RELATIVE="$FIXTURE_TMP/output-relative"
mkdir -p "$OUTPUT_RELATIVE"
if ! run_packaging "$OUTPUT_RELATIVE" UTC0 C 0 relative; then
  fail "relative tool overrides did not survive staging-directory entry"
else
  ARCHIVE_RELATIVE="$(archive_in "$OUTPUT_RELATIVE")" || fail "relative override output missing"
  [[ -z "${ARCHIVE_RELATIVE:-}" ]] || assert_final_modes "$ARCHIVE_RELATIVE"
fi

setup_fixture
OUTPUT_BARE="$FIXTURE_TMP/output-bare"
mkdir -p "$OUTPUT_BARE"
if ! run_packaging "$OUTPUT_BARE" UTC0 C 0 bare; then
  fail "bare tool overrides were not resolved through PATH"
else
  ARCHIVE_BARE="$(archive_in "$OUTPUT_BARE")" || fail "bare override output missing"
  [[ -z "${ARCHIVE_BARE:-}" ]] || assert_final_modes "$ARCHIVE_BARE"
fi

# Failures must preserve an existing archive/sidecar pair without leaving an orphan.
setup_fixture
OUTPUT_PRESERVED="$FIXTURE_TMP/output-preserved"
mkdir -p "$OUTPUT_PRESERVED"
if ! run_packaging "$OUTPUT_PRESERVED" UTC0 C; then
  fail "preservation baseline packaging command failed"
else
  ARCHIVE_PRESERVED="$(archive_in "$OUTPUT_PRESERVED")" || fail "preservation baseline archive missing"
  if [[ -n "${ARCHIVE_PRESERVED:-}" ]]; then
    cp "$ARCHIVE_PRESERVED" "$FIXTURE_TMP/expected.kwinscript"
    cp "$ARCHIVE_PRESERVED.sha256" "$FIXTURE_TMP/expected.kwinscript.sha256"
    assert_final_modes "$ARCHIVE_PRESERVED"
  fi
fi

for failure in build-fail validation-fail publish-archive-fail publish-sidecar-fail; do
  : > "$FIXTURE_TMP/state/$failure"
  if run_packaging "$OUTPUT_PRESERVED" UTC0 C; then
    fail "$failure did not fail packaging"
  fi
  assert_pair_preserved "$OUTPUT_PRESERVED" "$FIXTURE_TMP/expected.kwinscript" "$FIXTURE_TMP/expected.kwinscript.sha256"
  rm -f "$FIXTURE_TMP/state/$failure"
done

# Publication failures must restore every prior presence combination and its modes.
for prior_state in none archive sidecar pair; do
  for failure in publish-archive-fail publish-sidecar-fail; do
    setup_fixture
    OUTPUT_COMBINATION="$FIXTURE_TMP/output-$prior_state-$failure"
    SNAPSHOT="$FIXTURE_TMP/snapshot-$prior_state-$failure"
    mkdir -p "$OUTPUT_COMBINATION"
    seed_release_outputs "$OUTPUT_COMBINATION" "$prior_state"
    capture_release_outputs "$OUTPUT_COMBINATION" "$SNAPSHOT"
    : > "$FIXTURE_TMP/state/$failure"
    if run_packaging "$OUTPUT_COMBINATION" UTC0 C; then
      fail "$failure unexpectedly completed with prior $prior_state output"
    fi
    assert_release_outputs_restored "$OUTPUT_COMBINATION" "$SNAPSHOT"
  done
done

# TERM after the first publication move must use the same rollback path.
setup_fixture
OUTPUT_INTERRUPTED="$FIXTURE_TMP/output-interrupted"
SNAPSHOT_INTERRUPTED="$FIXTURE_TMP/snapshot-interrupted"
mkdir -p "$OUTPUT_INTERRUPTED"
seed_release_outputs "$OUTPUT_INTERRUPTED" pair
capture_release_outputs "$OUTPUT_INTERRUPTED" "$SNAPSHOT_INTERRUPTED"
: > "$FIXTURE_TMP/state/interrupt-after-first-publish"
if run_packaging "$OUTPUT_INTERRUPTED" UTC0 C; then
  fail "handled interruption after first publication unexpectedly completed"
fi
assert_release_outputs_restored "$OUTPUT_INTERRUPTED" "$SNAPSHOT_INTERRUPTED"

# Validation rejects files, empty directories, symlinks, and extra members outside its expected package tree.
for invalid_install in install-outside-package install-empty-outside-package install-unexpected-member install-symlink; do
  setup_fixture
  OUTPUT_INVALID_INSTALL="$FIXTURE_TMP/output-$invalid_install"
  mkdir -p "$OUTPUT_INVALID_INSTALL"
  : > "$FIXTURE_TMP/state/$invalid_install"
  if run_packaging "$OUTPUT_INVALID_INSTALL" UTC0 C; then
    fail "$invalid_install was accepted"
  fi
  assert_no_artifacts "$OUTPUT_INVALID_INSTALL"
done

# A first build that fails leaves no archive or sidecar output.
setup_fixture
OUTPUT_ARCHIVE_FAIL="$FIXTURE_TMP/output-archive-fail"
mkdir -p "$OUTPUT_ARCHIVE_FAIL"
touch "$FIXTURE_TMP/state/archive-fail"
if run_packaging "$OUTPUT_ARCHIVE_FAIL" UTC0 C; then
  fail "archive failure did not fail packaging"
fi
assert_no_artifacts "$OUTPUT_ARCHIVE_FAIL"

if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi

printf 'build-kpackage contract tests passed\n'

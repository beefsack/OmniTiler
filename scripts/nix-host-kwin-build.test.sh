#!/usr/bin/env bash
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BUILDER="$REPO_ROOT/scripts/nix-host-kwin-build.sh"
WORK="$(mktemp -d)"
FAKE_BIN="$WORK/fake-bin"
OUTPUT="$WORK/output.log"
NIX_LOG="$WORK/nix.log"
HOST_CMAKE_LOG="$WORK/host-cmake.log"
STORE_ROOT="$WORK/fake-store"
HOST_BIN_DIR="$WORK/host-bin"
HOST_NATIVE_BIN="$WORK/host-native/bin"
PASS=0
FAIL=0
EXIT=0

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

BASH_PATH="$(command -v bash)"
REAL_JQ="$(command -v jq || true)"
if [[ -z "$REAL_JQ" ]]; then
  echo "FAIL: jq not found in PATH; builder tests require it" >&2
  exit 1
fi

make_fake_tools() {
  mkdir -p "$FAKE_BIN" "$STORE_ROOT" "$HOST_BIN_DIR" "$WORK/src" "$HOST_NATIVE_BIN"
  printf 'cmake_minimum_required(VERSION 3.19)\nproject(fake)\n' > "$WORK/src/CMakeLists.txt"

  # Simulated host-native cmake: only available inside `nix develop`
  # (fake develop prepends HOST_NATIVE_BIN to PATH before exec). Logs to
  # HOST_CMAKE_LOG and creates marker .so outputs for --build.
  cat > "$HOST_NATIVE_BIN/cmake" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'host-cmake %s\n' "$*" >> "${FAKE_HOST_CMAKE_LOG:?}"
printf 'host-cmake PATH=%s\n' "${PATH:-}" >> "${FAKE_HOST_CMAKE_LOG:?}"
if printf '%s\n' "$*" | grep -Fq -- "-DKWin_DIR="; then
  printf 'host-cmake saw-KWin_DIR\n' >> "${FAKE_HOST_CMAKE_LOG:?}"
fi
if printf '%s\n' "$*" | grep -Fq -- "KWinConfig"; then
  printf 'host-cmake saw-KWinConfig-path\n' >> "${FAKE_HOST_CMAKE_LOG:?}"
fi
args=("$@")
build_dir=""
for ((i=0; i<${#args[@]}; i++)); do
  if [[ "${args[$i]}" == "-B" && $((i+1)) -lt ${#args[@]} ]]; then
    build_dir="${args[$((i+1))]}"
  elif [[ "${args[$i]}" == "--build" && $((i+1)) -lt ${#args[@]} ]]; then
    build_dir="${args[$((i+1))]}"
  fi
done
if [[ -n "$build_dir" ]]; then
  case "$*" in
    *"--build"*)
      mkdir -p "$build_dir/bin/kwin/effects/plugins" "$build_dir/bin/kwin/effects/configs"
      printf 'fake-so\n' > "$build_dir/bin/kwin/effects/plugins/omnitiler-active-border.so"
      printf 'fake-kcm\n' > "$build_dir/bin/kwin/effects/configs/omnitiler-active-border_config.so"
      ;;
  esac
fi
exit 0
EOF
  chmod +x "$HOST_NATIVE_BIN/cmake"

  # Fake nix: hermetic provenance + build/develop that EXECUTES the inner bash
  # script with host-native cmake on PATH. Controlled via state dir and
  # FAKE_* env. Logs every invocation; captures leaked pinned env.
  cat > "$FAKE_BIN/nix" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'nix %s\n' "$*" >> "${FAKE_NIX_LOG:?}"
if [[ -n "${OMNITILER_KWIN_DEV_CMAKE_DIR:-}" ]]; then
  printf 'LEAKED_PINNED=%s\n' "$OMNITILER_KWIN_DEV_CMAKE_DIR" >> "${FAKE_NIX_LOG:?}"
fi
if [[ -n "${DOGFOOD_KWIN_DEV_CMAKE_DIR:-}" ]]; then
  printf 'LEAKED_DOGFOOD=%s\n' "$DOGFOOD_KWIN_DEV_CMAKE_DIR" >> "${FAKE_NIX_LOG:?}"
fi
if [[ -n "${CMAKE_BIN:-}" ]]; then
  printf 'LEAKED_CMAKE_BIN=%s\n' "$CMAKE_BIN" >> "${FAKE_NIX_LOG:?}"
fi
if [[ -n "${CARGO_BIN:-}" ]]; then
  printf 'LEAKED_CARGO_BIN=%s\n' "$CARGO_BIN" >> "${FAKE_NIX_LOG:?}"
fi
state="${FAKE_STATE_DIR:?}"
if [[ "${1:-}" == "path-info" ]]; then
  target="${@: -1}"
  if [[ -f "$state/nix-path-info-fail" ]]; then
    echo "fake nix: simulated path-info failure" >&2
    exit 1
  fi
  # The builder must never require path-info on the (possibly unrealized)
  # dev output. Record if it does so tests can prove it does not.
  if [[ -n "${FAKE_DEV_OUT:-}" && "$target" == "$FAKE_DEV_OUT" ]]; then
    printf 'path-info-dev-called %s\n' "$target" >> "${FAKE_NIX_LOG:?}"
    printf '%s\n' "${FAKE_DRV:?}"
    exit 0
  fi
  if [[ -n "${FAKE_MALFORMED_DRV:-}" ]]; then
    printf '%s\n' "$FAKE_MALFORMED_DRV"
    exit 0
  fi
  printf '%s\n' "${FAKE_DRV:?}"
  exit 0
fi
if [[ "${1:-}" == "derivation" ]]; then
  if [[ -f "$state/nix-derivation-show-fail" ]]; then
    echo "fake nix: simulated derivation show failure" >&2
    exit 1
  fi
  if [[ -f "$state/nix-no-dev-output" ]]; then
    printf '{"derivations":{"%s":{"outputs":{"out":{"path":"%s"}}}}}\n' "${FAKE_DRV:?}" "${FAKE_STORE_PATH:?}"
  exit 0
fi
  if [[ -f "$state/nix-legacy-schema" ]]; then
    printf '{"%s":{"outputs":{"out":{"path":"%s"},"dev":{"path":"%s"}}}}\n' "${FAKE_DRV:?}" "${FAKE_STORE_PATH:?}" "${FAKE_DEV_OUT:?}"
    exit 0
  fi
  # Current observed host schema: top-level derivations map keyed by
  # derivation basename with absolute env.dev and relative
  # outputs.dev.path for the same derivation.
  drv_key="$(basename "${FAKE_DRV:?}")"
  dev_rel="$(basename "${FAKE_DEV_OUT:?}")"
  printf '{"derivations":{"%s":{"env":{"dev":"%s"},"outputs":{"dev":{"path":"%s"}}}}}\n' "$drv_key" "${FAKE_DEV_OUT:?}" "$dev_rel"
    exit 0
  fi
if [[ "${1:-}" == "build" ]]; then
  if [[ -f "$state/nix-build-fail" ]]; then
    echo "fake nix: simulated build failure" >&2
    exit 1
  fi
  [[ "${2:-}" == "${FAKE_DRV:?}^dev" ]] || { echo "fake nix: expected exact dev output" >&2; exit 2; }
  if [[ ! -f "$state/nix-build-no-config" ]]; then
    mkdir -p "${FAKE_DEV_OUT:?}/lib/cmake/KWin"
    printf '# realized KWinConfig\n' > "${FAKE_DEV_OUT:?}/lib/cmake/KWin/KWinConfig.cmake"
  fi
  exit 0
fi
if [[ "${1:-}" == "develop" ]]; then
  if [[ -f "$state/nix-develop-fail" ]]; then
    echo "fake nix: simulated develop failure" >&2
    exit 1
  fi
  # Log full develop command line (includes injected rustc dir + KWin_DIR).
  printf 'develop-args %s\n' "$*" >> "${FAKE_NIX_LOG:?}"
  shift
  drv_arg="${1:-}"; shift || true
  [[ "${1:-}" == "--command" ]] || { echo "fake nix: expected --command" >&2; exit 2; }
  shift
  # Simulate the host dev shell: host-native cmake first on PATH, then exec
  # the inner bash script so host cmake (not outer) runs for real.
  export PATH="${FAKE_HOST_NATIVE_BIN:?}:$PATH"
  # Test-only inner leak simulation: when nix-inject-cargo-bin is marked,
  # make CARGO_BIN readonly via BASH_ENV inside the inner bash process so
  # the inner `unset CARGO_BIN` cannot clear it and the builder's
  # `pinned CARGO_BIN leaked` assertion must fire (proves the check is
  # load-bearing, not dead code).
  if [[ -f "$state/nix-inject-cargo-bin" ]]; then
    printf 'CARGO_BIN="/tmp/injected-cargo-bin"\nreadonly CARGO_BIN\nexport CARGO_BIN\n' > "$state/inject-cargo-env.sh"
    export BASH_ENV="$state/inject-cargo-env.sh"
  fi
  exec "$@"
fi
echo "fake nix: unexpected args: $*" >&2
exit 2
EOF
  chmod +x "$FAKE_BIN/nix"

  # Fake portable cargo+rustc from the project dev shell nixpkgs Rust, kept
  # in distinct store bin dirs to exercise multi-dir injection (real dev
  # shell canonicalizes to distinct cargo/rustc stores behind one wrapper).
  # Outer cmake is deliberately absent (must come from nix develop).
  mkdir -p "$STORE_ROOT/hash-rustc/bin" "$STORE_ROOT/hash-cargo/bin"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$STORE_ROOT/hash-rustc/bin/rustc"
  chmod +x "$STORE_ROOT/hash-rustc/bin/rustc"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$STORE_ROOT/hash-cargo/bin/cargo"
  chmod +x "$STORE_ROOT/hash-cargo/bin/cargo"
}

setup_provenance() {
  local drv_base="${1:-abc123-kwin-6.7.5.drv}"
  FAKE_DRV="$STORE_ROOT/$drv_base"
  FAKE_STORE_PATH="$STORE_ROOT/hash-kwin-6.7.5/bin/kwin_wayland"
  FAKE_DEV_OUT="$STORE_ROOT/hash-kwin-dev-6.7.5"
  mkdir -p "$(dirname "$FAKE_STORE_PATH")" "$FAKE_DEV_OUT/lib/cmake/KWin"
  printf 'fake-kwin\n' > "$FAKE_STORE_PATH"
  printf '# fake KWinConfig\n' > "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake"
  rm -f "$HOST_BIN_DIR/kwin_wayland"
  ln -sf "$FAKE_STORE_PATH" "$HOST_BIN_DIR/kwin_wayland"
  export FAKE_DRV FAKE_STORE_PATH FAKE_DEV_OUT
  export FAKE_STATE_DIR="$WORK/state"
  export FAKE_NIX_LOG="$NIX_LOG"
  export FAKE_HOST_NATIVE_BIN="$HOST_NATIVE_BIN"
  export FAKE_HOST_CMAKE_LOG="$HOST_CMAKE_LOG"
  mkdir -p "$WORK/state"
  rm -f "$WORK/state"/nix-*
  unset FAKE_MALFORMED_DRV
  unset FAKE_BUILD_DIR
}

reset_state() {
  rm -rf "$WORK/state"
  mkdir -p "$WORK/state"
  : > "$NIX_LOG"
  : > "$HOST_CMAKE_LOG"
  : > "$OUTPUT"
  unset FAKE_MALFORMED_DRV
  unset FAKE_BUILD_DIR
}

run_builder() {
  set +e
  env -u NIX_BIN -u JQ_BIN -u RUSTC_BIN -u CARGO_BIN -u CMAKE_BIN \
    -u OMNITILER_HOST_KWIN_BIN -u OMNITILER_STORE_ROOT \
    -u OMNITILER_REPO_ROOT -u OMNITILER_KWIN_DEV_CMAKE_DIR \
    -u DOGFOOD_KWIN_DEV_CMAKE_DIR \
    "PATH=$FAKE_BIN:$PATH" \
    "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
    "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
    "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
    "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
    "OMNITILER_STORE_ROOT=$STORE_ROOT" \
    "OMNITILER_REPO_ROOT=$WORK/repo" \
    "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
    "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
    "FAKE_DRV=${FAKE_DRV:-}" "FAKE_STORE_PATH=${FAKE_STORE_PATH:-}" \
    "FAKE_DEV_OUT=${FAKE_DEV_OUT:-}" \
    ${FAKE_MALFORMED_DRV:+FAKE_MALFORMED_DRV="$FAKE_MALFORMED_DRV"} \
    ${FAKE_BUILD_DIR:+FAKE_BUILD_DIR="$FAKE_BUILD_DIR"} \
    ${EXTRA_ENV:+} \
    "$BASH_PATH" "$BUILDER" "$@" >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
}

check_exit() {
  local expected="$1" label="$2"
  if [[ "$EXIT" -ne "$expected" ]]; then
    echo "FAIL [$label]: expected exit $expected, got $EXIT" >&2
    cat "$OUTPUT" >&2
    FAIL=$((FAIL + 1))
  else
    PASS=$((PASS + 1))
  fi
}

assert_contains() {
  local needle="$1" label="$2"
  if grep -Fq -- "$needle" "$OUTPUT"; then
    PASS=$((PASS + 1))
  else
    echo "FAIL [$label]: output does not contain '$needle'" >&2
    cat "$OUTPUT" >&2
    FAIL=$((FAIL + 1))
  fi
}

assert_not_contains() {
  local needle="$1" label="$2"
  if grep -Fq -- "$needle" "$OUTPUT"; then
    echo "FAIL [$label]: output unexpectedly contains '$needle'" >&2
    cat "$OUTPUT" >&2
    FAIL=$((FAIL + 1))
  else
    PASS=$((PASS + 1))
  fi
}

assert_nix_log_contains() {
  local needle="$1" label="$2"
  if grep -Fq -- "$needle" "$NIX_LOG"; then
    PASS=$((PASS + 1))
  else
    echo "FAIL [$label]: nix log does not contain '$needle'" >&2
    cat "$NIX_LOG" >&2
    FAIL=$((FAIL + 1))
  fi
}

assert_nix_log_missing() {
  local needle="$1" label="$2"
  if grep -Fq -- "$needle" "$NIX_LOG"; then
    echo "FAIL [$label]: nix log unexpectedly contains '$needle'" >&2
    cat "$NIX_LOG" >&2
    FAIL=$((FAIL + 1))
  else
    PASS=$((PASS + 1))
  fi
}

assert_host_cmake_log_contains() {
  local needle="$1" label="$2"
  if grep -Fq -- "$needle" "$HOST_CMAKE_LOG"; then
    PASS=$((PASS + 1))
  else
    echo "FAIL [$label]: host cmake log does not contain '$needle'" >&2
    cat "$HOST_CMAKE_LOG" >&2
    FAIL=$((FAIL + 1))
  fi
}

make_fake_tools
mkdir -p "$WORK/repo/target"
setup_provenance

# --help documents resolve/build, metadata-only resolve, exact dev realization,
# host cmake, and no dry-run.
reset_state
run_builder --help
check_exit 0 "help exit"
assert_contains "usage: nix-host-kwin-build.sh" "help usage"
assert_contains "resolve" "help resolve"
assert_contains "Proves derivation metadata only" "help metadata-only"
assert_contains "nix develop" "help develop cost"
assert_contains "nix build <host-drv>^dev --no-link" "help exact dev realization"
assert_contains "nix build --dry-run" "help no auto dry-run"
assert_contains "RUSTC_BIN" "help rustc"
assert_contains "CARGO_BIN" "help cargo"
assert_contains "cargo+rustc" "help cargo rustc injected"
assert_contains "expected-identity" "help expected-identity"
assert_contains "must resolve within" "help host cmake"

# dry-run alias is eliminated (undocumented): now unknown command.
reset_state
run_builder dry-run
check_exit 1 "dry-run eliminated"
assert_contains "unknown command" "dry-run unknown msg"

# resolve: missing host binary fails closed with NixOS-first message.
reset_state
rm -f "$HOST_BIN_DIR/kwin_wayland"
run_builder resolve
check_exit 1 "resolve missing host"
assert_contains "host KWin binary not found" "resolve missing host msg"
assert_contains "no fallback" "resolve missing host no-fallback"
setup_provenance

# resolve: host binary outside the store fails (no fallback).
reset_state
printf '#!/usr/bin/env bash\nexit 0\n' > "$WORK/outside-kwin"
chmod +x "$WORK/outside-kwin"
set +e
env "PATH=$FAKE_BIN:$PATH" "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "OMNITILER_HOST_KWIN_BIN=$WORK/outside-kwin" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" resolve >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "resolve outside store"
assert_contains "does not resolve under" "resolve outside store msg"

# resolve: nix path-info failure fails closed.
reset_state
touch "$WORK/state/nix-path-info-fail"
run_builder resolve
check_exit 1 "resolve path-info fail"
assert_contains "could not identify exact derivation" "resolve path-info msg"

# resolve: malformed derivation (not *.drv) fails.
reset_state
export FAKE_MALFORMED_DRV="$STORE_ROOT/not-a-derivation"
run_builder resolve
check_exit 1 "resolve malformed drv"
assert_contains "malformed derivation" "resolve malformed drv msg"
unset FAKE_MALFORMED_DRV

# resolve: derivation without dev output fails (no fallback).
reset_state
touch "$WORK/state/nix-no-dev-output"
run_builder resolve
check_exit 1 "resolve no dev output"
assert_contains "no exact 'dev' output" "resolve no dev msg"

# resolve: current schema prints provenance + safe identity.
reset_state
run_builder resolve
check_exit 0 "resolve ok current schema"
assert_contains "derivation=$FAKE_DRV" "resolve drv line"
assert_contains "dev_output=$FAKE_DEV_OUT" "resolve dev line"
assert_contains "kwin_cmake_dir=$FAKE_DEV_OUT/lib/cmake/KWin" "resolve kwin dir"
assert_contains "identity=abc123-kwin-6.7.5" "resolve identity opaque"
assert_contains "default_build_dir=$WORK/repo/target/kwin-native-host-abc123-kwin-6.7.5-build" "resolve identity-keyed build dir"
assert_contains "default_stage_dir=$WORK/repo/target/kwin-native-effect-stage" "resolve stage dir"

# resolve: legacy direct-mapping schema still works (straightforward fallback).
reset_state
touch "$WORK/state/nix-legacy-schema"
run_builder resolve
check_exit 0 "resolve ok legacy schema"
assert_contains "dev_output=$FAKE_DEV_OUT" "resolve legacy dev line"
assert_contains "identity=abc123-kwin-6.7.5" "resolve legacy identity"

# resolve is read-only and never requires path-info on dev (unrealized dev
# must remain buildable): only host path-info + derivation show, no develop,
# no dev path-info.
reset_state
run_builder resolve
check_exit 0 "resolve read-only"
assert_nix_log_contains "path-info" "resolve uses path-info"
assert_nix_log_contains "derivation show" "resolve uses derivation show"
assert_nix_log_missing "develop" "resolve never develops"
assert_nix_log_missing "path-info-dev-called" "resolve never path-info on dev"

# resolve: unrealized dev (KWinConfig missing) still succeeds metadata-only.
reset_state
rm "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake"
run_builder resolve
check_exit 0 "resolve unrealized dev ok"
assert_contains "dev_output=$FAKE_DEV_OUT" "resolve unrealized dev line"
assert_contains "kwin_config=$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake" "resolve unrealized config metadata-selected"
printf '# fake KWinConfig\n' > "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake"

# build: executes inner script with host-native cmake, explicit cargo+rustc
# paths, KWin_DIR, strips pinned env; outer cmake never injected.
reset_state
export FAKE_BUILD_DIR="$WORK/build-out"
IDENT_OUT="$(env "PATH=$FAKE_BIN:$PATH" "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" resolve 2>/dev/null | sed -n 's/^identity=//p' | head -n 1)"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "OMNITILER_KWIN_DEV_CMAKE_DIR=/tmp/pinned-kwin-cmake" \
  "DOGFOOD_KWIN_DEV_CMAKE_DIR=/tmp/pinned-dogfood" \
  "CMAKE_BIN=/tmp/pinned-cmake" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" --expected-identity "$IDENT_OUT" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 0 "build ok host cmake"
assert_nix_log_contains "develop $FAKE_DRV --command" "build uses host drv dev shell"
assert_nix_log_contains "build $FAKE_DRV^dev --no-link" "build realizes exact dev output"
assert_nix_log_contains "$STORE_ROOT/hash-rustc/bin" "build injects rustc store dir"
assert_nix_log_contains "$STORE_ROOT/hash-cargo/bin" "build injects cargo store dir"
assert_nix_log_missing "/tmp/pinned-kwin-cmake" "build strips pinned cmake dir"
assert_nix_log_missing "/tmp/pinned-dogfood" "build strips dogfood pinned dir"
assert_nix_log_missing "LEAKED_PINNED" "build env has no pinned leak"
assert_nix_log_missing "LEAKED_DOGFOOD" "build env has no dogfood leak"
assert_nix_log_missing "LEAKED_CMAKE_BIN" "build env has no cmake leak"
assert_nix_log_missing "LEAKED_CARGO_BIN" "build env has no cargo leak"
assert_nix_log_missing "path-info-dev-called" "build never path-info on dev"
assert_host_cmake_log_contains "host-cmake -S" "host cmake configure ran"
assert_host_cmake_log_contains "-DKWin_DIR=$FAKE_DEV_OUT/lib/cmake/KWin" "host cmake explicit resolved KWin dev dir"
assert_host_cmake_log_contains "saw-KWin_DIR" "host cmake saw KWin_DIR"
assert_host_cmake_log_contains "$STORE_ROOT/hash-rustc/bin" "host cmake PATH has explicit rustc"
assert_host_cmake_log_contains "$STORE_ROOT/hash-cargo/bin" "host cmake PATH has explicit cargo"
if [[ -f "$FAKE_BUILD_DIR/bin/kwin/effects/plugins/omnitiler-active-border.so" ]]; then
  PASS=$((PASS + 1))
else
  echo "FAIL [build payload outputs]: host cmake did not stage marker .so" >&2
  FAIL=$((FAIL + 1))
fi
unset FAKE_BUILD_DIR

# build: outer CMAKE_BIN is ignored (host cmake still used). CARGO_BIN is
# an explicit tool path (not ignored): the valid store cargo below succeeds.
reset_state
export FAKE_BUILD_DIR="$WORK/build-outer-ignored"
printf '#!/usr/bin/env bash\necho outer-cmake-should-never-run >&2\nexit 1\n' > "$WORK/failing-cmake"
chmod +x "$WORK/failing-cmake"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "CMAKE_BIN=$WORK/failing-cmake" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 0 "build ignores outer cmake"
assert_host_cmake_log_contains "host-cmake -S" "host cmake ran despite failing outer cmake"
assert_nix_log_missing "LEAKED_CMAKE_BIN" "outer cmake never reaches nix develop"
assert_nix_log_missing "LEAKED_CARGO_BIN" "outer cargo never reaches nix develop"
unset FAKE_BUILD_DIR

# build: outer CARGO_BIN outside the store fails closed (explicit tool path
# must be under the store; no impure fallback).
reset_state
export FAKE_BUILD_DIR="$WORK/build-outer-cargo-impure"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$WORK/failing-cmake" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build outer cargo impure refused"
assert_contains "cargo is not under" "build outer cargo impure msg"
unset FAKE_BUILD_DIR

# build: CARGO_BIN leaked inside the dev shell fails closed via the inner
# assertion (functional proof: fake develop makes it readonly via BASH_ENV
# so inner `unset` cannot clear it; builder must refuse before configure).
reset_state
export FAKE_BUILD_DIR="$WORK/build-cargo-leak"
touch "$WORK/state/nix-inject-cargo-bin"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build inner cargo leak refused"
assert_contains "pinned CARGO_BIN leaked" "build inner cargo leak msg"
if grep -Fq -- "host-cmake -S" "$HOST_CMAKE_LOG"; then
  echo "FAIL [build inner cargo leak no configure]: host cmake ran despite leaked CARGO_BIN" >&2
  cat "$HOST_CMAKE_LOG" >&2
  FAIL=$((FAIL + 1))
else
  PASS=$((PASS + 1))
fi
unset FAKE_BUILD_DIR

# build: a realized dev output without KWinConfig fails closed (no fallback).
reset_state
rm "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake"
touch "$WORK/state/nix-build-no-config"
export FAKE_BUILD_DIR="$WORK/build-missing-config"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build missing KWinConfig inside"
assert_contains "KWinConfig.cmake not found" "build missing KWinConfig msg"
assert_contains "inside host dev shell" "build missing KWinConfig inside msg"
printf '# fake KWinConfig\n' > "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake"
unset FAKE_BUILD_DIR

# build: unrealized dev succeeds when the explicit exact-output build realizes it.
reset_state
rm "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake"
export FAKE_BUILD_DIR="$WORK/build-unrealized"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 0 "build unrealized dev success"
assert_host_cmake_log_contains "host-cmake -S" "unrealized build ran host cmake after realize"
assert_nix_log_contains "build $FAKE_DRV^dev --no-link" "unrealized build realizes exact dev output"
if [[ -f "$FAKE_DEV_OUT/lib/cmake/KWin/KWinConfig.cmake" ]]; then
  PASS=$((PASS + 1))
else
  echo "FAIL [unrealized dev realized]: exact dev build did not realize KWinConfig" >&2
  FAIL=$((FAIL + 1))
fi
unset FAKE_BUILD_DIR

# build: --expected-identity disagreement fails before realization/develop.
reset_state
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$WORK/build-identity-mismatch" --expected-identity "wrong-identity-123" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build identity mismatch"
assert_contains "host identity changed" "build identity mismatch msg"
assert_nix_log_missing "develop" "identity mismatch never develops"
assert_nix_log_missing "build" "identity mismatch never realizes"

# build: portable rustc outside the store fails closed.
reset_state
printf '#!/usr/bin/env bash\nexit 0\n' > "$WORK/impure-rustc"
chmod +x "$WORK/impure-rustc"
set +e
env "PATH=$FAKE_BIN:$PATH" "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$WORK/impure-rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$WORK/build-impure" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build impure rustc"
assert_contains "is not under" "build impure rustc msg"

# build: portable cargo outside the store fails closed.
reset_state
printf '#!/usr/bin/env bash\nexit 0\n' > "$WORK/impure-cargo"
chmod +x "$WORK/impure-cargo"
set +e
env "PATH=$FAKE_BIN:$PATH" "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$WORK/impure-cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$WORK/build-impure-cargo" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build impure cargo"
assert_contains "cargo is not under" "build impure cargo msg"

# build: missing cargo fails closed (explicit CARGO_BIN not executable).
reset_state
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$WORK/src" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$WORK/build-missing-cargo" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build missing cargo"
assert_contains "cargo is not under" "build missing cargo msg"

# build: exact dev realization failure fails closed before develop.
reset_state
export FAKE_BUILD_DIR="$WORK/build-dev-realize-fail"
touch "$WORK/state/nix-build-fail"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build exact dev realization failure"
assert_contains "could not realize exact dev output" "build exact dev realization msg"
assert_nix_log_missing "develop" "exact dev realization failure never develops"
unset FAKE_BUILD_DIR

# build: nix develop failure fails closed with no fallback.
reset_state
export FAKE_BUILD_DIR="$WORK/build-devfail"
touch "$WORK/state/nix-develop-fail"
set +e
env "PATH=$FAKE_BIN:$PATH" \
  "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
  "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
  "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
  "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
  "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
  "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
  "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
  "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
  "$BASH_PATH" "$BUILDER" build --source "$WORK/src" --build-dir "$FAKE_BUILD_DIR" >"$OUTPUT" 2>&1
EXIT=$?
set -e
check_exit 1 "build develop fail"
assert_contains "host-matched native build failed" "build develop fail msg"
assert_contains "no fallback" "build develop no fallback"
unset FAKE_BUILD_DIR

# ---- relocation recovery: stale CMake cache from a moved checkout ----
run_stale_build() {
  set +e
  env "PATH=$FAKE_BIN:$PATH" \
    "NIX_BIN=$FAKE_BIN/nix" "JQ_BIN=$REAL_JQ" \
    "RUSTC_BIN=$STORE_ROOT/hash-rustc/bin/rustc" \
    "CARGO_BIN=$STORE_ROOT/hash-cargo/bin/cargo" \
    "OMNITILER_HOST_KWIN_BIN=$HOST_BIN_DIR/kwin_wayland" \
    "OMNITILER_STORE_ROOT=$STORE_ROOT" "OMNITILER_REPO_ROOT=$WORK/repo" \
    "FAKE_STATE_DIR=$WORK/state" "FAKE_NIX_LOG=$NIX_LOG" \
    "FAKE_HOST_NATIVE_BIN=$HOST_NATIVE_BIN" "FAKE_HOST_CMAKE_LOG=$HOST_CMAKE_LOG" \
    "FAKE_DRV=$FAKE_DRV" "FAKE_STORE_PATH=$FAKE_STORE_PATH" "FAKE_DEV_OUT=$FAKE_DEV_OUT" \
    "$BASH_PATH" "$BUILDER" build --source "$1" --build-dir "$2" >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
}

# moved checkout: cache records different source/build dirs; build removes the
# regenerable dir and configures fresh with a one-line notice.
reset_state
MOVED_BUILD="$WORK/build-moved"
mkdir -p "$MOVED_BUILD"
printf 'CMAKE_HOME_DIRECTORY:FILEPATH=/old/checkout/kwin/native-effect\nCMAKE_CACHEFILE_DIR:INTERNAL=/old/checkout/target/kwin-native-host-abc-build\n' > "$MOVED_BUILD/CMakeCache.txt"
printf 'stale-marker\n' > "$MOVED_BUILD/stale-marker.txt"
run_stale_build "$WORK/src" "$MOVED_BUILD"
check_exit 0 "moved cache rebuild"
assert_contains "stale CMake cache" "moved cache notice"
assert_contains "$MOVED_BUILD" "moved cache notice names dir"
if [[ -e "$MOVED_BUILD/stale-marker.txt" ]]; then echo "FAIL [moved cache removed]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
assert_host_cmake_log_contains "host-cmake -S" "moved cache fresh configure ran"
if [[ -f "$MOVED_BUILD/bin/kwin/effects/plugins/omnitiler-active-border.so" ]]; then PASS=$((PASS + 1)); else echo "FAIL [moved cache fresh payload]" >&2; FAIL=$((FAIL + 1)); fi

# matching cache stays incremental: sentinel preserved, no notice.
reset_state
MATCH_BUILD="$WORK/build-match"
mkdir -p "$MATCH_BUILD"
SRC_CANON="$(readlink -f -- "$WORK/src")"
MATCH_CANON="$(readlink -f -- "$MATCH_BUILD")"
printf 'CMAKE_HOME_DIRECTORY:INTERNAL=%s\nCMAKE_CACHEFILE_DIR:INTERNAL=%s\nCMAKE_HOME_DIRECTORY_EXTRA:INTERNAL=/unrelated\n' "$SRC_CANON" "$MATCH_CANON" > "$MATCH_BUILD/CMakeCache.txt"
printf 'keep-me\n' > "$MATCH_BUILD/keep.txt"
run_stale_build "$WORK/src" "$MATCH_BUILD"
check_exit 0 "matching cache incremental"
if [[ -f "$MATCH_BUILD/keep.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [matching cache preserved]" >&2; FAIL=$((FAIL + 1)); fi
assert_not_contains "stale CMake cache" "matching cache no notice"

# trailing-slash recording still matches (exact parser normalization).
reset_state
SLASH_BUILD="$WORK/build-slash"
mkdir -p "$SLASH_BUILD"
SLASH_CANON="$(readlink -f -- "$SLASH_BUILD")"
printf 'CMAKE_HOME_DIRECTORY:INTERNAL=%s/\nCMAKE_CACHEFILE_DIR:INTERNAL=%s/\n' "$SRC_CANON" "$SLASH_CANON" > "$SLASH_BUILD/CMakeCache.txt"
printf 'keep-me\n' > "$SLASH_BUILD/keep.txt"
run_stale_build "$WORK/src" "$SLASH_BUILD"
check_exit 0 "trailing slash cache incremental"
if [[ -f "$SLASH_BUILD/keep.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [trailing slash preserved]" >&2; FAIL=$((FAIL + 1)); fi
assert_not_contains "stale CMake cache" "trailing slash no notice"

# duplicate recorded keys are ambiguous: never delete, stay incremental.
reset_state
DUP_BUILD="$WORK/build-dup"
mkdir -p "$DUP_BUILD"
printf 'CMAKE_HOME_DIRECTORY:INTERNAL=/old/checkout/src\nCMAKE_HOME_DIRECTORY:INTERNAL=/other/src\nCMAKE_CACHEFILE_DIR:INTERNAL=/old/checkout/build\n' > "$DUP_BUILD/CMakeCache.txt"
printf 'keep-me\n' > "$DUP_BUILD/keep.txt"
run_stale_build "$WORK/src" "$DUP_BUILD"
check_exit 0 "duplicate cache keys incremental"
if [[ -f "$DUP_BUILD/keep.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [duplicate cache preserved]" >&2; FAIL=$((FAIL + 1)); fi
assert_not_contains "stale CMake cache" "duplicate cache no notice"

# malformed recorded value (empty) is ambiguous: never delete.
reset_state
MAL_BUILD="$WORK/build-malformed"
mkdir -p "$MAL_BUILD"
printf 'CMAKE_HOME_DIRECTORY:INTERNAL=\nCMAKE_CACHEFILE_DIR:INTERNAL=/old/checkout/build\n' > "$MAL_BUILD/CMakeCache.txt"
printf 'keep-me\n' > "$MAL_BUILD/keep.txt"
run_stale_build "$WORK/src" "$MAL_BUILD"
check_exit 0 "malformed cache incremental"
if [[ -f "$MAL_BUILD/keep.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [malformed cache preserved]" >&2; FAIL=$((FAIL + 1)); fi
assert_not_contains "stale CMake cache" "malformed cache no notice"

# symlinked build dir is never deleted: fail closed, target preserved.
reset_state
LINK_TARGET="$WORK/link-target"
mkdir -p "$LINK_TARGET"
printf 'precious\n' > "$LINK_TARGET/precious.txt"
LINK_BUILD="$WORK/build-link"
rm -f "$LINK_BUILD"
ln -s -- "$LINK_TARGET" "$LINK_BUILD"
run_stale_build "$WORK/src" "$LINK_BUILD"
check_exit 1 "symlinked build dir refused"
assert_contains "refusing" "symlinked build dir msg"
if [[ -f "$LINK_TARGET/precious.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [symlink target preserved]" >&2; FAIL=$((FAIL + 1)); fi

# symlinked CMakeCache is never trusted for deletion: preserved, no notice.
reset_state
CACHELINK_BUILD="$WORK/build-cachelink"
mkdir -p "$CACHELINK_BUILD" "$WORK/cache-real"
printf 'CMAKE_HOME_DIRECTORY:INTERNAL=/old/checkout/src\nCMAKE_CACHEFILE_DIR:INTERNAL=/old/checkout/build\n' > "$WORK/cache-real/CMakeCache.txt"
rm -f "$CACHELINK_BUILD/CMakeCache.txt"
ln -s -- "$WORK/cache-real/CMakeCache.txt" "$CACHELINK_BUILD/CMakeCache.txt"
printf 'keep-me\n' > "$CACHELINK_BUILD/keep.txt"
run_stale_build "$WORK/src" "$CACHELINK_BUILD"
check_exit 0 "symlinked cache incremental"
if [[ -f "$CACHELINK_BUILD/keep.txt" && -L "$CACHELINK_BUILD/CMakeCache.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [symlinked cache preserved]" >&2; FAIL=$((FAIL + 1)); fi
assert_not_contains "stale CMake cache" "symlinked cache no notice"

# build dir equal to source dir would delete the source: fail closed.
reset_state
OVERLAP_SRC="$WORK/overlap-src"
mkdir -p "$OVERLAP_SRC"
printf 'cmake_minimum_required(VERSION 3.19)\nproject(fake)\n' > "$OVERLAP_SRC/CMakeLists.txt"
run_stale_build "$OVERLAP_SRC" "$OVERLAP_SRC"
check_exit 1 "build dir equal source refused"
assert_contains "refusing" "overlap refusal msg"
if [[ -f "$OVERLAP_SRC/CMakeLists.txt" ]]; then PASS=$((PASS + 1)); else echo "FAIL [source preserved on overlap]" >&2; FAIL=$((FAIL + 1)); fi

# non-regular build dir (plain file) fails closed without deletion.
reset_state
printf 'not-a-dir\n' > "$WORK/build-file"
run_stale_build "$WORK/src" "$WORK/build-file"
check_exit 1 "file build dir refused"
if [[ -f "$WORK/build-file" ]] && [[ "$(cat "$WORK/build-file")" == "not-a-dir" ]]; then PASS=$((PASS + 1)); else echo "FAIL [file build dir preserved]" >&2; FAIL=$((FAIL + 1)); fi

# symlinked build dir with trailing slash is still a symlink: fail closed,
# never rm -rf through the link into the target.
reset_state
SLLINK_TARGET="$WORK/sllink-target"
mkdir -p "$SLLINK_TARGET"
printf 'sentinel\n' > "$SLLINK_TARGET/sentinel.txt"
SLLINK_BUILD="$WORK/build-sllink"
rm -f "$SLLINK_BUILD"
ln -s -- "$SLLINK_TARGET" "$SLLINK_BUILD"
run_stale_build "$WORK/src" "$SLLINK_BUILD/"
check_exit 1 "trailing slash symlink refused"
assert_contains "refusing" "trailing slash symlink msg"
if [[ -f "$SLLINK_TARGET/sentinel.txt" ]] && [[ "$(cat "$SLLINK_TARGET/sentinel.txt")" == "sentinel" ]]; then PASS=$((PASS + 1)); else echo "FAIL [trailing slash target preserved]" >&2; FAIL=$((FAIL + 1)); fi

echo "passes: $PASS failures: $FAIL"
if [[ "$FAIL" -gt 0 ]]; then
  exit 1
fi

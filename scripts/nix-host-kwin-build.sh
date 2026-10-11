#!/usr/bin/env bash
set -euo pipefail

# Host-matched native development builder (NixOS-first, no fallbacks).
#
# Authority: the actual host Nix KWin package at
#   /run/current-system/sw/bin/kwin_wayland
# (override for hermetic tests only via OMNITILER_HOST_KWIN_BIN).
# Resolution is read-only and provenance-bound:
#   host bin -> realpath under /nix/store -> exact derivation via
#   `nix path-info --derivation` -> exact `dev` output path from the SAME
#   derivation metadata via `nix derivation show` + jq.
# `resolve` proves derivation metadata only; it does NOT assert KWinConfig
# file availability because an unrealized dev output must remain buildable
# (no `nix path-info` on dev, no filesystem require on KWinConfig.cmake).
# No nixpkgs version guessing, no store-name version parsing, no fallback
# to a pinned package set.
#
# Build first realizes the exact selected dev output, then runs in the original
# host derivation dev shell:
#   nix build <host-drv>^dev --no-link
#   nix develop <host-drv> --command bash -c 'cmake configure + build'
# with only validated portable cargo+rustc injected by explicit /nix/store
# bin dir(s) and explicit -DKWin_DIR=<dev>/lib/cmake/KWin. Outer cmake is
# never required and never injected: cmake must resolve within the original
# host `nix develop <drv>` environment. Cargo/rustc come from the existing
# project dev shell nixpkgs Rust (CARGO_BIN/RUSTC_BIN or PATH lookup, both
# verified under /nix/store); no toolchain channel/target change. The inner
# shell requires the exact KWinConfig.cmake from the selected realized dev
# output before configure, and asserts the inner cmake is host-native (not
# an injected pinned cmake) while cargo and rustc resolve from the explicit
# injected path(s).
# The legacy OMNITILER_KWIN_DEV_CMAKE_DIR (and DOGFOOD_KWIN_DEV_CMAKE_DIR)
# can neither drive nor leak into the native build: it is unset on entry,
# stripped from the `nix develop` environment, and unset again inside the
# dev shell; cmake receives only the resolved dev output.
#
# Cost model (documented, no hidden extra Nix command):
#   `resolve` is read-only: filesystem checks plus
#   `nix path-info --derivation` and `nix derivation show` only. It never
#   realizes a store path and is safe to run for inspection.
#   `build` first runs `nix build <drv>^dev --no-link` to realize the exact
#   selected dev output, then `nix develop <drv> --command ...` to compile in
#   the original host environment. The builder deliberately does NOT run
#   `nix build --dry-run` automatically on every compile. Run `resolve` first
#   to inspect provenance without cost; then `build` to pay the one-time
#   dev-output closure cost.
#
# Identity keying: build directories embed a safe host package identity
# derived opaquely from the derivation basename (sanitized, no version
# parsing). Callers construct:
#   justfile staging: target/kwin-native-host-<identity>-build (stage stays
#     at target/kwin-native-effect-stage to preserve session delivery)
#   dogfood payload: <transaction>/host-<identity>-build
# `resolve` prints `identity`, `default_build_dir`, and `default_stage_dir`
# so both callers share one implementation. Callers resolve read-only, then
# pass `--expected-identity <identity>` to `build`; build fails closed if
# the host changed between the two resolves (no staging lifecycle redesign).

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
DEFAULT_HOST_BIN="/run/current-system/sw/bin/kwin_wayland"
DEFAULT_STORE_ROOT="/nix/store"

# The legacy pinned CMake dir must neither drive nor leak. Unset on entry.
# Outer cmake is never used: unset so it cannot leak into the host dev shell
# (cmake must come from `nix develop <drv>` itself). CARGO_BIN is a
# legitimate explicit cargo path (like RUSTC_BIN) and is resolved in
# cmd_build; only the legacy pinned env plus CMAKE_BIN are stripped here.
unset OMNITILER_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
unset DOGFOOD_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
unset CMAKE_BIN 2>/dev/null || true

TOOL=""

usage() {
  cat <<'EOF'
usage: nix-host-kwin-build.sh <command> [options]

NixOS-first host-matched native builder. Resolves the actual host KWin
derivation and builds the native effect source inside that derivation's
dev shell. No fallbacks, no guessed nixpkgs version.

Commands:
  resolve [--host-bin PATH] [--store-root DIR] [--repo-root DIR]
    Read-only provenance resolution. Proves derivation metadata only; does
    not assert KWinConfig file availability (dev output may be unrealized).
    Prints key=value lines:
      host_bin, store_path, derivation, dev_output, kwin_cmake_dir,
      kwin_config, identity, default_build_dir, default_stage_dir.
    kwin_cmake_dir/kwin_config are metadata-selected paths, not validated
    files at resolve time. Uses only filesystem checks plus
    `nix path-info --derivation` (host path only) and `nix derivation show`;
    never realizes a store path. Safe for inspection. Exits non-zero with
    `error:` on missing/malformed provenance.

  build --source DIR --build-dir DIR [--host-bin PATH]
        [--store-root DIR] [--repo-root DIR]
        [--expected-identity ID]
    Resolve (as above), fail if --expected-identity disagrees with the
    freshly resolved host identity (host changed; no fallback), then run
    exactly:
      nix build <host-drv>^dev --no-link
      nix develop <host-drv> --command bash -c 'cmake -S ... -B ... \
        -DKWin_DIR=<resolved-dev>/lib/cmake/KWin -DBUILD_TESTING=OFF; \
        cmake --build ...'
    with only validated current-shell cargo+rustc injected by explicit
    /nix/store bin dir(s). Outer cmake is never required and never
    injected; cmake must resolve within the original host
    `nix develop <drv>` environment (asserted host-native, not an injected
    pinned cmake). Cargo/rustc come from the existing project dev shell
    nixpkgs Rust (verified under /nix/store, no channel/target change).
    Inside that environment, the exact KWinConfig.cmake from the selected
    realized dev output is required before configure. The legacy
    OMNITILER_KWIN_DEV_CMAKE_DIR / DOGFOOD_KWIN_DEV_CMAKE_DIR are
    stripped and never passed to cmake. First use may realize the selected dev
    output closure (network/store cost).

  --help  show this help and exit

Environment (test-only overrides; production defaults are NixOS paths):
  OMNITILER_HOST_KWIN_BIN  host kwin_wayland path
  OMNITILER_STORE_ROOT     store prefix (default /nix/store)
  OMNITILER_REPO_ROOT      repo root for default dirs
  NIX_BIN, JQ_BIN, RUSTC_BIN, CARGO_BIN
    explicit tool paths (must be executable); otherwise PATH lookup.
    Cargo and rustc are both required for the AR10 Cargo workspace
    staticlib and both are injected. There is no CMAKE_BIN: outer cmake is
    unsupported and ignored.

Cost: `resolve` never realizes; `build` realizes the exact dev output with
`nix build <host-drv>^dev --no-link`, then compiles in `nix develop <host-drv>`.
No automatic `nix build --dry-run` is run on any path.
EOF
}

require_tool() {
  local var_name="$1" default_name="$2"
  local value="${!var_name:-}"
  if [[ -n "$value" ]]; then
    if [[ ! -x "$value" ]]; then
      echo "error: $var_name is set but is not an executable: $value" >&2
      exit 1
    fi
    TOOL="$value"
    return 0
  fi
  if ! command -v "$default_name" >/dev/null 2>&1; then
    echo "error: required tool '$default_name' not found in PATH; install it or set $var_name to its absolute path" >&2
    exit 1
  fi
  TOOL="$(command -v "$default_name")"
}

canonicalize() {
  local path="$1"
  local out=""
  if ! out="$(readlink -f -- "$path" 2>/dev/null)"; then
    echo "error: could not canonicalize path: $path" >&2
    return 1
  fi
  if [[ -z "$out" ]]; then
    echo "error: could not canonicalize path (empty result): $path" >&2
    return 1
  fi
  printf '%s\n' "$out"
}

# Resolve provenance and print key=value lines. Sets globals:
# RES_HOST_BIN RES_STORE_PATH RES_DRV RES_DEV_OUT RES_KWIN_CMAKE_DIR
# RES_KWIN_CONFIG RES_IDENTITY RES_DEFAULT_BUILD_DIR RES_DEFAULT_STAGE_DIR
# NOTE: kwin_cmake_dir/kwin_config are metadata-selected only; resolve does
# not require the files to exist (unrealized dev output must remain
# buildable). Build realizes the output before validating KWinConfig inside
# `nix develop`.
do_resolve() {
  local host_bin="$1" store_root="$2" repo_root="$3"
  local nix_bin="$4" jq_bin="$5"

  if [[ -z "$host_bin" ]]; then
    echo "error: host KWin binary path is empty; this builder is NixOS-first and requires /run/current-system/sw/bin/kwin_wayland (no fallback)" >&2
    return 1
  fi
  if [[ ! -e "$host_bin" && ! -L "$host_bin" ]]; then
    echo "error: host KWin binary not found: $host_bin; this builder is NixOS-first and requires /run/current-system/sw/bin/kwin_wayland (no fallback)" >&2
    return 1
  fi
  local store_path=""
  store_path="$(canonicalize "$host_bin")" || return 1
  case "$store_path" in
    "$store_root"/*) ;;
    *)
      echo "error: host KWin binary does not resolve under $store_root: $host_bin -> $store_path; refusing (not a NixOS current-system package, no fallback)" >&2
      return 1
      ;;
  esac
  if [[ "$store_path" == *$'\n'* ]]; then
    echo "error: resolved host store path is malformed (newline): $store_path" >&2
    return 1
  fi

  local drv=""
  if ! drv="$(env -u OMNITILER_KWIN_DEV_CMAKE_DIR -u DOGFOOD_KWIN_DEV_CMAKE_DIR -u CMAKE_BIN -u CARGO_BIN "$nix_bin" path-info --derivation -- "$store_path" 2>/dev/null)"; then
    echo "error: could not identify exact derivation for host store path: $store_path (nix path-info --derivation failed); refusing" >&2
    return 1
  fi
  # Single line, trimmed of surrounding whitespace.
  drv="$(printf '%s\n' "$drv" | head -n 1 | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  if [[ -z "$drv" ]]; then
    echo "error: empty derivation for host store path: $store_path; refusing" >&2
    return 1
  fi
  case "$drv" in
    "$store_root"/*.drv) ;;
    *)
      echo "error: malformed derivation for host store path $store_path: '$drv' (expected $store_root/*.drv); refusing" >&2
      return 1
      ;;
  esac
  if [[ "$drv" == *$'\n'* || "$drv" == *" "* ]]; then
    echo "error: malformed derivation (whitespace/newline): '$drv'; refusing" >&2
    return 1
  fi

  local drv_json=""
  if ! drv_json="$(env -u OMNITILER_KWIN_DEV_CMAKE_DIR -u DOGFOOD_KWIN_DEV_CMAKE_DIR -u CMAKE_BIN -u CARGO_BIN "$nix_bin" derivation show -- "$drv" 2>/dev/null)"; then
    echo "error: could not read derivation metadata for: $drv (nix derivation show failed); refusing" >&2
    return 1
  fi
  # Current Nix derivation JSON schema nests under top-level `derivations`
  # keyed by derivation basename (`.derivations[$drv_base].env.dev` absolute
  # and `.derivations[$drv_base].outputs.dev.path` relative, as observed on
  # the live host); legacy schema maps the drv directly
  # (`.[$drv].outputs.dev.path`). Prefer current absolute env.dev, then
  # current outputs path prefixed with the store root only when relative,
  # fall back to legacy only if straightforward. The selected path is
  # metadata only, never realized here, so no `nix path-info` on dev (an
  # unrealized dev output must remain buildable).
  local dev_out=""
  local drv_key="${drv##*/}"
  if ! dev_out="$(printf '%s' "$drv_json" | "$jq_bin" -r --arg drv "$drv" --arg drv_base "$drv_key" --arg store_root "$store_root" '
    (.derivations[$drv_base].env.dev // .derivations[$drv].env.dev // "") as $b
    | (.derivations[$drv_base].outputs.dev.path // .derivations[$drv].outputs.dev.path // "") as $raw_a
    | (if $raw_a == "" then "" elif ($raw_a | startswith("/")) then $raw_a else "\($store_root)/\($raw_a)" end) as $a
    | (.[$drv].outputs.dev.path // "") as $c
    | (.[$drv].env.dev // "") as $d
    | ([$b, $a, $c, $d] | map(select(. != "")) | first // empty)
  ' 2>/dev/null)"; then
    echo "error: could not parse derivation metadata for dev output: $drv; refusing" >&2
    return 1
  fi
  dev_out="$(printf '%s\n' "$dev_out" | head -n 1 | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  if [[ -z "$dev_out" ]]; then
    echo "error: derivation has no exact 'dev' output: $drv; refusing (no fallback to another package set)" >&2
    return 1
  fi
  case "$dev_out" in
    "$store_root"/*) ;;
    *)
      echo "error: malformed dev output for derivation $drv: '$dev_out' (expected under $store_root); refusing" >&2
      return 1
      ;;
  esac
  if [[ "$dev_out" == *$'\n'* || "$dev_out" == *" "* ]]; then
    echo "error: malformed dev output (whitespace/newline): '$dev_out'; refusing" >&2
    return 1
  fi

  local kwin_cmake_dir="$dev_out/lib/cmake/KWin"
  local kwin_config="$kwin_cmake_dir/KWinConfig.cmake"

  local drv_base="${drv##*/}"
  local drv_noext="${drv_base%.drv}"
  if [[ -z "$drv_noext" || "$drv_noext" == "$drv_base" ]]; then
    echo "error: malformed derivation basename: $drv; refusing" >&2
    return 1
  fi
  local identity=""
  identity="$(printf '%s' "$drv_noext" | tr -c 'A-Za-z0-9._-' '_' | cut -c1-128)"
  if [[ -z "$identity" ]]; then
    echo "error: empty host package identity from derivation: $drv; refusing" >&2
    return 1
  fi
  if [[ ! "$identity" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "error: unsafe host package identity from derivation: '$identity'; refusing" >&2
    return 1
  fi

  RES_HOST_BIN="$host_bin"
  RES_STORE_PATH="$store_path"
  RES_DRV="$drv"
  RES_DEV_OUT="$dev_out"
  RES_KWIN_CMAKE_DIR="$kwin_cmake_dir"
  RES_KWIN_CONFIG="$kwin_config"
  RES_IDENTITY="$identity"
  RES_DEFAULT_BUILD_DIR="$repo_root/target/kwin-native-host-$identity-build"
  RES_DEFAULT_STAGE_DIR="$repo_root/target/kwin-native-effect-stage"
  return 0
}

print_resolve() {
  printf 'host_bin=%s\n' "$RES_HOST_BIN"
  printf 'store_path=%s\n' "$RES_STORE_PATH"
  printf 'derivation=%s\n' "$RES_DRV"
  printf 'dev_output=%s\n' "$RES_DEV_OUT"
  printf 'kwin_cmake_dir=%s\n' "$RES_KWIN_CMAKE_DIR"
  printf 'kwin_config=%s\n' "$RES_KWIN_CONFIG"
  printf 'identity=%s\n' "$RES_IDENTITY"
  printf 'default_build_dir=%s\n' "$RES_DEFAULT_BUILD_DIR"
  printf 'default_stage_dir=%s\n' "$RES_DEFAULT_STAGE_DIR"
}

cmd_resolve() {
  local host_bin="${OMNITILER_HOST_KWIN_BIN:-$DEFAULT_HOST_BIN}"
  local store_root="${OMNITILER_STORE_ROOT:-$DEFAULT_STORE_ROOT}"
  local repo_root="${OMNITILER_REPO_ROOT:-$DEFAULT_REPO_ROOT}"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --host-bin)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --host-bin requires a value" >&2; exit 1; }
        host_bin="$2"; shift 2 ;;
      --store-root)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --store-root requires a value" >&2; exit 1; }
        store_root="$2"; shift 2 ;;
      --repo-root)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --repo-root requires a value" >&2; exit 1; }
        repo_root="$2"; shift 2 ;;
      --help|-h) usage; exit 0 ;;
      *) echo "error: unknown resolve argument '$1'" >&2; exit 1 ;;
    esac
  done
  # Strip legacy pinned env even if exported after script entry (paranoia).
  # Outer cmake is unsupported: strip so it cannot leak. CARGO_BIN/RUSTC_BIN
  # are legitimate explicit tool paths and are left alone (resolve needs
  # neither; build resolves them).
  unset OMNITILER_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
  unset DOGFOOD_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
  unset CMAKE_BIN 2>/dev/null || true
  require_tool NIX_BIN nix
  local nix_bin="$TOOL"
  require_tool JQ_BIN jq
  local jq_bin="$TOOL"
  do_resolve "$host_bin" "$store_root" "$repo_root" "$nix_bin" "$jq_bin" || exit 1
  print_resolve
}

cmd_build() {
  local host_bin="${OMNITILER_HOST_KWIN_BIN:-$DEFAULT_HOST_BIN}"
  local store_root="${OMNITILER_STORE_ROOT:-$DEFAULT_STORE_ROOT}"
  local repo_root="${OMNITILER_REPO_ROOT:-$DEFAULT_REPO_ROOT}"
  local source_dir="" build_dir="" expected_identity=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --source)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --source requires a value" >&2; exit 1; }
        source_dir="$2"; shift 2 ;;
      --build-dir)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --build-dir requires a value" >&2; exit 1; }
        build_dir="$2"; shift 2 ;;
      --host-bin)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --host-bin requires a value" >&2; exit 1; }
        host_bin="$2"; shift 2 ;;
      --store-root)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --store-root requires a value" >&2; exit 1; }
        store_root="$2"; shift 2 ;;
      --repo-root)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --repo-root requires a value" >&2; exit 1; }
        repo_root="$2"; shift 2 ;;
      --expected-identity)
        [[ $# -ge 2 && -n "${2:-}" ]] || { echo "error: --expected-identity requires a value" >&2; exit 1; }
        expected_identity="$2"; shift 2 ;;
      --help|-h) usage; exit 0 ;;
      *) echo "error: unknown build argument '$1'" >&2; exit 1 ;;
    esac
  done
  [[ -n "$source_dir" ]] || { echo "error: build requires --source DIR" >&2; exit 1; }
  [[ -n "$build_dir" ]] || { echo "error: build requires --build-dir DIR" >&2; exit 1; }
  [[ -d "$source_dir" ]] || { echo "error: source directory not found: $source_dir" >&2; exit 1; }
  [[ -f "$source_dir/CMakeLists.txt" ]] || { echo "error: source directory has no CMakeLists.txt: $source_dir" >&2; exit 1; }
  if [[ -n "$expected_identity" && ! "$expected_identity" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "error: malformed --expected-identity: '$expected_identity'; refusing" >&2
    exit 1
  fi
  unset OMNITILER_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
  unset DOGFOOD_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
  unset CMAKE_BIN 2>/dev/null || true
  require_tool NIX_BIN nix
  local nix_bin="$TOOL"
  require_tool JQ_BIN jq
  local jq_bin="$TOOL"
  do_resolve "$host_bin" "$store_root" "$repo_root" "$nix_bin" "$jq_bin" || exit 1
  if [[ -n "$expected_identity" && "$RES_IDENTITY" != "$expected_identity" ]]; then
    echo "error: host identity changed (expected '$expected_identity', got '$RES_IDENTITY' from $RES_DRV); refusing (re-resolve and retry, no fallback)" >&2
    exit 1
  fi

  # Validate portable current-shell cargo+rustc from the existing project
  # dev shell nixpkgs Rust (no toolchain channel/target change) and inject
  # by explicit /nix/store bin dir(s). Both are required for the AR10 Cargo
  # workspace staticlib; fail closed on missing cargo or non-store paths.
  require_tool CARGO_BIN cargo
  local cargo_bin="$TOOL"
  require_tool RUSTC_BIN rustc
  local rustc_bin="$TOOL"
  local cargo_real="" rustc_real=""
  cargo_real="$(canonicalize "$cargo_bin")" || exit 1
  rustc_real="$(canonicalize "$rustc_bin")" || exit 1
  case "$cargo_real" in
    "$store_root"/*) ;;
    *)
      echo "error: cargo is not under $store_root: $cargo_real; enter the project dev shell so cargo resolves to an explicit /nix/store path (no impure fallback)" >&2
      exit 1
      ;;
  esac
  case "$rustc_real" in
    "$store_root"/*) ;;
    *)
      echo "error: rustc is not under $store_root: $rustc_real; enter the project dev shell so rustc resolves to an explicit /nix/store path (no impure fallback)" >&2
      exit 1
      ;;
  esac
  local cargo_dir="" rustc_dir=""
  cargo_dir="$(dirname -- "$cargo_real")"
  rustc_dir="$(dirname -- "$rustc_real")"
  [[ -d "$cargo_dir" ]] || { echo "error: cargo bin dir not found: $cargo_dir" >&2; exit 1; }
  [[ -d "$rustc_dir" ]] || { echo "error: rustc bin dir not found: $rustc_dir" >&2; exit 1; }
  local injected_path=""
  if [[ "$cargo_dir" == "$rustc_dir" ]]; then
    injected_path="$cargo_dir"
  else
    injected_path="$cargo_dir:$rustc_dir"
  fi

  # Normalize trailing slashes first: a path like <symlink>/ defeats the
  # -L guard below and rm -rf would empty the link target.
  while [[ "$build_dir" == */ && "$build_dir" != "/" ]]; do
    build_dir="${build_dir%/}"
  done
  if [[ -z "$build_dir" || "$build_dir" == "/" ]]; then
    echo "error: refusing to clean unsafe build directory: ${build_dir:-empty}" >&2
    exit 1
  fi
  mkdir -p -- "$build_dir" || { echo "error: could not create build directory: $build_dir" >&2; exit 1; }

  # Relocation recovery: a moved checkout leaves a CMakeCache.txt recording a
  # different source or build directory. That build dir is regenerable, so
  # remove it and configure fresh with a one-line notice. Matching caches
  # stay incremental (CMake handles source changes itself). Anything unsafe
  # or ambiguous (symlinked build dir, symlinked/non-regular cache, absent
  # or duplicate/malformed keys, unowned dir, deletion overlapping the
  # source) is left alone: never a broad cleanup, fail closed on real errors.
  local canon_source="" canon_build="" cache_file=""
  if [[ -L "$build_dir" ]]; then
    echo "error: refusing to use symlinked build directory: $build_dir" >&2
    exit 1
  fi
  canon_source="$(canonicalize "$source_dir")" || exit 1
  canon_build="$(canonicalize "$build_dir")" || exit 1
  if [[ "$canon_build" == "/" ]]; then
    echo "error: refusing to clean unsafe build directory: $build_dir" >&2
    exit 1
  fi
  if [[ "$canon_build" == "$canon_source" || "$canon_source" == "$canon_build"/* ]]; then
    echo "error: refusing to delete build directory overlapping source (source $canon_source, build $canon_build)" >&2
    exit 1
  fi
  cache_file="$build_dir/CMakeCache.txt"
  if [[ -e "$cache_file" || -L "$cache_file" ]]; then
    if [[ -L "$cache_file" || ! -f "$cache_file" ]]; then
      : # Symlinked/non-regular cache is never trusted: stay incremental.
    elif [[ ! -O "$build_dir" ]]; then
      echo "error: build directory not owned by current user, refusing automatic cleanup: $build_dir" >&2
      exit 1
    else
      local home_n="" dir_n="" home_val="" dir_val="" home_norm="" dir_norm="" stale=0 ambiguous=0
      home_n="$(grep -c '^CMAKE_HOME_DIRECTORY:[A-Za-z_][A-Za-z0-9_]*=' -- "$cache_file" 2>/dev/null || true)"
      dir_n="$(grep -c '^CMAKE_CACHEFILE_DIR:[A-Za-z_][A-Za-z0-9_]*=' -- "$cache_file" 2>/dev/null || true)"
      [[ "$home_n" =~ ^[0-9]+$ ]] || home_n="2"
      [[ "$dir_n" =~ ^[0-9]+$ ]] || dir_n="2"
      # Duplicate keys or empty values are ambiguous: stay incremental.
      if [[ "$home_n" != "0" && "$home_n" != "1" ]]; then
        ambiguous=1
      elif [[ "$home_n" == "1" ]]; then
        home_val="$(sed -n 's/^CMAKE_HOME_DIRECTORY:[A-Za-z_][A-Za-z0-9_]*=//p' -- "$cache_file")"
        if [[ -z "$home_val" ]]; then
          ambiguous=1
        else
          home_norm="${home_val%/}"; [[ -n "$home_norm" ]] || home_norm="/"
        fi
      fi
      if [[ "$dir_n" != "0" && "$dir_n" != "1" ]]; then
        ambiguous=1
      elif [[ "$dir_n" == "1" ]]; then
        dir_val="$(sed -n 's/^CMAKE_CACHEFILE_DIR:[A-Za-z_][A-Za-z0-9_]*=//p' -- "$cache_file")"
        if [[ -z "$dir_val" ]]; then
          ambiguous=1
        else
          dir_norm="${dir_val%/}"; [[ -n "$dir_norm" ]] || dir_norm="/"
        fi
      fi
      if [[ "$ambiguous" == "0" && ( "$home_n" == "1" || "$dir_n" == "1" ) ]]; then
        if [[ "$home_n" == "1" && "$home_norm" != "$canon_source" ]]; then stale=1; fi
        if [[ "$dir_n" == "1" && "$dir_norm" != "$canon_build" ]]; then stale=1; fi
        if [[ "$stale" == "1" ]]; then
          echo "notice: stale CMake cache records a different source/build directory; recreating fresh build dir: $build_dir"
          if [[ -L "$build_dir" ]]; then
            echo "error: build directory became a symlink during setup: $build_dir; refusing" >&2
            exit 1
          fi
          rm -rf -- "$build_dir" || { echo "error: could not remove stale build directory: $build_dir" >&2; exit 1; }
          if [[ -L "$build_dir" || -e "$build_dir" ]]; then
            echo "error: stale build directory reappeared during cleanup: $build_dir; refusing" >&2
            exit 1
          fi
          mkdir -p -- "$build_dir" || { echo "error: could not recreate build directory: $build_dir" >&2; exit 1; }
        fi
      fi
    fi
  fi

  # Keying note for diagnostics (identity already validated safe).
  echo "host package: $RES_IDENTITY ($RES_DRV)" >&2
  echo "dev output (metadata-selected): $RES_DEV_OUT" >&2

  # Realize the exact selected output before entering the original host dev
  # shell. `nix develop <drv>` alone does not necessarily realize split outputs.
  if ! env -u OMNITILER_KWIN_DEV_CMAKE_DIR -u DOGFOOD_KWIN_DEV_CMAKE_DIR -u CMAKE_BIN -u CARGO_BIN "$nix_bin" build "$RES_DRV^dev" --no-link; then
    echo "error: could not realize exact dev output: $RES_DEV_OUT (nix build $RES_DRV^dev failed); refusing (no fallback attempted)" >&2
    exit 1
  fi

  # Build inside the ORIGINAL host derivation dev shell. Valid Nix CLI:
  # `nix develop <drv> --command <cmd> <args...>`. Strip the legacy pinned
  # env (and any outer CMAKE_BIN/CARGO_BIN override) from the outer
  # environment and again inside the shell; cmake resolves from the host dev
  # shell itself and receives only the resolved -DKWin_DIR. Cargo/rustc
  # resolve from the explicit injected /nix/store bin dir(s). KWinConfig is
  # required inside after the exact dev output has been realized. No
  # toolchain channel/target change: no RUSTUP_TOOLCHAIN, no --target.
  if ! env -u OMNITILER_KWIN_DEV_CMAKE_DIR -u DOGFOOD_KWIN_DEV_CMAKE_DIR -u CMAKE_BIN -u CARGO_BIN \
    "$nix_bin" develop "$RES_DRV" --command bash -c '
      set -euo pipefail
      injected="$1"; src="$2"; bdir="$3"; kwin_dir="$4"
      export PATH="$injected:${PATH:-}"
      unset OMNITILER_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
      unset DOGFOOD_KWIN_DEV_CMAKE_DIR 2>/dev/null || true
      unset CMAKE_BIN 2>/dev/null || true
      unset CARGO_BIN 2>/dev/null || true
      under_injected() {
        local p="$1" d
        local IFS=":"
        for d in $injected; do
          case "$p" in "$d"/*) return 0 ;; esac
        done
        return 1
      }
      if [[ -n "${OMNITILER_KWIN_DEV_CMAKE_DIR:-}" ]]; then
        echo "error: pinned OMNITILER_KWIN_DEV_CMAKE_DIR leaked into host dev shell; refusing" >&2
        exit 1
      fi
      if [[ -n "${DOGFOOD_KWIN_DEV_CMAKE_DIR:-}" ]]; then
        echo "error: pinned DOGFOOD_KWIN_DEV_CMAKE_DIR leaked into host dev shell; refusing" >&2
        exit 1
      fi
      if [[ -n "${CMAKE_BIN:-}" ]]; then
        echo "error: pinned CMAKE_BIN leaked into host dev shell; refusing (cmake must come from nix develop)" >&2
        exit 1
      fi
      if [[ -n "${CARGO_BIN:-}" ]]; then
        echo "error: pinned CARGO_BIN leaked into host dev shell; refusing (cargo must resolve from explicit injected PATH, not an override)" >&2
        exit 1
      fi
      if [[ ! -f "$kwin_dir/KWinConfig.cmake" ]]; then
        echo "error: KWinConfig.cmake not found in exact dev output inside host dev shell: $kwin_dir/KWinConfig.cmake; refusing" >&2
        exit 1
      fi
      cmake_path="$(command -v cmake 2>/dev/null || true)"
      if [[ -z "$cmake_path" ]]; then
        echo "error: cmake not found in host dev shell (nix develop $kwin_dir); refusing (no outer cmake fallback)" >&2
        exit 1
      fi
      cmake_real="$(readlink -f -- "$cmake_path" 2>/dev/null || printf "%s" "$cmake_path")"
      if under_injected "$cmake_real"; then
        echo "error: inner cmake resolves to injected path ($cmake_real); refusing (cmake must be host-native from nix develop)" >&2
        exit 1
      fi
      cargo_path="$(command -v cargo 2>/dev/null || true)"
      if [[ -z "$cargo_path" ]]; then
        echo "error: cargo not found even with explicit injected path; refusing (enter the project dev shell so cargo resolves to /nix/store)" >&2
        exit 1
      fi
      cargo_real="$(readlink -f -- "$cargo_path" 2>/dev/null || printf "%s" "$cargo_path")"
      if ! under_injected "$cargo_real"; then
        echo "error: cargo does not resolve from explicit injected path ($cargo_real not under $injected); refusing" >&2
        exit 1
      fi
      rustc_path="$(command -v rustc 2>/dev/null || true)"
      if [[ -z "$rustc_path" ]]; then
        echo "error: rustc not found even with explicit injected path; refusing" >&2
        exit 1
      fi
      rustc_real="$(readlink -f -- "$rustc_path" 2>/dev/null || printf "%s" "$rustc_path")"
      if ! under_injected "$rustc_real"; then
        echo "error: rustc does not resolve from explicit injected path ($rustc_real not under $injected); refusing" >&2
        exit 1
      fi
      cmake -S "$src" -B "$bdir" -DKWin_DIR="$kwin_dir" -DBUILD_TESTING=OFF
      cmake --build "$bdir"
    ' _ "$injected_path" "$source_dir" "$build_dir" "$RES_KWIN_CMAKE_DIR"; then
    echo "error: host-matched native build failed (nix develop $RES_DRV); no fallback attempted" >&2
    exit 1
  fi
  echo "built: $build_dir (KWin_DIR=$RES_KWIN_CMAKE_DIR)"
}

if [[ $# -eq 0 ]]; then
  echo "error: missing command (resolve, build)" >&2
  usage >&2
  exit 1
fi

case "${1:-}" in
  --help|-h)
    [[ $# -eq 1 ]] || { echo "error: '--help' takes no arguments" >&2; exit 1; }
    usage
    exit 0
    ;;
  resolve) cmd_resolve "${@:2}" ;;
  build) cmd_build "${@:2}" ;;
  *)
    echo "error: unknown command '$1'" >&2
    usage >&2
    exit 1
    ;;
esac

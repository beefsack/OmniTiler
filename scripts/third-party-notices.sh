#!/usr/bin/env bash
# Regenerate or freshness-check the committed THIRD-PARTY-NOTICES union file.
#
#   third-party-notices.sh regenerate  - fetch locked sources (network),
#                                        render + harvested-text,
#                                        completeness, and supplemental
#                                        checks, then replace
#                                        THIRD-PARTY-NOTICES
#   third-party-notices.sh check       - render + harvested-text,
#                                        completeness, and supplemental
#                                        checks into a scratch
#                                        path and compare against the committed
#                                        file; never updates it, never uses
#                                        the network (sources must already be
#                                        fetched, e.g. via `cargo fetch
#                                        --locked`, which CI runs first)
#
# Generation is cargo-about reading local license files only (locked
# Cargo.lock, default features, Linux x86_64 + aarch64 targets from
# about.toml, --frozen so no network and no lockfile change). The
# harvested-text check fails if any included third-party crate has no
# harvested license-file text, so a dependency that would render as generic
# synthesized text fails loudly instead of going missing. The completeness
# check asserts every expected third-party crate name@version (union of the
# normal dependencies of the `omnitiler` and `tiler-kwin-effect-ffi` roots for
# both Linux targets, minus workspace members) appears in the rendered output.
# The supplemental step then appends verbatim top-level upstream
# COPYRIGHT*/NOTICE* files for exactly those expected crates (crate source
# dirs from `cargo metadata --frozen`, deterministic order, local registry
# paths stripped), followed by the END marker, so the marker stays last.
# Scratch lives under $TMPDIR (plain mktemp); callers that want an isolated
# location export TMPDIR explicitly.
set -euo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
NOTICES_FILE="$REPO_ROOT/THIRD-PARTY-NOTICES"
ABOUT_CONFIG="$REPO_ROOT/about.toml"
ABOUT_TEMPLATE="$REPO_ROOT/about.hbs"

SHIPPING_ROOTS="omnitiler tiler-kwin-effect-ffi"
LINUX_TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu"
WORKSPACE_MEMBERS="omnitiler tiler-core tiler-protocol tiler-kwin-effect-ffi tiler-windows tiler-macos"

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/third-party-notices.XXXXXX")"
trap 'rm -rf -- "$SCRATCH"' EXIT
export CARGO_TARGET_DIR="$SCRATCH/cargo-target"

fetch_sources() {
  command -v cargo >/dev/null 2>&1 || { echo "error: required tool 'cargo' not found in PATH; refusing" >&2; exit 1; }
  cargo fetch --locked --manifest-path "$REPO_ROOT/Cargo.toml" \
    || { echo "error: cargo fetch --locked failed; refusing" >&2; exit 1; }
}

render_notices() {
  local out="$1"
  command -v cargo-about >/dev/null 2>&1 || { echo "error: required tool 'cargo-about' not found in PATH; refusing" >&2; exit 1; }
  [[ -f "$ABOUT_CONFIG" ]] || { echo "error: cargo-about config missing: $ABOUT_CONFIG" >&2; exit 1; }
  [[ -f "$ABOUT_TEMPLATE" ]] || { echo "error: notices template missing: $ABOUT_TEMPLATE" >&2; exit 1; }
  cargo about generate \
    --manifest-path "$REPO_ROOT/Cargo.toml" \
    --workspace \
    --frozen \
    -c "$ABOUT_CONFIG" \
    -o "$out" \
    "$ABOUT_TEMPLATE" || { echo "error: cargo about generate failed; refusing" >&2; exit 1; }
  [[ -s "$out" ]] || { echo "error: generated notices file is empty: $out" >&2; exit 1; }
}

check_harvested() {
  local json_out="$SCRATCH/licenses.json"
  local missing="$SCRATCH/unharvested.txt"
  command -v jq >/dev/null 2>&1 || { echo "error: required tool 'jq' not found in PATH; refusing" >&2; exit 1; }
  cargo about generate \
    --manifest-path "$REPO_ROOT/Cargo.toml" \
    --workspace \
    --frozen \
    -c "$ABOUT_CONFIG" \
    --format json \
    > "$json_out" || { echo "error: cargo about generate (json) failed; refusing" >&2; exit 1; }
  jq -r '[.licenses[] | select(.source_path == null) | .used_by[].crate | select(.source != null) | "\(.name) \(.version)"] | unique | .[]' "$json_out" > "$missing" \
    || { echo "error: could not parse license inventory; refusing" >&2; exit 1; }
  if [[ -s "$missing" ]]; then
    local nv
    while IFS= read -r nv; do
      [[ -n "$nv" ]] || continue
      echo "error: third-party crate has no harvested license-file text: $nv" >&2
    done < "$missing"
    echo "error: harvested-text check failed; refusing" >&2
    exit 1
  fi
  echo "harvested-text: every included third-party crate has harvested license-file text"
}

check_completeness() {
  local rendered="$1"
  local expected="$SCRATCH/expected-names.txt"
  local tree_out="$SCRATCH/tree.txt"
  : > "$expected"
  local root target line name version
  for root in $SHIPPING_ROOTS; do
    for target in $LINUX_TARGETS; do
      cargo tree --manifest-path "$REPO_ROOT/Cargo.toml" -p "$root" --edges normal --prefix none --target "$target" --locked --offline \
        > "$tree_out" || { echo "error: cargo tree failed for $root $target; refusing" >&2; exit 1; }
      [[ -s "$tree_out" ]] || { echo "error: cargo tree produced no output for $root $target; refusing" >&2; exit 1; }
      while IFS= read -r line; do
        name="${line%% *}"
        version="${line##* }"
        case " $WORKSPACE_MEMBERS " in
          *" $name "*) continue ;;
        esac
        printf '%s %s\n' "$name" "$version" >> "$expected"
      done < <(sed -e 's/ ([^)]*)//' -e 's/ v/ /' "$tree_out" | awk '{print $1" "$2}' | sort -u)
    done
  done
  sort -uo "$expected" "$expected"
  [[ -s "$expected" ]] || { echo "error: expected-crate list is empty; refusing" >&2; exit 1; }
  local missing=0 nv
  while IFS= read -r nv; do
    [[ -n "$nv" ]] || continue
    if ! grep -Fq -- "- $nv " "$rendered"; then
      echo "error: expected third-party crate missing from notices: $nv" >&2
      missing=1
    fi
  done < "$expected"
  [[ "$missing" -eq 0 ]] || { echo "error: completeness check failed; refusing" >&2; exit 1; }
  echo "completeness: $(wc -l < "$expected") expected third-party name@version entries all present"
}

append_supplemental() {
  local rendered="$1"
  local expected="$SCRATCH/expected-names.txt"
  local metadata="$SCRATCH/metadata.json"
  local pkgmap="$SCRATCH/pkg-dirs.txt"
  local entries="$SCRATCH/supplemental-entries.txt"
  [[ -f "$expected" ]] || { echo "error: expected-crate list missing (run completeness first): $expected" >&2; exit 1; }
  command -v cargo >/dev/null 2>&1 || { echo "error: required tool 'cargo' not found in PATH; refusing" >&2; exit 1; }
  command -v jq >/dev/null 2>&1 || { echo "error: required tool 'jq' not found in PATH; refusing" >&2; exit 1; }
  cargo metadata --frozen --format-version 1 --manifest-path "$REPO_ROOT/Cargo.toml" \
    > "$metadata" || { echo "error: cargo metadata failed; refusing" >&2; exit 1; }
  jq -r '.packages[] | "\(.name)\t\(.version)\t\(.manifest_path | rtrimstr("/Cargo.toml"))"' "$metadata" \
    > "$pkgmap" || { echo "error: could not parse package metadata; refusing" >&2; exit 1; }
  : > "$entries"
  local key name version dir src base
  while IFS= read -r key; do
    [[ -n "$key" ]] || continue
    name="${key%% *}"
    version="${key#* }"
    dir="$(awk -F '\t' -v n="$name" -v v="$version" '$1 == n && $2 == v { print $3; exit }' "$pkgmap")"
    [[ -n "$dir" ]] || { echo "error: no metadata source dir for third-party crate: $key" >&2; exit 1; }
    shopt -s nullglob
    for src in "$dir"/COPYRIGHT* "$dir"/NOTICE*; do
      [[ -f "$src" && ! -L "$src" ]] || continue
      base="$(basename -- "$src")"
      printf '%s\t%s\t%s\n' "$key" "$base" "$src" >> "$entries"
    done
    shopt -u nullglob
  done < "$expected"
  sort -o "$entries" "$entries"
  if [[ -s "$entries" ]]; then
    {
      printf '%s\n' "--------------------------------------------------------------------------------"
      printf 'SUPPLEMENTARY UPSTREAM NOTICES\n'
      printf 'The blocks below reproduce verbatim the top-level COPYRIGHT* and NOTICE*\n'
      printf 'files shipped inside the listed in-scope crates. Each block is labelled\n'
      printf 'with the crate name/version and the file name; local paths are stripped.\n'
    } >> "$rendered"
    local key2 base2 src2
    while IFS=$'\t' read -r key2 base2 src2; do
      [[ -n "${key2:-}" && -n "${base2:-}" && -n "${src2:-}" ]] || continue
      {
        printf '%s\n' "--------------------------------------------------------------------------------"
        printf 'SUPPLEMENTARY NOTICE: %s (%s)\n' "$key2" "$base2"
        printf 'FULL TEXT:\n'
      } >> "$rendered"
      cat -- "$src2" >> "$rendered"
      if [[ -n "$(tail -c 1 -- "$src2")" ]]; then
        printf '\n' >> "$rendered"
      fi
    done < "$entries"
  fi
  {
    printf '%s\n' "--------------------------------------------------------------------------------"
    printf 'END OF THIRD-PARTY NOTICES\n'
  } >> "$rendered"
  echo "supplemental: $(wc -l < "$entries") upstream COPYRIGHT/NOTICE files appended"
}

cmd="${1:-}"
case "$cmd" in
  regenerate)
    fetch_sources
    render_notices "$SCRATCH/notices"
    check_harvested
    check_completeness "$SCRATCH/notices"
    append_supplemental "$SCRATCH/notices"
    mv -- "$SCRATCH/notices" "$NOTICES_FILE"
    trap - EXIT
    rm -rf -- "$SCRATCH"
    echo "regenerated: $NOTICES_FILE"
    ;;
  check)
    [[ -f "$NOTICES_FILE" ]] || { echo "error: committed notices file missing: $NOTICES_FILE" >&2; exit 1; }
    render_notices "$SCRATCH/notices"
    check_harvested
    check_completeness "$SCRATCH/notices"
    append_supplemental "$SCRATCH/notices"
    if cmp -s -- "$SCRATCH/notices" "$NOTICES_FILE"; then
      echo "check-third-party-notices: committed THIRD-PARTY-NOTICES is fresh"
    else
      echo "error: THIRD-PARTY-NOTICES is stale; run 'just third-party-notices' and commit the result" >&2
      diff --unified=3 -- "$NOTICES_FILE" "$SCRATCH/notices" | head -n 60 || true
      exit 1
    fi
    ;;
  *)
    echo "usage: $(basename -- "$0") {regenerate|check}" >&2
    exit 2
    ;;
esac

#!/usr/bin/env bash
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
GUARD="$REPO_ROOT/scripts/kwin-66-api-guard.sh"
OUTPUT="$(mktemp)"
WORK="$(mktemp -d)"
PASS=0
FAIL=0

cleanup() {
  rm -f "$OUTPUT"
  rm -rf "$WORK"
}
trap cleanup EXIT

pass() {
  PASS=$((PASS + 1))
}

fail() {
  echo "FAIL [$1]: $2" >&2
  FAIL=$((FAIL + 1))
}

# ---- Part 1: the guard passes on the current tree ----
if bash "$GUARD" >"$OUTPUT" 2>&1; then
  pass
else
  fail "clean-tree" "guard failed on the current tree"
  cat "$OUTPUT" >&2
fi
if grep -Fq "kwin-66-api-guard: clean" "$OUTPUT"; then
  pass
else
  fail "clean-marker" "missing clean marker"
  cat "$OUTPUT" >&2
fi

# ---- Part 2: direct 6.7-only use outside the seam is caught ----
mkdir -p "$WORK/src"
cp "$REPO_ROOT/kwin/src/"*.ts "$WORK/src/"
printf '\nconst leak = readProp(surface, "currentDesktopForScreen");\n' >> "$WORK/src/plan-adapter-entry.ts"
if bash "$GUARD" "$WORK/src" >"$OUTPUT" 2>&1; then
  fail "confinement-negative" "guard missed an unseamed 6.7-only read"
else
  pass
fi
if grep -Fq "6.7-only API outside workspace-native.ts" "$OUTPUT"; then
  pass
else
  fail "confinement-message" "missing confinement violation message"
  cat "$OUTPUT" >&2
fi

# ---- Part 3: a brand-new API symbol is caught until audited ----
mkdir -p "$WORK/src2"
cp "$REPO_ROOT/kwin/src/"*.ts "$WORK/src2/"
printf '\nconst grid = readProp(surface, "desktopGridSize");\n' >> "$WORK/src2/plan-adapter-entry.ts"
if bash "$GUARD" "$WORK/src2" >"$OUTPUT" 2>&1; then
  fail "allowlist-negative" "guard missed a new unaudited API symbol"
else
  pass
fi
if grep -Fq "unaudited API symbol: desktopGridSize" "$OUTPUT"; then
  pass
else
  fail "allowlist-message" "missing allowlist violation message"
  cat "$OUTPUT" >&2
fi

echo "kwin-66-api-guard: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]]

#!/usr/bin/env bash
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SCANNER="$REPO_ROOT/scripts/check-portable-scan.sh"
WORK="$(mktemp -d)"
PASS=0
FAIL=0

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

pass() { PASS=$((PASS + 1)); }
fail() { echo "FAIL [$1]: $2" >&2; FAIL=$((FAIL + 1)); }

# Comment-only doc prose mentioning cfg(windows) must pass.
mkdir -p "$WORK/comment/src"
printf '%s\n' '//! `admission_clear_eligible` is private and `cfg(windows)`-gated, so it is' '/// itself, never here. The private `cfg(windows)` Windows function of the' 'pub fn ok() {}' > "$WORK/comment/src/a.rs"
if bash "$SCANNER" "$WORK/comment/src" >/dev/null 2>&1; then
  pass
else
  fail "comment-fixture" "comment-only cfg(windows) prose scanned as a leak"
fi

# Real platform-token code must fail.
mkdir -p "$WORK/leak/src"
printf '%s\n' '#[cfg(windows)]' 'pub fn bad() {}' > "$WORK/leak/src/a.rs"
if bash "$SCANNER" "$WORK/leak/src" >/dev/null 2>&1; then
  fail "code-fixture" "real #[cfg(windows)] code passed the scan"
else
  pass
fi

# Current tree must pass (no real leaks in tiler-core).
if bash "$SCANNER" "$REPO_ROOT/crates/tiler-core/src" >/dev/null 2>&1; then
  pass
else
  fail "clean-tree" "scanner failed on crates/tiler-core/src"
fi

# An unreadable scan target must never pass as an empty scan.
if bash "$SCANNER" "$WORK/missing" >/dev/null 2>&1; then
  fail "scan-error" "missing scan target passed"
else
  pass
fi

echo "check-portable-scan: pass=$PASS fail=$FAIL"
[[ "$FAIL" -eq 0 ]]

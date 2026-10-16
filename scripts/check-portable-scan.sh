#!/usr/bin/env bash
# Portable-leak scanner for crates/tiler-core/src.
# Excludes comment-only lines (optional whitespace then `//`, covering
# `//`, `///`, `//!`) so doc prose mentioning `cfg(windows)` does not
# false-positive, while real code leaks (e.g. `#[cfg(windows)]`) still fail.
# Usage: check-portable-scan.sh <dir>
set -euo pipefail
DIR="${1:?usage: check-portable-scan.sh <dir>}"
if MATCHED="$(grep -rEn --include='*.rs' 'std::os::|std::process::|/proc|libc|rustix|zbus|extern "C"|cfg[[:space:]]*\([[:space:]]*(unix|windows|target_os)|target_os' "$DIR")"; then
  :
else
  status=$?
  [[ "$status" -eq 1 ]] || exit "$status"
fi
if [[ -z "$MATCHED" ]]; then
  exit 0
fi
# Drop comment-only lines: `file:line:` prefix then optional space then `//`.
FILTERED="$(printf '%s\n' "$MATCHED" | grep -vE ':[0-9]+:[[:space:]]*//' || true)"
if [[ -z "$FILTERED" ]]; then
  exit 0
fi
printf '%s\n' "$FILTERED"
exit 1

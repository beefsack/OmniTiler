#!/usr/bin/env bash
# Bounded LaunchServices launch for the scoped-tile dev session.
#
#   scripts/macos-tile.sh --user-start --scope com.apple.TextEdit --seconds 30
#   scripts/macos-tile.sh --user-start --scope ... --seconds 30 --input tap
#
# Route: `scripts/macos-launch.sh --timeout T -- tile "$@"` (fixed signed
# bundle, `open -n --args`, ephemeral-output polling). No default implicit
# scope or start flag is added here: `--user-start` plus a non-empty
# `--scope` stay mandatory and are validated inside the tile entry (exit 2
# without them). `--timeout` always covers the tile's own bounded lifetime:
# `--seconds` (default 30, max 120) plus 60 s of shutdown/drain slack,
# capped at the wrapper max of 300. See `scripts/macos-launch.sh` for the
# honest-status limit (the JSON payload is the outcome, never our exit
# code alone).
set -euo pipefail

SECONDS_WANT=30
prev=""
for arg in "$@"; do
  if [[ "$prev" == "--seconds" ]]; then
    SECONDS_WANT="$arg"
  fi
  prev="$arg"
done

[[ "$SECONDS_WANT" =~ ^[0-9]+$ ]] || {
  echo "error: --seconds must be a positive integer (got '$SECONDS_WANT')" >&2
  exit 2
}
[[ "$SECONDS_WANT" -ge 1 && "$SECONDS_WANT" -le 120 ]] || {
  echo "error: --seconds out of bound 1..=120 (got $SECONDS_WANT)" >&2
  exit 2
}

TIMEOUT_SECS=$((SECONDS_WANT + 60))
[[ "$TIMEOUT_SECS" -le 300 ]] || TIMEOUT_SECS=300

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$SCRIPT_DIR/macos-launch.sh" --timeout "$TIMEOUT_SECS" -- tile "$@"

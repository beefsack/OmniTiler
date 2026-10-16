#!/usr/bin/env bash
# TCC-trusted LaunchServices launch for the Phase 0 macOS probe.
#
# A directly executed `tiler-macos` binary (cargo run, or the executable path
# inside the .app) runs under the terminal's responsible-process attribution,
# so `AXIsProcessTrusted` falsely reports false even with the bundle granted
# in System Settings. Every TCC-dependent probe must instead be launched
# through LaunchServices so the window server attributes it to the signed
# bundle identity (`io.github.omnitiler.macos.probe`):
#
#   scripts/macos-launch.sh -- preflight
#   scripts/macos-launch.sh --timeout 60 -- spaces-session --seconds 20
#   scripts/macos-launch.sh --timeout 120 --stream -- input-session --route tap --seconds 90 ...
#
# `--stream` passes the bundle's stderr through live while polling (physical
# press-now prompts surface in real time); stdout still prints once at
# terminal completion so the JSON payload stays intact, and the closing
# stderr section carries only lines not already streamed (never duplicated).
#
# Route: `open -n -o <ephemeral-out> --stderr <ephemeral-err> <fixed .app>
# --args <probe args...>`, then poll the ephemeral outputs under a bounded
# timeout. `open -W` is deliberately NOT used: it fails with "Unable to block
# on application" for this bundle. Completion is output stability (~1s
# size+checksum-identical) observed AFTER no process from this launch remains
# (so a holder's early `ready` line never counts as done); the timeout bounds
# everything. Probe failures surface fast (stderr stable, no process left)
# with a nonzero exit; a timeout preserves whatever partial output exists and
# fails — never claims clean or complete. `--timeout` must cover the probe's
# own bounded runtime (e.g. `--timeout 120 -- probe-spawn --seconds 90`).
#
# HONEST-STATUS LIMIT: `open` reports only its own launch status, never the
# probe's exit code, so this wrapper CANNOT prove the command's exit status.
# exit 0 means exactly "terminal stdout observed, stderr quiet, launch-spawned
# processes quiesced" — never "probe exited 0". Callers must read the JSON
# payload (verified_exit/restored/effect_verified fields) as the outcome, not
# this wrapper's exit code; anything else (timeout, stderr content, partial
# output) exits nonzero and must never be accepted as success evidence.
#
# Default `preflight` via plain `cargo run` stays nonmutating and keeps
# reporting the terminal attribution (ax_trusted:false); that false is
# expected there, not a consent failure. Use this script (or `just launch`)
# whenever the bundle's own grant is what is under test.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
APP="$REPO_ROOT/target/macos/OmniTilerProbe.app"
BUNDLE_ID="io.github.omnitiler.macos.probe"
MARKER_REL="Contents/Resources/.omnitiler-probe-marker"
TIMEOUT_SECS=30
STREAM=0

usage() {
  echo "usage: $(basename -- "$0") [--timeout SECS] [--stream] -- <probe args...>" >&2
  exit 2
}

fail() { echo "error: $*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --timeout)
      # A missing value is a usage error (exit 2), never an abrupt shift
      # failure: `$# -ge 2` means the flag plus its value are both present.
      [[ $# -ge 2 ]] || usage
      TIMEOUT_SECS="$2"; shift 2 ;;
    --stream)
      STREAM=1; shift ;;
    # A bare `--` only separates wrapper layers (e.g. just forwards its own
    # `--` into the recipe args): skip it and keep parsing script flags, so
    # both `launch -- preflight` and `launch -- --timeout N -- <probe>` work.
    --) shift ;;
    -h|--help) usage ;;
    --*) echo "error: unknown arg '$1' (probe args go after --)" >&2; usage ;;
    *) break ;; # probe subcommand (never flag-like): rest are probe args.
  esac
done

[[ "$TIMEOUT_SECS" =~ ^[0-9]+$ ]] || fail "--timeout must be a positive integer (got '$TIMEOUT_SECS')"
[[ "$TIMEOUT_SECS" -ge 1 && "$TIMEOUT_SECS" -le 300 ]] \
  || fail "--timeout out of bound 1..=300 (got $TIMEOUT_SECS)"
[[ $# -ge 1 ]] || usage
[[ "$(uname -s)" == "Darwin" ]] || fail "macOS only (uname=$(uname -s))"

# Fixed-bundle guard: only the assembled owned bundle may be launched.
[[ -d "$APP" ]] || fail "app missing: $APP (build via: just app 'NAME')"
[[ ! -L "$APP" ]] || fail "app path is a symlink: $APP"
[[ -f "$APP/$MARKER_REL" && ! -L "$APP/$MARKER_REL" ]] \
  || fail "bundle lacks owned marker ($APP/$MARKER_REL); refusing foreign bundle"
grep -Fqx "$BUNDLE_ID" "$APP/$MARKER_REL" \
  || fail "bundle marker mismatch ($APP/$MARKER_REL); refusing foreign bundle"
[[ -x "$APP/Contents/MacOS/tiler-macos" && ! -L "$APP/Contents/MacOS/tiler-macos" ]] \
  || fail "bundle executable missing or symlinked"

OUT="$(mktemp "${TMPDIR:-/tmp}/omnitiler-ls-out.XXXXXX")"
ERR="$(mktemp "${TMPDIR:-/tmp}/omnitiler-ls-err.XXXXXX")"
PIDS_BEFORE="$(mktemp "${TMPDIR:-/tmp}/omnitiler-ls-pids.XXXXXX")"
PIDS_NOW="$(mktemp "${TMPDIR:-/tmp}/omnitiler-ls-pids.XXXXXX")"
cleanup() { rm -f -- "$OUT" "$ERR" "$PIDS_BEFORE" "$PIDS_NOW"; }
trap cleanup EXIT

# Snapshot unrelated bundle processes BEFORE launch: `open` reports no PID,
# so completion waits only for processes this launch started (current set
# minus the before set). A foreign bundle instance starting mid-run is
# conservatively treated as ours (extra waiting, still timeout-bounded).
pgrep -f "OmniTilerProbe\.app/Contents/MacOS" >"$PIDS_BEFORE" 2>/dev/null || true

# New LS instance per invocation; never reuses (or disturbs) a running one.
# `-o/--stderr` redirect the bundle's stdout/stderr to the ephemeral files.
open -n -o "$OUT" --stderr "$ERR" "$APP" --args "$@" \
  || fail "open failed for $APP"

# Poll for completion: either stream stable (~1s size+checksum-identical
# across 0.2s polls — checksum, not just size, so a same-size rewrite never
# counts as stable) AND no process from THIS launch remains (a holder's early
# `ready` line never counts while its processes still run), or the timeout.
# Only PIDs absent from the pre-launch snapshot belong to this launch;
# anything else (an earlier holder still running) never delays us. `pgrep`
# absence alone never succeeds an empty result.
our_procs_running() {
  pgrep -f "OmniTilerProbe\.app/Contents/MacOS" >"$PIDS_NOW" 2>/dev/null || true
  # Lines in NOW absent from BEFORE are this launch's processes. (With an
  # empty BEFORE set, `grep -v` passes everything through: any live bundle
  # process then counts as ours, which is the conservative reading.)
  grep -vxFf "$PIDS_BEFORE" "$PIDS_NOW" 2>/dev/null | grep -q .
}

bundle_procs_running() {
  our_procs_running
}

# Print ERR bytes $1..$2 (both 1-based, $2 inclusive) to our stderr. The
# head bound snapshots the read: bytes appended after the caller measured
# the size are left for the next poll, so a racing append can neither slip
# into this print (which would duplicate it next poll) nor be skipped.
stream_err_range() {
  head -c "$2" "$ERR" 2>/dev/null | tail -c +"$1" >&2 || true
}

deadline=$((SECONDS + TIMEOUT_SECS))
last_out=""
last_err=""
stable_polls=0
# Bytes of ERR already passed through live under --stream (final section
# prints only the remainder, so streamed lines never appear twice).
streamed_err=0
# Set only by the terminal-completion break below. A timeout exit with
# partial stdout plus dead processes must still fail: without this flag the
# tail branch could mistake stale partial output for a completed run.
completed_terminal=0
while [[ $SECONDS -lt $deadline ]]; do
  # `cksum` covers size AND content: equal checksums mean byte-identical.
  sum_out="$(cksum <"$OUT" 2>/dev/null || echo "0 0")"
  sum_err="$(cksum <"$ERR" 2>/dev/null || echo "0 0")"
  sums="$sum_out/$sum_err"
  if [[ "$sums" == "$last_out/$last_err" ]]; then
    stable_polls=$((stable_polls + 1))
  else
    stable_polls=0
  fi
  last_out="$sum_out"
  last_err="$sum_err"
  # Live stderr under --stream: pass through bytes not yet shown (the
  # physical press-now prompt) with poll latency (~0.2s), well before any
  # bounded await expires. Stdout never streams: the JSON payload prints
  # once at terminal completion only. The read is bounded at the
  # snapshot: bytes appended between the size check and the read stay for
  # the next poll and are never duplicated here.
  if [[ $STREAM -eq 1 && -s "$ERR" ]]; then
    err_bytes="$(wc -c <"$ERR" 2>/dev/null | tr -d ' ' || echo 0)"
    case "$err_bytes" in ''|*[!0-9]*) err_bytes=0 ;; esac
    if [[ "$err_bytes" -gt "$streamed_err" ]]; then
      stream_err_range $((streamed_err + 1)) "$err_bytes"
      streamed_err="$err_bytes"
    fi
  fi
  # Stable content plus no live bundle process: the probe said its last word.
  if [[ $stable_polls -ge 5 ]] && [[ -s "$OUT" || -s "$ERR" ]] \
    && ! bundle_procs_running; then
    completed_terminal=1
    break
  fi
  sleep 0.2
done

# Probe stdout goes to our stdout (callers capture/redirect it); anything on
# stderr is diagnostic context only.
if [[ -s "$OUT" ]]; then
  cat -- "$OUT"
fi
if [[ $STREAM -eq 1 ]]; then
  # Already streamed live above: emit only the tail not yet shown (often
  # empty), so no line ever appears twice. Bounded at the snapshot like
  # the poll read: a concurrent append stays for no one (the probe is
  # quiescent here) and is still printed, never skipped nor duplicated.
  if [[ -s "$ERR" ]]; then
    err_total="$(wc -c <"$ERR" 2>/dev/null | tr -d ' ' || echo 0)"
    case "$err_total" in ''|*[!0-9]*) err_total=0 ;; esac
    if [[ "$err_total" -gt "$streamed_err" ]]; then
      echo "--- probe stderr (remainder) ---" >&2
      stream_err_range $((streamed_err + 1)) "$err_total"
    fi
  fi
elif [[ -s "$ERR" ]]; then
  echo "--- probe stderr ---" >&2
  cat -- "$ERR" >&2
fi

# No terminal completion observed (timeout): partial output above is stale
# evidence at best — always fail, even with stdout content and dead
# processes. See HONEST-STATUS LIMIT above.
if [[ $completed_terminal -eq 0 ]]; then
  fail "no terminal completion within ${TIMEOUT_SECS}s (partial output above at best; outcome unknown, never success evidence)"
fi
# Terminal completion with stdout only: the one shape that may mean success —
# and even then the JSON payload (not this exit code) is the outcome.
if [[ -s "$OUT" && ! -s "$ERR" ]]; then
  exit 0
fi
# Terminal completion with stderr content (alone, or beside stdout): the probe
# reported failure/ambiguity. Stdout beside stderr never upgrades this to
# success — exit nonzero and let the payload/stderr speak.
exit 1

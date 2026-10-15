#!/usr/bin/env bash
# Foreground macOS dogfood launcher (`just dev`).
#
#   just dev [verbose|trace] [tile args...]
#   scripts/macos-dev.sh [verbose|trace] [tile args...]
#   scripts/macos-dev.sh --stop
#
# Default (no --scope): unscoped ordinary-app tiling, unbounded
# (`--seconds 0` unless the caller passes --seconds). `--scope` selects
# the bounded allowlist verification path instead.
#
# Lifecycle (graceful, no signals to the owner): the wrapper creates an
# exact per-run directory and passes it as `tile --run-dir DIR`. The tile
# polls `DIR/stop-requested` every cadence, runs its existing bounded
# shutdown, and publishes its terminal outcome JSON to
# `DIR/outcome.json` (stdout carries the same payload). Interruption
# (Ctrl+C/SIGTERM) and `--stop` publish the stop file; the wrapper then
# waits for the exact run's outcome file. Mutual exclusion is an atomic
# `mkdir` lock directory holding a pointer to the current run directory;
# ownership combines the pointer, the run directory, the `tilestarted`
# nonce, and the owner process identity (executable match plus start-time
# equality, never a bare PID). Stale locks are taken over only after the
# recorded identity verifies dead or foreign; a live owner is never
# adopted, never signalled, and never removed. An empty lock directory is
# never stolen (retry, then fail closed). The lock is retained while the
# identity may be live, unknown, or starting, or the exit is unconfirmed;
# only a verified clean terminal exit clears, and only a provably stale
# (dead) identity permits takeover/removal. Run-directory removals and
# stop writes validate owned containment under the runs directory first
# (never a foreign lock pointer). Caller `--run-dir` is refused (the
# wrapper owns the exact per-run directory). Dirty or missing outcomes
# are reported as such (never success); only a parsed clean terminal
# outcome reports success. JSON parses via jq (mise provides it).
#
# Route: fixed signed bundle through LaunchServices (`open -n --args`,
# output-file polling) — never `open -W`, never the direct binary — so the
# run carries the bundle's TCC grant. Logs stream to the terminal; run,
# log, and outcome paths print at start and end. Windows stay in place on
# stop (never restored).
#
# Offline harness seams (environment only): OMNITILER_DEV_MOCK_OPEN runs
# instead of `open` as `$MOCK_OPEN "$OUT" "$ERR" -- <tile args...>` with
# OMNITILER_DEV_RUN_ID / OMNITILER_DEV_RUN_DIR exported;
# OMNITILER_DEV_STATE_BASE overrides the state base directory;
# OMNITILER_DEV_EXE_PATTERN overrides the owner executable match;
# OMNITILER_DEV_STARTUP_GRACE / OMNITILER_DEV_STOP_WAIT bound the startup
# and post-stop waits. See scripts/macos-dev.test.sh.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
APP="$REPO_ROOT/target/macos/OmniTilerProbe.app"
BUNDLE_ID="io.github.omnitiler.macos.probe"
MARKER_REL="Contents/Resources/.omnitiler-probe-marker"
BUNDLE_EXE="OmniTilerProbe.app/Contents/MacOS/tiler-macos"
BASE="${OMNITILER_DEV_STATE_BASE:-${TMPDIR:-/tmp}/omnitiler-macos-dev}"
LOCK="$BASE/lock"
RUNS="$BASE/runs"
EXE_PATTERN="${OMNITILER_DEV_EXE_PATTERN:-$BUNDLE_EXE}"
STARTUP_GRACE="${OMNITILER_DEV_STARTUP_GRACE:-30}"
STOP_WAIT="${OMNITILER_DEV_STOP_WAIT:-30}"
MOCK_OPEN="${OMNITILER_DEV_MOCK_OPEN:-}"

usage() {
  echo "usage: $(basename -- "$0") [verbose|trace] [tile args...]" >&2
  echo "       $(basename -- "$0") --stop" >&2
  exit 2
}

fail() { echo "error: $*" >&2; exit 1; }

# ---- argument parsing -------------------------------------------------------
MODE="normal"
STOP=0
TILE_ARGS=()
if [[ $# -ge 1 && ("$1" == "-h" || "$1" == "--help" || "$1" == "help") ]]; then
  usage
elif [[ $# -ge 1 && ("$1" == "--stop" || "$1" == "stop") ]]; then
  STOP=1
  shift
  [[ $# -eq 0 ]] || usage
elif [[ $# -ge 1 && "$1" == "--" ]]; then
  shift
  TILE_ARGS=("$@")
elif [[ $# -ge 1 && -z "$1" ]]; then
  shift # empty mode word means normal.
  TILE_ARGS=("$@")
elif [[ $# -ge 1 && ("$1" == "verbose" || "$1" == "trace") ]]; then
  MODE="$1"
  shift
  TILE_ARGS=("$@")
else
  TILE_ARGS=("$@")
fi

# ---- small helpers ------------------------------------------------------------
pid_alive() { kill -0 "$1" 2>/dev/null; }

require_jq() {
  command -v jq >/dev/null 2>&1 \
    || fail "jq not found (mise provides it: run 'mise install' from the repo root)"
}

proc_command() {
  # Full command line of one PID (empty when unreadable).
  ps -o command= -p "$1" 2>/dev/null | head -1
}

proc_start() {
  # Process start time of one PID (empty when unreadable).
  ps -o lstart= -p "$1" 2>/dev/null | head -1
}

parse_field() {
  # One scalar field from JSON text via the declared jq dependency (mise
  # provides it; no python3, no regex fallback). Missing, null, or
  # malformed input yields empty (never a raw-key log, never success).
  local body="$1" field="$2"
  require_jq
  printf '%s' "$body" \
    | jq -r --arg f "$field" '.[$f] // empty' 2>/dev/null || true
  return 0
}

json_field() {
  # One scalar field from a small JSON file.
  local body=""
  body="$(cat -- "$1" 2>/dev/null)" || body=""
  [[ -n "$body" ]] || return 1
  parse_field "$body" "$2"
}

is_clean_shutdown() {
  # is_clean_shutdown <true-ish>: True for either boolean spelling.
  [[ "$1" == "True" || "$1" == "true" ]]
}

terminal_outcome_ready_for() {
  # terminal_outcome_ready_for <run_dir>: true only when that run's
  # outcome file exists AND parses to a terminal outcome token.
  local dir="$1" outcome=""
  [[ -f "$dir/outcome.json" ]] || return 1
  outcome="$(json_field "$dir/outcome.json" outcome)"
  [[ -n "$outcome" ]]
}

terminal_outcome_ready() {
  terminal_outcome_ready_for "$RUN_DIR"
}

# ---- owner identity ------------------------------------------------------------
# owner.json (written by the launcher after observing tilestarted):
#   pid=<tilestarted pid>
#   runid=<tilestarted nonce, must equal our run id>
#   lstart=<ps start time at observation>
# owner_state <run_dir> prints exactly one of:
#   live     fully verified live owner (PID alive now, executable still
#            matches, start time still equal, nonce still names this run)
#   stale    recorded identity provably gone (PID dead, PID alive with a
#            different start time, or a nonce that never named this run:
#            reuse or residue, never this owner)
#   unknown  cannot verify (unreadable record fields, unreadable command
#            line, or a live PID whose start time matches but whose
#            executable does not — ambiguous, never acted on)
#   starting no owner record yet
# A bare PID never decides liveness on its own.
write_owner_record() {
  local run_dir="$1" pid="$2" runid="$3"
  local start
  start="$(proc_start "$pid")"
  {
    printf 'pid=%s\n' "$pid"
    printf 'runid=%s\n' "$runid"
    printf 'lstart=%s\n' "$start"
  } >"$run_dir/owner.json"
}

owner_state() {
  local run_dir="$1" pid="" runid="" lstart="" base="" exe="" now=""
  if [[ ! -f "$run_dir/owner.json" ]]; then
    echo "starting"
    return 0
  fi
  while IFS='=' read -r key value; do
    case "$key" in
      pid) pid="$value" ;;
      runid) runid="$value" ;;
      lstart) lstart="$value" ;;
    esac
  done <"$run_dir/owner.json"
  if [[ -z "$pid" || -z "$runid" || -z "$lstart" ]]; then
    echo "unknown"
    return 0
  fi
  base="$(basename -- "$run_dir")"
  if [[ "$runid" != "$base" ]]; then
    echo "stale"
    return 0
  fi
  if ! pid_alive "$pid"; then
    echo "stale"
    return 0
  fi
  now="$(proc_start "$pid")"
  if [[ -z "$now" || "$now" != "$lstart" ]]; then
    # Dead between the checks, or a reused PID number: provably not the
    # recorded owner either way.
    echo "stale"
    return 0
  fi
  exe="$(proc_command "$pid")"
  if [[ -z "$exe" ]]; then
    echo "unknown"
    return 0
  fi
  case "$exe" in
    *"$EXE_PATTERN"*) echo "live" ;;
    *) echo "unknown" ;;
  esac
  return 0
}

owner_is_live() {
  [[ "$(owner_state "$1")" == "live" ]]
}

# ---- lock ownership --------------------------------------------------------------
# $LOCK is a mkdir-owned directory holding one file `run` with the current
# run directory path. Every destructive step re-reads the pointer and
# touches nothing unless it still names the expected run.
lock_pointer() {
  if [[ -f "$LOCK/run" ]]; then
    cat -- "$LOCK/run" 2>/dev/null || true
  fi
}

valid_run_pointer() {
  # valid_run_pointer <pointer>: true only for an owned per-run directory:
  # canonical containment under $RUNS, expected `run.*` basename, no `..`,
  # and no symlink at the pointer, $RUNS, $BASE, or $LOCK. Gates every
  # `rm -rf` of a pointed-at directory and every stop-file write to one
  # (the pointer file itself is never followed blindly).
  local pointer="$1" base=""
  [[ -n "$pointer" ]] || return 1
  case "$pointer" in
    "$RUNS"/run.*) ;;
    *) return 1 ;;
  esac
  case "$pointer" in
    *".."* ) return 1 ;;
  esac
  base="$(basename -- "$pointer")"
  case "$base" in
    run.*) ;;
    *) return 1 ;;
  esac
  [[ ! -L "$pointer" ]] || return 1
  [[ ! -L "$RUNS" ]] || return 1
  [[ ! -L "$BASE" ]] || return 1
  [[ ! -L "$LOCK" ]] || return 1
  return 0
}

write_stop_for() {
  # write_stop_for <run_dir>: publish `stop-requested` only after
  # validating owned containment and confirming the lock still names this
  # exact run (no write to a moved/foreign pointer).
  local dir="$1" current=""
  valid_run_pointer "$dir" || return 1
  current="$(lock_pointer)"
  [[ "$current" == "$dir" ]] || return 1
  touch -- "$dir/stop-requested" 2>/dev/null || return 1
  return 0
}

remove_run_dir_if_valid() {
  # remove_run_dir_if_valid <run_dir>: `rm -rf` only an owned, validated
  # per-run directory (never a foreign path from a lock pointer).
  local dir="$1"
  valid_run_pointer "$dir" || return 1
  [[ -d "$dir" ]] || return 1
  rm -rf -- "$dir"
}

lock_release_if_ours() {
  # Remove the lock pointer (and the lock dir when empty) only while it
  # still names our run directory. Never removes a newer run.
  local ours="$1" pointer=""
  pointer="$(lock_pointer)"
  if [[ "$pointer" == "$ours" ]]; then
    rm -f -- "$LOCK/run"
    rmdir -- "$LOCK" 2>/dev/null || true
    return 0
  fi
  return 1
}

# ---- --stop (graceful request to the current exact run) ---------------------------
# Never unlocks a live, unknown, or starting owner and never enables an
# overlapping second tiler: the origin clears its own lock only after a
# verified clean exit (stale, provably dead, is the only takeover path).
# Stop writes and run-directory removals validate owned containment first
# (never `rm -rf` a foreign lock pointer).
if [[ $STOP -eq 1 ]]; then
  pointer="$(lock_pointer)"
  if [[ -z "$pointer" ]]; then
    echo "dev-stop: no current run (lock free); nothing to stop"
    exit 0
  fi
  if ! valid_run_pointer "$pointer"; then
    echo "error: dev-stop: refusing: lock pointer is not an owned run directory (retained for inspection)" >&2
    exit 1
  fi
  if [[ ! -d "$pointer" ]]; then
    echo "dev-stop: run directory gone ($pointer); clearing stale pointer"
    lock_release_if_ours "$pointer" || true
    exit 0
  fi
  case "$(owner_state "$pointer")" in
    starting)
      # No owner record yet: the origin may still be launching. A terminal
      # outcome without an owner record still reports; otherwise publish
      # the stop to the exact run directory below and wait like the live
      # path (stop during startup from a second terminal).
      if terminal_outcome_ready_for "$pointer"; then
        body=""
        body="$(cat -- "$pointer/outcome.json" 2>/dev/null)" || body=""
        outcome="$(parse_field "$body" outcome)"
        clean="$(parse_field "$body" shutdown_clean)"
        if is_clean_shutdown "$clean" && [[ "$outcome" == "completed" || "$outcome" == "stopped" ]]; then
          echo "dev-stop: verified stop (outcome=$outcome, shutdown clean; origin clears its state)"
          exit 0
        fi
        echo "error: dev-stop: terminal outcome is not clean (outcome=${outcome:-unknown}; run retained at $pointer)" >&2
        exit 1
      fi
      echo "dev-stop: run is still starting; publishing stop to the exact run directory: $pointer"
      ;;
    unknown)
      echo "error: dev-stop: refusing: cannot verify the recorded owner identity (run $pointer); retry shortly" >&2
      exit 1
      ;;
    live)
      ;;
    stale)
      if terminal_outcome_ready_for "$pointer"; then
        body=""
        body="$(cat -- "$pointer/outcome.json" 2>/dev/null)" || body=""
        outcome="$(parse_field "$body" outcome)"
        clean="$(parse_field "$body" shutdown_clean)"
        if is_clean_shutdown "$clean" && [[ "$outcome" == "completed" || "$outcome" == "stopped" ]]; then
          echo "dev-stop: no live owner; verified terminal outcome (outcome=$outcome, shutdown clean)"
          if lock_release_if_ours "$pointer"; then
            remove_run_dir_if_valid "$pointer" || true
            echo "dev-stop: run state cleared"
          fi
          exit 0
        fi
        echo "error: dev-stop: no live owner; terminal outcome is not clean (outcome=${outcome:-unknown}; run retained at $pointer)" >&2
        lock_release_if_ours "$pointer" || true
        exit 1
      fi
      echo "dev-stop: no live owner and no terminal outcome (run retained at $pointer for inspection)"
      lock_release_if_ours "$pointer" || true
      exit 0
      ;;
  esac
  echo "dev-stop: publishing stop to the exact run directory: $pointer"
  if ! write_stop_for "$pointer"; then
    echo "error: dev-stop: refusing: lock moved or run not owned; retry shortly" >&2
    exit 1
  fi
  deadline=$((SECONDS + STOP_WAIT))
  outcome=""
  clean=""
  while [[ $SECONDS -lt $deadline ]]; do
    if terminal_outcome_ready_for "$pointer"; then
      # Single read: the origin may clear its directory concurrently.
      body=""
      body="$(cat -- "$pointer/outcome.json" 2>/dev/null)" || body=""
      outcome="$(parse_field "$body" outcome)"
      clean="$(parse_field "$body" shutdown_clean)"
      break
    fi
    sleep 0.2
  done
  if [[ -z "$outcome" ]]; then
    echo "error: dev-stop: no terminal outcome within ${STOP_WAIT}s after stop (run retained at $pointer; lock retained while the owner may be live)" >&2
    exit 1
  fi
  if is_clean_shutdown "$clean" && [[ "$outcome" == "completed" || "$outcome" == "stopped" ]]; then
    echo "dev-stop: verified stop (outcome=$outcome, shutdown clean; run retained for the origin to clear; lock retained for the origin)"
    exit 0
  fi
  echo "error: dev-stop: terminal outcome is not clean (outcome=${outcome:-unknown}; run retained at $pointer; lock retained)" >&2
  exit 1
fi

# ---- mode validation ---------------------------------------------------------
case "$MODE" in
  normal | verbose | trace) ;;
  *) echo "error: unknown dev mode '$MODE' (expected '' | verbose | trace)" >&2; exit 2 ;;
esac

# ---- tile arg assembly -------------------------------------------------------
HAS_SCOPE=0
HAS_UNSCOPED=0
HAS_SECONDS=0
HAS_LOG=0
HAS_USER_START=0
for arg in ${TILE_ARGS[@]+"${TILE_ARGS[@]}"}; do
  case "$arg" in
    --scope) HAS_SCOPE=1 ;;
    --unscoped) HAS_UNSCOPED=1 ;;
    --seconds) HAS_SECONDS=1 ;;
    --log) HAS_LOG=1 ;;
    --user-start) HAS_USER_START=1 ;;
    --run-dir|--run-dir=*)
      echo "error: --run-dir is owned by the wrapper (exact per-run directory); caller override refused" >&2
      exit 2
      ;;
  esac
done
if [[ "$HAS_SCOPE" -eq 1 && "$HAS_UNSCOPED" -eq 1 ]]; then
  echo "error: --scope and --unscoped are mutually exclusive (drop --unscoped for the allowlist mode)" >&2
  exit 2
fi
if [[ "$HAS_USER_START" -eq 0 ]]; then
  TILE_ARGS+=("--user-start")
fi
if [[ "$HAS_SCOPE" -eq 0 && "$HAS_UNSCOPED" -eq 0 ]]; then
  TILE_ARGS+=("--unscoped")
  HAS_UNSCOPED=1
fi
if [[ "$HAS_UNSCOPED" -eq 1 && "$HAS_SECONDS" -eq 0 ]]; then
  TILE_ARGS+=("--seconds" "0")
fi
if [[ "$HAS_LOG" -eq 0 && "$MODE" != "normal" ]]; then
  TILE_ARGS+=("--log" "$MODE")
fi

# ---- exact run directory + mutual exclusion -----------------------------------
mkdir -p -- "$RUNS" || fail "cannot create runs directory $RUNS"
RUN_DIR="$(mktemp -d "$RUNS/run.XXXXXX")" || fail "cannot create run directory"
RUN_ID="$(basename -- "$RUN_DIR")"
printf '%s\n' "$RUN_ID" >"$RUN_DIR/run-id"
printf '%s\n' "tile ${TILE_ARGS[*]}" >"$RUN_DIR/tile-args"
TILE_ARGS+=("--run-dir" "$RUN_DIR")

# Interrupt trap BEFORE open and process discovery: publish a stop to this
# exact run directory (idempotent). The owner observes it and runs its
# normal bounded shutdown; nothing is ever signalled.
INTERRUPTED=0
OPEN_DONE=0
LAUNCH_ATTEMPTED=0
on_signal() {
  INTERRUPTED=1
  if [[ -d "$RUN_DIR" ]]; then
    touch -- "$RUN_DIR/stop-requested" 2>/dev/null || true
  fi
}
trap on_signal INT TERM

LOCKED=0
# A refused launch removes its own never-started run directory (nothing
# else can reference it); every refusal below ends there.
drop_own_run_dir() {
  rm -rf -- "$RUN_DIR" 2>/dev/null || true
}
for _ in $(seq 1 5); do
  if mkdir -- "$LOCK" 2>/dev/null; then
    printf '%s\n' "$RUN_DIR" >"$LOCK/run" \
      || { drop_own_run_dir; fail "cannot publish the lock pointer"; }
    LOCKED=1
    break
  fi
  pointer="$(lock_pointer)"
  if [[ -z "$pointer" ]]; then
    # Empty lock directory: another launcher may have just created it and
    # not yet published its pointer. Never steal it with `rmdir`; sleep
    # and retry the atomic gate, failing closed after retries (an orphaned
    # empty lock then refuses with a clear message rather than risking an
    # overlap).
    sleep 0.5
    continue
  fi
  if ! valid_run_pointer "$pointer"; then
    echo "error: just dev: refusing: lock pointer is not an owned run directory (retained for inspection)" >&2
    drop_own_run_dir
    exit 1
  fi
  if [[ ! -d "$pointer" ]]; then
    # Run directory gone: stale pointer, take over (lock file only; the
    # directory is already gone, nothing to remove).
    if lock_release_if_ours "$pointer"; then
      continue
    fi
    echo "error: just dev: refusing: lock moved under us; retry" >&2
    drop_own_run_dir
    exit 1
  fi
  case "$(owner_state "$pointer")" in
    live)
      echo "error: just dev: refusing: a dev owner is already live (run $pointer); this launch makes no changes" >&2
      echo "hint: stop it with 'just dev-stop', then re-run" >&2
      drop_own_run_dir
      exit 1
      ;;
    starting)
      echo "error: just dev: refusing: another dev session is STARTING (run $pointer); retry shortly" >&2
      drop_own_run_dir
      exit 1
      ;;
    unknown)
      echo "error: just dev: refusing: cannot verify the recorded owner identity (run $pointer); retry shortly" >&2
      drop_own_run_dir
      exit 1
      ;;
    stale)
      # Recorded identity dead or foreign: stale, take over.
      if lock_release_if_ours "$pointer"; then
        continue
      fi
      echo "error: just dev: refusing: lock moved under us; retry" >&2
      drop_own_run_dir
      exit 1
      ;;
  esac
done
if [[ "$LOCKED" -ne 1 ]]; then
  echo "error: just dev: refusing: could not acquire the dev lock after retries" >&2
  drop_own_run_dir
  exit 1
fi

# Early-failure exit after the lock exists: release the lock (only while
# it still names our run) and remove our own pre-launch run directory, so
# a failed launch never blocks the next one.
die() {
  lock_release_if_ours "$RUN_DIR" || true
  if [[ -d "$RUN_DIR" && ! -f "$RUN_DIR/outcome.json" && ! -f "$RUN_DIR/owner.json" ]]; then
    rm -rf -- "$RUN_DIR"
  fi
  fail "$@"
}

# ---- launch guards (skipped only under the offline mock) ---------------------
if [[ -z "$MOCK_OPEN" ]]; then
  [[ "$(uname -s)" == "Darwin" ]] || die "macOS only (uname=$(uname -s))"
  [[ -d "$APP" ]] || die "app missing: $APP (build via: just app 'NAME')"
  [[ ! -L "$APP" ]] || die "app path is a symlink: $APP"
  [[ -f "$APP/$MARKER_REL" && ! -L "$APP/$MARKER_REL" ]] \
    || die "bundle lacks owned marker ($APP/$MARKER_REL); refusing foreign bundle"
  grep -Fqx "$BUNDLE_ID" "$APP/$MARKER_REL" \
    || die "bundle marker mismatch ($APP/$MARKER_REL); refusing foreign bundle"
  [[ -x "$APP/Contents/MacOS/tiler-macos" && ! -L "$APP/Contents/MacOS/tiler-macos" ]] \
    || die "bundle executable missing or symlinked"
fi

OUT="$(mktemp "${TMPDIR:-/tmp}/omnitiler-dev-out.XXXXXX")"
ERR="$(mktemp "${TMPDIR:-/tmp}/omnitiler-dev-err.XXXXXX")"

echo "dev: mode=$MODE run=$RUN_ID"
echo "dev: tile args: tile ${TILE_ARGS[*]}"
echo "dev: run directory: $RUN_DIR"
echo "dev: log paths:"
echo "dev:   stdout: $OUT"
echo "dev:   stderr: $ERR"

if [[ "$INTERRUPTED" -ne 0 ]]; then
  echo "dev interrupted: stopped before launch; no owner started" >&2
  echo "dev: end paths: stdout=$OUT stderr=$ERR"
  lock_release_if_ours "$RUN_DIR" || true
  drop_own_run_dir
  exit 130
fi
LAUNCH_ATTEMPTED=1
if [[ -n "$MOCK_OPEN" ]]; then
  OMNITILER_DEV_RUN_ID="$RUN_ID" OMNITILER_DEV_RUN_DIR="$RUN_DIR" \
    "$MOCK_OPEN" "$OUT" "$ERR" -- tile "${TILE_ARGS[@]}" \
    || die "mock open failed"
else
  # New LS instance per invocation; never reuses (or disturbs) a running one.
  open -n -o "$OUT" --stderr "$ERR" "$APP" --args tile "${TILE_ARGS[@]}" \
    || die "open failed for $APP"
fi
OPEN_DONE=1

# Stream logs foreground (reaped on every exit path below: no orphaned
# children; a final settle delay flushes trailing output first).
TAIL_PIDS=()
cleanup_tails() {
  for pid in ${TAIL_PIDS[@]+"${TAIL_PIDS[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in ${TAIL_PIDS[@]+"${TAIL_PIDS[@]}"}; do
    wait "$pid" 2>/dev/null || true
  done
  TAIL_PIDS=()
}
tail -n +0 -F "$OUT" &
TAIL_PIDS+=($!)
tail -n +0 -F "$ERR" >&2 &
TAIL_PIDS+=($!)

# ---- wait: exact-run outcome file, never process sets --------------------------
# Completion is the run directory's outcome.json (written by this exact
# owner) or a bounded wait expiring. Log files are streamed, never hashed
# or re-read wholesale: OUT stays tiny (tilestarted plus terminal lines)
# and only it is ever scanned, for the tilestarted nonce binding.
observe_tilestarted() {
  # Parse OUT for this run's tilestarted line; on a nonce match, record
  # the owner identity. Returns 0 once recorded. JSON parses via jq.
  [[ -f "$RUN_DIR/owner.json" ]] && return 0
  [[ -f "$OUT" ]] || return 1
  line="$(grep -F '"tilestarted"' "$OUT" 2>/dev/null | head -1 || true)"
  [[ -n "$line" ]] || return 1
  require_jq
  pid="$(printf '%s' "$line" | jq -r '.pid // empty' 2>/dev/null || true)"
  runid="$(printf '%s' "$line" | jq -r '.runid // empty' 2>/dev/null || true)"
  case "$pid" in
    ''|*[!0-9]*) return 1 ;;
  esac
  [[ -n "$runid" ]] || return 1
  if [[ "$runid" != "$RUN_ID" ]]; then
    echo "error: dev: refusing: tilestarted nonce mismatch (foreign output in our log)" >&2
    return 2
  fi
  if ! pid_alive "$pid"; then
    # Owner already exited before observation: the outcome file (if any)
    # decides below; no identity to record.
    return 1
  fi
  case "$(proc_command "$pid")" in
    *"$EXE_PATTERN"*) ;;
    *)
      echo "error: dev: refusing: owner PID $pid does not match the bundle executable" >&2
      return 2
    ;;
  esac
  write_owner_record "$RUN_DIR" "$pid" "$runid"
  echo "dev: owner identity recorded (pid $pid, run $runid)"
  return 0
}

START_SECS=$SECONDS
WRAPPER_TIMEOUT=0
prev=""
for arg in ${TILE_ARGS[@]+"${TILE_ARGS[@]}"}; do
  if [[ "$prev" == "--seconds" && "$arg" =~ ^[0-9]+$ && "$arg" -gt 0 ]]; then
    WRAPPER_TIMEOUT=$((arg + 60))
    [[ "$WRAPPER_TIMEOUT" -le 300 ]] || WRAPPER_TIMEOUT=300
  fi
  prev="$arg"
done

BREAK="wait"
OUTCOME=""
CLEAN=""
# The parsed terminal values are captured at break time from a single
# read: a concurrent `--stop` may clear the run directory afterwards, and
# re-reading then would lose the values (the stdout fallback below only
# recovers the outcome token, never cleanliness).
capture_outcome() {
  local body=""
  body="$(cat -- "$RUN_DIR/outcome.json" 2>/dev/null)" || body=""
  OUTCOME="$(parse_field "$body" outcome)"
  CLEAN="$(parse_field "$body" shutdown_clean)"
}
while true; do
  if [[ "$INTERRUPTED" -ne 0 ]]; then
    BREAK="interrupted"
    break
  fi
  if terminal_outcome_ready; then
    capture_outcome
    BREAK="terminal"
    break
  fi
  observe_tilestarted || {
    rc=$?
    if [[ "$rc" -eq 2 ]]; then
      BREAK="identity"
      break
    fi
  }
  if [[ "$WRAPPER_TIMEOUT" -gt 0 && $((SECONDS - START_SECS)) -ge "$WRAPPER_TIMEOUT" ]]; then
    BREAK="timeout"
    break
  fi
  if [[ $((SECONDS - START_SECS)) -ge "$STARTUP_GRACE" ]]; then
    if [[ ! -f "$RUN_DIR/owner.json" ]]; then
      BREAK="startup"
      break
    fi
  fi
  sleep 0.2
done

# Post-stop bounded wait: after an interruption the owner needs a finite
# window to publish its terminal outcome; absence afterwards is a dirty
# report, never a kill (there is no signal path in this launcher). An
# launch attempt may have started an owner even before readiness is visible.
if [[ "$BREAK" == "interrupted" && "$OPEN_DONE" -eq 1 ]] && ! terminal_outcome_ready; then
  deadline=$((SECONDS + STOP_WAIT))
  while [[ $SECONDS -lt $deadline ]]; do
    if terminal_outcome_ready; then
      capture_outcome
      BREAK="terminal"
      break
    fi
    observe_tilestarted || true
    sleep 0.2
  done
fi
trap - INT TERM

# Flush trailing log output, then reap the streaming tails.
sleep 1
cleanup_tails

# ---- report: parsed terminal outcome only --------------------------------------
report_paths() {
  echo "dev: end paths:"
  echo "dev:   run directory: $RUN_DIR"
  echo "dev:   outcome: $RUN_DIR/outcome.json"
  echo "dev:   stdout: $OUT"
  echo "dev:   stderr: $ERR"
}

# Release the lock only while it still names our run (never a newer run).
release_lock() {
  lock_release_if_ours "$RUN_DIR" || true
}

if [[ -z "$OUTCOME" && -f "$OUT" ]]; then
  # Best-effort stdout fallback for the terminal line (the parsed file
  # stays the authority; this only covers transports that never wrote it).
  # Never decides cleanliness: only the outcome token is recovered here.
  require_jq
  OUTCOME="$(grep -F '"outcome"' "$OUT" 2>/dev/null | tail -1 | jq -r '.outcome // empty' 2>/dev/null || true)"
fi

# Retain the lock while the recorded identity may still be live, unknown,
# or starting, or the exit is unconfirmed: retaining prevents an
# overlapping second tiler. Only a verified clean terminal exit (owner
# done) or a provably stale (dead) identity clears/takes over. A
# pre-launch interruption (nothing could have started) clears its own
# never-started run.
retain_for_owner() {
  # retain_for_owner <why>: keep lock + run dir, requesting a stop to the
  # exact run where one may still be observed.
  touch -- "$RUN_DIR/stop-requested" 2>/dev/null || true
}

case "$BREAK" in
  terminal)
    report_paths
    if is_clean_shutdown "$CLEAN" && [[ "$OUTCOME" == "completed" || "$OUTCOME" == "stopped" ]]; then
      if [[ "$INTERRUPTED" -ne 0 ]]; then
        echo "dev interrupted: verified stop (outcome=$OUTCOME, shutdown clean; windows left in place)" >&2
        release_lock
        rm -rf -- "$RUN_DIR"
        exit 130
      fi
      echo "dev success: owner exited clean (outcome=$OUTCOME)"
      release_lock
      rm -rf -- "$RUN_DIR"
      exit 0
    fi
    echo "error: dev: terminal outcome is not clean (outcome=${OUTCOME:-unknown}; run retained at $RUN_DIR; lock retained while the exit is unconfirmed)" >&2
    exit 1
    ;;
  interrupted)
    report_paths
    if [[ "$LAUNCH_ATTEMPTED" -eq 0 ]]; then
      # Only a launch never attempted proves no owner could have started.
      echo "error: dev interrupted: launch never produced an owner or outcome (no owner started; own run cleared)" >&2
      release_lock
      rm -rf -- "$RUN_DIR"
      exit 130
    fi
    echo "error: dev interrupted: no terminal outcome published (run retained at $RUN_DIR; lock retained; retry 'just dev-stop')" >&2
    retain_for_owner "interrupted" || true
    exit 130
    ;;
  timeout)
    report_paths
    retain_for_owner "timeout" || true
    echo "error: dev timeout: no terminal completion within ${WRAPPER_TIMEOUT}s (partial logs above at best; stop published to the exact run; run retained at $RUN_DIR; lock retained)" >&2
    exit 1
    ;;
  startup)
    report_paths
    retain_for_owner "startup" || true
    echo "error: dev: no owner start observed within ${STARTUP_GRACE}s and no terminal outcome (stop published to the exact run; run retained at $RUN_DIR; lock retained)" >&2
    exit 1
    ;;
  identity)
    report_paths
    echo "error: dev: owner identity failure (see above; run retained at $RUN_DIR; lock retained)" >&2
    exit 1
    ;;
esac

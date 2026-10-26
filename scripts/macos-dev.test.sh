#!/usr/bin/env bash
# Offline harness for scripts/macos-dev.sh (no bundle, no Darwin, no TCC).
#
# The mock opener below simulates LaunchServices plus a tile owner that
# speaks the exact-run protocol: per-run directory, `tilestarted` nonce
# binding, `stop-requested` observation, and `outcome.json` publication.
# Owner processes are bash fixtures whose full command line carries a tag
# matching the wrapper's executable pattern (argument, never argv[0]:
# renaming a Nix multicall applet argv[0] fails with `unknown program`).
# Covers: shell syntax, flag validation and assembly,
# concurrent refusal without cross-talk, unrelated-process immunity,
# interruption after-ready / during-startup / before-open-return, stale
# and reused-PID takeover, nonzero terminal JSON with quiet stderr, normal
# stderr logs with clean JSON, --stop paths, and exact-owner state
# clearing. No signals are ever sent to owners by the wrapper; the mock
# exits its own sleep directly.
#
# NOTE: `set -m` below is load-bearing for the INT cases. A backgrounded
# wrapper started without job control inherits SIG_IGN for INT (trap never
# fires); with job control the background wrapper keeps trappable
# dispositions, approximating real foreground `just dev` Ctrl+C delivery.
set -uo pipefail
set -m

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
DEV="$REPO_ROOT/scripts/macos-dev.sh"
command -v jq >/dev/null 2>&1 || { echo "FAIL [prereq]: jq not found (mise provides it: run 'mise install')" >&2; exit 1; }
WORK="$(mktemp -d)"
export TMPDIR="$WORK/tmp"
mkdir -p "$TMPDIR"
BASE="$WORK/state"
OUTPUT="$WORK/output.log"
MOCK="$WORK/mock-open.sh"
MANIFEST="$WORK/owner-manifest"
: >"$MANIFEST"
PASS=0
FAIL=0
EXIT=0

COMMON_ENV="OMNITILER_DEV_STATE_BASE=$BASE OMNITILER_DEV_STARTUP_GRACE=10 OMNITILER_DEV_STOP_WAIT=8"

record_owner() {
  # record_owner <pid> <tag>: append this WORK run's exact fixture
  # identity (PID plus `ps` start time) to the manifest for fallback
  # cleanup. Never authorizes a bare PID: cleanup re-verifies both.
  local pid="$1" tag="$2" start=""
  start="$(ps -o lstart= -p "$pid" 2>/dev/null | head -1 || true)"
  printf '%s|%s|%s\n' "$pid" "$start" "$tag" >>"$MANIFEST" 2>/dev/null || true
}

cleanup() {
  # Fallback only: per-leg code owns its exact kill/wait. Here send TERM
  # solely to manifest-recorded PIDs whose start identity still matches
  # AND whose current command line still carries the recorded fixture
  # tag — never a pattern scan, never a bare PID. Then reap whatever of
  # them are direct children (non-children fail silently, reaped by init).
  if [[ -f "$MANIFEST" ]]; then
    local pid="" start="" tag="" now="" cmd=""
    while IFS='|' read -r pid start tag; do
      case "$pid" in ''|*[!0-9]*) continue ;; esac
      [[ -n "$start" && -n "$tag" ]] || continue
      now="$(ps -o lstart= -p "$pid" 2>/dev/null | head -1 || true)"
      [[ -n "$now" && "$now" == "$start" ]] || continue
      cmd="$(ps -o command= -p "$pid" 2>/dev/null | head -1 || true)"
      case "$cmd" in *"$tag"*) ;; *) continue ;; esac
      kill -TERM "$pid" 2>/dev/null || true
    done <"$MANIFEST"
    while IFS='|' read -r pid _ _; do
      case "$pid" in ''|*[!0-9]*) continue ;; esac
      wait "$pid" 2>/dev/null || true
    done <"$MANIFEST"
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

pid_alive() { kill -0 "$1" 2>/dev/null; }
pass() { PASS=$((PASS + 1)); }
fail() {
  echo "FAIL [$1]: $2" >&2
  FAIL=$((FAIL + 1))
}

run_dev() {
  # run_dev <expected-exit> <label> [args...]
  local expected="$1" label="$2"
  shift 2
  set +e
  # shellcheck disable=SC2086
  env $COMMON_ENV OMNITILER_DEV_MOCK_OPEN="$MOCK" MOCK_MARKER="$WORK/mock-invoked" MOCK_BEHAVIOR="${MOCK_BEHAVIOR:-success}" MOCK_ARGS="${MOCK_ARGS:-}" MOCK_MANIFEST="$MANIFEST" \
    bash "$DEV" "$@" >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  if [[ "$EXIT" -ne "$expected" ]]; then
    fail "$label" "expected exit $expected, got $EXIT"
    cat "$OUTPUT" >&2
  else
    pass
  fi
}

run_dev_bg() {
  # start_dev_bg <label> [args...]: start the wrapper in background WITHOUT
  # command substitution (a $(...) would reparent the child and break
  # kill/wait). Sets BG_PID. The caller waits with `wait "$BG_PID"`.
  local label="$1"
  shift
  set +e
  # shellcheck disable=SC2086
  env $COMMON_ENV OMNITILER_DEV_MOCK_OPEN="$MOCK" MOCK_MARKER="$WORK/mock-invoked" MOCK_BEHAVIOR="${MOCK_BEHAVIOR:-success}" MOCK_MANIFEST="$MANIFEST" \
    bash "$DEV" "$@" >"$OUTPUT.$label" 2>&1 &
  BG_PID=$!
  record_owner "$BG_PID" "macos-dev.sh"
  set -e
}

assert_contains() {
  # assert_contains <file> <needle> <label>
  if grep -Fq -- "$2" "$1"; then
    pass
  else
    fail "$3" "missing '$2'"
    cat "$1" >&2
  fi
}

wait_for_content() {
  # wait_for_content <file> <needle> [tries]: poll up to ~tries/10 s.
  local file="$1" needle="$2" tries="${3:-100}" i=0
  while [[ $i -lt $tries ]]; do
    if grep -Fq -- "$needle" "$file" 2>/dev/null; then
      return 0
    fi
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

current_run_dir() {
  # The lock pointer's run directory (empty when unlocked).
  if [[ -f "$BASE/lock/run" ]]; then
    cat -- "$BASE/lock/run" 2>/dev/null || true
  fi
}

MOCK_TAG="OmniTilerProbe.app/Contents/MacOS/tiler-macos-mock"
FOREIGN_TAG="plain-sleep-no-pattern"

spawn_tagged_owner() {
  # spawn_tagged_owner <out-var> <tag>: start a bash owner whose full
  # command line carries <tag> as an argument (never argv[0], so Nix
  # multicall applets keep working). Sets <out-var> to the exact child
  # PID and records PID + start identity in this WORK run's manifest;
  # the caller owns kill/wait. The bash wrapper forwards TERM/INT to
  # its single sleep child and its trailing `wait`s reap that exact
  # child, so `kill <pid>` plus `wait <pid>` leaves no orphan
  # grandchildren. Last command is `wait`, never `exec`, so no
  # last-command replacement hides the wrapper.
  local __out="$1" __tag="$2" __pid=""
  bash -c 'child=""; on_term(){ if [[ -n "$child" ]]; then kill "$child" 2>/dev/null || true; fi; }; trap on_term TERM INT; sleep 60 & child=$!; wait "$child" 2>/dev/null; wait "$child" 2>/dev/null || true' "$__tag" &
  __pid=$!
  printf -v "$__out" '%s' "$__pid"
  record_owner "$__pid" "$__tag"
}

cat >"$MOCK" <<'MOCK_EOF'
#!/usr/bin/env bash
# Mock LaunchServices opener + mock tile owner.
# Args: $1=OUT $2=ERR, then `-- tile args...`. Reads OMNITILER_DEV_RUN_ID /
# OMNITILER_DEV_RUN_DIR / MOCK_BEHAVIOR. The graceful behaviors observe
# `$RUN_DIR/stop-requested` and publish `$RUN_DIR/outcome.json` exactly
# like the real tile; no signals involved.
set -uo pipefail
OUT="$1"
ERR="$2"
RUN_DIR="${OMNITILER_DEV_RUN_DIR:-}"
RUN_ID="${OMNITILER_DEV_RUN_ID:-mock-run}"
touch "${MOCK_MARKER:-/dev/null}"
printf '%s\n' "$@" >>"${MOCK_ARGS:-/dev/null}"

complete() {
  # complete <outcome-json>: append OUT line + publish outcome file
  # atomically (a temp file plus rename, so existence implies content).
  printf '%s\n' "$1" >>"$OUT"
  if [[ -n "$RUN_DIR" ]]; then
    printf '%s\n' "$1" >"$RUN_DIR/.outcome.json.tmp"
    mv -- "$RUN_DIR/.outcome.json.tmp" "$RUN_DIR/outcome.json"
  fi
}

serve_stop() {
  # serve_stop [cap-secs]: wait for the exact run's stop file, then
  # publish a clean stopped outcome and exit 0. Exits 2 past the cap.
  local cap="${1:-40}" waited=0
  while [[ ! -f "$RUN_DIR/stop-requested" ]]; do
    sleep 0.1
    waited=$((waited + 1))
    if [[ "$waited" -gt $((cap * 10)) ]]; then
      complete '{"outcome":"unclean-shutdown","shutdown_clean":false}'
      exit 2
    fi
  done
  complete '{"outcome":"stopped","seconds":0,"mode":"unscoped","shutdown_outcome":"Clean","shutdown_clean":true}'
}

spawn_owner() {
  # Bash owner carrying the wrapper's executable pattern in its command
  # line (argument, never argv[0]; Nix multicall-safe). Sets SLEEP_PID
  # to the exact owner PID and records PID + start identity in this
  # WORK run's manifest (via $MOCK_MANIFEST). The TERM/INT trap kills
  # the single sleep child and the trailing `wait`s reap exactly it:
  # no orphan grandchildren. Last command is `wait`, never `exec`.
  bash -c 'child=""; on_term(){ if [[ -n "$child" ]]; then kill "$child" 2>/dev/null || true; fi; }; trap on_term TERM INT; sleep 60 & child=$!; wait "$child" 2>/dev/null; wait "$child" 2>/dev/null || true' "OmniTilerProbe.app/Contents/MacOS/tiler-macos-mock" &
  SLEEP_PID=$!
  if [[ -n "${MOCK_MANIFEST:-}" ]]; then
    OWNER_START="$(ps -o lstart= -p "$SLEEP_PID" 2>/dev/null | head -1 || true)"
    printf '%s|%s|%s\n' "$SLEEP_PID" "$OWNER_START" "OmniTilerProbe.app/Contents/MacOS/tiler-macos-mock" >>"$MOCK_MANIFEST" 2>/dev/null || true
  fi
}

case "${MOCK_BEHAVIOR:-success}" in
  success)
    printf '%s\n' "{\"tilestarted\":true,\"pid\":$$,\"runid\":\"$RUN_ID\"}" >>"$OUT"
    sleep 0.3
    complete '{"outcome":"completed","seconds":30,"mode":"scoped","shutdown_outcome":"Clean","shutdown_clean":true}'
    ;;
  loud-clean)
    printf '%s\n' "{\"tilestarted\":true,\"pid\":$$,\"runid\":\"$RUN_ID\"}" >>"$OUT"
    printf '%s\n' 'tiler-log seq=1 t=10ms pid=9 gen=1 ref=2 kind=observation outcome=applied count=1' >>"$ERR"
    printf '%s\n' 'tiler-macos dev heartbeat mode=unscoped launched=2 terminated=0 topo_updates=1 lock_stops=0' >>"$ERR"
    sleep 0.3
    complete '{"outcome":"completed","seconds":0,"mode":"unscoped","shutdown_outcome":"Clean","shutdown_clean":true}'
    ;;
  refused-quiet)
    printf '%s\n' '{"outcome":"refused","reason":"tile requires explicit --user-start"}' >>"$OUT"
    if [[ -n "$RUN_DIR" ]]; then
      printf '%s\n' '{"outcome":"refused","reason":"tile requires explicit --user-start","shutdown_clean":false}' >"$RUN_DIR/outcome.json"
    fi
    ;;
  input-refused-quiet)
    printf '%s\n' "{\"tilestarted\":true,\"pid\":$$,\"runid\":\"$RUN_ID\"}" >>"$OUT"
    sleep 0.3
    complete '{"outcome":"input-refused","shutdown_outcome":"Clean","shutdown_clean":false}'
    ;;
  hang-no-outcome)
    # Live owner that never publishes an outcome (stop observed, but no
    # terminal JSON): exercises lock retention while live/unconfirmed.
    if [[ -z "$RUN_DIR" ]]; then
      exit 2
    fi
    spawn_owner
    printf '%s\n' "{\"tilestarted\":true,\"pid\":$SLEEP_PID,\"runid\":\"$RUN_ID\"}" >>"$OUT"
    ( while [[ ! -f "$RUN_DIR/stop-requested" ]]; do sleep 0.1; done
      sleep 2 ) &
    ;;
  malformed-outcome)
    printf '%s\n' "{\"tilestarted\":true,\"pid\":$$,\"runid\":\"$RUN_ID\"}" >>"$OUT"
    sleep 0.3
    printf '%s\n' '{"outcome": ' >>"$OUT"
    if [[ -n "$RUN_DIR" ]]; then
      printf '%s\n' '{"outcome": ' >"$RUN_DIR/outcome.json"
    fi
    ;;
  fail-open)
    exit 3
    ;;
  hang-stop)
    if [[ -z "$RUN_DIR" ]]; then
      exit 2
    fi
    spawn_owner
    printf '%s\n' "{\"tilestarted\":true,\"pid\":$SLEEP_PID,\"runid\":\"$RUN_ID\"}" >>"$OUT"
    # Owner agent: like `open`, the opener itself returns at once; the
    # agent below observes the exact run's stop file in the background.
    ( serve_stop 40
      kill "$SLEEP_PID" 2>/dev/null || true
      wait "$SLEEP_PID" 2>/dev/null || true ) &
    ;;
  slow-tilestarted)
    # Readiness itself arrives late: the opener returns at once while a
    # background agent publishes tilestarted after 2s — unless the exact
    # run's stop file is already present (pre-ready stop).
    if [[ -z "$RUN_DIR" ]]; then
      exit 2
    fi
    ( sleep 2
      if [[ -f "$RUN_DIR/stop-requested" ]]; then
        printf '%s\n' 'pre-ready-stop: stop observed before readiness' >>"$OUT"
        complete '{"outcome":"stopped","shutdown_outcome":"not-started","shutdown_clean":true}'
        exit 0
      fi
      spawn_owner
      printf '%s\n' "{\"tilestarted\":true,\"pid\":$SLEEP_PID,\"runid\":\"$RUN_ID\"}" >>"$OUT"
      serve_stop 40
      kill "$SLEEP_PID" 2>/dev/null || true
      wait "$SLEEP_PID" 2>/dev/null || true ) &
    ;;
  open-hang)
    # `open` itself stalls (never launches): the wrapper must still
    # answer an interruption exactly and honestly.
    sleep 6
    ;;
esac
exit 0
MOCK_EOF
chmod +x "$MOCK"

# ---- Part 0: syntax ----------------------------------------------------------
bash -n "$DEV" || { fail "syntax" "macos-dev.sh failed bash -n"; exit 1; }
bash -n "$MOCK" || { fail "syntax" "mock failed bash -n"; exit 1; }
pass
pass

# ---- Part 1: flag validation + arg assembly (no launch for the refusal) -------
rm -f "$WORK/mock-invoked"
MOCK_BEHAVIOR=success run_dev 2 "scope+unscoped refused" --scope com.example.app --unscoped
assert_contains "$OUTPUT" "mutually exclusive" "scope+unscoped message"
if [[ -f "$WORK/mock-invoked" ]]; then
  fail "scope+unscoped refused" "mock opener must not run"
else
  pass
fi

rm -f "$WORK/mock-invoked" "$WORK/mock-args"
MOCK_BEHAVIOR=success MOCK_ARGS="$WORK/mock-args" run_dev 0 "default assembly runs" verbose --exclude com.example.skip
assert_contains "$WORK/mock-args" "--user-start" "user-start assembled"
assert_contains "$WORK/mock-args" "--unscoped" "unscoped default assembled"
assert_contains "$WORK/mock-args" "--log" "mode maps to --log"
assert_contains "$WORK/mock-args" "verbose" "verbose value forwarded"
assert_contains "$WORK/mock-args" "--exclude" "exclusion flag forwarded"
assert_contains "$WORK/mock-args" "com.example.skip" "exclusion value forwarded"
assert_contains "$WORK/mock-args" "--run-dir" "run dir passed"
assert_contains "$OUTPUT" "run directory:" "run dir printed at start"
assert_contains "$OUTPUT" "end paths:" "end paths printed"

# ---- Part 2: concurrent refusal without cross-talk + --stop -------------------
MOCK_BEHAVIOR=hang-stop run_dev_bg "A"
A_PID="$BG_PID"
A_RUN=""
for _ in $(seq 1 100); do
  A_RUN="$(current_run_dir)"
  if [[ -n "$A_RUN" && -f "$A_RUN/owner.json" ]]; then
    break
  fi
  A_RUN=""
  sleep 0.1
done
if [[ -z "$A_RUN" ]]; then
  fail "concurrent setup" "run A never owned the lock"
  kill -KILL "$A_PID" 2>/dev/null || true
else
  pass
fi
MOCK_BEHAVIOR=success run_dev 1 "concurrent refused"
assert_contains "$OUTPUT" "already live" "concurrent refusal names the live run"
if [[ -f "$A_RUN/stop-requested" ]]; then
  fail "concurrent refused" "refused launcher must not publish a stop to A's run"
else
  pass
fi
# --stop ends exactly run A, gracefully.
set +e
env OMNITILER_DEV_STATE_BASE="$BASE" bash "$DEV" --stop >"$OUTPUT" 2>&1
EXIT=$?
set -e
if [[ "$EXIT" -ne 0 ]]; then
  fail "stop live run" "expected exit 0, got $EXIT"
  cat "$OUTPUT" >&2
else
  pass
fi
assert_contains "$OUTPUT" "verified stop" "stop reports verified"
set +e
wait "$A_PID"
A_EXIT=$?
set -e
if [[ "$A_EXIT" -ne 0 ]]; then
  fail "stopped run exits clean" "wrapper A expected exit 0, got $A_EXIT"
else
  pass
fi
assert_contains "$OUTPUT.A" "dev success" "origin reports clean owner exit"
assert_contains "$OUTPUT.A" "outcome=stopped" "origin names the stopped outcome"
if [[ -e "$A_RUN" ]]; then
  fail "exact state clearing" "clean run directory must be removed: $A_RUN"
else
  pass
fi
if [[ -n "$(current_run_dir)" ]]; then
  fail "exact state clearing" "lock must be free after clean completion"
else
  pass
fi

# ---- Part 3: unrelated processes are never adopted ----------------------------
spawn_tagged_owner LONER "$MOCK_TAG"
MOCK_BEHAVIOR=success run_dev 0 "unrelated ignored"
if pid_alive "$LONER"; then
  pass
else
  fail "unrelated ignored" "unrelated matching process must survive"
fi
kill "$LONER" 2>/dev/null || true
wait "$LONER" 2>/dev/null || true

# ---- Part 4: interruption after ready ------------------------------------------
MOCK_BEHAVIOR=hang-stop run_dev_bg "B"
B_PID="$BG_PID"
if ! wait_for_content "$OUTPUT.B" "tilestarted"; then
  fail "after-ready interruption" "tilestarted never streamed"
  kill -KILL "$B_PID" 2>/dev/null || true
else
  pass
  kill -INT "$B_PID"
  set +e
  wait "$B_PID"
  EXIT=$?
  set -e
  if [[ "$EXIT" -ne 130 ]]; then
    fail "after-ready interruption" "expected exit 130, got $EXIT"
  else
    pass
  fi
fi
assert_contains "$OUTPUT.B" "verified stop" "interruption verified honestly"
assert_contains "$OUTPUT.B" "end paths:" "end paths printed after interrupt"
if [[ -n "$(current_run_dir)" ]]; then
  fail "after-ready interruption" "lock must clear after verified stop"
else
  pass
fi
# No pattern scan: resolve the exact owner PID this leg published, then
# verify it is gone — a live PID counts only if its start identity still
# matches this WORK run's manifest record (never a bare-PID verdict).
B_OWNER_PID="$(grep -F '"tilestarted"' "$OUTPUT.B" 2>/dev/null | tail -1 | jq -r '.pid // empty' 2>/dev/null || true)"
B_OWNER_LEFT=0
if [[ -n "${B_OWNER_PID:-}" ]] && pid_alive "$B_OWNER_PID"; then
  B_NOW="$(ps -o lstart= -p "$B_OWNER_PID" 2>/dev/null | head -1 || true)"
  B_CMD="$(ps -o command= -p "$B_OWNER_PID" 2>/dev/null | head -1 || true)"
  B_WAS="$(grep -F -- "${B_OWNER_PID}|" "$MANIFEST" 2>/dev/null | tail -1 || true)"
  if [[ "$B_WAS" == *"|${B_NOW}|"* && "$B_CMD" == *"$MOCK_TAG"* ]]; then
    B_OWNER_LEFT=1
  fi
fi
if [[ "$B_OWNER_LEFT" -eq 1 ]]; then
  fail "after-ready interruption" "mock owner left running"
else
  pass
fi

# ---- Part 5: interruption during startup ---------------------------------------
# The stop (Ctrl+C here) lands while readiness is still 2s out; the exact
# run observes it first and never becomes ready.
MOCK_BEHAVIOR=slow-tilestarted run_dev_bg "C"
C_PID="$BG_PID"
sleep 0.5 # before the mock's 2s readiness; the stop must still land exactly.
kill -INT "$C_PID"
set +e
wait "$C_PID"
EXIT=$?
set -e
if [[ "$EXIT" -ne 130 ]]; then
  fail "startup interruption" "expected exit 130, got $EXIT"
else
  pass
fi
assert_contains "$OUTPUT.C" "verified stop" "pre-ready stop verified"
# The pre-ready marker lives in the persisted OUT log (tail timing must
# not gate it): it proves the stop landed before readiness in this run.
C_OUT="$(grep "stdout:" "$OUTPUT.C" | head -1 | sed 's/.*stdout: //')"
if [[ -n "$C_OUT" ]] && grep -Fq -- "pre-ready-stop" "$C_OUT" 2>/dev/null; then
  pass
else
  fail "pre-ready marker" "stop landed before readiness in the exact run (out: ${C_OUT:-unknown})"
fi
if [[ -n "$(current_run_dir)" ]]; then
  fail "startup interruption" "lock must clear after verified stop"
else
  pass
fi

# ---- Part 5b: stop from a second terminal during startup ----------------------
MOCK_BEHAVIOR=slow-tilestarted run_dev_bg "C2"
C2_PID="$BG_PID"
sleep 0.5 # before readiness: --stop must publish to the exact run anyway.
set +e
env OMNITILER_DEV_STATE_BASE="$BASE" bash "$DEV" --stop >"$OUTPUT.C2stop" 2>&1
EXIT=$?
set -e
if [[ "$EXIT" -ne 0 ]]; then
  fail "pre-ready stop request" "expected exit 0, got $EXIT"
  cat "$OUTPUT.C2stop" >&2
else
  pass
fi
assert_contains "$OUTPUT.C2stop" "verified stop" "pre-ready stop verified from second terminal"
set +e
wait "$C2_PID"
EXIT=$?
set -e
if [[ "$EXIT" -ne 0 ]]; then
  fail "pre-ready origin exit" "origin expected exit 0, got $EXIT"
else
  pass
fi
assert_contains "$OUTPUT.C2" "dev success" "origin reports pre-ready stop"
assert_contains "$OUTPUT.C2" "outcome=stopped" "origin names the stopped outcome"

# ---- Part 6: interruption before the launch returns ----------------------------
MOCK_BEHAVIOR=open-hang run_dev_bg "D"
D_PID="$BG_PID"
sleep 1 # inside the stalled open; the trap still answers exactly.
kill -INT "$D_PID"
set +e
wait "$D_PID"
EXIT=$?
set -e
if [[ "$EXIT" -ne 130 ]]; then
  fail "pre-launch interruption" "expected exit 130, got $EXIT"
else
  pass
fi
assert_contains "$OUTPUT.D" "interrupted" "pre-launch interruption reported"
assert_contains "$OUTPUT.D" "dev: end paths:" "pre-launch interruption prints end log paths"
assert_contains "$OUTPUT.D" "stdout:" "pre-launch interruption prints stdout path"
if [[ -n "$(current_run_dir)" ]]; then
  pass
else
  fail "pre-launch interruption" "an attempted launch without readiness must retain ownership"
fi
D_RUN="$(grep "run directory:" "$OUTPUT.D" | head -1 | sed 's/.*run directory: //')"
if [[ -n "$D_RUN" && -f "$D_RUN/stop-requested" ]]; then
  pass
else
  fail "pre-launch interruption" "uncertain launch must retain its stop request"
fi
MOCK_BEHAVIOR=success run_dev 1 "uncertain launch refuses overlap"
assert_contains "$OUTPUT" "STARTING" "missing readiness never proves owner absence"
# The mock launch child has exited and creates no owner; only the fixture can
# prove that fact independently, so clear its retained control state here.
rm -f -- "$BASE/lock/run"
rmdir -- "$BASE/lock"
rm -rf -- "$D_RUN"

# ---- Part 7: stale lock takeover ----------------------------------------------
STALE_RUN="$BASE/runs/run.stale-fixture"
mkdir -p "$STALE_RUN"
printf 'pid=%s\nrunid=%s\nlstart=%s\n' "999999" "run.stale-fixture" "stale" >"$STALE_RUN/owner.json"
mkdir -p "$BASE/lock"
printf '%s\n' "$STALE_RUN" >"$BASE/lock/run"
MOCK_BEHAVIOR=success run_dev 0 "stale lock taken over"
if [[ "$(current_run_dir)" == "$STALE_RUN" ]]; then
  fail "stale lock taken over" "lock must move to the new run"
else
  pass
fi
if [[ -f "$STALE_RUN/stop-requested" ]]; then
  fail "stale lock taken over" "stale run must not be touched"
else
  pass
fi

# ---- Part 8: reused numeric PID is distinguished by identity -------------------
REUSED_RUN="$BASE/runs/run.reused-fixture"
mkdir -p "$REUSED_RUN"
printf 'pid=%s\nrunid=%s\nlstart=%s\n' "$$" "run.reused-fixture" "stale-start-time" >"$REUSED_RUN/owner.json"
mkdir -p "$BASE/lock"
printf '%s\n' "$REUSED_RUN" >"$BASE/lock/run"
MOCK_BEHAVIOR=success run_dev 0 "reused PID taken over"
if pid_alive "$$"; then
  pass
else
  fail "reused PID taken over" "harness itself must survive (never signal unknowns)"
fi
if [[ -f "$REUSED_RUN/stop-requested" ]]; then
  fail "reused PID taken over" "foreign run directory must not be touched"
else
  pass
fi

# ---- Part 9: nonzero terminal JSON with quiet stderr ---------------------------
MOCK_BEHAVIOR=input-refused-quiet run_dev 1 "refusal reported, not silent"
assert_contains "$OUTPUT" "input-refused" "terminal outcome parsed"
assert_contains "$OUTPUT" "dev: end paths:" "dirty terminal prints end log paths"
assert_contains "$OUTPUT" "stdout:" "dirty terminal prints stdout path"
assert_contains "$OUTPUT" "stderr:" "dirty terminal prints stderr path"
if [[ -z "$(current_run_dir)" ]]; then
  fail "refusal state" "lock must be retained while the exit is unconfirmed (dirty terminal)"
else
  pass
fi
RETAINED="$(grep "run directory:" "$OUTPUT" | head -1 | sed 's/.*run directory: //')"
if [[ -n "$RETAINED" && -f "$RETAINED/outcome.json" ]]; then
  pass
else
  fail "refusal state" "dirty run directory must be retained with its outcome"
fi
# Dirty terminal retains the lock by design (no overlap while unconfirmed);
# free it here so later legs start from a clean gate (owner is dead: the
# short-lived mock already exited).
rm -f "$BASE/lock/run"
rmdir "$BASE/lock" 2>/dev/null || true

# ---- Part 10: normal stderr logs never decide success --------------------------
MOCK_BEHAVIOR=loud-clean run_dev 0 "stderr volume ignored on clean outcome"
assert_contains "$OUTPUT" "heartbeat" "operational logs stream"

# ---- Part 11: failed launch releases the lock ----------------------------------
MOCK_BEHAVIOR=fail-open run_dev 1 "failed launch reported"
assert_contains "$OUTPUT" "mock open failed" "launch failure reported"
assert_contains "$OUTPUT" "dev: end paths:" "failed launch prints end log paths"
assert_contains "$OUTPUT" "stdout:" "failed launch prints stdout path"
assert_contains "$OUTPUT" "stderr:" "failed launch prints stderr path"
if [[ -n "$(current_run_dir)" ]]; then
  fail "failed launch releases" "lock must release after failed launch"
else
  pass
fi
FAILED_RUN="$(grep "run directory:" "$OUTPUT" | head -1 | sed 's/.*run directory: //')"
if [[ -n "$FAILED_RUN" && -e "$FAILED_RUN" ]]; then
  fail "failed launch releases" "pre-launch run directory must be removed: $FAILED_RUN"
else
  pass
fi

# ---- Part 12: --stop on stale and ambiguous states -------------------------------
STALE2_RUN="$BASE/runs/run.stale2-fixture"
mkdir -p "$STALE2_RUN"
printf 'pid=%s\nrunid=%s\nlstart=%s\n' "999999" "run.stale2-fixture" "stale" >"$STALE2_RUN/owner.json"
mkdir -p "$BASE/lock"
printf '%s\n' "$STALE2_RUN" >"$BASE/lock/run"
set +e
env OMNITILER_DEV_STATE_BASE="$BASE" bash "$DEV" --stop >"$OUTPUT" 2>&1
EXIT=$?
set -e
if [[ "$EXIT" -ne 0 ]]; then
  fail "stop stale state" "expected exit 0, got $EXIT"
else
  pass
fi
assert_contains "$OUTPUT" "no live owner" "stale stop reported"
# Foreign live process recorded under a run: the executable no longer
# matches, so the state is ambiguous — --stop refuses without touching
# anything (never signals unknowns).
spawn_tagged_owner FOREIGN "$FOREIGN_TAG"
FOREIGN_START="$(ps -o lstart= -p "$FOREIGN" | head -1)"
FOREIGN_RUN="$BASE/runs/run.foreign-fixture"
mkdir -p "$FOREIGN_RUN"
{
  printf 'pid=%s\n' "$FOREIGN"
  printf 'runid=%s\n' "run.foreign-fixture"
  printf 'lstart=%s\n' "$FOREIGN_START"
} >"$FOREIGN_RUN/owner.json"
mkdir -p "$BASE/lock"
printf '%s\n' "$FOREIGN_RUN" >"$BASE/lock/run"
set +e
env OMNITILER_DEV_STATE_BASE="$BASE" bash "$DEV" --stop >"$OUTPUT" 2>&1
EXIT=$?
set -e
if [[ "$EXIT" -ne 1 ]]; then
  fail "stop ambiguous identity" "expected exit 1, got $EXIT"
else
  pass
fi
assert_contains "$OUTPUT" "cannot verify" "ambiguous stop refused"
if pid_alive "$FOREIGN"; then
  pass
else
  fail "stop ambiguous identity" "foreign process must survive (never signal unknowns)"
fi
if [[ "$(current_run_dir)" == "$FOREIGN_RUN" ]]; then
  pass
else
  fail "stop ambiguous identity" "ambiguous lock pointer must stay put"
fi
kill "$FOREIGN" 2>/dev/null || true
wait "$FOREIGN" 2>/dev/null || true
rm -f "$BASE/lock/run"
rmdir "$BASE/lock" 2>/dev/null || true

# ---- Part 13: live owner with no outcome cannot unlock via --stop -----------
MOCK_BEHAVIOR=hang-no-outcome run_dev_bg "E"
E_PID="$BG_PID"
E_RUN=""
for _ in $(seq 1 100); do
  E_RUN="$(current_run_dir)"
  if [[ -n "$E_RUN" && -f "$E_RUN/owner.json" ]]; then
    break
  fi
  E_RUN=""
  sleep 0.1
done
if [[ -z "$E_RUN" ]]; then
  fail "live no-outcome setup" "run E never owned the lock"
  kill -KILL "$E_PID" 2>/dev/null || true
else
  pass
  # Short stop wait so this leg stays bounded; the owner never publishes.
  set +e
  env OMNITILER_DEV_STATE_BASE="$BASE" OMNITILER_DEV_STOP_WAIT=2 bash "$DEV" --stop >"$OUTPUT.Estop" 2>&1
  EXIT=$?
  set -e
  if [[ "$EXIT" -ne 1 ]]; then
    fail "live no-outcome stop" "expected exit 1, got $EXIT"
  else
    pass
  fi
  assert_contains "$OUTPUT.Estop" "no terminal outcome" "missing outcome reported honestly"
  if [[ "$(current_run_dir)" == "$E_RUN" ]]; then
    pass
  else
    fail "live no-outcome stop" "lock must stay while the live owner is unconfirmed"
  fi
  if [[ -f "$E_RUN/stop-requested" ]]; then
    pass
  else
    fail "live no-outcome stop" "stop must be published to the exact run"
  fi
  # A second launch must still refuse (no overlap while live).
  MOCK_BEHAVIOR=success run_dev 1 "live overlap refused"
  assert_contains "$OUTPUT" "already live" "second launch refused while live"
  # Cleanup: the mock sleep is an exact spawned PID (verified identity);
  # terminate only it, then free the gate for later legs. The EXIT trap
  # manifest is the only fallback (exact PID + start identity, no scan).
  E_OWNER_PID="$(grep -F '"tilestarted"' "$OUTPUT.E" 2>/dev/null | tail -1 | jq -r '.pid // empty' 2>/dev/null || true)"
  if [[ -n "${E_OWNER_PID:-}" ]]; then
    kill "$E_OWNER_PID" 2>/dev/null || true
  fi
  kill -INT "$E_PID" 2>/dev/null || true
  set +e
  wait "$E_PID" 2>/dev/null || true
  set -e
  rm -f "$BASE/lock/run"
  rmdir "$BASE/lock" 2>/dev/null || true
  if [[ -n "$E_RUN" ]]; then
    rm -rf -- "$E_RUN"
  fi
fi

# ---- Part 14: caller --run-dir override refused ------------------------------
rm -f "$WORK/mock-invoked"
MOCK_BEHAVIOR=success run_dev 2 "run-dir override refused" --run-dir /tmp/foreign-run-dir
assert_contains "$OUTPUT" "owned by the wrapper" "run-dir override refused"
if [[ -f "$WORK/mock-invoked" ]]; then
  fail "run-dir override refused" "mock opener must not run"
else
  pass
fi
if [[ -n "$(current_run_dir)" ]]; then
  fail "run-dir override refused" "lock must stay free after refusal"
else
  pass
fi

# ---- Part 15: foreign lock pointer never removed ------------------------------
FOREIGN_DIR="$WORK/foreign-not-a-run"
mkdir -p "$FOREIGN_DIR"
printf 'sentinel\n' >"$FOREIGN_DIR/sentinel.txt"
mkdir -p "$BASE/lock"
printf '%s\n' "$FOREIGN_DIR" >"$BASE/lock/run"
MOCK_BEHAVIOR=success run_dev 1 "foreign pointer refused"
assert_contains "$OUTPUT" "not an owned run" "foreign pointer refused"
if [[ -f "$FOREIGN_DIR/sentinel.txt" ]]; then
  pass
else
  fail "foreign pointer refused" "foreign directory must never be removed"
fi
if [[ "$(current_run_dir)" == "$FOREIGN_DIR" ]]; then
  pass
else
  fail "foreign pointer refused" "foreign pointer must stay put"
fi
set +e
env OMNITILER_DEV_STATE_BASE="$BASE" bash "$DEV" --stop >"$OUTPUT" 2>&1
EXIT=$?
set -e
if [[ "$EXIT" -ne 1 ]]; then
  fail "foreign stop refused" "expected exit 1, got $EXIT"
else
  pass
fi
assert_contains "$OUTPUT" "not an owned run" "foreign stop refused"
if [[ -f "$FOREIGN_DIR/sentinel.txt" ]]; then
  pass
else
  fail "foreign stop refused" "foreign directory must never be removed by --stop"
fi
rm -f "$BASE/lock/run"
rmdir "$BASE/lock" 2>/dev/null || true

# ---- Part 16: malformed JSON never reports success ------------------------------
MOCK_BEHAVIOR=malformed-outcome run_dev 1 "malformed outcome dirty"
if grep -Fq -- "dev success" "$OUTPUT"; then
  fail "malformed outcome" "malformed JSON must never report success"
else
  pass
fi
assert_contains "$OUTPUT" "run retained" "malformed outcome retained honestly"
if [[ -z "$(current_run_dir)" ]]; then
  fail "malformed outcome" "lock must be retained while the exit is unconfirmed"
else
  pass
fi
rm -f "$BASE/lock/run"
rmdir "$BASE/lock" 2>/dev/null || true

# ---- Part 17: empty lock directory fails closed (never stolen) ------------------
mkdir -p "$BASE/lock"
rm -f "$BASE/lock/run"
rm -f "$WORK/mock-invoked"
MOCK_BEHAVIOR=success run_dev 1 "empty lock refused"
assert_contains "$OUTPUT" "could not acquire" "empty lock fails closed after retries"
if [[ -f "$WORK/mock-invoked" ]]; then
  fail "empty lock refused" "mock opener must not run while the gate is contested"
else
  pass
fi
rmdir "$BASE/lock" 2>/dev/null || true

echo "macos-dev offline: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]]

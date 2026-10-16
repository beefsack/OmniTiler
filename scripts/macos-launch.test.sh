#!/usr/bin/env bash
# Offline harness for scripts/macos-launch.sh (no bundle, no Darwin, no TCC).
#
# Mocks `open` (background writer fixtures), `pgrep` (no bundle processes)
# and `uname` (Darwin) so every leg runs headless on any OS. The wrapper
# under test is copied into a temp repo layout whose fake bundle carries
# the owned marker; the real target/ tree is never touched and nothing is
# ever launched for real.
#
# Covers: shell syntax, flag/timeout validation, classic (non-stream)
# behavior, --stream live prompt delivery before completion, stdout printed
# exactly once, every stderr line exactly once (including rapid appended
# chunks that interleave with polls), timeout partial-output behavior, and
# that streamed bytes are bounded at the snapshot (no racing append is
# duplicated or skipped).
#
# Cleanup kills only exact fixture PIDs this harness spawned (recorded by
# the mock opener); never a pattern scan, never a broad kill.
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
LAUNCH="$REPO_ROOT/scripts/macos-launch.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/macos-launch-test.XXXXXX")"
export TMPDIR="$WORK/tmp"
mkdir -p "$TMPDIR"
FAKE="$WORK/repo"
MANIFEST="$WORK/spawned-pids"
: >"$MANIFEST"
PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "ok: $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL: $1"; }

cleanup() {
  # Exact-PID cleanup only: PIDs recorded by the mock opener below.
  if [[ -f "$MANIFEST" ]]; then
    local pid=""
    while IFS= read -r pid; do
      case "$pid" in '' | *[!0-9]*) continue ;; esac
      if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
      fi
    done <"$MANIFEST"
    while IFS= read -r pid; do
      case "$pid" in '' | *[!0-9]*) continue ;; esac
      wait "$pid" 2>/dev/null || true
    done <"$MANIFEST"
  fi
  rm -rf -- "$WORK"
}
trap cleanup EXIT

mkdir -p "$FAKE/scripts" \
  "$FAKE/target/macos/OmniTilerProbe.app/Contents/Resources" \
  "$FAKE/target/macos/OmniTilerProbe.app/Contents/MacOS" \
  "$WORK/mockbin"
cp "$LAUNCH" "$FAKE/scripts/macos-launch.sh"
printf 'io.github.omnitiler.macos.probe\n' > \
  "$FAKE/target/macos/OmniTilerProbe.app/Contents/Resources/.omnitiler-probe-marker"
printf '#!/usr/bin/env bash\nexit 0\n' > \
  "$FAKE/target/macos/OmniTilerProbe.app/Contents/MacOS/tiler-macos"
chmod +x "$FAKE/target/macos/OmniTilerProbe.app/Contents/MacOS/tiler-macos"

# Mock pgrep: no bundle processes ever (immediate terminal completion).
printf '#!/usr/bin/env bash\nexit 1\n' >"$WORK/mockbin/pgrep"
# Mock uname: Darwin, so the macOS guard passes on any CI host.
printf '#!/usr/bin/env bash\nif [[ "${1:-}" == "-s" ]]; then echo Darwin; else exec /bin/uname "$@"; fi\n' >"$WORK/mockbin/uname"
# Mock open: reads -o OUT / --stderr ERR, then backgrounds the scenario
# named by $MOCK_SCENARIO and returns at once (like real `open`). The
# writer's exact PID is recorded for harness-owned cleanup.
cat >"$WORK/mockbin/open" <<'MOCKOPEN'
#!/usr/bin/env bash
out=""; err=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    --stderr) err="$2"; shift 2 ;;
    *) shift ;;
  esac
done
case "${MOCK_SCENARIO:-prompt}" in
  prompt)
    # Stderr stays ahead of completion: the JSON lands 0.3s after the last
    # line, long before the ~1s stability window closes, so the completed
    # run always carries both files.
    ( echo "tiler-macos physical-stall-exact: press ctrl+h ONCE now" >>"$err"
      sleep 0.5
      echo "second diagnostic line" >>"$err"
      sleep 0.3
      printf '{"outcome":"completed"}\n' >"$out" ) &
    ;;
  rapid)
    ( i=1
      while [[ $i -le 40 ]]; do printf 'chunk-%02d\n' "$i" >>"$err"; sleep 0.05; i=$((i + 1)); done
      printf '{"outcome":"completed"}\n' >"$out" ) &
    ;;
  stall)
    # Never stable, never complete: a tick every 0.4s defeats the ~1s
    # stability window until the wrapper deadline fires. Bounded (13 ticks)
    # so the fixture self-exits even if cleanup missed it.
    ( echo "tiler-macos physical-stall-exact: press ctrl+h ONCE now" >>"$err"
      i=1
      while [[ $i -le 13 ]]; do sleep 0.4; echo "still waiting $i" >>"$err"; i=$((i + 1)); done ) &
    ;;
esac
printf '%s\n' "$!" >>"${MOCK_PIDFILE:-/dev/null}" 2>/dev/null || true
exit 0
MOCKOPEN
chmod +x "$WORK/mockbin/pgrep" "$WORK/mockbin/uname" "$WORK/mockbin/open"
export PATH="$WORK/mockbin:$PATH"
export MOCK_PIDFILE="$MANIFEST"

WRAP="$FAKE/scripts/macos-launch.sh"

# 1. syntax.
if bash -n "$LAUNCH"; then ok "syntax"; else bad "syntax"; fi

# 2. flag/timeout validation (all exit before any launch).
if bash "$WRAP" --help >/dev/null 2>&1; then bad "--help accepted"; else
  [[ $? -eq 2 ]] && ok "--help exit 2" || bad "--help exit code"
fi
if bash "$WRAP" --bogus -- x >/dev/null 2>&1; then bad "--bogus accepted"; else ok "--bogus refused"; fi
if bash "$WRAP" --timeout 0 -- x >/dev/null 2>&1; then bad "--timeout 0 accepted"; else ok "--timeout 0 refused"; fi
if bash "$WRAP" --timeout 301 -- x >/dev/null 2>&1; then bad "--timeout 301 accepted"; else ok "--timeout 301 refused"; fi
if bash "$WRAP" --timeout abc -- x >/dev/null 2>&1; then bad "--timeout abc accepted"; else ok "--timeout abc refused"; fi
if bash "$WRAP" --timeout >/dev/null 2>&1; then bad "--timeout without value accepted"; else ok "--timeout without value refused"; fi
if bash "$WRAP" --timeout 10 >/dev/null 2>&1; then bad "missing probe args accepted"; else ok "missing probe args refused"; fi

# 3. classic (non-stream) run: stdout JSON once, full stderr once.
MOCK_SCENARIO=prompt bash "$WRAP" --timeout 20 -- probe-args \
  >"$WORK/classic-out.txt" 2>"$WORK/classic-err.txt"
[[ $? -eq 1 ]] && ok "classic exits 1 (stderr present)" || bad "classic exit code"
[[ "$(cat "$WORK/classic-out.txt")" == '{"outcome":"completed"}' ]] \
  && ok "classic stdout JSON once" || bad "classic stdout"
grep -q '^--- probe stderr ---$' "$WORK/classic-err.txt" \
  && ok "classic header" || bad "classic header missing"
[[ "$(grep -c 'ONCE now' "$WORK/classic-err.txt" || true)" -eq 1 ]] \
  && ok "classic prompt once" || bad "classic prompt count"

# 4. --stream run: prompt visible BEFORE completion, stdout once,
# stderr every line exactly once, in order.
MOCK_SCENARIO=prompt bash "$WRAP" --timeout 20 --stream -- probe-args \
  >"$WORK/stream-out.txt" 2>"$WORK/stream-err.txt" &
stream_pid=$!
prompt_seen=0
start=$SECONDS
while kill -0 $stream_pid 2>/dev/null; do
  if [[ $prompt_seen -eq 0 ]] && grep -q "ONCE now" "$WORK/stream-err.txt" 2>/dev/null; then
    prompt_seen=1
  fi
  sleep 0.1
done
wait $stream_pid; stream_code=$?
prompt_at=$((SECONDS - start))
[[ $stream_code -eq 1 ]] && ok "stream exits 1 (stderr present)" || bad "stream exit $stream_code"
[[ "$(cat "$WORK/stream-out.txt")" == '{"outcome":"completed"}' ]] \
  && ok "stream stdout JSON once" || bad "stream stdout"
[[ "$(grep -c 'ONCE now' "$WORK/stream-err.txt" || true)" -eq 1 ]] \
  && ok "stream prompt exactly once" || bad "stream prompt count"
[[ "$(grep -c 'second diagnostic line' "$WORK/stream-err.txt" || true)" -eq 1 ]] \
  && ok "stream second line exactly once" || bad "stream second-line count"
if [[ $prompt_seen -eq 1 && $prompt_at -le 5 ]]; then
  ok "stream prompt live before done (~${prompt_at}s)"
else
  bad "stream prompt not live (seen=$prompt_seen at ~${prompt_at}s)"
fi

# 5. rapid appended chunks: 40 lines interleaved with polls, each once.
MOCK_SCENARIO=rapid bash "$WRAP" --timeout 20 --stream -- probe-args \
  >"$WORK/rapid-out.txt" 2>"$WORK/rapid-err.txt"
[[ $? -eq 1 ]] && ok "rapid exits 1" || bad "rapid exit code"
i=1; rapid_ok=1
while [[ $i -le 40 ]]; do
  line="$(printf 'chunk-%02d' "$i")"
  n="$(grep -cx "$line" "$WORK/rapid-err.txt" || true)"
  [[ -z "$n" ]] && n=0
  if [[ "$n" -ne 1 ]]; then rapid_ok=0; bad "rapid $line count $n (want 1)"; fi
  i=$((i + 1))
done
[[ $rapid_ok -eq 1 ]] && ok "rapid chunks each exactly once"
[[ "$(grep -cv '^chunk-[0-9][0-9]$' "$WORK/rapid-err.txt" || true)" -eq 0 ]] \
  && ok "rapid stderr has no extra lines" || bad "rapid stderr extras"
[[ "$(cat "$WORK/rapid-out.txt")" == '{"outcome":"completed"}' ]] \
  && ok "rapid stdout JSON once" || bad "rapid stdout"

# 6. timeout: partial output streams live, wrapper still fails, never success.
MOCK_SCENARIO=stall bash "$WRAP" --timeout 3 --stream -- probe-args \
  >"$WORK/timeout-out.txt" 2>"$WORK/timeout-err.txt"
[[ $? -ne 0 ]] && ok "timeout exits nonzero" || bad "timeout exit 0"
grep -q "no terminal completion within 3s" "$WORK/timeout-err.txt" \
  && ok "timeout names the bound" || bad "timeout message"
[[ "$(grep -c 'ONCE now' "$WORK/timeout-err.txt" || true)" -eq 1 ]] \
  && ok "timeout partial prompt exactly once" || bad "timeout prompt count"
[[ "$(grep -c '^still waiting [0-9]*$' "$WORK/timeout-err.txt" || true)" -ge 3 ]] \
  && ok "timeout ticks streamed live" || bad "timeout ticks missing"

echo "---"
echo "pass=$PASS fail=$FAIL"
[[ $FAIL -eq 0 ]]

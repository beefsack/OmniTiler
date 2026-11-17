#!/usr/bin/env bash
# Serial release Callgrind gate for steady WorldEngine::handle_report cost.
# Measures the direct inclusive edge from report_scale::call to handle_report
# for two representative S1 one-output cases (50 and 200 windows).
set -euo pipefail

# Thresholds: single source of truth for this gate.
readonly CEIL_W50=24000000       # Ir/report, 50 windows, 1 output
readonly CEIL_W200=100000000     # Ir/report, 200 windows, 1 output
readonly MAX_RATIO=6.5           # 200w/50w growth bound (catches superlinear regressions)

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  case "$CARGO_TARGET_DIR" in
    /*) target_dir="$CARGO_TARGET_DIR" ;;
    *) target_dir="$REPO_ROOT/$CARGO_TARGET_DIR" ;;
  esac
else
  target_dir="$REPO_ROOT/target"
fi
binary="$target_dir/release/examples/report_scale"

scratch="$(mktemp -d)"
cleanup() { rm -rf -- "$scratch"; }
trap cleanup EXIT

# Extract the direct inclusive handle_report edge from one Callgrind profile.
# Two passes over the same file: build the function-id name map, then read the
# cfn edge whose caller is report_scale::call and whose target is handle_report.
# Fails closed (nonzero exit) on anything other than exactly four calls with a
# positive instruction count and a single Ir event type.
parse_cost() {
  awk '
    NR == FNR {
      if ($0 ~ /^(fn|cfn)=\([0-9]+\)/) {
        id = $0; sub(/^[^=]*=\(/, "", id); sub(/\).*/, "", id); id += 0
        nm = $0; sub(/^(fn|cfn)=\([0-9]+\)[ \t]*/, "", nm)
        if (nm != "") names[id] = nm
      }
      next
    }
    /^events:/ {
      if ($0 != "events: Ir") bad_events = 1
      events_seen++
      next
    }
    /^fn=\([0-9]+\)/ {
      id = $0; sub(/^fn=\(/, "", id); sub(/\).*/, "", id); id += 0
      caller = (id in names) ? names[id] : ""
      next
    }
    /^cfn=\([0-9]+\)/ {
      id = $0; sub(/^cfn=\(/, "", id); sub(/\).*/, "", id); id += 0
      target = (id in names) ? names[id] : ""
      if (target == "") { target = $0; sub(/^cfn=\([0-9]+\)[ \t]*/, "", target) }
      if (caller == "report_scale::call" && target ~ /WorldEngine>::handle_report$/) {
        if ((getline line1) <= 0) { bad = 1; next }
        if (line1 !~ /^calls=[0-9]+([ \t]|$)/) { bad = 1; next }
        c = line1; sub(/^calls=/, "", c); split(c, parts, /[ \t]+/); cnt = parts[1] + 0
        if ((getline line2) <= 0) { bad = 1; next }
        n = split(line2, fields, /[ \t]+/)
        if (fields[n] !~ /^[0-9]+$/) { bad = 1; next }
        cost = fields[n] + 0
        if (cost <= 0) { bad = 1; next }
        calls += cnt; instr += cost; edges++
      }
      next
    }
    END {
      if (bad_events || events_seen != 1 || bad || edges != 1 || calls != 4 || instr <= 0) {
        print "parse_cost: malformed or missing profile" > "/dev/stderr"
        exit 1
      }
      printf "%d %d\n", calls, instr
    }
  ' "$1" "$1"
}

profile_case() {
  local case_name="$1" out="$2"
  valgrind --tool=callgrind --vgdb=no --collect-atstart=no \
    --toggle-collect=report_scale::call \
    --callgrind-out-file="$out" \
    "$binary" "$case_name" 1 1 S1 >"$out.stdout" 2>"$out.stderr"
}

CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --locked --release -p tiler-core --example report_scale

profile_case w50-o1 "$scratch/w50.out"
profile_case w200-o1 "$scratch/w200.out"

read -r w50_calls w50_instr < <(parse_cost "$scratch/w50.out")
read -r w200_calls w200_instr < <(parse_cost "$scratch/w200.out")

w50_per="$(awk -v c="$w50_calls" -v i="$w50_instr" 'BEGIN { printf "%.1f", i / c }')"
w200_per="$(awk -v c="$w200_calls" -v i="$w200_instr" 'BEGIN { printf "%.1f", i / c }')"
ratio="$(awk -v a="$w50_instr" -v ac="$w50_calls" -v b="$w200_instr" -v bc="$w200_calls" \
  'BEGIN { printf "%.2f", (b / bc) / (a / ac) }')"

echo "report cost gate: w50=${w50_per} Ir/report (ceiling ${CEIL_W50}), w200=${w200_per} Ir/report (ceiling ${CEIL_W200}), growth=${ratio} (max ${MAX_RATIO})"

failed=0
if awk -v v="$w50_per" -v t="$CEIL_W50" 'BEGIN { exit !(v > t) }'; then
  echo "FAIL: 50-window cost exceeds ceiling" >&2; failed=1
fi
if awk -v v="$w200_per" -v t="$CEIL_W200" 'BEGIN { exit !(v > t) }'; then
  echo "FAIL: 200-window cost exceeds ceiling" >&2; failed=1
fi
if awk -v v="$ratio" -v t="$MAX_RATIO" 'BEGIN { exit !(v > t) }'; then
  echo "FAIL: 200w/50w growth exceeds bound" >&2; failed=1
fi

if (( failed )); then
  cat >&2 <<EOF
Report-cost gate FAILED.

Investigate an unexpected regression before changing anything. If the cost change is intentional (a deliberate cost or toolchain change), rerun this script to confirm stable numbers, then update only the threshold constants at the top of this script.

The user-approved latency budget is a separate manual check: run the native harness with "report_scale all 31 3 all" (samples 31, reps 3, all shapes).
EOF
  exit 1
fi

echo "report cost gate: PASS"

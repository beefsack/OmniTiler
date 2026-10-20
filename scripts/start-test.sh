#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
KWIN_DIR="$REPO_ROOT/kwin"
BUNDLE="$KWIN_DIR/contents/code/main.js"
META="$KWIN_DIR/metadata.json"
PROC_ROOT="${PROC_ROOT:-/proc}"

BUS_SCOPE="--user"
BUS_DEST="org.kde.KWin"
BUS_PATH="/Scripting"
BUS_SCRIPTING_IFACE="org.kde.kwin.Scripting"
BUS_SCRIPT_IFACE="org.kde.kwin.Script"
OWNED_RECEIPT=""
OWNED_NONCE=""
OWNED_BUILD=""
EXACT_CLEANUP_AFTER=""

VDSK_PATH="/VirtualDesktopManager"
VDSK_IFACE="org.kde.KWin.VirtualDesktopManager"

KG_DEST="org.kde.kglobalaccel"
KG_PATH="/kglobalaccel"
KG_IFACE="org.kde.KGlobalAccel"
KG_COMP_IFACE="org.kde.kglobalaccel.Component"

PLUGIN_ID=""
SCRIPT_ID=""
KWIN_PID=""
KWIN_START_IDENTITY=""
# Immutable identity captured before a lifecycle operation. Rechecks compare
# against this value without replacing it with a reused-PID identity.
KWIN_PREOP_PID=""
KWIN_PREOP_START_IDENTITY=""
KWIN_IDENTITY_MISMATCH=0
CONTROLLER_OWNERSHIP_FILE="${CONTROLLER_OWNERSHIP_FILE:-}"
START_NONCE="${START_NONCE:-}"

# The exact project action IDs this lifecycle interface owns.
PROJECT_ACTIONS=(
  omnitiler-focus-left
  omnitiler-focus-down
  omnitiler-focus-up
  omnitiler-focus-right
  omnitiler-focus-left-arrow
  omnitiler-focus-down-arrow
  omnitiler-focus-up-arrow
  omnitiler-focus-right-arrow
  omnitiler-move-left
  omnitiler-move-down
  omnitiler-move-up
  omnitiler-move-right
  omnitiler-move-left-arrow
  omnitiler-move-down-arrow
  omnitiler-move-up-arrow
  omnitiler-move-right-arrow
  omnitiler-resize-outwards-left
  omnitiler-resize-outwards-down
  omnitiler-resize-outwards-up
  omnitiler-resize-outwards-right
  omnitiler-resize-inwards-left
  omnitiler-resize-inwards-down
  omnitiler-resize-inwards-up
  omnitiler-resize-inwards-right
  omnitiler-resize-inwards-left-arrow
  omnitiler-resize-inwards-down-arrow
  omnitiler-resize-inwards-up-arrow
  omnitiler-resize-inwards-right-arrow
  omnitiler-toggle-float
  omnitiler-toggle-sticky
  omnitiler-toggle-maximize
  omnitiler-toggle-fullscreen
)
PROJECT_ACTIONS_JSON=""

# Expected source-default active sequence per project action, in the Qt
# integer encoding KGlobalAccel exposes through the allShortcutInfos active
# field and accepts through setShortcutKeys (modifier bits OR key code).
# Provenance: controller action defaults, encoded with the pinned Qt 6
# KeyboardModifier bits and verified against the live collector on 2026-08-12.
declare -A EXPECTED_SEQUENCES=(
  [omnitiler-focus-left]="268435528"
  [omnitiler-focus-down]="268435530"
  [omnitiler-focus-up]="268435531"
  [omnitiler-focus-right]="268435532"
  [omnitiler-focus-left-arrow]="285212690"
  [omnitiler-focus-down-arrow]="285212693"
  [omnitiler-focus-up-arrow]="285212691"
  [omnitiler-focus-right-arrow]="285212692"
  [omnitiler-move-left]="301989960"
  [omnitiler-move-down]="301989962"
  [omnitiler-move-up]="301989963"
  [omnitiler-move-right]="301989964"
  [omnitiler-move-left-arrow]="318767122"
  [omnitiler-move-down-arrow]="318767125"
  [omnitiler-move-up-arrow]="318767123"
  [omnitiler-move-right-arrow]="318767124"
  [omnitiler-resize-outwards-left]="402653256"
  [omnitiler-resize-outwards-down]="402653258"
  [omnitiler-resize-outwards-up]="402653259"
  [omnitiler-resize-outwards-right]="402653260"
  [omnitiler-resize-inwards-left]="436207688"
  [omnitiler-resize-inwards-down]="436207690"
  [omnitiler-resize-inwards-up]="436207691"
  [omnitiler-resize-inwards-right]="436207692"
  [omnitiler-resize-inwards-left-arrow]="452984850"
  [omnitiler-resize-inwards-down-arrow]="452984853"
  [omnitiler-resize-inwards-up-arrow]="452984851"
  [omnitiler-resize-inwards-right-arrow]="452984852"
  [omnitiler-toggle-float]="268435527"
  [omnitiler-toggle-sticky]="301989959"
  [omnitiler-toggle-maximize]="268435533"
  [omnitiler-toggle-fullscreen]="285212730"
)

# KGlobalAccelD::SetShortcutFlag values (pinned kglobalacceld 6.7.3 source):
# SetPresent=2, NoAutoloading=4, IsDefault=8. A user-style active assignment
# on an existing record forces the change with SetPresent|NoAutoloading = 6.
KG_SET_SHORTCUT_FLAGS=6

# Strict JSON envelope predicates (jq).
isloaded_valid='((keys | sort) == ["data","type"]) and (.type == "b") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "boolean")'
load_valid='((keys | sort) == ["data","type"]) and (.type == "i") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "number") and ((.data[0] | floor) == .data[0]) and ((.data[0] >= 0) and (.data[0] <= 2147483647))'
script_iface_valid='type == "array" and any(.[]; ((.type == "interface") and (.name == "org.kde.kwin.Script")))'
unload_valid="$isloaded_valid"
ownership_valid='((keys | sort) == ["build","kind","nonce","pid","plugin","script_id","start_identity"]) and (.kind == $kind) and (.nonce | type) == "string" and (.nonce | test("^[A-Za-z0-9][A-Za-z0-9._-]{7,63}$")) and (.build | type) == "string" and (.build | test($build_pattern)) and (.plugin | type) == "string" and (.plugin | test("^[A-Za-z0-9][A-Za-z0-9._-]{7,127}$")) and (.script_id | type) == "number" and (.script_id | floor) == .script_id and (.script_id >= 0) and (.script_id <= 2147483647) and (.pid | type) == "number" and (.pid | floor) == .pid and (.pid > 0) and (.start_identity | type) == "string" and (.start_identity | test("^[1-9][0-9]*$"))'
components_valid='((keys | sort) == ["data","type"]) and (.type == "ao") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "array") and (all(.data[0][]; (. | type) == "string"))'
shortcut_infos_valid='((keys | sort) == ["data","type"]) and (.type == "a(ssssssaiai)") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "array") and (all(.data[0][]; ((. | length) == 8) and ((.[0] | type) == "string") and ((.[1] | type) == "string") and ((.[2] | type) == "string") and ((.[3] | type) == "string") and ((.[4] | type) == "string") and ((.[5] | type) == "string") and ((.[6] | type) == "array") and (all(.[6][]; (. | type) == "number")) and ((.[7] | type) == "array") and (all(.[7][]; (. | type) == "number"))))'
dbus_string_valid='((keys | sort) == ["data","type"]) and (.type == "s") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "string") and ((.data[0] | length) > 0)'
dbus_uint_valid='((keys | sort) == ["data","type"]) and (.type == "u") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "number") and ((.data[0] | floor) == .data[0]) and ((.data[0] | tostring | test("^(0|[1-9][0-9]*)$"))) and (.data[0] >= 0) and (.data[0] <= 4294967295)'
dbus_pid_valid='((keys | sort) == ["data","type"]) and (.type == "u") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "number") and ((.data[0] | floor) == .data[0]) and ((.data[0] | tostring | test("^[1-9][0-9]*$"))) and (.data[0] > 0) and (.data[0] <= 4294967295)'
desktops_valid='((keys | sort) == ["data","type"]) and (.type == "a(uss)") and ((.data | type) == "array") and (all(.data[]; ((. | type) == "array") and ((. | length) == 3) and ((.[0] | type) == "number") and ((.[0] | floor) == .[0]) and (.[0] >= 0) and ((.[1] | type) == "string") and ((.[1] | length) > 0) and ((.[2] | type) == "string") and ((.[2] | length) > 0))) and ((.data | map(.[0])) as $positions | ($positions | unique | length) == ($positions | length)) and ((.data | map(.[1])) as $ids | ($ids | unique | length) == ($ids | length))'
# Group E single-engine dev loop: no journal readiness wait, no
# dual-runtime lifecycle assertions. Bring-up proves the load through the
# strict isScriptLoaded envelope plus the receipt-bound exact Script<ID>
# introspection below; teardown is receipt-bound exact unload.

usage() {
  cat <<'EOF'
usage: start-test.sh <command> [--help]

Manual lifecycle interface for the omnitiler-kwin KWin script.

Commands:
  start [--scope-class NAME]...
            build the kwin bundle, load and run omnitiler through
            KWin's /Scripting D-Bus interface, and confirm the exact
            loaded state bound to the captured KWin PID/start identity.
            Each --scope-class adds one comma-joined allowlist fragment;
            repeated flags accumulate. Only windows whose exact
            resourceClass (Wayland app_id) is listed are managed; empty
            (default) manages the whole world
  status   report the exact plugin load state and persisted KGlobalAccel
            action records
  stop <script-id>
            unload only the exact controller script ID returned by start and
            report any persisted action records
  desktops
           read the exact VirtualDesktopManager desktops envelope through
           busctl and report the strictly decoded position/id/name rows;
           read-only, never mutates
  reconcile-shortcuts
           report persisted project shortcut records whose active
           sequence differs from the source-default expected sequence;
           read-only, never mutates
  reconcile-shortcuts --apply
            write the expected active sequence to each mismatched project
            record through org.kde.KGlobalAccel.setShortcutKeys, but only
            after a read-only preflight proves the exact setter contract,
            target ownership, and absence of unrelated conflicts

  snapshot-shortcuts
            print the exact project-owned KGlobalAccel tuples as JSON;
            read-only, never mutates
  snapshot-kglobalaccel
            print the exact current KGlobalAccel service owner identity;
            read-only, never mutates

  --help   show this help and exit

start mutates live KWin state and still requires explicit authorization.
start never mutates shortcut records; only reconcile-shortcuts --apply does.
  stop <script-id> requires the nonce-owned receipt created by start.
  start without CONTROLLER_OWNERSHIP_FILE creates a private random receipt and
  prints its exact stop command. Malformed scope entries fail closed
  before any build or load; changing scope requires a fresh start
  (stop first, then start with the new scope flags).
KGlobalAccel records persist after unload and do not prove live callbacks.
status, desktops, and reconcile-shortcuts are read-only.
EOF
}

require_tools() {
  local tool
  for tool in "$@"; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      echo "error: required tool '$tool' not found in PATH" >&2
      exit 1
    fi
  done
}

strict_json_matches() {
  local predicate="$1" value="$2"
  jq -s -e "length == 1 and (.[0] | $predicate)" <<<"$value" >/dev/null 2>&1
}

# Dev/test admission scope allowlist: comma-joined exact resourceClass
# identities (the class charset excludes commas, so splitting is exact).
# scope_validate rejects any malformed entry; scope_append validates one
# flag fragment and accumulates it onto OMNITILER_SCOPE_CLASS in order.
scope_validate() {
  local allowlist="$1" origin="$2" entry
  local -a entries=()
  # `read -a` drops a trailing empty field, so comma edges are checked
  # explicitly: no leading, trailing, or doubled commas (no empty entry).
  case "$allowlist" in
    ""|,*|*,|*",,"*)
      echo "error: invalid $origin entry '' (expected 1-128 chars of [A-Za-z0-9._-], leading alnum)" >&2
      return 1
      ;;
  esac
  IFS=',' read -r -a entries <<< "$allowlist"
  for entry in ${entries[@]+"${entries[@]}"}; do
    [[ "$entry" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || {
      echo "error: invalid $origin entry '$entry' (expected 1-128 chars of [A-Za-z0-9._-], leading alnum)" >&2
      return 1
    }
  done
}

scope_append() {
  local fragment="$1"
  [[ -n "$fragment" ]] || {
    echo "error: --scope-class requires a non-empty value" >&2
    return 1
  }
  scope_validate "$fragment" "--scope-class" || return 1
  if [[ -z "${OMNITILER_SCOPE_CLASS:-}" ]]; then
    OMNITILER_SCOPE_CLASS="$fragment"
  else
    OMNITILER_SCOPE_CLASS="${OMNITILER_SCOPE_CLASS:-},$fragment"
  fi
}

read_plugin_id() {
  PLUGIN_ID="$(jq -r '.KPlugin.Id' "$META")" || {
    echo "error: could not read KPlugin.Id from $META" >&2
    exit 1
  }
  if [[ -z "$PLUGIN_ID" || "$PLUGIN_ID" == "null" ]]; then
    echo "error: missing KPlugin.Id in $META" >&2
    exit 1
  fi
  if [[ "$PLUGIN_ID" != "omnitiler-kwin" ]]; then
    echo "error: refusing to operate on unexpected plugin id '$PLUGIN_ID' (expected omnitiler-kwin)" >&2
    exit 1
  fi
}

safe_output_path() {
  local path="$1" parent current component
  [[ "$path" == /* && "$path" != *'//'* && "$path" != *'/../'* && "$path" != */.. && "$path" != */./* && "$path" != */. ]] || return 1
  parent="${path%/*}"
  [[ -n "$parent" ]] || parent=/
  current=/
  IFS=/ read -r -a components <<<"${parent#/}"
  for component in "${components[@]}"; do
    [[ -n "$component" ]] || continue
    current="${current%/}/$component"
    [[ -d "$current" && ! -L "$current" ]] || return 1
  done
  [[ ! -L "$path" ]]
}

ownership_json() {
  local kind="$1" nonce="$2" build="$3" plugin="$4" script_id="$5"
  jq -cn --arg kind "$kind" --arg nonce "$nonce" --arg build "$build" \
    --arg plugin "$plugin" --arg script_id "$script_id" --arg pid "$KWIN_PID" \
    --arg start "$KWIN_START_IDENTITY" \
    '{kind:$kind,nonce:$nonce,build:$build,plugin:$plugin,script_id:($script_id|tonumber),pid:($pid|tonumber),start_identity:$start}'
}

write_ownership() {
  local file="$1" kind="$2" nonce="$3" build="$4" plugin="$5" script_id="$6" tmp
  [[ -n "$file" ]] || return 0
  safe_output_path "$file" || return 1
  [[ ! -e "$file" && ! -L "$file" ]] || return 1
  tmp="$(mktemp "${file%/*}/.ownership.XXXXXX")" || return 1
  safe_output_path "$tmp" || { remove_owned_file "$tmp" || true; return 1; }
  ownership_json "$kind" "$nonce" "$build" "$plugin" "$script_id" > "$tmp" \
    && safe_output_path "$file" \
    && mv -n -- "$tmp" "$file" || { remove_owned_file "$tmp" || true; return 1; }
  safe_output_path "$file" || return 1
  [[ ! -e "$tmp" && -f "$file" && ! -L "$file" ]] || { remove_owned_file "$tmp" || true; return 1; }
  OWNED_RECEIPT="$(<"$file")"
}

ensure_controller_receipt() {
  [[ -z "$CONTROLLER_OWNERSHIP_FILE" ]] || return 0
  local runtime_dir="${XDG_RUNTIME_DIR:-/tmp}"
  safe_output_path "$runtime_dir/.omnitiler-controller-receipt" || return 1
  local receipt_dir
  receipt_dir="$(mktemp -d -- "$runtime_dir/omnitiler-controller.XXXXXX")" || return 1
  chmod 700 "$receipt_dir" || { rmdir -- "$receipt_dir"; return 1; }
  CONTROLLER_OWNERSHIP_FILE="$receipt_dir/ownership"
}

validate_controller_receipt_target() {
  [[ -n "$CONTROLLER_OWNERSHIP_FILE" ]] || return 0
  safe_output_path "$CONTROLLER_OWNERSHIP_FILE" || return 1
  [[ ! -e "$CONTROLLER_OWNERSHIP_FILE" && ! -L "$CONTROLLER_OWNERSHIP_FILE" ]]
}


remove_owned_file() {
  local file="$1" expected="${2:-}" parent name identity inode
  safe_output_path "$file" || return 1
  [[ -f "$file" && ! -L "$file" ]] || return 0
  if [[ -n "$expected" && "$(<"$file")" != "$expected" ]]; then
    return 1
  fi
  identity="$(stat -c '%d:%i' -- "$file")" || return 1
  inode="${identity#*:}"
  parent="${file%/*}"
  name="${file##*/}"
  safe_output_path "$file" || return 1
  [[ "$(stat -c '%d:%i' -- "$file")" == "$identity" ]] || return 1
  find -P -- "$parent" -xdev -maxdepth 1 -type f -name "$name" -inum "$inode" -delete || return 1
  [[ ! -e "$file" && ! -L "$file" ]]
}


remove_ownership() {
  [[ -n "$1" ]] || return 0
  remove_owned_file "$@"
}


load_ownership() {
  local file="$1" kind="$2" expected_id="$3" expected_plugin="$4" value
  [[ -n "$file" && -f "$file" && ! -L "$file" ]] || return 1
  value="$(<"$file")" || return 1
  local build_pattern='^controller-v1-[0-9a-f]{64}$'
  jq -s -e --arg kind "$kind" --arg build_pattern "$build_pattern" "length == 1 and (.[0] | $ownership_valid)" <<<"$value" >/dev/null 2>&1 || return 1
  jq -s -e --argjson expected_id "$expected_id" 'length == 1 and (.[0].script_id == $expected_id)' <<<"$value" >/dev/null 2>&1 || return 1
  [[ -z "$expected_plugin" || "$(jq -r '.plugin' <<<"$value")" == "$expected_plugin" ]] || return 1
  KWIN_PID="$(jq -r '.pid' <<<"$value")"
  KWIN_START_IDENTITY="$(jq -r '.start_identity' <<<"$value")"
  KWIN_PREOP_PID="$KWIN_PID"
  KWIN_PREOP_START_IDENTITY="$KWIN_START_IDENTITY"
  KWIN_IDENTITY_MISMATCH=0
  OWNED_RECEIPT="$value"
  OWNED_NONCE="$(jq -r '.nonce' <<<"$value")"
  OWNED_BUILD="$(jq -r '.build' <<<"$value")"
  printf '%s\n' "$value"
}


capture_kwin_identity() {
  [[ "$KWIN_PID" =~ ^[1-9][0-9]*$ ]] || return 1
  local stat_line stat_pid rest
  local -a fields=()
  stat_line="$(<"$PROC_ROOT/$KWIN_PID/stat")" || return 1
  [[ "$stat_line" != *$'\n'* ]] || return 1
  stat_pid="${stat_line%% *}"
  [[ "$stat_pid" == "$KWIN_PID" ]] || return 1
  rest="${stat_line##*) }"
  [[ "$rest" != "$stat_line" ]] || return 1
  read -r -a fields <<<"$rest"
  [[ "${#fields[@]}" -ge 20 && "${fields[0]:-}" =~ ^[A-Za-z]$ ]] || return 1
  [[ "${fields[19]:-}" =~ ^[1-9][0-9]*$ ]] || return 1
  KWIN_START_IDENTITY="${fields[19]}"
}


kwin_identity_unchanged() {
  local expected_pid="${KWIN_PREOP_PID:-$KWIN_PID}" expected_start="${KWIN_PREOP_START_IDENTITY:-$KWIN_START_IDENTITY}"
  local current_pid
  current_pid="$(find_kwin_pid 2>/dev/null || true)"
  [[ "$current_pid" == "$expected_pid" ]] || return 1
  local current_start
  KWIN_PID="$current_pid"
  capture_kwin_identity || return 1
  current_start="$KWIN_START_IDENTITY"
  KWIN_PID="$expected_pid"
  KWIN_START_IDENTITY="$expected_start"
  if [[ "$current_start" != "$expected_start" ]]; then
    KWIN_IDENTITY_MISMATCH=1
    return 1
  fi
}


verify_exact_script() {
  local id="$1" plugin="$2" introspect loaded
  [[ "$id" =~ ^[0-9]+$ && "$id" -le 2147483647 ]] || return 1
  local receipt_file receipt_kind receipt
  receipt_file="$CONTROLLER_OWNERSHIP_FILE"
  receipt_kind=controller
  [[ -n "$receipt_file" && -f "$receipt_file" && ! -L "$receipt_file" ]] || return 1
  receipt="$(load_ownership "$receipt_file" "$receipt_kind" "$id" "$plugin")" || return 1
  [[ "$receipt" == "$OWNED_RECEIPT" ]] || return 1
  kwin_identity_unchanged || return 1
  introspect="$(busctl $BUS_SCOPE --json=short introspect "$BUS_DEST" "/Scripting/Script$id" 2>/dev/null)" || return 1
  strict_json_matches "$script_iface_valid" "$introspect" || return 1
  loaded="$(plugin_loaded_word 2>/dev/null || true)"
  [[ "$loaded" == loaded ]] || return 1
  kwin_identity_unchanged || return 1
}


kwin_identity_matches_receipt() {
  local receipt="$1" expected_pid expected_start current_pid current_start
  expected_pid="$(jq -r '.pid' <<<"$receipt")" || return 1
  expected_start="$(jq -r '.start_identity' <<<"$receipt")" || return 1
  [[ "$expected_pid" =~ ^[1-9][0-9]*$ && "$expected_start" =~ ^[1-9][0-9]*$ ]] || return 1
  current_pid="$(find_kwin_pid 2>/dev/null || true)"
  [[ "$current_pid" == "$expected_pid" ]] || return 1
  local previous_pid="$KWIN_PID" previous_start="$KWIN_START_IDENTITY"
  KWIN_PID="$current_pid"
  if ! capture_kwin_identity; then
    KWIN_PID="$previous_pid"
    KWIN_START_IDENTITY="$previous_start"
    return 1
  fi
  current_start="$KWIN_START_IDENTITY"
  KWIN_PID="$previous_pid"
  KWIN_START_IDENTITY="$previous_start"
  if [[ "$current_start" != "$expected_start" ]]; then
    KWIN_IDENTITY_MISMATCH=1
    return 1
  fi
}


exact_cleanup() {
  local id="$1" plugin="$2" rc=0 out after receipt="${OWNED_RECEIPT:-}"
  local stop_rc=0 unload_false=0 unload_bad=0 after_ok=0 identity_ok=0
  EXACT_CLEANUP_AFTER=""
  verify_exact_script "$id" "$plugin" || return 1
  busctl $BUS_SCOPE call "$BUS_DEST" "/Scripting/Script$id" $BUS_SCRIPT_IFACE stop >/dev/null 2>&1 || { stop_rc=1; rc=1; }
  if ! out="$(busctl $BUS_SCOPE --json=short call "$BUS_DEST" "$BUS_PATH" $BUS_SCRIPTING_IFACE unloadScript s "$plugin" 2>/dev/null)"; then
    echo "error: unloadScript call failed; teardown remains unverified" >&2
    rc=1
    unload_bad=1
    out=""
  elif ! strict_json_matches "$unload_valid" "$out"; then
    echo "error: unloadScript reply was malformed; teardown remains unverified" >&2
    rc=1
    unload_bad=1
  elif [[ "$(jq -r '.data[0]' <<<"$out")" != true ]]; then
    unload_false=1
  fi
  # Strict postcondition plus receipt-bound KWin identity are the only
  # positive proof. A failed/malformed isScriptLoaded call leaves after empty
  # and therefore unverified. No plugin-name fallback, no unload retries.
  after="$(plugin_loaded_word 2>/dev/null || true)"
  EXACT_CLEANUP_AFTER="$after"
  [[ "$after" == not-loaded ]] && after_ok=1
  if kwin_identity_matches_receipt "$receipt" 2>/dev/null; then
    identity_ok=1
  else
    identity_ok=0
  fi
  if [[ "$unload_false" -eq 1 ]]; then
    if [[ "$stop_rc" -eq 0 && "$unload_bad" -eq 0 && "$after_ok" -eq 1 && "$identity_ok" -eq 1 ]]; then
      return 0
    fi
    echo "error: unloadScript returned false; teardown remains unverified" >&2
    if [[ "$identity_ok" -eq 0 ]]; then
      echo "error: KWin PID/start identity after unloadScript did not match the immutable ownership receipt; teardown remains unverified" >&2
    fi
    return 1
  fi
  [[ "$after_ok" -eq 1 ]] || rc=1
  if [[ "$identity_ok" -eq 0 ]]; then
    echo "error: KWin PID/start identity after unloadScript did not match the immutable ownership receipt; teardown remains unverified" >&2
    rc=1
  fi
  return "$rc"
}

# Exact idempotent stop/unload of a directly loaded script. No script ID means
# no teardown: the public API cannot safely identify a partially loaded script.

cleanup_loaded() {
  [[ -n "$SCRIPT_ID" ]] || return 1
  if exact_cleanup "$SCRIPT_ID" "$PLUGIN_ID"; then
    remove_ownership "$CONTROLLER_OWNERSHIP_FILE" "$OWNED_RECEIPT"
  else
    return 1
  fi
}


cleanup_after_load() {
  local msg="$1"
  local cleanup_state=unverified
  if cleanup_loaded; then cleanup_state=verified; fi
  printf 'start: partial script-id=%s cleanup=%s\n' "${SCRIPT_ID:-unknown}" "$cleanup_state"
  echo "error: $msg" >&2
  echo "note: exact controller teardown was $cleanup_state; no plugin-name fallback was attempted" >&2
  exit 1
}

find_kwin_pid() {
  local owner_out owner pid_out pid
  owner_out="$(busctl $BUS_SCOPE --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$BUS_DEST")" || return 1
  strict_json_matches "$dbus_string_valid" "$owner_out" || return 1
  owner="$(jq -r '.data[0]' <<<"$owner_out")"
  [[ "$owner" =~ ^:[0-9]+\.[0-9]+$ ]] || return 1
  pid_out="$(busctl $BUS_SCOPE --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$owner")" || return 1
  strict_json_matches "$dbus_uint_valid" "$pid_out" || return 1
  pid="$(jq -r '.data[0]' <<<"$pid_out")"
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  printf '%s\n' "$pid"
}

# Prints "loaded" or "not-loaded"; fails the script on transport or shape errors.
plugin_loaded_word() {
  local out
  out="$(busctl $BUS_SCOPE --json=short call "$BUS_DEST" "$BUS_PATH" $BUS_SCRIPTING_IFACE isScriptLoaded s "$PLUGIN_ID")" || {
    echo "error: isScriptLoaded call failed: $out" >&2
    exit 1
  }
  if ! strict_json_matches "$isloaded_valid" "$out"; then
    echo "error: unexpected isScriptLoaded reply: $out" >&2
    exit 1
  fi
  if [[ "$(jq -r '.data[0]' <<<"$out")" == "true" ]]; then
    printf 'loaded\n'
  else
    printf 'not-loaded\n'
  fi
}

ensure_actions_json() {
  if [[ -z "$PROJECT_ACTIONS_JSON" ]]; then
    PROJECT_ACTIONS_JSON="$(printf '%s\n' "${PROJECT_ACTIONS[@]}" | jq -R -s -c 'split("\n") | map(select(length > 0))')"
  fi
}

# Prints one TSV line per persisted project action record:
# component<TAB>action<TAB>label<TAB>active<TAB>default.
# Fail-closed: malformed envelopes are an error, never zero matches.
collect_project_action_records() {
  local comps comp infos
  ensure_actions_json
  comps="$(busctl $BUS_SCOPE --json=short call "$KG_DEST" "$KG_PATH" "$KG_IFACE" allComponents)" || {
    echo "error: KGlobalAccel allComponents call failed: $comps" >&2
    return 1
  }
  if ! strict_json_matches "$components_valid" "$comps"; then
    echo "error: unexpected allComponents reply: $comps" >&2
    return 1
  fi
  while IFS= read -r comp; do
    infos="$(busctl $BUS_SCOPE --json=short call "$KG_DEST" "$comp" "$KG_COMP_IFACE" allShortcutInfos s default)" || {
      echo "error: allShortcutInfos call failed for $comp: $infos" >&2
      return 1
    }
    if ! strict_json_matches "$shortcut_infos_valid" "$infos"; then
      echo "error: unexpected allShortcutInfos reply for $comp" >&2
      return 1
    fi
    jq -r --argjson actions "$PROJECT_ACTIONS_JSON" \
      '.data[0][] | select((.[0] as $id | $actions | index($id)) != null) | [.[2], .[0], .[1], (.[6] | join(",")), (.[7] | join(","))] | @tsv' \
      <<<"$infos" || {
      echo "error: could not filter project action records for $comp" >&2
      return 1
    }
  done < <(jq -r '.data[0][]' <<<"$comps")
}

collect_project_action_tuples() {
  local comps comp infos rows tuples='[]'
  ensure_actions_json
  comps="$(busctl $BUS_SCOPE --json=short call "$KG_DEST" "$KG_PATH" "$KG_IFACE" allComponents)" || {
    echo "error: KGlobalAccel allComponents call failed: $comps" >&2
    return 1
  }
  if ! strict_json_matches "$components_valid" "$comps"; then
    echo "error: unexpected allComponents reply: $comps" >&2
    return 1
  fi
  while IFS= read -r comp; do
    infos="$(busctl $BUS_SCOPE --json=short call "$KG_DEST" "$comp" "$KG_COMP_IFACE" allShortcutInfos s default)" || {
      echo "error: allShortcutInfos call failed for $comp: $infos" >&2
      return 1
    }
    if ! strict_json_matches "$shortcut_infos_valid" "$infos"; then
      echo "error: unexpected allShortcutInfos reply for $comp" >&2
      return 1
    fi
    rows="$(jq -c --argjson actions "$PROJECT_ACTIONS_JSON" '.data[0] | map(select((.[0] as $id | $actions | index($id)) != null))' <<<"$infos")" || return 1
    tuples="$(jq -c --argjson rows "$rows" '. + $rows' <<<"$tuples")" || return 1
  done < <(jq -r '.data[0][]' <<<"$comps")
  printf '%s\n' "$tuples"
}

print_records() {
  local records="$1"
  local comp action label active default
  if [[ -z "$records" ]]; then
    return 0
  fi
  while IFS=$'\t' read -r comp action label active default; do
    printf '  component "%s" action "%s" label "%s" active "%s" default "%s"\n' "$comp" "$action" "$label" "$active" "$default"
  done <<<"$records"
}

count_records() {
  local records="$1"
  printf '%s\n' "$records" | awk 'NF { n++ } END { print n+0 }'
}

report_shortcut_drift() {
  local records="$1"
  local comp action label active default expected
  local -A active_by_action=()
  while IFS=$'\t' read -r comp action label active default; do
    [[ -z "$action" ]] && continue
    active_by_action["$action"]="$active"
  done <<<"$records"

  local matched=0 mismatched=0 missing=0
  for action in "${PROJECT_ACTIONS[@]}"; do
    expected="${EXPECTED_SEQUENCES[$action]}"
    if [[ ! "${active_by_action[$action]+x}" ]]; then
      missing=$((missing + 1))
    elif [[ "${active_by_action[$action]}" == "$expected" ]]; then
      matched=$((matched + 1))
    else
      mismatched=$((mismatched + 1))
      echo "  drift: action \"$action\" active \"${active_by_action[$action]}\" expected \"$expected\""
    fi
  done
  echo "shortcut assignments: matched $matched, drift $mismatched, missing $missing"
  if [[ "$mismatched" -gt 0 || "$missing" -gt 0 ]]; then
    echo "note: persisted shortcut assignments drift from controller source; run '$0 reconcile-shortcuts' to inspect."
  fi
}

cmd_snapshot_shortcuts() {
  require_tools busctl jq stat
  collect_project_action_tuples
}

cmd_snapshot_kglobalaccel() {
  require_tools busctl jq
  local owner pid uid out uid_compact uid_canonical
  out="$(busctl $BUS_SCOPE --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$KG_DEST")" || exit 1
  strict_json_matches "$dbus_string_valid" "$out" || { echo "error: malformed KGlobalAccel service owner reply" >&2; exit 1; }
  owner="$(jq -r '.data[0]' <<<"$out")"
  [[ "$owner" =~ ^:[0-9]+\.[0-9]+$ ]] || { echo "error: KGlobalAccel owner is not a unique name" >&2; exit 1; }
  out="$(busctl $BUS_SCOPE --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$owner")" || exit 1
  strict_json_matches "$dbus_pid_valid" "$out" || { echo "error: malformed KGlobalAccel owner PID reply" >&2; exit 1; }
  pid="$(jq -r '.data[0]' <<<"$out")"
  out="$(busctl $BUS_SCOPE --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixUser s "$owner")" || exit 1
  strict_json_matches "$dbus_uint_valid" "$out" || { echo "error: malformed KGlobalAccel owner UID reply" >&2; exit 1; }
  uid_compact="${out//[[:space:]]/}"
  uid_canonical="$(jq -c '.' <<<"$out")" || { echo "error: malformed KGlobalAccel owner UID reply" >&2; exit 1; }
  [[ "$uid_compact" == "$uid_canonical" ]] || { echo "error: malformed KGlobalAccel owner UID reply" >&2; exit 1; }
  uid="$(jq -r '.data[0]' <<<"$out")"
  jq -cn --arg owner "$owner" --argjson pid "$pid" --argjson uid "$uid" \
    '{service:"org.kde.kglobalaccel",owner:$owner,pid:$pid,uid:$uid}'
}

cmd_start() {
  require_tools npm busctl jq sha256sum stat
  read_plugin_id
  safe_output_path "$BUNDLE" || { echo "error: controller bundle path is unsafe" >&2; exit 1; }
  # Dev/test admission scope: comma-joined exact resourceClass allowlist
  # baked into the bundle at build time ("" disables). Every entry is
  # validated here, before any build or load, so a typo fails closed
  # instead of silently managing the whole world.
  local scope_class="${OMNITILER_SCOPE_CLASS:-}"
  if [[ -n "$scope_class" ]]; then
    scope_validate "$scope_class" "scope class" || exit 1
  fi
  export OMNITILER_SCOPE_CLASS="$scope_class"
  local start_nonce controller_build source_digest
  start_nonce="${START_NONCE:-start-$(date +%Y%m%dT%H%M%S)-$$}"
  [[ "$start_nonce" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{7,63}$ ]] || {
    echo "error: start nonce is invalid" >&2
    exit 1
  }
  source_digest="$(sha256sum "$KWIN_DIR"/src/*.ts | sha256sum | awk '{print $1}')" || {
    echo "error: could not calculate controller source identity" >&2
    exit 1
  }
  [[ "$source_digest" =~ ^[[:xdigit:]]{64}$ ]] || {
    echo "error: controller source identity is invalid" >&2
    exit 1
  }
  controller_build="controller-v1-$source_digest"

  local build_script="build"
  if [[ "${OMNITILER_TRACE:-}" == "1" ]]; then
    build_script="build:trace"
  fi
  if ! ( cd "$KWIN_DIR" && npm run "$build_script" ); then
    echo "error: npm run $build_script failed in $KWIN_DIR" >&2
    exit 1
  fi
  if [[ ! -f "$BUNDLE" ]]; then
    echo "error: bundle not found after build: $BUNDLE" >&2
    exit 1
  fi

  local is_loaded_out
  is_loaded_out="$(busctl $BUS_SCOPE --json=short call "$BUS_DEST" "$BUS_PATH" $BUS_SCRIPTING_IFACE isScriptLoaded s "$PLUGIN_ID")" || {
    echo "error: isScriptLoaded call failed: $is_loaded_out" >&2
    exit 1
  }
  if ! strict_json_matches "$isloaded_valid" "$is_loaded_out"; then
    echo "error: unexpected isScriptLoaded reply: $is_loaded_out" >&2
    exit 1
  fi
  if [[ "$(jq -r '.data[0]' <<<"$is_loaded_out")" == "true" ]]; then
    echo "error: plugin '$PLUGIN_ID' is already loaded; refusing to load again" >&2
    echo "unload it first:" >&2
    echo "  $0 stop" >&2
    exit 1
  fi
  validate_controller_receipt_target || {
    echo "error: controller ownership receipt path is unsafe, already exists, or has an unsafe parent" >&2
    exit 1
  }

  KWIN_PID="$(find_kwin_pid)" || {
    echo "error: could not identify one KWin process" >&2
    exit 1
  }
  capture_kwin_identity || {
    echo "error: could not capture KWin PID/start identity" >&2
    exit 1
  }
  KWIN_PREOP_PID="$KWIN_PID"
  KWIN_PREOP_START_IDENTITY="$KWIN_START_IDENTITY"
  KWIN_IDENTITY_MISMATCH=0

  local load_out
  load_out="$(busctl $BUS_SCOPE --json=short call "$BUS_DEST" "$BUS_PATH" $BUS_SCRIPTING_IFACE loadScript ss "$BUNDLE" "$PLUGIN_ID")" || {
    cleanup_after_load "loadScript reply was lost; no public API identifies a partially loaded controller by exact ID"
  }
  if ! strict_json_matches "$load_valid" "$load_out"; then
    cleanup_after_load "loadScript reply is not a strict {\"type\":\"i\",\"data\":[ID]}: $load_out"
  fi

  SCRIPT_ID="$(jq -r '.data[0]' <<<"$load_out")"
  kwin_identity_unchanged || cleanup_after_load "KWin process identity changed immediately after controller load"
  ensure_controller_receipt || cleanup_after_load "could not create a private controller ownership receipt path"
  write_ownership "$CONTROLLER_OWNERSHIP_FILE" controller "$start_nonce" "$controller_build" "$PLUGIN_ID" "$SCRIPT_ID" || cleanup_after_load "could not atomically retain controller ownership"
  local script_obj="/Scripting/Script$SCRIPT_ID"

  local introspect_out
  introspect_out="$(busctl $BUS_SCOPE --json=short introspect "$BUS_DEST" "$script_obj")" || {
    cleanup_after_load "introspect failed for $script_obj"
  }
  if ! strict_json_matches "$script_iface_valid" "$introspect_out"; then
    cleanup_after_load "$script_obj does not expose the org.kde.kwin.Script interface"
  fi

  if ! busctl $BUS_SCOPE --json=short call "$BUS_DEST" "$script_obj" $BUS_SCRIPT_IFACE run >/dev/null 2>&1; then
    cleanup_after_load "run() failed on $script_obj"
  fi

  # Single-engine bring-up proof: the strict isScriptLoaded envelope must
  # report the plugin loaded after run(), bound to the unchanged KWin
  # PID/start identity. No journal readiness wait, no nonce/build
  # diagnostics, no dual-runtime assertions.
  kwin_identity_unchanged || cleanup_after_load "KWin process identity changed after controller run"
  local loaded_after
  loaded_after="$(plugin_loaded_word)" || cleanup_after_load "could not verify controller load state"
  if [[ "$loaded_after" != loaded ]]; then
    cleanup_after_load "controller plugin '$PLUGIN_ID' is not reported loaded after run"
  fi
  echo "started: plugin '$PLUGIN_ID' loaded as script id $SCRIPT_ID; script-id=$SCRIPT_ID plugin=$PLUGIN_ID nonce=$start_nonce build=$controller_build scope-class=${scope_class:-} kwin-pid=$KWIN_PID start-identity=$KWIN_START_IDENTITY receipt=$(ownership_json controller "$start_nonce" "$controller_build" "$PLUGIN_ID" "$SCRIPT_ID")"
  echo
  echo "stop it:"
  printf '  CONTROLLER_OWNERSHIP_FILE=%q bash %q stop %s\n' "$CONTROLLER_OWNERSHIP_FILE" "$0" "$SCRIPT_ID"
  echo
  echo "inspect it:"
  echo "  $0 status"
  echo
  echo "note: current public KWin APIs provide operational lifecycle binding, not direct evaluated-memory source proof."
  echo "shortcut assignments: not checked; loaded state does not prove requested keys are active."
}

cmd_status() {
  require_tools busctl jq
  read_plugin_id

  local loaded pid records count
  loaded="$(plugin_loaded_word)"
  echo "plugin: $PLUGIN_ID"
  echo "loaded: $loaded"
  echo "controller running/callbacks: not proven by loaded state or KGlobalAccel records"

  pid="$(find_kwin_pid 2>/dev/null || true)"
  if [[ -z "$pid" ]]; then
    echo "KWin identity: unavailable (no single KWin process identified)"
  else
    KWIN_PID="$pid"
    if ! capture_kwin_identity; then
      echo "KWin identity: unavailable (PID/start identity unavailable)"
    else
      echo "KWin identity: PID/start identity captured"
    fi
  fi

  records="$(collect_project_action_records)" || exit 1
  count="$(count_records "$records")"
  echo "project action records (KGlobalAccel): $count"
  print_records "$records"
  report_shortcut_drift "$records"
  echo "note: KGlobalAccel records persist after unload and do not prove live callbacks."
  echo "note: public KWin APIs provide operational lifecycle binding, not direct evaluated-memory source proof."
}

cmd_desktops() {
  require_tools busctl jq stat
  local out
  out="$(busctl $BUS_SCOPE --json=short get-property "$BUS_DEST" "$VDSK_PATH" "$VDSK_IFACE" desktops)" || {
    echo "error: VirtualDesktopManager desktops call failed: $out" >&2
    exit 1
  }
  if ! strict_json_matches "$desktops_valid" "$out"; then
    echo "error: unexpected desktops reply (expected a strict {\"type\":\"a(uss)\",\"data\":[[position,id,name],...]} envelope): $out" >&2
    exit 1
  fi
  echo "virtual desktops: $(jq -r '.data | length' <<<"$out")"
  jq -r '.data[] | [.[0], .[1], .[2]] | @tsv' <<<"$out" | sed 's/^/  /'
  echo "note: desktops is a read-only strict decode of the live VirtualDesktopManager envelope; it never mutates."
}

cmd_stop() {
  require_tools busctl jq
  read_plugin_id

  if [[ $# -ne 1 || ! "$1" =~ ^[0-9]+$ || "$1" -gt 2147483647 ]]; then
    echo "error: stop requires the exact non-negative 32-bit script ID returned by start" >&2
    exit 1
  fi
  local script_id="$1" loaded out records count
  [[ -n "$CONTROLLER_OWNERSHIP_FILE" ]] || {
    echo "error: stop requires the nonce-owned controller receipt; refusing stale script teardown" >&2
    exit 1
  }
  load_ownership "$CONTROLLER_OWNERSHIP_FILE" controller "$script_id" "$PLUGIN_ID" >/dev/null || {
    echo "error: controller ownership receipt does not match script id $script_id; refusing teardown" >&2
    exit 1
  }
  loaded="$(plugin_loaded_word)"
  if [[ "$loaded" == "not-loaded" ]]; then
    echo "error: plugin '$PLUGIN_ID' is not loaded; refusing to use stale script id $script_id" >&2
    exit 1
  else
    if ! exact_cleanup "$script_id" "$PLUGIN_ID"; then
      echo "error: exact controller teardown was not verified; refusing to touch another script" >&2
      exit 1
    fi
    remove_ownership "$CONTROLLER_OWNERSHIP_FILE" "$OWNED_RECEIPT"
    echo "stop: plugin '$PLUGIN_ID' unloaded"
  fi

  records="$(collect_project_action_records)" || exit 1
  count="$(count_records "$records")"
  echo "project action records still registered in KGlobalAccel: $count (stale; left untouched)"
  print_records "$records"
  echo "note: KGlobalAccel records do not prove live callbacks and are not unregistered by this command."
  echo "note: stopping/unloading does not roll back window geometries the single world engine already applied."
}

# Prints one TSV line per unrelated KGlobalAccel record (any component) whose
# active sequence matches any of the given actions' expected sequences.
# Fail-closed exactly like the collector: malformed envelopes are an error,
# never zero matches.
collect_unrelated_target_conflicts() {
  local -a targets=()
  local action
  for action in "$@"; do
    targets+=("${EXPECTED_SEQUENCES[$action]}")
  done
  if [[ "${#targets[@]}" -eq 0 ]]; then
    return 0
  fi
  local targets_json
  targets_json="$(printf '%s\n' "${targets[@]}" | jq -R -s -c 'split("\n") | map(select(length > 0)) | map(tonumber)')" || return 1
  local comps
  comps="$(busctl $BUS_SCOPE --json=short call "$KG_DEST" "$KG_PATH" "$KG_IFACE" allComponents)" || {
    echo "error: KGlobalAccel allComponents call failed: $comps" >&2
    return 1
  }
  if ! strict_json_matches "$components_valid" "$comps"; then
    echo "error: unexpected allComponents reply: $comps" >&2
    return 1
  fi
  local comp infos
  while IFS= read -r comp; do
    infos="$(busctl $BUS_SCOPE --json=short call "$KG_DEST" "$comp" "$KG_COMP_IFACE" allShortcutInfos s default)" || {
      echo "error: allShortcutInfos call failed for $comp: $infos" >&2
      return 1
    }
    if ! strict_json_matches "$shortcut_infos_valid" "$infos"; then
      echo "error: unexpected allShortcutInfos reply for $comp" >&2
      return 1
    fi
    jq -r --argjson targets "$targets_json" --argjson actions "$PROJECT_ACTIONS_JSON" \
      '.data[0][] | select((.[0] as $id | $actions | index($id)) == null) | select(any(.[6][]; . as $active | ($targets | index($active)) != null)) | [.[2], .[0], .[1], (.[6] | join(","))] | @tsv' \
      <<<"$infos" || return 1
  done < <(jq -r '.data[0][]' <<<"$comps")
}

# Calls the exact setter contract. keys_json is the QSet<QKeySequence> D-Bus
# value, represented as an array of integer arrays.
set_shortcut_keys() {
  local comp="$1" action="$2" label="$3" keys_json="$4" flags="$5"
  if ! jq -e 'type == "array" and all(.[]; type == "array" and all(.[]; type == "number" and floor == .))' <<<"$keys_json" >/dev/null 2>&1; then
    return 1
  fi
  local -a key_args=()
  mapfile -t key_args < <(jq -r '([length] + (map([length] + .) | add // []))[]' <<<"$keys_json")
  busctl $BUS_SCOPE --json=short call "$KG_DEST" "$KG_PATH" "$KG_IFACE" setShortcutKeys "asa(ai)u" \
    4 "$comp" "$action" "KWin" "$label" "${key_args[@]}" "$flags"
}

# allShortcutInfos exposes one active integer sequence per project action.
# Re-wrap that exact captured sequence as the setter's QSet<QKeySequence>.
captured_sequence_to_keys_json() {
  local sequence="$1"
  if [[ -z "$sequence" ]]; then
    printf '[]\n'
    return 0
  fi
  jq -cn --arg sequence "$sequence" '[$sequence | split(",") | map(tonumber)]'
}

setter_reply_confirms() {
  local reply="$1" expected="$2"
  jq -s -e --argjson expected "$expected" \
    'length == 1 and (.[0] | (((keys | sort) == ["data","type"]) and (.type == "a(ai)") and ((.data | type) == "array") and ((.data | flatten | index($expected)) != null)))' \
    <<<"$reply" >/dev/null 2>&1
}

# Narrow explicit shortcut reconciliation. Read-only by default; --apply
# writes the expected source-default active sequence to each mismatched
# project record through the exact setter contract after read-only gates.
cmd_reconcile_shortcuts() {
  require_tools busctl jq
  local mode="${1:-}"
  if [[ -n "$mode" && "$mode" != "--apply" ]]; then
    echo "error: unknown reconcile-shortcuts argument '$mode' (expected --apply or nothing)" >&2
    exit 1
  fi

  # Read-only preflight: the running KGlobalAccel must expose the exact
  # setter contract (method name, D-Bus signature, and result type).
  local introspect_out
  introspect_out="$(busctl $BUS_SCOPE --json=short introspect "$KG_DEST" "$KG_PATH")" || {
    echo "error: could not introspect $KG_DEST $KG_PATH" >&2
    exit 1
  }
  if ! strict_json_matches 'type == "array" and any(.[]; ((.type == "method") and (.name == ".setShortcutKeys") and (.signature == "asa(ai)u") and (.result_value == "a(ai)")))' "$introspect_out"; then
    echo "error: KGlobalAccel setShortcutKeys is absent or does not expose exactly asa(ai)u -> a(ai)" >&2
    exit 1
  fi

  ensure_actions_json

  local records comp action label active default
  records="$(collect_project_action_records)" || exit 1
  declare -A RECORD_COMP=() RECORD_LABEL=() RECORD_ACTIVE=() RECORD_COUNT=()
  local ownership_errors=0
  while IFS=$'\t' read -r comp action label active default; do
    [[ -z "$action" ]] && continue
    RECORD_COUNT["$action"]=$(( ${RECORD_COUNT[$action]:-0} + 1 ))
    if [[ "$comp" != "kwin" ]]; then
      ownership_errors=$((ownership_errors + 1))
      echo "  ownership error: action \"$action\" is under component \"$comp\", expected \"kwin\""
    fi
    RECORD_COMP["$action"]="$comp"
    RECORD_LABEL["$action"]="$label"
    RECORD_ACTIVE["$action"]="$active"
  done <<<"$records"

  local -a mismatch_actions=()
  local matched=0 mismatched=0 missing=0 expected
  for action in "${PROJECT_ACTIONS[@]}"; do
    expected="${EXPECTED_SEQUENCES[$action]}"
    if [[ ! "${RECORD_ACTIVE[$action]+x}" ]]; then
      missing=$((missing + 1))
      echo "  missing: action \"$action\" has no persisted record"
      continue
    fi
    if [[ "${RECORD_ACTIVE[$action]}" == "$expected" ]]; then
      matched=$((matched + 1))
    else
      mismatched=$((mismatched + 1))
      mismatch_actions+=("$action")
    fi
    if [[ "${RECORD_COUNT[$action]}" -ne 1 ]]; then
      ownership_errors=$((ownership_errors + 1))
      echo "  ownership error: action \"$action\" has ${RECORD_COUNT[$action]} project records, expected exactly one under \"kwin\""
    fi
  done

  local conflicts
  conflicts="$(collect_unrelated_target_conflicts "${PROJECT_ACTIONS[@]}")" || exit 1

  if [[ -z "$mode" ]]; then
    echo "reconcile-shortcuts: read-only report (no mutation)"
    echo "  setter contract: org.kde.KGlobalAccel.setShortcutKeys asa(ai)u -> a(ai) (introspection-proven)"
    echo "  matched: $matched"
    echo "  mismatched: $mismatched"
    for action in "${mismatch_actions[@]}"; do
      echo "    action \"$action\" active \"${RECORD_ACTIVE[$action]}\" expected \"${EXPECTED_SEQUENCES[$action]}\""
    done
    echo "  missing: $missing"
    echo "  ownership errors: $ownership_errors"
    echo "  unrelated target conflicts: $(count_records "$conflicts")"
    print_records "$conflicts"
    echo "  note: run 'reconcile-shortcuts --apply' to write the expected active sequences."
    echo "  note: normal 'start' never mutates shortcut records."
    return 0
  fi

  # --apply gates: target ownership and unrelated-conflict absence.
  if [[ "$missing" -gt 0 ]]; then
    echo "error: refusing to apply with $missing missing project action record(s); cannot reconcile unregistered actions" >&2
    exit 1
  fi
  if [[ "$ownership_errors" -gt 0 ]]; then
    echo "error: refusing to apply with $ownership_errors project ownership error(s); expected exactly one kwin record per action" >&2
    exit 1
  fi
  if [[ -n "$conflicts" ]]; then
    echo "error: refusing to apply; expected target sequences are claimed by unrelated records:" >&2
    print_records "$conflicts" >&2
    exit 1
  fi
  if [[ "$mismatched" -eq 0 ]]; then
    echo "reconcile-shortcuts --apply: all project action records already match expected source defaults; nothing to write"
    return 0
  fi

  echo "reconcile-shortcuts --apply: preflight passed (exact setter contract, target ownership, no unrelated conflicts)"
  echo "  before:"
  for action in "${mismatch_actions[@]}"; do
    echo "    action \"$action\" active \"${RECORD_ACTIVE[$action]}\" expected \"${EXPECTED_SEQUENCES[$action]}\""
  done

  local -a touched_actions=()
  local touched=0 reply keys_json failure=""
  echo "  writing:"
  for action in "${mismatch_actions[@]}"; do
    expected="${EXPECTED_SEQUENCES[$action]}"
    comp="${RECORD_COMP[$action]}"
    label="${RECORD_LABEL[$action]}"
    keys_json="[[${expected},0,0,0]]"
    # Record before the call because a transport or reply failure can still
    # leave the daemon changed.
    touched_actions+=("$action")
    touched=$((touched + 1))
    if ! reply="$(set_shortcut_keys "$comp" "$action" "$label" "$keys_json" "$KG_SET_SHORTCUT_FLAGS")"; then
      failure="setShortcutKeys call failed for action \"$action\": $reply"
      break
    fi
    if ! setter_reply_confirms "$reply" "$expected"; then
      failure="setShortcutKeys reply for action \"$action\" did not confirm expected key: $reply"
      break
    fi
    echo "    action \"$action\" -> \"$expected\""
  done

  local after after_active ok_count=0 bad_count=0
  if [[ -z "$failure" ]]; then
    after="$(collect_project_action_records)" || failure="could not collect project records for post-write verification"
  fi
  declare -A AFTER_ACTIVE=()
  if [[ -z "$failure" ]]; then
    while IFS=$'\t' read -r comp action label active default; do
      [[ -z "$action" ]] && continue
      AFTER_ACTIVE["$action"]="$active"
    done <<<"$after"
    echo "  after:"
    for action in "${mismatch_actions[@]}"; do
      after_active="${AFTER_ACTIVE[$action]:-}"
      expected="${EXPECTED_SEQUENCES[$action]}"
      if [[ "$after_active" == "$expected" ]]; then
        ok_count=$((ok_count + 1))
        echo "    action \"$action\" active \"$after_active\" (verified)"
      else
        bad_count=$((bad_count + 1))
        echo "    action \"$action\" active \"$after_active\" (expected \"$expected\"; not verified)" >&2
      fi
    done
    if [[ "$bad_count" -gt 0 ]]; then
      failure="$bad_count post-write project assignment(s) did not verify"
    fi
  fi

  if [[ -n "$failure" ]]; then
    echo "error: reconciliation failed: $failure" >&2
    echo "rollback: restoring $touched touched project assignment(s)" >&2
    local restore_failed=0 restore_reply restore_keys restored
    for action in "${touched_actions[@]}"; do
      if ! restore_keys="$(captured_sequence_to_keys_json "${RECORD_ACTIVE[$action]}")"; then
        restore_failed=1
        echo "  rollback failed: could not encode captured assignment for action \"$action\"" >&2
        continue
      fi
      if ! restore_reply="$(set_shortcut_keys "${RECORD_COMP[$action]}" "$action" "${RECORD_LABEL[$action]}" "$restore_keys" "$KG_SET_SHORTCUT_FLAGS")"; then
        restore_failed=1
        echo "  rollback failed: setShortcutKeys call for action \"$action\": $restore_reply" >&2
      fi
    done
    if restored="$(collect_project_action_records)"; then
      declare -A RESTORED_ACTIVE=()
      while IFS=$'\t' read -r comp action label active default; do
        [[ -z "$action" ]] && continue
        RESTORED_ACTIVE["$action"]="$active"
      done <<<"$restored"
      for action in "${touched_actions[@]}"; do
        if [[ "${RESTORED_ACTIVE[$action]:-}" != "${RECORD_ACTIVE[$action]}" ]]; then
          restore_failed=1
          echo "  rollback unverified: action \"$action\" active \"${RESTORED_ACTIVE[$action]:-}\" expected captured \"${RECORD_ACTIVE[$action]}\"" >&2
        fi
      done
    else
      restore_failed=1
      echo "  rollback unverified: could not re-read project assignments" >&2
    fi
    if [[ "$restore_failed" -eq 0 ]]; then
      echo "rollback: verified exact restoration of $touched touched project assignment(s)" >&2
    else
      echo "rollback: restoration was not fully verified" >&2
    fi
    exit 1
  fi
  echo "reconcile-shortcuts --apply: touched $touched, verified $ok_count, unverified $bad_count"
}

if [[ $# -eq 0 ]]; then
  echo "error: missing command (start, status, stop, desktops, reconcile-shortcuts, snapshot-shortcuts, or snapshot-kglobalaccel)" >&2
  usage >&2
  exit 1
fi

case "${1:-}" in
  --help|-h)
    if [[ $# -ne 1 ]]; then
      echo "error: '--help' takes no arguments" >&2
      exit 1
    fi
    usage
    exit 0
    ;;
  start)
    # Optional repeatable admission scope: start [--scope-class NAME]...
    # Each flag carries one comma-joined allowlist fragment; repeated flags
    # accumulate in order. Parsed and validated before any build or load;
    # malformed entries fail closed here. The validated allowlist rides
    # OMNITILER_SCOPE_CLASS into the bundle build below.
    shift
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --scope-class)
          [[ $# -ge 2 ]] || { echo "error: --scope-class requires a value" >&2; exit 1; }
          scope_append "$2"
          shift 2
          ;;
        --scope-class=*)
          scope_append "${1#--scope-class=}"
          shift
          ;;
        *)
          echo "error: unknown 'start' argument '$1' (expected --scope-class NAME)" >&2
          exit 1
          ;;
      esac
    done
    if [[ -n "${OMNITILER_SCOPE_CLASS:-}" ]]; then
      scope_validate "${OMNITILER_SCOPE_CLASS:-}" "--scope-class" || exit 1
    fi
    export OMNITILER_SCOPE_CLASS="${OMNITILER_SCOPE_CLASS:-}"
    cmd_start
    ;;
  status)
    if [[ $# -ne 1 ]]; then
      echo "error: 'status' takes no arguments" >&2
      exit 1
    fi
    cmd_status
    ;;
  stop)
    cmd_stop "${@:2}"
    ;;
  desktops)
    if [[ $# -ne 1 ]]; then
      echo "error: 'desktops' takes no arguments" >&2
      exit 1
    fi
    cmd_desktops
    ;;
  reconcile-shortcuts)
    if [[ $# -gt 2 ]]; then
      echo "error: 'reconcile-shortcuts' takes at most one argument (--apply)" >&2
      exit 1
    fi
    cmd_reconcile_shortcuts "${2:-}"
    ;;
  snapshot-shortcuts)
    if [[ $# -ne 1 ]]; then
      echo "error: 'snapshot-shortcuts' takes no arguments" >&2
      exit 1
    fi
    cmd_snapshot_shortcuts
    ;;
  snapshot-kglobalaccel)
    if [[ $# -ne 1 ]]; then
      echo "error: 'snapshot-kglobalaccel' takes no arguments" >&2
      exit 1
    fi
    cmd_snapshot_kglobalaccel
    ;;
  *)
    echo "error: unknown command '$1'" >&2
    usage >&2
    exit 1
    ;;
esac

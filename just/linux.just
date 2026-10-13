# Development loop for the worktree Planner and KWin script.
#
# Run from inside the devenv shell as `just <recipe>`. Only the build step
# needs the devenv environment; every other step uses host Plasma tools
# directly. Never resolves the Planner through /nix/store and never creates
# a `result` symlink. All preconditions fail closed loudly.
#
# State pointers, never the receipt itself:
#   $XDG_RUNTIME_DIR/omnitiler-dev/planner-pid
#   $XDG_RUNTIME_DIR/omnitiler-dev/planner-exe
#   $XDG_RUNTIME_DIR/omnitiler-dev/controller-receipt-path
# The controller receipt itself is a per-run file under $XDG_RUNTIME_DIR
# (omnitiler-controller.XXXXXX/ownership), derived dynamically via
# mktemp and threaded to start-test.sh through CONTROLLER_OWNERSHIP_FILE.

export OMNITILER_KCMSHELL6 := `command -v kcmshell6 || true`

plugin_id := "omnitiler-kwin"
planner_bus_name := "com.omnitiler.Planner"
planner_unit := "omnitiler-planner.service"

# Detached lifecycle: disable packaged script, verify unloaded, build and launch the worktree Planner, then load the KWin bundle.
dev-on:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    PLUGIN_ID="omnitiler-kwin"
    PLANNER_BUS="com.omnitiler.Planner"
    BIN="$REPO_ROOT/target/debug/omnitiler"
    RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    STATE_DIR="$RUNTIME_DIR/omnitiler-dev"
    PID_FILE="$STATE_DIR/planner-pid"
    EXE_FILE="$STATE_DIR/planner-exe"
    START_FILE="$STATE_DIR/planner-start"
    RECEIPT_PTR="$STATE_DIR/controller-receipt-path"
    planner_start_identity() {
      local pid="$1" stat_line stat_pid rest
      local -a fields=()
      stat_line="$(<"/proc/$pid/stat")" || return 1
      [[ "$stat_line" != *$'\n'* ]] || return 1
      stat_pid="${stat_line%% *}"
      [[ "$stat_pid" == "$pid" ]] || return 1
      rest="${stat_line##*) }"
      [[ "$rest" != "$stat_line" ]] || return 1
      read -r -a fields <<<"$rest"
      [[ "${#fields[@]}" -ge 20 && "${fields[0]:-}" =~ ^[A-Za-z]$ ]] || return 1
      [[ "${fields[19]:-}" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "${fields[19]}"
    }
    planner_exe_is_worktree() {
      local raw="$1" normalized="$1"
      [[ -n "$raw" ]] || return 1
      case "$BIN" in *" (deleted)") return 1 ;; esac
      case "$normalized" in *" (deleted)") normalized="${normalized%' (deleted)'}"; ;; esac
      case "$normalized" in */nix/store/*) return 1 ;; esac
      [[ "$normalized" == "$BIN" ]] || return 1
    }
    case "$BIN" in *" (deleted)") echo "error: configured worktree binary path is ambiguous ('$BIN'); refusing" >&2; exit 1 ;; esac
    planner_verify_worktree() {
      local pid="$1" exe candidate_start
      [[ "$pid" =~ ^[0-9]+$ ]] || return 1
      [[ -d "/proc/$pid" ]] || return 1
      exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
      planner_exe_is_worktree "$exe" || return 1
      tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -Fq "planner-service" || return 1
      candidate_start="$(planner_start_identity "$pid")" || return 1
      [[ "$candidate_start" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "$candidate_start"
    }
    planner_dbus_owner_pid() {
      local owner_reply owner_name owner_pid
      owner_reply="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" 2>/dev/null)" || return 1
      owner_name="$(echo "$owner_reply" | awk '{print $NF}' | tr -d '\"')"
      [[ -n "$owner_name" ]] || return 1
      owner_pid="$(busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$owner_name" 2>/dev/null | jq -r '.data[0] // empty' 2>/dev/null || true)"
      [[ "$owner_pid" =~ ^[0-9]+$ ]] || return 1
      printf '%s\n' "$owner_pid"
    }
    controller_loaded_word() {
      local out
      out="$(busctl --user --json=short call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "$PLUGIN_ID" 2>/dev/null)" || { echo "error: isScriptLoaded call failed" >&2; return 1; }
      if ! echo "$out" | jq -e '((keys | sort) == ["data","type"]) and (.type == "b") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "boolean")' >/dev/null 2>&1; then
        echo "error: unexpected isScriptLoaded reply: $out" >&2
        return 1
      fi
      if [[ "$(echo "$out" | jq -r '.data[0]')" == "true" ]]; then printf 'loaded\n'; else printf 'not-loaded\n'; fi
    }
    planner_probe_verified() {
      local owner_pid cand_start
      owner_pid="$(planner_dbus_owner_pid 2>/dev/null)" || return 1
      cand_start="$(planner_verify_worktree "$owner_pid" 2>/dev/null)" || return 1
      printf '%s %s\n' "$owner_pid" "$cand_start"
    }
    ROLLBACK_ARMED=0
    RECOVERY_ARMED=0
    ROLLBACK_RECEIPT_DIR=""
    VERIFIED_PID=""
    VERIFIED_EXE=""
    VERIFIED_START=""
    LOADED_OK=0
    RECEIPT=""
    RECEIPT_DIR=""
    SCRIPT_ID=""
    dev_on_rollback() {
      local orig_rc="${1:-1}"
      set +e
      if [[ "$LOADED_OK" -eq 1 ]]; then
        if [[ "$SCRIPT_ID" =~ ^[0-9]+$ && "$SCRIPT_ID" -le 2147483647 && -n "$RECEIPT" && -f "$RECEIPT" && ! -L "$RECEIPT" ]]; then
          CONTROLLER_OWNERSHIP_FILE="$RECEIPT" bash "$REPO_ROOT/scripts/start-test.sh" stop "$SCRIPT_ID" >/dev/null 2>&1 || echo "error: rollback: start-test.sh stop $SCRIPT_ID failed; worktree script may still be loaded (receipt $RECEIPT)" >&2
        else
          echo "error: rollback: worktree script was loaded but script ID/receipt is not valid for exact unload (script_id '${SCRIPT_ID:-unknown}', receipt '${RECEIPT:-unknown}'); manual cleanup may be required" >&2
        fi
      fi
      if [[ -n "$VERIFIED_PID" ]]; then
        REVERIFY_START=""
        REVERIFY_START="$(planner_verify_worktree "$VERIFIED_PID" 2>/dev/null || true)"
        if [[ -n "$REVERIFY_START" && "$REVERIFY_START" == "$VERIFIED_START" ]]; then
          kill "$VERIFIED_PID" 2>/dev/null || echo "error: rollback: could not terminate verified worktree planner pid $VERIFIED_PID" >&2
        else
          echo "error: rollback: verified planner identity changed for pid $VERIFIED_PID; refusing to kill ambiguously" >&2
        fi
      fi
      bash "$REPO_ROOT/scripts/dogfood-install.sh" enable >/dev/null 2>&1 || echo "error: rollback: dogfood-install.sh enable failed; packaged script may still be disabled" >&2
      if [[ -n "$ROLLBACK_RECEIPT_DIR" ]]; then
        case "$ROLLBACK_RECEIPT_DIR" in
          "$RUNTIME_DIR"/omnitiler-controller.*)
            rm -rf -- "$ROLLBACK_RECEIPT_DIR" 2>/dev/null || echo "error: rollback: could not remove own receipt dir $ROLLBACK_RECEIPT_DIR" >&2
            ;;
          *) echo "error: rollback: refusing to remove unexpected receipt dir $ROLLBACK_RECEIPT_DIR" >&2 ;;
        esac
      fi
      rm -f -- "$PID_FILE" "$EXE_FILE" "$START_FILE" "$RECEIPT_PTR" "$STATE_DIR/planner-log" "$STATE_DIR/dev-log" "$STATE_DIR/dev-stream" "$STATE_DIR/dev-planner-stream" "$STATE_DIR/dev-kwin-stream" 2>/dev/null || echo "error: rollback: could not remove dev state files in $STATE_DIR" >&2
      rmdir -- "$STATE_DIR" 2>/dev/null || true
      return "$orig_rc"
    }
    trap 'trap_rc=$?; if [[ "${ROLLBACK_ARMED:-0}" -eq 1 ]]; then dev_on_rollback "$trap_rc"; trap_rc=$?; fi; if [[ "${RECOVERY_ARMED:-0}" -eq 1 ]]; then dev_on_recovery_rollback "$trap_rc"; trap_rc=$?; fi; exit "$trap_rc"' EXIT
    # Classify health from two independently measured facts before any mutation:
    # a verified worktree Planner owning the D-Bus name, and strictly parsed
    # KWin isScriptLoaded. Malformed/transport failures fail closed here.
    PROBE_OUT=""
    SPLIT_PLANNER_PID=""
    SPLIT_PLANNER_START=""
    PLAN_UP=0
    if PROBE_OUT="$(planner_probe_verified 2>/dev/null)"; then
      PLAN_UP=1
      SPLIT_PLANNER_PID="${PROBE_OUT%% *}"
      SPLIT_PLANNER_START="${PROBE_OUT##* }"
    fi
    CTRL_WORD="$(controller_loaded_word)" || exit 1
    CTRL_UP=0
    [[ "$CTRL_WORD" == "loaded" ]] && CTRL_UP=1
    # D5: a worktree owner whose exe is the kernel "(deleted)" form is a
    # pre-rebuild launch, not the current build. Refuse rather than reporting
    # already-up or recovering a controller against it; `just reload` safely
    # swaps exactly this case. Automatic restart here would terminate a live
    # Planner the controller depends on, so fail closed with one line.
    if [[ "$PLAN_UP" -eq 1 ]]; then
      SPLIT_PLANNER_EXE="$(readlink "/proc/$SPLIT_PLANNER_PID/exe" 2>/dev/null || true)"
      case "$SPLIT_PLANNER_EXE" in *" (deleted)")
        echo "error: Planner owning $PLANNER_BUS is stale (exe '$SPLIT_PLANNER_EXE'); run 'just reload' for a current build, refusing" >&2
        exit 1
        ;;
      esac
    fi
    if [[ "$PLAN_UP" -eq 1 && "$CTRL_UP" -eq 1 ]]; then
      echo "dev-on: dev mode already up (planner pid $SPLIT_PLANNER_PID owns $PLANNER_BUS, controller '$PLUGIN_ID' loaded); making no changes"
      if [[ -f "$RECEIPT_PTR" ]] && command -v jq >/dev/null 2>&1; then
        REC_PRINT="$(cat "$RECEIPT_PTR" 2>/dev/null || true)"
        if [[ -f "$REC_PRINT" && ! -L "$REC_PRINT" ]]; then
          SID="$(jq -r '.script_id // empty' "$REC_PRINT" 2>/dev/null || true)"
          [[ -n "$SID" ]] && echo "planner pid: $SPLIT_PLANNER_PID, script id: $SID"
        fi
      fi
      exit 0
    fi
    if [[ "$PLAN_UP" -eq 0 && "$CTRL_UP" -eq 1 ]]; then
      echo "error: controller '$PLUGIN_ID' is loaded but no verified worktree Planner owns $PLANNER_BUS; refusing to start a duplicate (unload the controller or restore the Planner, then re-run)" >&2
      exit 1
    fi
    if [[ "$PLAN_UP" -eq 1 && "$CTRL_UP" -eq 0 ]]; then
      echo "dev-on: verified planner pid $SPLIT_PLANNER_PID owns $PLANNER_BUS but controller '$PLUGIN_ID' is not loaded; recovering exactly one controller without touching the Planner"
      [[ -x "$BIN" ]] || { echo "error: worktree Planner binary missing: $BIN; refusing recovery" >&2; exit 1; }
      if [[ -e "$REPO_ROOT/result" ]]; then
        echo "error: refusing to proceed with a 'result' symlink present at $REPO_ROOT/result" >&2
        exit 1
      fi
      RECOVERY_ARMED=1
      RECOVERY_RECEIPT_DIR=""
      RECOVERY_RECEIPT=""
      RECOVERY_SCRIPT_ID=""
      RECOVERY_LOADED_OK=0
      dev_on_recovery_rollback() {
        local orig_rc="${1:-1}"
        set +e
        if [[ "$RECOVERY_LOADED_OK" -eq 1 ]]; then
          if [[ "$RECOVERY_SCRIPT_ID" =~ ^[0-9]+$ && "$RECOVERY_SCRIPT_ID" -le 2147483647 && -n "$RECOVERY_RECEIPT" && -f "$RECOVERY_RECEIPT" && ! -L "$RECOVERY_RECEIPT" ]]; then
            CONTROLLER_OWNERSHIP_FILE="$RECOVERY_RECEIPT" bash "$REPO_ROOT/scripts/start-test.sh" stop "$RECOVERY_SCRIPT_ID" >/dev/null 2>&1 || echo "error: recovery rollback: start-test.sh stop $RECOVERY_SCRIPT_ID failed; worktree script may still be loaded (receipt $RECOVERY_RECEIPT)" >&2
          else
            echo "error: recovery rollback: worktree script was loaded but script ID/receipt is not valid for exact unload (script_id '${RECOVERY_SCRIPT_ID:-unknown}', receipt '${RECOVERY_RECEIPT:-unknown}'); manual cleanup may be required" >&2
          fi
        fi
        if [[ -n "$RECOVERY_RECEIPT_DIR" ]]; then
          case "$RECOVERY_RECEIPT_DIR" in
            "$RUNTIME_DIR"/omnitiler-controller.*)
              rm -rf -- "$RECOVERY_RECEIPT_DIR" 2>/dev/null || echo "error: recovery rollback: could not remove own receipt dir $RECOVERY_RECEIPT_DIR" >&2
              ;;
            *) echo "error: recovery rollback: refusing to remove unexpected receipt dir $RECOVERY_RECEIPT_DIR" >&2 ;;
          esac
        fi
        return "$orig_rc"
      }
      RECOVERY_RECEIPT_DIR="$(mktemp -d "$RUNTIME_DIR/omnitiler-controller.XXXXXX")" || { echo "error: could not create controller receipt dir" >&2; exit 1; }
      chmod 700 "$RECOVERY_RECEIPT_DIR" || { rmdir -- "$RECOVERY_RECEIPT_DIR"; echo "error: could not secure controller receipt dir" >&2; exit 1; }
      RECOVERY_RECEIPT="$RECOVERY_RECEIPT_DIR/ownership"
      if [[ -e "$RECOVERY_RECEIPT" || -L "$RECOVERY_RECEIPT" ]]; then
        echo "error: controller receipt path already exists: $RECOVERY_RECEIPT" >&2
        exit 1
      fi
      CONTROLLER_OWNERSHIP_FILE="$RECOVERY_RECEIPT" bash "$REPO_ROOT/scripts/start-test.sh" start || {
        echo "error: start-test.sh start failed during recovery (verified planner $SPLIT_PLANNER_PID left running); no Planner was terminated and packaged script was not re-enabled" >&2
        exit 1
      }
      RECOVERY_LOADED_OK=1
      RECOVERY_SCRIPT_ID="$(jq -r '.script_id // empty' "$RECOVERY_RECEIPT" 2>/dev/null || true)"
      if [[ ! "$RECOVERY_SCRIPT_ID" =~ ^[0-9]+$ ]] || [[ "$RECOVERY_SCRIPT_ID" -gt 2147483647 ]]; then
        echo "error: controller receipt has no valid script_id: $RECOVERY_RECEIPT" >&2
        exit 1
      fi
      RECOVERY_OWNER=""
      RECOVERY_OWNER="$(planner_dbus_owner_pid 2>/dev/null || true)"
      if [[ -z "$RECOVERY_OWNER" || "$RECOVERY_OWNER" != "$SPLIT_PLANNER_PID" ]]; then
        echo "error: Planner D-Bus owner changed during recovery (expected $SPLIT_PLANNER_PID, got '${RECOVERY_OWNER:-unowned}'); refusing to record ambiguous state" >&2
        exit 1
      fi
      RECOVERY_START=""
      RECOVERY_START="$(planner_verify_worktree "$SPLIT_PLANNER_PID" 2>/dev/null || true)"
      if [[ -z "$RECOVERY_START" || "$RECOVERY_START" != "$SPLIT_PLANNER_START" ]]; then
        echo "error: pre-existing Planner identity changed during recovery for pid $SPLIT_PLANNER_PID; refusing to record ambiguous state" >&2
        exit 1
      fi
      mkdir -p "$STATE_DIR" && chmod 700 "$STATE_DIR" || { echo "error: could not create state dir $STATE_DIR" >&2; exit 1; }
      printf '%s\n' "$SPLIT_PLANNER_PID" > "$STATE_DIR/planner-pid" || { echo "error: could not record planner pid" >&2; exit 1; }
      printf '%s\n' "$BIN" > "$STATE_DIR/planner-exe" || { echo "error: could not record planner exe" >&2; exit 1; }
      printf '%s\n' "$SPLIT_PLANNER_START" > "$STATE_DIR/planner-start" || { echo "error: could not record planner start identity" >&2; exit 1; }
      printf '%s\n' "$RECOVERY_RECEIPT" > "$STATE_DIR/controller-receipt-path" || { echo "error: could not record controller receipt path" >&2; exit 1; }
      RECOVERY_ARMED=0
      trap - EXIT
      echo "dev-on: recovered controller script $RECOVERY_SCRIPT_ID against existing planner pid $SPLIT_PLANNER_PID"
      echo "receipt: $RECOVERY_RECEIPT"
      exit 0
    fi
    # Both down: follow the existing normal bring-up below.
    # 1. Disable the packaged KWin script (host tools, no devenv wrapper).
    bash "$REPO_ROOT/scripts/dogfood-install.sh" disable
    ROLLBACK_ARMED=1
    # 2. Verify isScriptLoaded is false; reconfiguration settles asynchronously.
    IS_LOADED_OUT="$(busctl --user --json=short call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "$PLUGIN_ID")" || { echo "error: isScriptLoaded call failed" >&2; exit 1; }
    if ! echo "$IS_LOADED_OUT" | jq -e '((keys | sort) == ["data","type"]) and (.type == "b") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "boolean")' >/dev/null 2>&1; then
      echo "error: unexpected isScriptLoaded reply: $IS_LOADED_OUT" >&2
      exit 1
    fi
    if [[ "$(echo "$IS_LOADED_OUT" | jq -r '.data[0]')" != "false" ]]; then
      echo "error: plugin '$PLUGIN_ID' is still loaded; refusing to continue (reconfiguration may still be settling)" >&2
      exit 1
    fi
    # 3. Require the Planner D-Bus name to be unowned. Never stop or mask units here.
    if busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" >/dev/null 2>&1; then
      OWNER_DETAIL="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" 2>&1 || true)"
      echo "error: $PLANNER_BUS is still owned ($OWNER_DETAIL); refusing to start worktree Planner" >&2
      echo "hint: if the owner is omnitiler-planner.service, stop only that unit, re-verify GetNameOwner fails, then re-run. Do not mask units." >&2
      exit 1
    fi
    # 4. Build. The devenv shell is required only here, and only when outside it.
    if [[ -n "${IN_NIX_SHELL:-}${DEVENV_PROFILE:-}" ]]; then
      ( cd "$REPO_ROOT" && cargo build -p omnitiler ) || { echo "error: cargo build failed" >&2; exit 1; }
    else
      devenv shell --impure -- cargo build -p omnitiler || { echo "error: cargo build failed (via devenv shell --impure)" >&2; exit 1; }
    fi
    [[ -x "$BIN" ]] || { echo "error: worktree Planner binary missing after build: $BIN" >&2; exit 1; }
    if [[ -e "$REPO_ROOT/result" ]]; then
      echo "error: refusing to proceed with a 'result' symlink present at $REPO_ROOT/result" >&2
      exit 1
    fi
    # 5. Derive the per-run receipt path dynamically under $XDG_RUNTIME_DIR.
    RECEIPT_DIR="$(mktemp -d "$RUNTIME_DIR/omnitiler-controller.XXXXXX")" || { echo "error: could not create controller receipt dir" >&2; exit 1; }
    ROLLBACK_RECEIPT_DIR="$RECEIPT_DIR"
    chmod 700 "$RECEIPT_DIR" || { rmdir -- "$RECEIPT_DIR"; echo "error: could not secure controller receipt dir" >&2; exit 1; }
    RECEIPT="$RECEIPT_DIR/ownership"
    if [[ -e "$RECEIPT" || -L "$RECEIPT" ]]; then
      echo "error: controller receipt path already exists: $RECEIPT" >&2
      exit 1
    fi
    # 6. Launch exactly the worktree Planner, detached. $! is a hint only and
    # never authoritative: setsid may fork when it is a process-group leader,
    # so identity is derived from the D-Bus owner instead.
    PLANNER_LOG="$(mktemp /tmp/omnitiler-planner-dev.XXXXXX.log)" || { echo "error: could not create planner log" >&2; exit 1; }
    setsid nohup "$BIN" planner-service >"$PLANNER_LOG" 2>&1 </dev/null &
    LAUNCH_PID=$!
    VERIFIED_PID=""
    VERIFIED_EXE=""
    VERIFIED_START=""
    PROVED=0
    for _ in $(seq 1 50); do
      if OWNER_PID="$(planner_dbus_owner_pid 2>/dev/null)"; then
        if CAND_START="$(planner_verify_worktree "$OWNER_PID" 2>/dev/null)"; then
          VERIFIED_PID="$OWNER_PID"
          VERIFIED_EXE="$BIN"
          VERIFIED_START="$CAND_START"
          PROVED=1
          break
        else
          echo "error: $PLANNER_BUS owned by unexpected pid $OWNER_PID (exe/cmdline/start did not verify as worktree $BIN planner-service); refusing (launch hint was ${LAUNCH_PID:-unknown}); rollback will re-enable the packaged script without touching that PID" >&2
          exit 1
        fi
      fi
      sleep 0.2
    done
    if [[ "$PROVED" -ne 1 ]]; then
      echo "error: $PLANNER_BUS was not owned by a verified worktree Planner within the bounded window (launch hint ${LAUNCH_PID:-unknown}); see $PLANNER_LOG" >&2
      exit 1
    fi
    busctl --user --no-pager status "$PLANNER_BUS" || { echo "error: busctl status for $PLANNER_BUS failed" >&2; exit 1; }
    # 7. Load the worktree KWin bundle. start-test.sh owns the duplicate
    # plugin guard; do not reimplement it here.
    CONTROLLER_OWNERSHIP_FILE="$RECEIPT" bash "$REPO_ROOT/scripts/start-test.sh" start || {
      echo "error: start-test.sh start failed (verified planner ${VERIFIED_PID:-unknown} will be terminated by rollback); see $PLANNER_LOG" >&2
      exit 1
    }
    LOADED_OK=1
    # 8. Print Planner PID plus script ID read from the controller receipt.
    SCRIPT_ID="$(jq -r '.script_id // empty' "$RECEIPT" 2>/dev/null || true)"
    if [[ ! "$SCRIPT_ID" =~ ^[0-9]+$ ]] || [[ "$SCRIPT_ID" -gt 2147483647 ]]; then
      echo "error: controller receipt has no valid script_id: $RECEIPT" >&2
      exit 1
    fi
    mkdir -p "$STATE_DIR" && chmod 700 "$STATE_DIR" || { echo "error: could not create state dir $STATE_DIR" >&2; exit 1; }
    printf '%s\n' "$VERIFIED_PID" > "$STATE_DIR/planner-pid" || { echo "error: could not record planner pid" >&2; exit 1; }
    printf '%s\n' "$VERIFIED_EXE" > "$STATE_DIR/planner-exe" || { echo "error: could not record planner exe" >&2; exit 1; }
    printf '%s\n' "$VERIFIED_START" > "$START_FILE" || { echo "error: could not record planner start identity" >&2; exit 1; }
    printf '%s\n' "$RECEIPT" > "$STATE_DIR/controller-receipt-path" || { echo "error: could not record controller receipt path" >&2; exit 1; }
    printf '%s\n' "$PLANNER_LOG" > "$STATE_DIR/planner-log" || { echo "error: could not record planner log path" >&2; exit 1; }
    ROLLBACK_ARMED=0
    trap - EXIT
    echo "dev-on: planner pid $VERIFIED_PID, script id $SCRIPT_ID"
    echo "planner log: $PLANNER_LOG"
    echo "receipt: $RECEIPT"

# Rebuild and safely swap only the recorded worktree Planner. Never touches the KWin script.
reload:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    PLANNER_BUS="com.omnitiler.Planner"
    BIN="$REPO_ROOT/target/debug/omnitiler"
    RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    STATE_DIR="$RUNTIME_DIR/omnitiler-dev"
    PID_FILE="$STATE_DIR/planner-pid"
    EXE_FILE="$STATE_DIR/planner-exe"
    START_FILE="$STATE_DIR/planner-start"
    planner_start_identity() {
      local pid="$1" stat_line stat_pid rest
      local -a fields=()
      stat_line="$(<"/proc/$pid/stat")" || return 1
      [[ "$stat_line" != *$'\n'* ]] || return 1
      stat_pid="${stat_line%% *}"
      [[ "$stat_pid" == "$pid" ]] || return 1
      rest="${stat_line##*) }"
      [[ "$rest" != "$stat_line" ]] || return 1
      read -r -a fields <<<"$rest"
      [[ "${#fields[@]}" -ge 20 && "${fields[0]:-}" =~ ^[A-Za-z]$ ]] || return 1
      [[ "${fields[19]:-}" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "${fields[19]}"
    }
    planner_exe_is_worktree() {
      local raw="$1" normalized="$1"
      [[ -n "$raw" ]] || return 1
      case "$BIN" in *" (deleted)") return 1 ;; esac
      case "$normalized" in *" (deleted)") normalized="${normalized%' (deleted)'}"; ;; esac
      case "$normalized" in */nix/store/*) return 1 ;; esac
      [[ "$normalized" == "$BIN" ]] || return 1
    }
    planner_verify_worktree() {
      local pid="$1" exe candidate_start
      [[ "$pid" =~ ^[0-9]+$ ]] || return 1
      [[ -d "/proc/$pid" ]] || return 1
      exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
      planner_exe_is_worktree "$exe" || return 1
      tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -Fq "planner-service" || return 1
      candidate_start="$(planner_start_identity "$pid")" || return 1
      [[ "$candidate_start" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "$candidate_start"
    }
    planner_dbus_owner_pid() {
      local owner_reply owner_name owner_pid
      owner_reply="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" 2>/dev/null)" || return 1
      owner_name="$(echo "$owner_reply" | awk '{print $NF}' | tr -d '\"')"
      [[ -n "$owner_name" ]] || return 1
      owner_pid="$(busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$owner_name" 2>/dev/null | jq -r '.data[0] // empty' 2>/dev/null || true)"
      [[ "$owner_pid" =~ ^[0-9]+$ ]] || return 1
      printf '%s\n' "$owner_pid"
    }
    [[ -f "$PID_FILE" ]] || { echo "error: no recorded planner pid ($PID_FILE missing); is dev mode on?" >&2; exit 1; }
    [[ -f "$EXE_FILE" ]] || { echo "error: no recorded planner exe ($EXE_FILE missing)" >&2; exit 1; }
    [[ -f "$START_FILE" ]] || { echo "error: no recorded planner start identity ($START_FILE missing)" >&2; exit 1; }
    OLD_PID="$(cat "$PID_FILE")"
    RECORDED_EXE="$(cat "$EXE_FILE")"
    RECORDED_START="$(cat "$START_FILE")"
    [[ "$OLD_PID" =~ ^[0-9]+$ ]] || { echo "error: recorded planner pid is invalid: $OLD_PID" >&2; exit 1; }
    case "$BIN" in *" (deleted)") echo "error: configured worktree binary path is ambiguous ('$BIN'); refusing" >&2; exit 1 ;; esac
    [[ "$RECORDED_EXE" == "$BIN" ]] || { echo "error: recorded planner exe ($RECORDED_EXE) is not the worktree binary ($BIN); refusing" >&2; exit 1; }
    [[ "$RECORDED_START" =~ ^[1-9][0-9]*$ ]] || { echo "error: recorded planner start identity is invalid: $RECORDED_START" >&2; exit 1; }
    [[ -d "/proc/$OLD_PID" ]] || { echo "error: recorded planner pid $OLD_PID is not running" >&2; exit 1; }
    CURRENT_EXE="$(readlink "/proc/$OLD_PID/exe" 2>/dev/null || true)"
    planner_exe_is_worktree "$CURRENT_EXE" || { echo "error: /proc/$OLD_PID/exe is '$CURRENT_EXE', expected '$BIN' (or '$BIN (deleted)'); refusing to touch it" >&2; exit 1; }
    CURRENT_START="$(planner_start_identity "$OLD_PID")" || { echo "error: could not capture planner start identity for pid $OLD_PID; refusing" >&2; exit 1; }
    [[ "$CURRENT_START" == "$RECORDED_START" ]] || { echo "error: planner start identity changed (current $CURRENT_START, recorded $RECORDED_START); refusing" >&2; exit 1; }
    if ! tr '\0' ' ' < "/proc/$OLD_PID/cmdline" 2>/dev/null | grep -Fq "planner-service"; then
      echo "error: pid $OLD_PID cmdline is not planner-service; refusing" >&2
      exit 1
    fi
    OWNER_PID="$(busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" 2>/dev/null | awk '{print $NF}' | tr -d '\"')" 2>/dev/null | jq -r '.data[0] // empty' 2>/dev/null || true)"
    [[ "$OWNER_PID" == "$OLD_PID" ]] || { echo "error: $PLANNER_BUS owner pid is '${OWNER_PID:-unowned}', expected recorded pid $OLD_PID; refusing swap" >&2; exit 1; }
    if [[ -n "${IN_NIX_SHELL:-}${DEVENV_PROFILE:-}" ]]; then
      ( cd "$REPO_ROOT" && cargo build -p omnitiler ) || { echo "error: cargo build failed; old planner $OLD_PID left running" >&2; exit 1; }
    else
      devenv shell --impure -- cargo build -p omnitiler || { echo "error: cargo build failed (via devenv shell --impure); old planner $OLD_PID left running" >&2; exit 1; }
    fi
    [[ -x "$BIN" ]] || { echo "error: worktree Planner binary missing after build: $BIN" >&2; exit 1; }
    # Re-verify identity immediately before terminating (TOCTOU guard).
    # Cargo can atomically replace $BIN, leaving the old exe as "$BIN (deleted)".
    CURRENT_EXE="$(readlink "/proc/$OLD_PID/exe" 2>/dev/null || true)"
    planner_exe_is_worktree "$CURRENT_EXE" || { echo "error: planner identity changed before swap (exe '$CURRENT_EXE'); refusing" >&2; exit 1; }
    CURRENT_START="$(planner_start_identity "$OLD_PID")" || { echo "error: planner identity changed before swap; refusing" >&2; exit 1; }
    [[ "$CURRENT_START" == "$RECORDED_START" ]] || { echo "error: planner start identity changed before swap; refusing" >&2; exit 1; }
    kill "$OLD_PID" || { echo "error: could not terminate planner pid $OLD_PID" >&2; exit 1; }
    for _ in $(seq 1 50); do [[ -d "/proc/$OLD_PID" ]] || break; sleep 0.2; done
    if [[ -d "/proc/$OLD_PID" ]]; then
      echo "error: old planner pid $OLD_PID did not exit; refusing to start a second Planner" >&2
      exit 1
    fi
    PLANNER_LOG="$(mktemp /tmp/omnitiler-planner-dev.XXXXXX.log)" || { echo "error: could not create planner log" >&2; exit 1; }
    setsid nohup "$BIN" planner-service >"$PLANNER_LOG" 2>&1 </dev/null &
    LAUNCH_PID=$!
    NEW_PID=""
    NEW_EXE=""
    NEW_START=""
    PROVED=0
    for _ in $(seq 1 50); do
      if OWNER_PID="$(planner_dbus_owner_pid 2>/dev/null)"; then
        if CAND_START="$(planner_verify_worktree "$OWNER_PID" 2>/dev/null)"; then
          NEW_PID="$OWNER_PID"
          NEW_EXE="$BIN"
          NEW_START="$CAND_START"
          PROVED=1
          break
        else
          echo "error: $PLANNER_BUS owned by unexpected pid $OWNER_PID (exe/cmdline/start did not verify as worktree $BIN planner-service); refusing (launch hint was ${LAUNCH_PID:-unknown}); not touching that PID" >&2
          exit 1
        fi
      fi
      sleep 0.2
    done
    if [[ "$PROVED" -ne 1 ]]; then
      echo "error: replacement Planner did not own $PLANNER_BUS within the bounded window (launch hint ${LAUNCH_PID:-unknown}); see $PLANNER_LOG; not touching any unverified PID" >&2
      exit 1
    fi
    kill_verified_replacement() {
      local reverify
      reverify="$(planner_verify_worktree "$NEW_PID" 2>/dev/null || true)"
      if [[ -n "$reverify" && "$reverify" == "$NEW_START" ]]; then
        kill "$NEW_PID" 2>/dev/null || true
      else
        echo "error: replacement planner identity changed for pid $NEW_PID; refusing to kill ambiguously" >&2
      fi
    }
    reload_fail() {
      echo "error: $*" >&2
      if [[ -n "${NEW_PID:-}" && -n "${NEW_START:-}" ]]; then
        kill_verified_replacement
      fi
      exit 1
    }
    printf '%s\n' "$NEW_PID" > "$PID_FILE" || reload_fail "could not record replacement planner pid"
    printf '%s\n' "$NEW_EXE" > "$EXE_FILE" || reload_fail "could not record replacement planner exe"
    printf '%s\n' "$NEW_START" > "$START_FILE" || reload_fail "could not record replacement planner start identity"
    printf '%s\n' "$PLANNER_LOG" > "$STATE_DIR/planner-log" || reload_fail "could not record replacement planner log path"
    echo "reload: old pid $OLD_PID replaced by $NEW_PID (launch hint was ${LAUNCH_PID:-unknown})"

# Unload the exact controller script, stop only the recorded Planner, re-enable packaged script.
dev-off:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    PLANNER_BUS="com.omnitiler.Planner"
    PLUGIN_ID="omnitiler-kwin"
    BIN="$REPO_ROOT/target/debug/omnitiler"
    RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    STATE_DIR="$RUNTIME_DIR/omnitiler-dev"
    PID_FILE="$STATE_DIR/planner-pid"
    EXE_FILE="$STATE_DIR/planner-exe"
    START_FILE="$STATE_DIR/planner-start"
    RECEIPT_PTR="$STATE_DIR/controller-receipt-path"
    planner_start_identity() {
      local pid="$1" stat_line stat_pid rest
      local -a fields=()
      stat_line="$(<"/proc/$pid/stat")" || return 1
      [[ "$stat_line" != *$'\n'* ]] || return 1
      stat_pid="${stat_line%% *}"
      [[ "$stat_pid" == "$pid" ]] || return 1
      rest="${stat_line##*) }"
      [[ "$rest" != "$stat_line" ]] || return 1
      read -r -a fields <<<"$rest"
      [[ "${#fields[@]}" -ge 20 && "${fields[0]:-}" =~ ^[A-Za-z]$ ]] || return 1
      [[ "${fields[19]:-}" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "${fields[19]}"
    }
    planner_exe_is_worktree() {
      local raw="$1" normalized="$1"
      [[ -n "$raw" ]] || return 1
      case "$BIN" in *" (deleted)") return 1 ;; esac
      case "$normalized" in *" (deleted)") normalized="${normalized%' (deleted)'}"; ;; esac
      case "$normalized" in */nix/store/*) return 1 ;; esac
      [[ "$normalized" == "$BIN" ]] || return 1
    }
    # Strict controller fact first; malformed/unknown replies fail closed
    # before any mutation. A known false skips the exact stop entirely.
    IS_LOADED_OUT="$(busctl --user --json=short call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "$PLUGIN_ID" 2>/dev/null)" || { echo "error: isScriptLoaded call failed" >&2; exit 1; }
    if ! echo "$IS_LOADED_OUT" | jq -e '((keys | sort) == ["data","type"]) and (.type == "b") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "boolean")' >/dev/null 2>&1; then
      echo "error: unexpected isScriptLoaded reply: $IS_LOADED_OUT" >&2
      exit 1
    fi
    CTRL_STATE="unloaded"
    [[ "$(echo "$IS_LOADED_OUT" | jq -r '.data[0]')" == "true" ]] && CTRL_STATE="loaded"
    # Planner fact: verify the recorded Planner without mutating. An absent
    # or unverified Planner is a split, not a rejection of controller teardown.
    PLANNER_OK=0
    PLANNER_REASON="no recorded planner identity"
    RECORDED_PID=""
    RECORDED_EXE=""
    RECORDED_START=""
    if [[ -f "$PID_FILE" && -f "$EXE_FILE" && -f "$START_FILE" ]]; then
      RECORDED_PID="$(cat "$PID_FILE")"
      RECORDED_EXE="$(cat "$EXE_FILE")"
      RECORDED_START="$(cat "$START_FILE")"
      if [[ ! "$RECORDED_PID" =~ ^[0-9]+$ ]]; then
        PLANNER_REASON="recorded planner pid is invalid: $RECORDED_PID"
      elif [[ "$BIN" == *" (deleted)" ]]; then
        PLANNER_REASON="configured worktree binary path is ambiguous ('$BIN')"
      elif [[ "$RECORDED_EXE" != "$BIN" ]]; then
        PLANNER_REASON="recorded planner exe ($RECORDED_EXE) is not the worktree binary ($BIN)"
      elif [[ ! "$RECORDED_START" =~ ^[1-9][0-9]*$ ]]; then
        PLANNER_REASON="recorded planner start identity is invalid: $RECORDED_START"
      elif [[ ! -d "/proc/$RECORDED_PID" ]]; then
        PLANNER_REASON="recorded planner pid $RECORDED_PID is not running"
      else
        CURRENT_EXE="$(readlink "/proc/$RECORDED_PID/exe" 2>/dev/null || true)"
        if ! planner_exe_is_worktree "$CURRENT_EXE"; then
          PLANNER_REASON="/proc/$RECORDED_PID/exe is '$CURRENT_EXE', expected '$BIN' (or '$BIN (deleted)')"
        else
          CURRENT_START="$(planner_start_identity "$RECORDED_PID" 2>/dev/null || true)"
          if [[ -z "$CURRENT_START" ]]; then
            PLANNER_REASON="could not capture planner start identity for pid $RECORDED_PID"
          elif [[ "$CURRENT_START" != "$RECORDED_START" ]]; then
            PLANNER_REASON="planner start identity changed (current $CURRENT_START, recorded $RECORDED_START)"
          elif ! tr '\0' ' ' < "/proc/$RECORDED_PID/cmdline" 2>/dev/null | grep -Fq "planner-service"; then
            PLANNER_REASON="pid $RECORDED_PID cmdline is not planner-service"
          else
            PLANNER_OK=1
            PLANNER_REASON=""
          fi
        fi
      fi
    elif [[ ! -f "$PID_FILE" ]]; then
      PLANNER_REASON="no recorded planner pid ($PID_FILE missing)"
    elif [[ ! -f "$EXE_FILE" ]]; then
      PLANNER_REASON="no recorded planner exe ($EXE_FILE missing)"
    else
      PLANNER_REASON="no recorded planner start identity ($START_FILE missing)"
    fi
    # Controller teardown only when loaded is strictly known. A known false
    # never runs the exact stop and never requires a receipt. Exact
    # receipt/script-ID ownership is retained: no plugin-name fallback, no
    # unload retries. A controller failure is recorded but never prevents an
    # independently verified Planner from being terminated below.
    RECEIPT=""
    SCRIPT_ID=""
    CONTROLLER_RC=0
    CONTROLLER_FAIL=""
    if [[ "$CTRL_STATE" == "loaded" ]]; then
      if [[ -f "$RECEIPT_PTR" ]]; then
        RECEIPT="$(cat "$RECEIPT_PTR")"
      else
        mapfile -t CANDIDATES < <(compgen -G "$RUNTIME_DIR/omnitiler-controller.*/ownership" || true)
        LIVE=()
        for c in ${CANDIDATES[@]+"${CANDIDATES[@]}"}; do [[ -f "$c" && ! -L "$c" ]] && LIVE+=("$c"); done
        if [[ "${#LIVE[@]}" -eq 1 ]]; then
          RECEIPT="${LIVE[0]}"
        elif [[ "${#LIVE[@]}" -eq 0 ]]; then
          CONTROLLER_FAIL="controller is loaded but no controller receipt found under $RUNTIME_DIR (and $RECEIPT_PTR missing); refusing ambiguous teardown"
        else
          CONTROLLER_FAIL="multiple controller receipts found; refusing ambiguous teardown"
          printf '  %s\n' "${LIVE[@]}" >&2
        fi
      fi
      [[ -n "$CONTROLLER_FAIL" ]] || [[ -n "$RECEIPT" ]] || CONTROLLER_FAIL="empty controller receipt path"
      [[ -n "$CONTROLLER_FAIL" ]] || [[ -f "$RECEIPT" && ! -L "$RECEIPT" ]] || CONTROLLER_FAIL="controller receipt missing or symlinked: $RECEIPT"
      if [[ -z "$CONTROLLER_FAIL" ]]; then
        SCRIPT_ID="$(jq -r '.script_id // empty' "$RECEIPT" 2>/dev/null || true)"
        if [[ ! "$SCRIPT_ID" =~ ^[0-9]+$ ]] || [[ "$SCRIPT_ID" -gt 2147483647 ]]; then
          CONTROLLER_FAIL="controller receipt has no valid script_id: $RECEIPT"
        fi
      fi
      if [[ -n "$CONTROLLER_FAIL" ]]; then
        echo "error: $CONTROLLER_FAIL" >&2
        CONTROLLER_RC=1
      elif CONTROLLER_OWNERSHIP_FILE="$RECEIPT" bash "$REPO_ROOT/scripts/start-test.sh" stop "$SCRIPT_ID"; then
        CONTROLLER_RC=0
      else
        echo "error: start-test.sh stop failed; teardown remains unverified" >&2
        CONTROLLER_RC=1
      fi
    else
      echo "dev-off: controller '$PLUGIN_ID' already unloaded; skipping exact stop"
    fi
    # Planner teardown only when verified, with a re-check immediately
    # before TERM. An unverified Planner is never killed.
    PLANNER_STOPPED=0
    if [[ "$PLANNER_OK" -eq 1 ]]; then
      [[ -d "/proc/$RECORDED_PID" ]] || { echo "error: recorded planner pid $RECORDED_PID is not running after script stop" >&2; exit 1; }
      RECHECK_EXE="$(readlink "/proc/$RECORDED_PID/exe" 2>/dev/null || true)"
      planner_exe_is_worktree "$RECHECK_EXE" || { echo "error: /proc/$RECORDED_PID/exe is '$RECHECK_EXE', expected '$BIN' (or '$BIN (deleted)'); refusing to kill" >&2; exit 1; }
      RECHECK_START="$(planner_start_identity "$RECORDED_PID")" || { echo "error: planner start identity changed after script stop; refusing" >&2; exit 1; }
      [[ "$RECHECK_START" == "$RECORDED_START" ]] || { echo "error: planner start identity changed after script stop (current $RECHECK_START, recorded $RECORDED_START); refusing" >&2; exit 1; }
      if ! tr '\0' ' ' < "/proc/$RECORDED_PID/cmdline" 2>/dev/null | grep -Fq "planner-service"; then
        echo "error: pid $RECORDED_PID cmdline is not planner-service; refusing" >&2
        exit 1
      fi
      kill "$RECORDED_PID" || { echo "error: could not terminate planner pid $RECORDED_PID" >&2; exit 1; }
      for _ in $(seq 1 50); do [[ -d "/proc/$RECORDED_PID" ]] || break; sleep 0.2; done
      if [[ -d "/proc/$RECORDED_PID" ]]; then
        echo "error: planner pid $RECORDED_PID did not exit after SIGTERM; refusing to SIGKILL ambiguously" >&2
        exit 1
      fi
      PLANNER_STOPPED=1
    else
      echo "dev-off: no verified worktree Planner to stop ($PLANNER_REASON); skipping Planner kill"
    fi
    if [[ "$CTRL_STATE" == "loaded" && "$CONTROLLER_RC" -ne 0 ]]; then
      if [[ "$PLANNER_STOPPED" -eq 1 ]]; then
        echo "dev-off: controller teardown failed but planner $RECORDED_PID stopped; teardown remains unverified (package enable skipped, state retained)" >&2
      else
        echo "error: controller teardown failed; teardown remains unverified (package enable skipped, state retained)" >&2
      fi
      exit 1
    fi
    bash "$REPO_ROOT/scripts/dogfood-install.sh" enable || { echo "error: dogfood-install.sh enable failed" >&2; exit 1; }
    rm -f -- "$PID_FILE" "$EXE_FILE" "$START_FILE" "$RECEIPT_PTR" "$STATE_DIR/planner-log" "$STATE_DIR/dev-log" "$STATE_DIR/dev-stream" "$STATE_DIR/dev-planner-stream" "$STATE_DIR/dev-kwin-stream"
    rmdir -- "$STATE_DIR" 2>/dev/null || true
    if [[ -n "$RECEIPT" ]]; then
      PARENT_DIR="$(dirname -- "$RECEIPT")"
      case "$PARENT_DIR" in
        "$RUNTIME_DIR"/omnitiler-controller.*)
          rmdir -- "$PARENT_DIR" 2>/dev/null || true
          ;;
        *) echo "error: refusing to remove unexpected receipt dir $PARENT_DIR" >&2 ;;
      esac
    fi
    if [[ "$CTRL_STATE" == "loaded" && "$PLANNER_STOPPED" -eq 1 ]]; then
      echo "dev-off: script $SCRIPT_ID unloaded, planner $RECORDED_PID stopped, packaged script re-enabled"
    elif [[ "$CTRL_STATE" == "loaded" ]]; then
      echo "dev-off: script $SCRIPT_ID unloaded, no verified planner to stop, packaged script re-enabled"
    elif [[ "$PLANNER_STOPPED" -eq 1 ]]; then
      echo "dev-off: controller already unloaded, planner $RECORDED_PID stopped, packaged script re-enabled"
    else
      echo "dev-off: controller already unloaded, no verified planner to stop, packaged script re-enabled"
    fi

# Read-only dev session report. Never mutates.
dev-status:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    PLUGIN_ID="omnitiler-kwin"
    PLANNER_BUS="com.omnitiler.Planner"
    PLANNER_UNIT="omnitiler-planner.service"
    BIN="$REPO_ROOT/target/debug/omnitiler"
    RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    STATE_DIR="$RUNTIME_DIR/omnitiler-dev"
    planner_start_identity() {
      local pid="$1" stat_line stat_pid rest
      local -a fields=()
      stat_line="$(<"/proc/$pid/stat")" || return 1
      [[ "$stat_line" != *$'\n'* ]] || return 1
      stat_pid="${stat_line%% *}"
      [[ "$stat_pid" == "$pid" ]] || return 1
      rest="${stat_line##*) }"
      [[ "$rest" != "$stat_line" ]] || return 1
      read -r -a fields <<<"$rest"
      [[ "${#fields[@]}" -ge 20 && "${fields[0]:-}" =~ ^[A-Za-z]$ ]] || return 1
      [[ "${fields[19]:-}" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "${fields[19]}"
    }
    planner_exe_is_worktree() {
      local raw="$1" normalized="$1"
      [[ -n "$raw" ]] || return 1
      case "$BIN" in *" (deleted)") return 1 ;; esac
      case "$normalized" in *" (deleted)") normalized="${normalized%' (deleted)'}"; ;; esac
      case "$normalized" in */nix/store/*) return 1 ;; esac
      [[ "$normalized" == "$BIN" ]] || return 1
    }
    planner_verify_worktree() {
      local pid="$1" exe candidate_start
      [[ "$pid" =~ ^[0-9]+$ ]] || return 1
      [[ -d "/proc/$pid" ]] || return 1
      exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
      planner_exe_is_worktree "$exe" || return 1
      tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -Fq "planner-service" || return 1
      candidate_start="$(planner_start_identity "$pid")" || return 1
      [[ "$candidate_start" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "$candidate_start"
    }
    echo "planner name: $PLANNER_BUS"
    if OWNER_REPLY="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" 2>/dev/null)"; then
      OWNER_NAME="$(echo "$OWNER_REPLY" | awk '{print $NF}' | tr -d '\"')"
      echo "name owner: $OWNER_NAME"
      if OWNER_PID="$(busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$OWNER_NAME" 2>/dev/null | jq -r '.data[0] // empty' 2>/dev/null)"; then
        echo "owner pid: ${OWNER_PID:-unknown}"
        if [[ -n "${OWNER_PID:-}" && -d "/proc/$OWNER_PID" ]]; then
          echo "owner exe: $(readlink "/proc/$OWNER_PID/exe" 2>/dev/null || echo unknown)"
        else
          echo "owner exe: unknown"
        fi
      fi
    else
      echo "name owner: unowned"
    fi
    IS_LOADED_OUT="$(busctl --user --json=short call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "$PLUGIN_ID" 2>/dev/null || true)"
    if echo "$IS_LOADED_OUT" | jq -e '((keys | sort) == ["data","type"]) and (.type == "b") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "boolean")' >/dev/null 2>&1; then
      echo "isScriptLoaded($PLUGIN_ID): $(echo "$IS_LOADED_OUT" | jq -r '.data[0]')"
    else
      echo "isScriptLoaded($PLUGIN_ID): unknown ($IS_LOADED_OUT)"
    fi
    RECEIPT=""
    if [[ -f "$STATE_DIR/controller-receipt-path" ]]; then
      RECEIPT="$(cat "$STATE_DIR/controller-receipt-path")"
      echo "receipt pointer: $RECEIPT"
    fi
    if [[ -z "$RECEIPT" ]]; then
      mapfile -t CANDIDATES < <(compgen -G "$RUNTIME_DIR/omnitiler-controller.*/ownership" || true)
      LIVE=()
      for c in ${CANDIDATES[@]+"${CANDIDATES[@]}"}; do [[ -f "$c" && ! -L "$c" ]] && LIVE+=("$c"); done
      if [[ "${#LIVE[@]}" -eq 1 ]]; then
        RECEIPT="${LIVE[0]}"
        echo "receipt (found): $RECEIPT"
      elif [[ "${#LIVE[@]}" -eq 0 ]]; then
        echo "recorded script id: none (no receipt found)"
        RECEIPT=""
      else
        echo "recorded script id: ambiguous (${#LIVE[@]} receipts found)"
        printf '  %s\n' "${LIVE[@]}"
        RECEIPT=""
      fi
    fi
    if [[ -n "$RECEIPT" ]]; then
      if [[ -f "$RECEIPT" && ! -L "$RECEIPT" ]]; then
        SID="$(jq -r '.script_id // empty' "$RECEIPT" 2>/dev/null || true)"
        echo "recorded script id: ${SID:-unknown}"
      else
        echo "recorded script id: unknown (receipt missing: $RECEIPT)"
      fi
    fi
    if [[ -f "$STATE_DIR/planner-pid" ]]; then
      echo "recorded planner pid: $(cat "$STATE_DIR/planner-pid")"
    else
      echo "recorded planner pid: none"
    fi
    if command -v systemctl >/dev/null 2>&1; then
      echo "unit $PLANNER_UNIT active: $(systemctl --user is-active "$PLANNER_UNIT" 2>&1 || true)"
      echo "unit $PLANNER_UNIT enabled: $(systemctl --user is-enabled "$PLANNER_UNIT" 2>&1 || true)"
    else
      echo "unit $PLANNER_UNIT: systemctl not available"
    fi
    PLANNER_FACT="unknown"
    PLANNER_DETAIL=""
    if OWNER_REPLY2="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$PLANNER_BUS" 2>/dev/null)"; then
      OWNER_NAME2="$(echo "$OWNER_REPLY2" | awk '{print $NF}' | tr -d '\"')"
      if [[ -z "$OWNER_NAME2" ]]; then
        PLANNER_FACT="unknown"
        PLANNER_DETAIL="empty owner name"
      elif OWNER_PID2="$(busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$OWNER_NAME2" 2>/dev/null | jq -r '.data[0] // empty' 2>/dev/null)" && [[ "$OWNER_PID2" =~ ^[0-9]+$ ]]; then
        if VSTART="$(planner_verify_worktree "$OWNER_PID2" 2>/dev/null)"; then
          OWNER_EXE2="$(readlink "/proc/$OWNER_PID2/exe" 2>/dev/null || true)"
          case "$OWNER_EXE2" in *" (deleted)")
            PLANNER_FACT="owned-unverified"
            PLANNER_DETAIL="pid $OWNER_PID2 exe is stale (deleted); run 'just reload'"
            ;;
          *)
            PLANNER_FACT="verified"
            PLANNER_DETAIL="pid $OWNER_PID2"
            ;;
          esac
        else
          PLANNER_FACT="owned-unverified"
          PLANNER_DETAIL="pid $OWNER_PID2 did not verify as worktree $BIN planner-service"
        fi
      else
        PLANNER_FACT="unknown"
        PLANNER_DETAIL="owner PID unavailable or malformed"
      fi
    else
      PLANNER_FACT="unowned"
      PLANNER_DETAIL="name unowned"
    fi
    CTRL_FACT="unknown"
    CTRL_RAW="$(busctl --user --json=short call org.kde.KWin /Scripting org.kde.kwin.Scripting isScriptLoaded s "$PLUGIN_ID" 2>/dev/null || true)"
    if echo "$CTRL_RAW" | jq -e '((keys | sort) == ["data","type"]) and (.type == "b") and ((.data | type) == "array") and ((.data | length) == 1) and ((.data[0] | type) == "boolean")' >/dev/null 2>&1; then
      if [[ "$(echo "$CTRL_RAW" | jq -r '.data[0]')" == "true" ]]; then
        CTRL_FACT="loaded"
      else
        CTRL_FACT="unloaded"
      fi
    fi
    if [[ "$PLANNER_FACT" == "verified" && "$CTRL_FACT" == "loaded" ]]; then
      echo "dev mode: UP (planner $PLANNER_DETAIL verified, controller loaded)"
    elif [[ "$PLANNER_FACT" == "unowned" && "$CTRL_FACT" == "unloaded" ]]; then
      echo "dev mode: DOWN (planner unowned, controller unloaded)"
    elif [[ "$PLANNER_FACT" == "unknown" || "$CTRL_FACT" == "unknown" ]]; then
      echo "dev mode: UNKNOWN (planner $PLANNER_FACT${PLANNER_DETAIL:+, $PLANNER_DETAIL}, controller $CTRL_FACT)"
    elif [[ "$PLANNER_FACT" == "verified" ]]; then
      echo "dev mode: SPLIT (planner $PLANNER_DETAIL verified, controller $CTRL_FACT)"
    elif [[ "$CTRL_FACT" == "loaded" ]]; then
      echo "dev mode: SPLIT (planner $PLANNER_FACT${PLANNER_DETAIL:+, $PLANNER_DETAIL}, controller loaded)"
    else
      echo "dev mode: SPLIT (planner $PLANNER_FACT${PLANNER_DETAIL:+, $PLANNER_DETAIL}, controller $CTRL_FACT)"
    fi

# Desktop dispatcher for `just dev`: KDE runs dev-kde, GNOME hits the
# dev-gnome stub, anything else fails closed. XDG_CURRENT_DESKTOP is read
# once; KDE wins when both KDE and GNOME are listed.
dev mode="":
    #!/usr/bin/env bash
    set -euo pipefail
    JUSTFILE="{{ justfile() }}"
    DESKTOP="${XDG_CURRENT_DESKTOP:-}"
    echo "just dev: XDG_CURRENT_DESKTOP='$DESKTOP'"
    DESKTOP_LOWER="$(printf '%s' "$DESKTOP" | tr '[:upper:]' '[:lower:]')"
    case ":$DESKTOP_LOWER:" in
      *:kde:*)
        echo "just dev: dispatching to dev-kde"
        exec just --justfile "$JUSTFILE" dev-kde "{{ mode }}"
        ;;
      *:gnome:*)
        echo "just dev: dispatching to dev-gnome"
        exec just --justfile "$JUSTFILE" dev-gnome
        ;;
      *)
        echo "error: just dev: unsupported XDG_CURRENT_DESKTOP '$DESKTOP' (expected KDE; GNOME is not supported yet)" >&2
        exit 1
        ;;
    esac

# GNOME dev session: not supported yet, fails closed.
dev-gnome:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "error: just dev-gnome: GNOME is not supported yet (XDG_CURRENT_DESKTOP='${XDG_CURRENT_DESKTOP:-}'); KDE Plasma is required" >&2
    exit 1

# `just dev verbose` keeps bounded lifecycle and failure diagnostics. `just dev trace`
# additionally enables bounded structural Planner request/reply, KWin per-window writes, and hook detail.
# Foreground full-solution dev session: build all components, refuse unless DOWN, run dev-on,
# start the worktree tray, tail logs, and tear down on exit/Ctrl-C.
[continue]
dev-kde mode="":
    #!/usr/bin/env bash
    set -euo pipefail
    JUSTFILE="{{ justfile() }}"
    RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
    STATE_DIR="$RUNTIME_DIR/omnitiler-dev"
    DEV_MODE="{{ mode }}"
    if [[ "$DEV_MODE" == "trace" ]]; then
      export OMNITILER_TRACE=1
    elif [[ "$DEV_MODE" == "verbose" ]]; then
      :
    elif [[ -n "$DEV_MODE" ]]; then
      echo "error: just dev: unknown mode '$DEV_MODE' (expected '', 'verbose', or 'trace')" >&2
      exit 1
    fi
    # Reuse the existing strict read-only health probe; only a known DOWN
    # state can be owned and torn down by this foreground invocation.
    DEV_STATUS="$(just --justfile "$JUSTFILE" dev-status)"
    printf '%s\n' "$DEV_STATUS"
    case "$DEV_STATUS" in
      *"dev mode: DOWN (planner unowned, controller unloaded)"*) ;;
      *"dev mode:"*)
        echo "error: just dev: refusing: dev mode is not DOWN; this foreground session never adopts UP/SPLIT" >&2
        echo "hint: run 'just dev-status' to inspect, 'just dev-off' to teardown pre-existing, 'just dev-on' for detached bring-up/recovery" >&2
        exit 1
        ;;
      *)
        echo "error: just dev: dev-status did not report a usable mode; refusing" >&2
        exit 1
        ;;
    esac
    # Native preflight before any build or startup mutation. Preserves prior
    # effect state; unsupported/unavailable fails closed with setup +
    # logout/login guidance. Ordinary transport/parse failures fail closed.
    REPO_ROOT="{{ justfile_directory() }}"
    BIN="$REPO_ROOT/target/debug/omnitiler"
    NATIVE_HELPER="$REPO_ROOT/scripts/dev-native-effect.sh"
    BORDER_EFFECT="omnitiler-active-border"
    NATIVE_PREFLIGHT_OUT=""
    NATIVE_PREFLIGHT_RC=0
    NATIVE_PREFLIGHT_OUT="$(bash "$NATIVE_HELPER" preflight 2>&1)" || NATIVE_PREFLIGHT_RC=$?
    printf '%s\n' "$NATIVE_PREFLIGHT_OUT"
    if [[ "$NATIVE_PREFLIGHT_RC" -eq 2 ]]; then
      echo "error: just dev: refusing: dev native effects are not discoverable; run 'just dev-native-setup', then log out and log back in (or start a new session), then re-run" >&2
      exit 2
    fi
    if [[ "$NATIVE_PREFLIGHT_RC" -ne 0 ]]; then
      echo "error: just dev: native preflight failed (exit $NATIVE_PREFLIGHT_RC); refusing bring-up; no dev lifecycle changes made" >&2
      exit "$NATIVE_PREFLIGHT_RC"
    fi
    NATIVE_KWIN_OWNER="$(printf '%s\n' "$NATIVE_PREFLIGHT_OUT" | sed -n 's/^kwin_owner=//p' | head -n 1)"
    NATIVE_KWIN_PID="$(printf '%s\n' "$NATIVE_PREFLIGHT_OUT" | sed -n 's/^kwin_pid=//p' | head -n 1)"
    NATIVE_KWIN_START="$(printf '%s\n' "$NATIVE_PREFLIGHT_OUT" | sed -n 's/^kwin_start=//p' | head -n 1)"
    [[ "$NATIVE_KWIN_OWNER" =~ ^:[0-9]+\.[0-9]+$ ]] || { echo "error: just dev: native preflight returned no valid KWin owner; refusing" >&2; exit 1; }
    [[ "$NATIVE_KWIN_PID" =~ ^[1-9][0-9]*$ ]] || { echo "error: just dev: native preflight returned no valid KWin pid; refusing" >&2; exit 1; }
    [[ "$NATIVE_KWIN_START" =~ ^[1-9][0-9]*$ ]] || { echo "error: just dev: native preflight returned no valid KWin start identity; refusing" >&2; exit 1; }
    BORDER_PRELOADED="false"
    BORDER_LINE="$(printf '%s\n' "$NATIVE_PREFLIGHT_OUT" | grep -F "effect $BORDER_EFFECT " | head -n 1 || true)"
    [[ -n "$BORDER_LINE" ]] || { echo "error: just dev: native preflight returned no usable effect state; refusing" >&2; exit 1; }
    case "$BORDER_LINE" in *"loaded=true"*) BORDER_PRELOADED="true" ;; *"loaded=false"*) BORDER_PRELOADED="false" ;; *) echo "error: just dev: ambiguous border preflight line; refusing" >&2; exit 1 ;; esac
    # Never rebuild a loaded plugin in a way that claims the new binary is
    # active: a loaded effect keeps its mapped library until logout/login.
    # Skip the native stage when preloaded; otherwise build all components.
    JUST_BUILD_RC=0
    if [[ "$BORDER_PRELOADED" == "true" ]]; then
      echo "warning: native effect already loaded (border preloaded=$BORDER_PRELOADED); skipping native rebuild so the loaded library is never overwritten. No hot reload is promised; a rebuilt binary still requires logout/login."
      just --justfile "$JUSTFILE" build-rust || JUST_BUILD_RC=$?
      if [[ "$JUST_BUILD_RC" -ne 0 ]]; then
        echo "error: just dev: build-rust failed (exit $JUST_BUILD_RC); refusing bring-up; no dev lifecycle changes made" >&2
        exit "$JUST_BUILD_RC"
      fi
      just --justfile "$JUSTFILE" build-kwin-script || JUST_BUILD_RC=$?
      if [[ "$JUST_BUILD_RC" -ne 0 ]]; then
        echo "error: just dev: build-kwin-script failed (exit $JUST_BUILD_RC); refusing bring-up; no dev lifecycle changes made" >&2
        exit "$JUST_BUILD_RC"
      fi
    else
      just --justfile "$JUSTFILE" build || JUST_BUILD_RC=$?
      if [[ "$JUST_BUILD_RC" -ne 0 ]]; then
        echo "error: just dev: build failed (exit $JUST_BUILD_RC); refusing bring-up; no dev lifecycle changes made" >&2
        exit "$JUST_BUILD_RC"
      fi
    fi
    echo "warning: native effects staged under target/kwin-native-effect-stage are not live in this already-running KWin until logout/login delivers them; transient loadEffect below never hot-reloads a rebuilt binary. omnitiler-active-border.so remains stale until logout/login."
    # Transiently load only effects this invocation owns (supported and not
    # preloaded). Preloaded effects are preserved. No persisted enabled
    # config is written. Owner identity (unique owner, pid, start) is pinned
    # from preflight; a changed KWin owner is never mutated.
    OWNED_LOADS=""
    native_unload_owned_reverse() {
      local rc=0 effect unload_rc
      local -a stack=()
      local item
      for item in ${OWNED_LOADS:-}; do [[ -n "$item" ]] && stack+=("$item"); done
      local i
      for (( i=${#stack[@]}-1; i>=0; i-- )); do
        effect="${stack[$i]}"
        unload_rc=0
        bash "$NATIVE_HELPER" unload "$effect" --expect-owner "$NATIVE_KWIN_OWNER" --expect-pid "$NATIVE_KWIN_PID" --expect-start "$NATIVE_KWIN_START" || unload_rc=$?
        if [[ "$unload_rc" -ne 0 ]]; then
          echo "error: just dev: failed to unload owned effect $effect; native state is unresolved (do not retry unload; recover with logout/login)" >&2
          rc=1
        fi
      done
      OWNED_LOADS=""
      return "$rc"
    }
    native_load_one() {
      local effect="$1" load_rc=0
      bash "$NATIVE_HELPER" load "$effect" --expect-owner "$NATIVE_KWIN_OWNER" --expect-pid "$NATIVE_KWIN_PID" --expect-start "$NATIVE_KWIN_START" || load_rc=$?
      if [[ "$load_rc" -ne 0 ]]; then
        echo "error: just dev: failed to load effect $effect; unwinding owned loads in reverse" >&2
        native_unload_owned_reverse || true
        return "$load_rc"
      fi
      if [[ -z "$OWNED_LOADS" ]]; then
        OWNED_LOADS="$effect"
      else
        OWNED_LOADS="$OWNED_LOADS $effect"
      fi
    }
    # Full session teardown (dev-off + owned native unload in reverse).
    # Defined before any native load. It is invoked only after observed
    # dev-on success (see DEV_ON_OK handoff below), so it never calls
    # dev-off before success.
    TEARDOWN_DONE=0
    DEV_ON_OK=0
    INT_RECEIVED=0
    TAIL_PID=""
    JOURNAL_PID=""
    FOLLOW_PID=""
    TEE_PID=""
    KWIN_PID=""
    PLANNER_LOG=""
    DEV_LOG=""
    DEV_FIFO="$STATE_DIR/dev-stream"
    PLANNER_STREAM="$STATE_DIR/dev-planner-stream"
    KWIN_STREAM="$STATE_DIR/dev-kwin-stream"
    TRAY_BUS="com.omnitiler.Tray"
    TRAY_OWNED=0
    TRAY_PID=""
    TRAY_START=""
    TRAY_LOG=""
    TRAY_TAIL_PID=""
    TRAY_STREAM="$STATE_DIR/dev-tray-stream"
    tray_start_identity() {
      local pid="$1" stat_line stat_pid rest
      local -a fields=()
      stat_line="$(<"/proc/$pid/stat")" || return 1
      [[ "$stat_line" != *$'\n'* ]] || return 1
      stat_pid="${stat_line%% *}"
      [[ "$stat_pid" == "$pid" ]] || return 1
      rest="${stat_line##*) }"
      [[ "$rest" != "$stat_line" ]] || return 1
      read -r -a fields <<<"$rest"
      [[ "${#fields[@]}" -ge 20 && "${fields[0]:-}" =~ ^[A-Za-z]$ ]] || return 1
      [[ "${fields[19]:-}" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "${fields[19]}"
    }
    tray_exe_is_worktree() {
      local raw="$1" normalized="$1"
      [[ -n "$raw" ]] || return 1
      case "$BIN" in *" (deleted)") return 1 ;; esac
      case "$normalized" in *" (deleted)") normalized="${normalized%' (deleted)'}"; ;; esac
      case "$normalized" in */nix/store/*) return 1 ;; esac
      [[ "$normalized" == "$BIN" ]] || return 1
    }
    tray_verify_worktree() {
      local pid="$1" exe candidate_start
      [[ "$pid" =~ ^[0-9]+$ ]] || return 1
      [[ -d "/proc/$pid" ]] || return 1
      exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
      tray_exe_is_worktree "$exe" || return 1
      tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null | grep -Fq "tray" || return 1
      candidate_start="$(tray_start_identity "$pid")" || return 1
      [[ "$candidate_start" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "$candidate_start"
    }
    tray_dbus_owner_pid() {
      local owner_reply owner_name owner_pid
      owner_reply="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s "$TRAY_BUS" 2>/dev/null)" || return 1
      owner_name="$(echo "$owner_reply" | awk '{print $NF}' | tr -d '\"')"
      [[ -n "$owner_name" ]] || return 1
      owner_pid="$(busctl --user --json=short call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s "$owner_name" 2>/dev/null | jq -r '.data[0] // empty' 2>/dev/null || true)"
      [[ "$owner_pid" =~ ^[0-9]+$ ]] || return 1
      printf '%s\n' "$owner_pid"
    }
    dev_cleanup() {
      local rc=$?
      if [[ "${1:-}" != "" ]]; then
        rc="$1"
      fi
      trap - INT TERM EXIT
      set +e
      if [[ -n "${TAIL_PID:-}" ]]; then kill "$TAIL_PID" 2>/dev/null || true; fi
      if [[ -n "${JOURNAL_PID:-}" ]]; then kill "$JOURNAL_PID" 2>/dev/null || true; fi
      if [[ -n "${TRAY_TAIL_PID:-}" ]]; then kill "$TRAY_TAIL_PID" 2>/dev/null || true; fi
      if [[ -n "${FOLLOW_PID:-}" ]]; then kill "$FOLLOW_PID" 2>/dev/null || true; fi
      if [[ -n "${TEE_PID:-}" ]]; then kill "$TEE_PID" 2>/dev/null || true; fi
      # No generic `kill $(jobs -p)`: the owned tray is a child job and is
      # stopped only through the verified tray teardown below.
      if [[ "${TEARDOWN_DONE:-0}" -eq 0 ]]; then
        TEARDOWN_DONE=1
        TRAY_OFF_RC=0
        if [[ "${TRAY_OWNED:-0}" -eq 1 ]]; then
          if [[ -z "${TRAY_PID:-}" ]]; then
            echo "error: just dev: tray marked owned but pid is missing; refusing ambiguous teardown" >&2
            TRAY_OFF_RC=1
          elif ! kill -0 "$TRAY_PID" 2>/dev/null; then
            echo "just dev: owned tray pid $TRAY_PID already exited; nothing to stop"
          else
            TRAY_REVERIFY=""
            TRAY_REVERIFY="$(tray_verify_worktree "$TRAY_PID" 2>/dev/null || true)"
            if [[ -z "$TRAY_REVERIFY" || -z "${TRAY_START:-}" || "$TRAY_REVERIFY" != "$TRAY_START" ]]; then
              echo "error: just dev: owned tray identity changed for pid $TRAY_PID; refusing to kill ambiguously" >&2
              TRAY_OFF_RC=1
            else
              kill "$TRAY_PID" 2>/dev/null || { echo "error: just dev: could not terminate owned tray pid $TRAY_PID" >&2; TRAY_OFF_RC=1; }
              if [[ "$TRAY_OFF_RC" -eq 0 ]]; then
                for _ in $(seq 1 50); do kill -0 "$TRAY_PID" 2>/dev/null || break; sleep 0.2; done
                if kill -0 "$TRAY_PID" 2>/dev/null; then
                  echo "error: just dev: owned tray pid $TRAY_PID did not exit after SIGTERM; refusing to SIGKILL ambiguously" >&2
                  TRAY_OFF_RC=1
                fi
              fi
            fi
          fi
        fi
        # Wait for the stopped log followers, never a tray whose identity
        # changed and therefore must remain untouched.
        for child in "${TAIL_PID:-}" "${JOURNAL_PID:-}" "${TRAY_TAIL_PID:-}" "${FOLLOW_PID:-}" "${TEE_PID:-}"; do
          if [[ -n "$child" ]]; then wait "$child" 2>/dev/null || true; fi
        done
        just --justfile "$JUSTFILE" dev-off
        OFF_RC=$?
        NATIVE_OFF_RC=0
        if [[ -n "${OWNED_LOADS:-}" ]]; then
          native_unload_owned_reverse || NATIVE_OFF_RC=$?
        fi
        if [[ -n "${DEV_LOG:-}" ]]; then echo "combined log: $DEV_LOG"; fi
        rm -f -- "$STATE_DIR/dev-stream" "$STATE_DIR/dev-planner-stream" "$STATE_DIR/dev-kwin-stream" "$STATE_DIR/dev-tray-stream" "$STATE_DIR/tray-pid" "$STATE_DIR/tray-exe" "$STATE_DIR/tray-start" "$STATE_DIR/tray-log" 2>/dev/null || true
        if [[ "$TRAY_OFF_RC" -ne 0 ]]; then
          echo "error: just dev: owned tray teardown failed; teardown is unverified" >&2
        fi
        if [[ "$NATIVE_OFF_RC" -ne 0 ]]; then
          echo "error: just dev: owned native effect unload failed; native state is unresolved (do not retry unload; recover with logout/login)" >&2
        fi
        if [[ "$OFF_RC" -ne 0 ]]; then
          echo "error: just dev: dev-off teardown failed (exit $OFF_RC)" >&2
          echo "error: just dev: teardown is unverified; do not retry unload. Recover with logout/login." >&2
          exit "$OFF_RC"
        fi
        if [[ "$TRAY_OFF_RC" -ne 0 ]]; then
          echo "error: just dev: teardown is unverified; do not retry unload. Recover with logout/login." >&2
          exit "$TRAY_OFF_RC"
        fi
        if [[ "$NATIVE_OFF_RC" -ne 0 ]]; then
          echo "error: just dev: teardown is unverified; do not retry unload. Recover with logout/login." >&2
          exit "$NATIVE_OFF_RC"
        fi
        if [[ "${INT_RECEIVED:-0}" -eq 1 && "$rc" -eq 130 ]]; then
          rc=0
        fi
      fi
      exit "$rc"
    }
    # Minimal native-only early cleanup for the narrow window from the first
    # native load through dev-on success. State-aware: once DEV_ON_OK is set
    # after observed dev-on success, it delegates to the full dev_cleanup so
    # an INT/TERM/EXIT arriving before the sequential trap replacements below
    # still tears down via dev-off. Early INT/TERM exit through this EXIT
    # handler, so they route the same way. Never calls dev-off directly.
    native_early_cleanup() {
      local rc=$?
      if [[ "${DEV_ON_OK:-0}" -eq 1 ]]; then
        dev_cleanup "$rc"
        exit "$rc"
      fi
      trap - EXIT INT TERM
      native_unload_owned_reverse || true
      exit "$rc"
    }
    trap native_early_cleanup EXIT
    trap 'if [[ "${DEV_ON_OK:-0}" -eq 1 ]]; then INT_RECEIVED=1; fi; exit 130' INT
    trap 'exit 143' TERM
    if [[ "$BORDER_PRELOADED" == "false" ]]; then
      native_load_one "$BORDER_EFFECT" || { echo "error: just dev: native load failed; no script/planner lifecycle started" >&2; exit 1; }
    else
      echo "just dev: preserving preloaded effect $BORDER_EFFECT (never unloaded by this session)"
    fi
    # Bring-up composes the existing detached recipe. Its own fail-closed
    # rollback owns failures here; native owned loads are unwound on failure
    # and no logs are tailed without success.
    JUST_DEV_ON_RC=0
    just --justfile "$JUSTFILE" dev-on || JUST_DEV_ON_RC=$?
    if [[ "$JUST_DEV_ON_RC" -ne 0 ]]; then
      echo "error: just dev: bring-up via dev-on failed (exit $JUST_DEV_ON_RC); unwinding owned native loads in reverse; no logs tailed; rollback handled by dev-on" >&2
      native_unload_owned_reverse || true
      exit "$JUST_DEV_ON_RC"
    fi
    # dev-on succeeded; publish the state-aware handoff before replacing
    # traps. DEV_ON_OK routes the still-installed early handlers to full
    # dev_cleanup, closing the interrupt window. The replacements below are
    # sequential (one signal per trap builtin, not simultaneous); the flag,
    # not replacement timing, provides correctness. The full handler was
    # defined before any load and is only invoked after success, so dev-off
    # is never called before success. One lifecycle structure, no durable recovery.
    DEV_ON_OK=1
    trap dev_cleanup EXIT
    trap 'INT_RECEIVED=1; exit 130' INT
    trap 'exit 143' TERM
    # Foreground-only worktree tray, started after observed dev-on success.
    # Detached dev-on/off never touch it. A pre-owned name means an installed
    # tray holds it: log that plainly and run without a worktree tray rather
    # than claiming it. Only a D-Bus owner that verifies as this worktree
    # `$BIN tray` (exe, cmdline, start identity) is recorded and stopped.
    PRE_TRAY_OWNER=""
    PRE_TRAY_OWNER="$(tray_dbus_owner_pid 2>/dev/null || true)"
    if [[ -n "$PRE_TRAY_OWNER" ]]; then
      echo "just dev: tray name $TRAY_BUS already owned (pid $PRE_TRAY_OWNER); installed tray preserved, worktree tray not started"
      TRAY_OWNED=0
    else
      [[ -x "$BIN" ]] || { echo "error: just dev: worktree binary missing for tray: $BIN" >&2; exit 1; }
      TRAY_LOG="$(mktemp "$RUNTIME_DIR/omnitiler-tray-dev.XXXXXX.log")" || { echo "error: just dev: could not create tray log" >&2; exit 1; }
      setsid nohup "$BIN" tray >"$TRAY_LOG" 2>&1 </dev/null &
      TRAY_LAUNCH_PID=$!
      # Direct-child identity only: $! is a hint (setsid may fork), so
      # capture its start identity immediately. Timeout teardown stops
      # only this PID when it still verifies as this worktree tray with
      # the same start; otherwise it logs unresolved and touches nothing.
      TRAY_LAUNCH_START=""
      TRAY_LAUNCH_START="$(tray_start_identity "$TRAY_LAUNCH_PID" 2>/dev/null || true)"
      TRAY_PID=""
      TRAY_START=""
      TRAY_PROVED=0
      for _ in $(seq 1 50); do
        if TRAY_OWNER_PID="$(tray_dbus_owner_pid 2>/dev/null)"; then
          if TRAY_CAND_START="$(tray_verify_worktree "$TRAY_OWNER_PID" 2>/dev/null)"; then
            TRAY_PID="$TRAY_OWNER_PID"
            TRAY_START="$TRAY_CAND_START"
            TRAY_PROVED=1
            break
          else
            echo "just dev: tray name $TRAY_BUS owned by unexpected pid $TRAY_OWNER_PID (not verified as worktree $BIN tray); installed tray preserved, worktree tray not running (launch hint was ${TRAY_LAUNCH_PID:-unknown})"
            TRAY_OWNED=0
            TRAY_PID=""
            TRAY_PROVED=2
            break
          fi
        fi
        sleep 0.2
      done
      if [[ "$TRAY_PROVED" -eq 1 ]]; then
        TRAY_OWNED=1
        printf '%s\n' "$TRAY_PID" > "$STATE_DIR/tray-pid" || { echo "error: just dev: could not record tray pid" >&2; exit 1; }
        printf '%s\n' "$BIN" > "$STATE_DIR/tray-exe" || { echo "error: just dev: could not record tray exe" >&2; exit 1; }
        printf '%s\n' "$TRAY_START" > "$STATE_DIR/tray-start" || { echo "error: just dev: could not record tray start identity" >&2; exit 1; }
        printf '%s\n' "$TRAY_LOG" > "$STATE_DIR/tray-log" || { echo "error: just dev: could not record tray log path" >&2; exit 1; }
        echo "just dev: tray pid $TRAY_PID acquired $TRAY_BUS"
        echo "tray log: $TRAY_LOG"
      elif [[ "$TRAY_PROVED" -eq 2 ]]; then
        :
      else
        # Bounded-window timeout: a hung direct child could acquire the
        # name after this exit. Stop only the directly launched PID when
        # it still proves as this worktree tray with the launch start
        # identity. Any fork/ambiguity logs unresolved without signaling.
        if [[ "${TRAY_LAUNCH_PID:-}" =~ ^[0-9]+$ && -n "${TRAY_LAUNCH_START:-}" ]]; then
          if kill -0 "$TRAY_LAUNCH_PID" 2>/dev/null; then
            TRAY_LAUNCH_REVERIFY=""
            TRAY_LAUNCH_REVERIFY="$(tray_verify_worktree "$TRAY_LAUNCH_PID" 2>/dev/null || true)"
            if [[ -n "$TRAY_LAUNCH_REVERIFY" && "$TRAY_LAUNCH_REVERIFY" == "$TRAY_LAUNCH_START" ]]; then
              if kill "$TRAY_LAUNCH_PID" 2>/dev/null; then
                for _ in $(seq 1 50); do kill -0 "$TRAY_LAUNCH_PID" 2>/dev/null || break; sleep 0.2; done
                if kill -0 "$TRAY_LAUNCH_PID" 2>/dev/null; then
                  echo "error: just dev: launched tray pid $TRAY_LAUNCH_PID did not exit after SIGTERM; leaving it running (refusing SIGKILL); teardown is unresolved" >&2
                else
                  echo "just dev: stopped hung tray launch pid $TRAY_LAUNCH_PID (verified worktree tray, start $TRAY_LAUNCH_START)"
                fi
              else
                echo "just dev: launched tray pid $TRAY_LAUNCH_PID could not be signaled; launch identity unresolved; not touching any other PID"
              fi
            elif [[ -n "$TRAY_LAUNCH_REVERIFY" ]]; then
              echo "error: just dev: launched tray identity changed for pid $TRAY_LAUNCH_PID (current $TRAY_LAUNCH_REVERIFY, launch $TRAY_LAUNCH_START); refusing to kill ambiguously" >&2
            else
              echo "just dev: launched tray pid $TRAY_LAUNCH_PID did not verify as worktree $BIN tray; launch identity unresolved; not touching pid $TRAY_LAUNCH_PID"
            fi
          else
            echo "just dev: launched tray pid $TRAY_LAUNCH_PID is no longer running; launch identity unresolved (setsid may have forked); not touching any other PID"
          fi
        else
          echo "just dev: tray launch identity unresolved (launch hint ${TRAY_LAUNCH_PID:-unknown}); not touching any PID; a hung tray may still be starting"
        fi
        echo "error: just dev: tray name $TRAY_BUS was not owned by a verified worktree tray within the bounded window (launch hint ${TRAY_LAUNCH_PID:-unknown}); see $TRAY_LOG" >&2
        exit 1
      fi
    fi
    # From here the session is UP via this command; refresh stream paths
    # (already initialized before loads) before touching logs.
    DEV_FIFO="$STATE_DIR/dev-stream"
    PLANNER_STREAM="$STATE_DIR/dev-planner-stream"
    KWIN_STREAM="$STATE_DIR/dev-kwin-stream"
    TRAY_STREAM="$STATE_DIR/dev-tray-stream"
    [[ -f "$STATE_DIR/planner-log" ]] || { echo "error: just dev: missing planner log pointer ($STATE_DIR/planner-log)" >&2; exit 1; }
    PLANNER_LOG="$(cat "$STATE_DIR/planner-log")"
    [[ -n "$PLANNER_LOG" && -f "$PLANNER_LOG" && ! -L "$PLANNER_LOG" ]] || { echo "error: just dev: planner log missing or symlinked: ${PLANNER_LOG:-unknown}" >&2; exit 1; }
    [[ -f "$STATE_DIR/controller-receipt-path" ]] || { echo "error: just dev: missing controller receipt pointer" >&2; exit 1; }
    RECEIPT="$(cat "$STATE_DIR/controller-receipt-path")"
    [[ -n "$RECEIPT" && -f "$RECEIPT" && ! -L "$RECEIPT" ]] || { echo "error: just dev: controller receipt missing or symlinked: ${RECEIPT:-unknown}" >&2; exit 1; }
    KWIN_PID="$(jq -r '.pid // empty' "$RECEIPT" 2>/dev/null || true)"
    [[ "$KWIN_PID" =~ ^[1-9][0-9]*$ ]] || { echo "error: just dev: controller receipt has no valid KWin pid: $RECEIPT" >&2; exit 1; }
    command -v tail >/dev/null 2>&1 || { echo "error: just dev: required tool 'tail' not found" >&2; exit 1; }
    command -v journalctl >/dev/null 2>&1 || { echo "error: just dev: required tool 'journalctl' not found" >&2; exit 1; }
    command -v tee >/dev/null 2>&1 || { echo "error: just dev: required tool 'tee' not found" >&2; exit 1; }
    if [[ "${TRAY_OWNED:-0}" -eq 1 ]]; then
      echo "just dev: up; tailing planner log, tray log, and KWin journal (Ctrl-C tears down via dev-off)"
    else
      echo "just dev: up without worktree tray; tailing planner log and KWin journal (Ctrl-C tears down via dev-off)"
    fi
    echo "planner log: $PLANNER_LOG"
    echo "kwin pid: $KWIN_PID"
    DEV_LOG="$(mktemp "$RUNTIME_DIR/omnitiler-dev.XXXXXX.log")" || { echo "error: just dev: could not create combined log" >&2; exit 1; }
    printf '%s\n' "$DEV_LOG" > "$STATE_DIR/dev-log" || { echo "error: just dev: could not record combined log path" >&2; exit 1; }
    rm -f -- "$DEV_FIFO" "$PLANNER_STREAM" "$KWIN_STREAM" "$TRAY_STREAM" 2>/dev/null || true
    mkfifo -- "$DEV_FIFO" || { echo "error: just dev: could not create stream $DEV_FIFO" >&2; exit 1; }
    : > "$PLANNER_STREAM" || { echo "error: just dev: could not create $PLANNER_STREAM" >&2; exit 1; }
    : > "$KWIN_STREAM" || { echo "error: just dev: could not create $KWIN_STREAM" >&2; exit 1; }
    : > "$TRAY_STREAM" || { echo "error: just dev: could not create $TRAY_STREAM" >&2; exit 1; }
    echo "combined log: $DEV_LOG"
    tee -a "$DEV_LOG" <"$DEV_FIFO" &
    TEE_PID=$!
    tail -n +1 -F "$PLANNER_LOG" 2>/dev/null | sed -u 's/^/[planner] /' >>"$PLANNER_STREAM" &
    TAIL_PID=$!
    journalctl --user -f _PID="$KWIN_PID" -o cat --no-pager 2>/dev/null | grep --line-buffered -F "omnitiler:" | sed -u 's/^/[kwin] /' >>"$KWIN_STREAM" &
    JOURNAL_PID=$!
    if [[ "${TRAY_OWNED:-0}" -eq 1 ]]; then
      tail -n +1 -F "$TRAY_LOG" 2>/dev/null | sed -u 's/^/[tray] /' >>"$TRAY_STREAM" &
      TRAY_TAIL_PID=$!
      tail -q -n +1 -s 0.2 -F "$PLANNER_STREAM" "$KWIN_STREAM" "$TRAY_STREAM" >"$DEV_FIFO" 2>/dev/null &
      FOLLOW_PID=$!
    else
      tail -q -n +1 -s 0.2 -F "$PLANNER_STREAM" "$KWIN_STREAM" >"$DEV_FIFO" 2>/dev/null &
      FOLLOW_PID=$!
    fi
    wait

# Build the Rust Planner binary via devenv-aware cargo build (subset build).
build-rust:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    BIN="$REPO_ROOT/target/debug/omnitiler"
    if [[ -n "${IN_NIX_SHELL:-}${DEVENV_PROFILE:-}" ]]; then
      ( cd "$REPO_ROOT" && cargo build -p omnitiler ) || { echo "error: cargo build failed" >&2; exit 1; }
    else
      devenv shell --impure -- cargo build -p omnitiler || { echo "error: cargo build failed (via devenv shell --impure)" >&2; exit 1; }
    fi
    [[ -x "$BIN" ]] || { echo "error: worktree Planner binary missing after build: $BIN" >&2; exit 1; }

# Verify tiler-core stays portable: zero normal deps (offline) + no platform leaks.
check-portable:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    cargo metadata --manifest-path "$REPO_ROOT/Cargo.toml" --no-deps --format-version 1 --offline \
      | jq -e '([.packages[] | select(.name == "tiler-core")] | length) == 1 and ([.packages[] | select(.name == "tiler-core") | .dependencies[]? | select(.kind == null)] | length) == 0' >/dev/null \
      || { echo "error: tiler-core must exist with zero normal dependencies" >&2; exit 1; }
    if grep -rEn --include='*.rs' 'std::os::|std::process::|/proc|libc|rustix|zbus|extern "C"|cfg[[:space:]]*\([[:space:]]*(unix|windows|target_os)|target_os' "$REPO_ROOT/crates/tiler-core/src"; then
      echo "error: platform leak found in crates/tiler-core/src" >&2
      exit 1
    else
      status=$?
      [[ "$status" -eq 1 ]] || { echo "error: could not scan crates/tiler-core/src" >&2; exit 1; }
    fi
    echo "check-portable: tiler-core has 0 normal deps and no platform leaks"

# Build the KWin script bundle via npm and verify the existing bundle (subset build).
build-kwin-script:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    KWIN_DIR="$REPO_ROOT/kwin"
    BUNDLE="$KWIN_DIR/contents/code/main.js"
    command -v npm >/dev/null 2>&1 || { echo "error: required tool 'npm' not found in PATH; refusing" >&2; exit 1; }
    npm --prefix "$KWIN_DIR" run build || { echo "error: npm run build failed for $KWIN_DIR" >&2; exit 1; }
    [[ -f "$BUNDLE" ]] || { echo "error: KWin bundle missing after build: $BUNDLE" >&2; exit 1; }

# Build the native OmniTiler effect + KCMs against the exact host KWin derivation dev output via scripts/nix-host-kwin-build.sh and stage all three .so files under target/ for QT_PLUGIN_PATH use. No KWin, D-Bus, loading, config, user/system-path, or live actions (subset build, static only).
build-native-effect:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    SOURCE_DIR="$REPO_ROOT/kwin/native-effect"
    TARGET_DIR="$REPO_ROOT/target"
    EFFECT_SO="omnitiler-active-border.so"
    KCM_SO="omnitiler-active-border_config.so"
    SCRIPT_KCM_SO="omnitiler-kwin_config.so"
    BUILDER="$REPO_ROOT/scripts/nix-host-kwin-build.sh"
    [[ -x "$BUILDER" ]] || { echo "error: host-matched builder missing or not executable: $BUILDER" >&2; exit 1; }
    # Read-only provenance first (no realization, metadata only; KWinConfig is
    # validated later inside `nix develop` where Nix has realized the dev
    # output): fails closed on missing/malformed host derivation.
    RESOLVE_OUT="$(env -u OMNITILER_KWIN_DEV_CMAKE_DIR -u DOGFOOD_KWIN_DEV_CMAKE_DIR -u CMAKE_BIN -u CARGO_BIN bash "$BUILDER" resolve)" || { echo "error: host KWin provenance resolution failed; refusing native build (no fallback)" >&2; exit 1; }
    IDENTITY="$(printf '%s\n' "$RESOLVE_OUT" | sed -n 's/^identity=//p' | head -n 1)"
    [[ "$IDENTITY" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "error: invalid host package identity from builder: '${IDENTITY:-empty}'" >&2; exit 1; }
    BUILD_DIR="${OMNITILER_NATIVE_BUILD:-$REPO_ROOT/target/kwin-native-host-$IDENTITY-build}"
    STAGE="${OMNITILER_NATIVE_STAGE:-$REPO_ROOT/target/kwin-native-effect-stage}"
    env -u OMNITILER_KWIN_DEV_CMAKE_DIR -u DOGFOOD_KWIN_DEV_CMAKE_DIR -u CMAKE_BIN -u CARGO_BIN bash "$BUILDER" build --source "$SOURCE_DIR" --build-dir "$BUILD_DIR" --expected-identity "$IDENTITY" || { echo "error: host-matched native build failed for $SOURCE_DIR" >&2; exit 1; }
    BUILT_SO="$BUILD_DIR/bin/kwin/effects/plugins/$EFFECT_SO"
    BUILT_KCM="$BUILD_DIR/bin/kwin/effects/configs/$KCM_SO"
    BUILT_SCRIPT_KCM="$BUILD_DIR/bin/kwin/scripts/configs/$SCRIPT_KCM_SO"
    [[ -f "$BUILT_SO" ]] || { echo "error: effect .so not found after build: $BUILT_SO" >&2; exit 1; }
    [[ -f "$BUILT_KCM" ]] || { echo "error: KCM .so not found after build: $BUILT_KCM" >&2; exit 1; }
    [[ -f "$BUILT_SCRIPT_KCM" ]] || { echo "error: script KCM .so not found after build: $BUILT_SCRIPT_KCM" >&2; exit 1; }
    PAYLOAD="$(mktemp -d "$TARGET_DIR/.kwin-native-effect-stage.XXXXXX")" || { echo "error: could not create staging transaction directory under $TARGET_DIR" >&2; exit 1; }
    cleanup() { [[ -n "${PAYLOAD:-}" && -d "${PAYLOAD:-}" ]] && rm -rf -- "$PAYLOAD"; }
    trap cleanup EXIT
    install -Dm0644 "$BUILT_SO" "$PAYLOAD/kwin/effects/plugins/$EFFECT_SO" || { echo "error: could not stage effect .so" >&2; exit 1; }
    install -Dm0644 "$BUILT_KCM" "$PAYLOAD/kwin/effects/configs/$KCM_SO" || { echo "error: could not stage KCM .so" >&2; exit 1; }
    install -Dm0644 "$BUILT_SCRIPT_KCM" "$PAYLOAD/kwin/scripts/configs/$SCRIPT_KCM_SO" || { echo "error: could not stage script KCM .so" >&2; exit 1; }
    [[ -f "$PAYLOAD/kwin/effects/plugins/$EFFECT_SO" ]] || { echo "error: staged effect .so missing: $PAYLOAD/kwin/effects/plugins/$EFFECT_SO" >&2; exit 1; }
    [[ -f "$PAYLOAD/kwin/effects/configs/$KCM_SO" ]] || { echo "error: staged KCM .so missing: $PAYLOAD/kwin/effects/configs/$KCM_SO" >&2; exit 1; }
    [[ -f "$PAYLOAD/kwin/scripts/configs/$SCRIPT_KCM_SO" ]] || { echo "error: staged script KCM .so missing: $PAYLOAD/kwin/scripts/configs/$SCRIPT_KCM_SO" >&2; exit 1; }
    rm -rf -- "$STAGE" || { echo "error: could not remove stale staging root: $STAGE" >&2; exit 1; }
    mv -- "$PAYLOAD" "$STAGE" || { echo "error: could not publish staging root: $STAGE" >&2; exit 1; }
    PAYLOAD=""
    trap - EXIT
    [[ -f "$STAGE/kwin/effects/plugins/$EFFECT_SO" ]] || { echo "error: staged effect .so missing: $STAGE/kwin/effects/plugins/$EFFECT_SO" >&2; exit 1; }
    [[ -f "$STAGE/kwin/effects/configs/$KCM_SO" ]] || { echo "error: staged KCM .so missing: $STAGE/kwin/effects/configs/$KCM_SO" >&2; exit 1; }
    [[ -f "$STAGE/kwin/scripts/configs/$SCRIPT_KCM_SO" ]] || { echo "error: staged script KCM .so missing: $STAGE/kwin/scripts/configs/$SCRIPT_KCM_SO" >&2; exit 1; }
    echo "staged: $STAGE/kwin/effects/plugins/$EFFECT_SO"
    echo "staged: $STAGE/kwin/effects/configs/$KCM_SO"
    echo "staged: $STAGE/kwin/scripts/configs/$SCRIPT_KCM_SO"
    echo "QT_PLUGIN_PATH=$STAGE"

# One-time dev delivery setup: stage native effects, then write the project-owned plasma-workspace env script for this checkout. Takes effect only after user logout/login. Deliberate; never run by `just dev`.
dev-native-setup: build-native-effect
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    bash "$REPO_ROOT/scripts/dev-native-effect.sh" setup

# Remove only the exact project-owned plasma-workspace env script for this checkout. Never removes parent dirs or touches the running KWin.
dev-native-remove:
    #!/usr/bin/env bash
    set -euo pipefail
    REPO_ROOT="{{ justfile_directory() }}"
    bash "$REPO_ROOT/scripts/dev-native-effect.sh" remove

# Build the whole solution: Rust Planner + KWin script bundle + native effects (aggregate full build for just dev).
build: build-rust build-kwin-script build-native-effect

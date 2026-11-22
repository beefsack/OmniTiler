# Shared Linux dev-loop helper functions sourced by just/linux.just recipes.
# Callers define RUNTIME_DIR (for validate_intent_root) and BIN (for worktree
# executable checks).

validate_intent_root() {
  local raw="${1:-}" cur="" part uid dir_uid dir_mode
  local -a parts=()
  [[ -n "$raw" ]] || return 1
  [[ "$raw" != *$'\n'* && "$raw" != *$'\r'* ]] || return 1
  [[ "$raw" == /* ]] || return 1
  case "$raw" in *//*) return 1 ;; esac
  [[ "$raw" == */ ]] && raw="${raw%/}"
  [[ -n "$raw" ]] || return 1
  IFS='/' read -r -a parts <<< "${raw#/}"
  [[ "${#parts[@]}" -gt 0 ]] || return 1
  for part in "${parts[@]}"; do
    [[ -n "$part" && "$part" != "." && "$part" != ".." ]] || return 1
    cur="$cur/$part"
    [[ ! -L "$cur" ]] || return 1
    [[ -d "$cur" ]] || return 1
  done
  uid="$(id -u)" || return 1
  dir_uid="$(stat -c '%u' "$cur" 2>/dev/null)" || return 1
  [[ "$dir_uid" == "$uid" ]] || return 1
  dir_mode="$(stat -c '%a' "$cur" 2>/dev/null)" || return 1
  [[ "$dir_mode" =~ ^[0-7]+$ ]] || return 1
  (( (8#$dir_mode & 077) == 0 )) || return 1
  local orig_canon=""
  orig_canon="$(cd -P -- "$RUNTIME_DIR" 2>/dev/null && pwd -P || printf '%s' "${RUNTIME_DIR%/}")"
  [[ -n "$orig_canon" && "$cur" != "$orig_canon" ]] || return 1
  printf '%s\n' "$cur"
}

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

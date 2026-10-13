#!/usr/bin/env bash
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
HELPER="$REPO_ROOT/scripts/dev-native-effect.sh"
WORK="$(mktemp -d)"
FAKE_BIN="$WORK/fake-tools"
OUTPUT="$WORK/output.log"
PASS=0
FAIL=0
EXIT=0

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

check_exit() {
  local expected="$1" label="$2"
  if [[ "$EXIT" -ne "$expected" ]]; then
    echo "FAIL [$label]: expected exit $expected, got $EXIT" >&2
    cat "$OUTPUT" >&2
    FAIL=$((FAIL + 1))
  else
    PASS=$((PASS + 1))
  fi
}

assert_contains() {
  local needle="$1" label="$2"
  if grep -Fq "$needle" "$OUTPUT"; then
    PASS=$((PASS + 1))
  else
    echo "FAIL [$label]: missing '$needle'" >&2
    cat "$OUTPUT" >&2
    FAIL=$((FAIL + 1))
  fi
}

assert_not_contains() {
  local needle="$1" label="$2"
  if grep -Fq "$needle" "$OUTPUT"; then
    echo "FAIL [$label]: unexpected '$needle'" >&2
    cat "$OUTPUT" >&2
    FAIL=$((FAIL + 1))
  else
    PASS=$((PASS + 1))
  fi
}

# ---- Part 1: setup/remove file ops (no D-Bus) ----

part1_setup() {
  local root="$WORK/p1"
  mkdir -p "$root/stage/kwin/effects/plugins" "$root/stage/kwin/effects/configs" "$root/stage/kwin/scripts/configs" "$root/config"
  printf 'border-so' > "$root/stage/kwin/effects/plugins/omnitiler-active-border.so"
  printf 'effect-kcm-so' > "$root/stage/kwin/effects/configs/omnitiler-active-border_config.so"
  printf 'script-kcm-so' > "$root/stage/kwin/scripts/configs/omnitiler-kwin_config.so"
  printf 'oracle-so' > "$root/stage/kwin/effects/plugins/omnitiler-drag-oracle.so"
  export OMNITILER_NATIVE_STAGE="$root/stage"
  export XDG_CONFIG_HOME="$root/config"
  local env_file="$root/config/plasma-workspace/env/60-omnitiler-native-effect.sh"

  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "setup creates"
  [[ -f "$env_file" ]] && PASS=$((PASS + 1)) || { echo "FAIL [setup file exists]" >&2; FAIL=$((FAIL + 1)); }
  grep -Fq "QT_PLUGIN_PATH" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [setup content]" >&2; FAIL=$((FAIL + 1)); }
  grep -Fq "$root/stage" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [setup stage path]" >&2; FAIL=$((FAIL + 1)); }
  # Prepend/retain semantics: source with existing value.
  QT_PLUGIN_PATH="/existing/path" sh -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
  grep -Fq "$root/stage:/existing/path" "$OUTPUT" && PASS=$((PASS + 1)) || { echo "FAIL [setup prepend retain]" >&2; cat "$OUTPUT" >&2; FAIL=$((FAIL + 1)); }
  # Unset case: no leading/trailing colon.
  env -u QT_PLUGIN_PATH sh -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
  grep -Fq "$root/stage" "$OUTPUT" && PASS=$((PASS + 1)) || { echo "FAIL [setup unset]" >&2; FAIL=$((FAIL + 1)); }
  if grep -Fq "::" "$OUTPUT" || grep -Fq ":$" "$OUTPUT"; then echo "FAIL [setup no stray colon]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi

  # Idempotent second run.
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "setup idempotent"
  assert_contains "already current" "setup idempotent msg"

  # Unfamiliar content refused.
  printf 'export QT_PLUGIN_PATH="/something/else"\n' > "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup refuses unfamiliar"
  assert_contains "refusing to overwrite unfamiliar" "setup unfamiliar msg"
  [[ "$(cat "$env_file")" == 'export QT_PLUGIN_PATH="/something/else"' ]] && PASS=$((PASS + 1)) || { echo "FAIL [setup unfamiliar preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Symlink refused.
  rm -f "$env_file"
  ln -s /tmp "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup refuses symlink"
  assert_contains "symlink" "setup symlink msg"
  rm -f "$env_file"

  # Non-regular refused (directory).
  rm -f "$env_file"
  mkdir -p "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup refuses non-regular"
  rmdir "$env_file"

  # Alternate checkout refused.
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  local other="$WORK/p1-other"
  mkdir -p "$other/stage/kwin/effects/plugins" "$other/stage/kwin/effects/configs" "$other/stage/kwin/scripts/configs"
  printf 'b' > "$other/stage/kwin/effects/plugins/omnitiler-active-border.so"
  printf 'k' > "$other/stage/kwin/effects/configs/omnitiler-active-border_config.so"
  printf 's' > "$other/stage/kwin/scripts/configs/omnitiler-kwin_config.so"
  printf 'o' > "$other/stage/kwin/effects/plugins/omnitiler-drag-oracle.so"
  set +e
  OMNITILER_NATIVE_STAGE="$other/stage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup refuses alternate checkout"
  assert_contains "unfamiliar" "setup alternate msg"
  grep -Fq "$root/stage" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [setup alternate preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Missing staged .so fails actionable.
  rm -f "$root/stage/kwin/effects/plugins/omnitiler-active-border.so"
  rm -f "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup missing stage fails"
  assert_contains "just build-native-effect" "setup missing actionable"
  printf 'border-so' > "$root/stage/kwin/effects/plugins/omnitiler-active-border.so"

  # Missing staged effect KCM fails actionable even when the effect is staged.
  rm -f "$root/stage/kwin/effects/configs/omnitiler-active-border_config.so"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup missing effect KCM fails"
  assert_contains "staged effect KCM missing" "setup missing effect KCM actionable"
  assert_contains "just build-native-effect" "setup missing effect KCM build hint"
  printf 'effect-kcm-so' > "$root/stage/kwin/effects/configs/omnitiler-active-border_config.so"

  # Missing staged script settings KCM fails actionable even when the effect is staged.
  rm -f "$root/stage/kwin/scripts/configs/omnitiler-kwin_config.so"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup missing script KCM fails"
  assert_contains "staged script settings KCM missing" "setup missing script KCM actionable"
  assert_contains "just build-native-effect" "setup missing script KCM build hint"
  printf 'script-kcm-so' > "$root/stage/kwin/scripts/configs/omnitiler-kwin_config.so"

  # No kwinrc writes, no D-Bus use: setup must not create kwinrc.
  [[ ! -e "$root/config/kwinrc" ]] && PASS=$((PASS + 1)) || { echo "FAIL [setup no kwinrc]" >&2; FAIL=$((FAIL + 1)); }

  # Remove exact owned.
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  set +e
  bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "remove owned"
  assert_contains "removed:" "remove owned msg"
  [[ ! -e "$env_file" ]] && PASS=$((PASS + 1)) || { echo "FAIL [remove deleted]" >&2; FAIL=$((FAIL + 1)); }
  # Parent dirs retained.
  [[ -d "$root/config/plasma-workspace/env" ]] && PASS=$((PASS + 1)) || { echo "FAIL [remove parent retained]" >&2; FAIL=$((FAIL + 1)); }

  # Remove idempotent missing.
  set +e
  bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "remove idempotent missing"
  assert_contains "nothing to do" "remove missing msg"

  # Remove refuses unfamiliar, symlink, alternate; never removes parents; unrelated untouched.
  printf 'unrelated' > "$root/config/unrelated.conf"
  printf 'export QT_PLUGIN_PATH="/foreign"\n' > "$env_file"
  set +e
  bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "remove refuses unfamiliar"
  [[ -f "$env_file" ]] && PASS=$((PASS + 1)) || { echo "FAIL [remove unfamiliar preserved]" >&2; FAIL=$((FAIL + 1)); }
  [[ "$(cat "$root/config/unrelated.conf")" == "unrelated" ]] && PASS=$((PASS + 1)) || { echo "FAIL [remove unrelated untouched]" >&2; FAIL=$((FAIL + 1)); }
  rm -f "$env_file"
  ln -s /tmp "$env_file"
  set +e
  bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "remove refuses symlink"
  rm -f "$env_file"
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  set +e
  OMNITILER_NATIVE_STAGE="$other/stage" bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "remove refuses alternate"
  [[ -f "$env_file" ]] && PASS=$((PASS + 1)) || { echo "FAIL [remove alternate preserved]" >&2; FAIL=$((FAIL + 1)); }
  [[ ! -e "$root/config/kwinrc" ]] && PASS=$((PASS + 1)) || { echo "FAIL [remove no kwinrc]" >&2; FAIL=$((FAIL + 1)); }
}

part1_quoting() {
  local root="$WORK/p1q"
  local weird="$root/a b'\$x\"y;z&|()!"
  mkdir -p "$weird/kwin/effects/plugins" "$weird/kwin/effects/configs" "$weird/kwin/scripts/configs" "$root/config"
  printf 'b' > "$weird/kwin/effects/plugins/omnitiler-active-border.so"
  printf 'k' > "$weird/kwin/effects/configs/omnitiler-active-border_config.so"
  printf 's' > "$weird/kwin/scripts/configs/omnitiler-kwin_config.so"
  printf 'o' > "$weird/kwin/effects/plugins/omnitiler-drag-oracle.so"
  export OMNITILER_NATIVE_STAGE="$weird"
  export XDG_CONFIG_HOME="$root/config"
  local env_file="$root/config/plasma-workspace/env/60-omnitiler-native-effect.sh"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "quoting setup"
  # POSIX sh sourcing must yield exact stage prepended.
  QT_PLUGIN_PATH="/keep/me" sh -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
  EXIT=$?
  if [[ "$(cat "$OUTPUT")" == "$weird:/keep/me" ]]; then PASS=$((PASS + 1)); else echo "FAIL [quoting prepend exact]" >&2; echo "got: $(cat "$OUTPUT")" >&2; echo "want: $weird:/keep/me" >&2; FAIL=$((FAIL + 1)); fi
  env -u QT_PLUGIN_PATH sh -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
  if [[ "$(cat "$OUTPUT")" == "$weird" ]]; then PASS=$((PASS + 1)); else echo "FAIL [quoting unset exact]" >&2; cat "$OUTPUT" >&2; FAIL=$((FAIL + 1)); fi
  # dash if available.
  if command -v dash >/dev/null 2>&1; then
    QT_PLUGIN_PATH="/keep/me" dash -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
    if [[ "$(cat "$OUTPUT")" == "$weird:/keep/me" ]]; then PASS=$((PASS + 1)); else echo "FAIL [quoting dash]" >&2; cat "$OUTPUT" >&2; FAIL=$((FAIL + 1)); fi
  else
    PASS=$((PASS + 1))
  fi
  set +e
  bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "quoting remove"
}

part1_canonical_and_collision() {
  local root="$WORK/p1c"
  mkdir -p "$root/stage/kwin/effects/plugins" "$root/stage/kwin/effects/configs" "$root/stage/kwin/scripts/configs" "$root/config"
  printf 'b' > "$root/stage/kwin/effects/plugins/omnitiler-active-border.so"
  printf 'k' > "$root/stage/kwin/effects/configs/omnitiler-active-border_config.so"
  printf 's' > "$root/stage/kwin/scripts/configs/omnitiler-kwin_config.so"
  printf 'o' > "$root/stage/kwin/effects/plugins/omnitiler-drag-oracle.so"
  export OMNITILER_NATIVE_STAGE="$root/stage"
  export XDG_CONFIG_HOME="$root/config"
  local env_file="$root/config/plasma-workspace/env/60-omnitiler-native-effect.sh"

  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "canonical setup"
  # Canonical trailing newline: last byte is LF.
  if [[ "$(tail -c 1 "$env_file" | od -An -t u1 | tr -d ' \n')" == "10" ]]; then PASS=$((PASS + 1)); else echo "FAIL [canonical trailing newline]" >&2; FAIL=$((FAIL + 1)); fi
  # Missing-newline twin is not owned: setup must refuse, not claim current.
  local twin="$WORK/p1c-twin"
  head -c -1 "$env_file" > "$twin"
  cp "$twin" "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "missing newline not current"
  assert_contains "refusing to overwrite unfamiliar" "missing newline unfamiliar"
  # Dogfood/alternate content refused by both setup and remove.
  printf 'export QT_PLUGIN_PATH="/fake/data/omnitiler-native-effect${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}"\n' > "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup refuses dogfood content"
  assert_contains "unfamiliar" "setup dogfood msg"
  set +e
  bash "$HELPER" remove >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "remove refuses dogfood content"
  [[ -f "$env_file" ]] && PASS=$((PASS + 1)) || { echo "FAIL [dogfood preserved]" >&2; FAIL=$((FAIL + 1)); }
  rm -f "$env_file"
  # Directory at destination is refused and preserved (ln -T guards the race).
  mkdir -p "$env_file"
  set +e
  bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "setup refuses directory"
  [[ -d "$env_file" ]] && PASS=$((PASS + 1)) || { echo "FAIL [directory preserved]" >&2; FAIL=$((FAIL + 1)); }
  rmdir "$env_file"
  # Friendly fail-closed for missing option values (no bare shift failure).
  set +e
  bash "$HELPER" load omnitiler-active-border --expect-owner >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "load missing owner value friendly"
  assert_contains "expect-owner requires a value" "load missing owner friendly msg"
  assert_not_contains "can't shift" "load no bare shift"
  set +e
  bash "$HELPER" unload omnitiler-active-border --expect-pid >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "unload missing pid value friendly"
  assert_contains "expect-pid requires a value" "unload missing pid friendly msg"
  set +e
  bash "$HELPER" load omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "load missing start value friendly"
  assert_contains "expect-start requires a value" "load missing start friendly msg"
}

# ---- Part 1b: relocation recovery (moved checkout) ----

part1_relocation() {
  local cfg="$WORK/p1r-config" oldroot="$WORK/p1r-old" newroot="$WORK/p1r-new"
  local env_file="$cfg/plasma-workspace/env/60-omnitiler-native-effect.sh"
  local oldstage="$oldroot/target/kwin-native-effect-stage"
  local newstage="$newroot/target/kwin-native-effect-stage"
  mkrelstage() {
    mkdir -p "$1/kwin/effects/plugins" "$1/kwin/effects/configs" "$1/kwin/scripts/configs"
    printf 'b' > "$1/kwin/effects/plugins/omnitiler-active-border.so"
    printf 'k' > "$1/kwin/effects/configs/omnitiler-active-border_config.so"
    printf 's' > "$1/kwin/scripts/configs/omnitiler-kwin_config.so"
  }
  mkrelstage "$oldstage"
  mkrelstage "$newstage"
  export XDG_CONFIG_HOME="$cfg"

  # Seed this machine's env script from the old checkout, then move it away.
  set +e
  OMNITILER_NATIVE_STAGE="$oldstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "relocation seed old"
  rm -rf "$oldroot"

  # Canonical content whose stage no longer exists is replaced (old+new named).
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "relocation replaced"
  assert_contains "$oldstage" "relocation notice names old"
  assert_contains "$newstage" "relocation notice names new"
  grep -Fq "$newstage" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [relocation new content]" >&2; FAIL=$((FAIL + 1)); }
  env -u QT_PLUGIN_PATH sh -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
  if [[ "$(cat "$OUTPUT")" == "$newstage" ]]; then PASS=$((PASS + 1)); else echo "FAIL [relocation source roundtrip]" >&2; cat "$OUTPUT" >&2; FAIL=$((FAIL + 1)); fi

  # Two live checkouts: canonical other stage that still exists is refused
  # with a dev-native-remove hint naming the other checkout root, and the
  # current file is preserved.
  mkrelstage "$oldstage"
  set +e
  OMNITILER_NATIVE_STAGE="$oldstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "two checkouts refused"
  assert_contains "another checkout" "two checkout msg"
  assert_contains "dev-native-remove" "two checkout remove hint"
  assert_contains "from that checkout ('$newroot')" "two checkout names root"
  grep -Fq "$newstage" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [two checkout preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Dangling symlink stage is not absent: refused like a live checkout.
  rm -f "$env_file"
  set +e
  OMNITILER_NATIVE_STAGE="$oldstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "dangling seed old"
  rm -rf "$oldstage"
  ln -s -- "$oldroot/nonexistent-target" "$oldstage"
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "dangling stage refused"
  assert_contains "another checkout" "dangling msg"
  grep -Fq "$oldstage" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [dangling preserved]" >&2; FAIL=$((FAIL + 1)); }
  rm -f "$oldstage"
  mkrelstage "$oldstage"

  # Trailing content after a canonical line is unfamiliar: refused, preserved.
  rm -f "$env_file"
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "trailing seed"
  { cat "$env_file"; printf 'export OMNITILER_UNRELATED=1\n'; } > "$env_file.tmp"
  mv -- "$env_file.tmp" "$env_file"
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "trailing content refused"
  assert_contains "unfamiliar" "trailing content msg"
  grep -Fq "OMNITILER_UNRELATED=1" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [trailing preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Non-suffix canonical line is not owned relocation content: refused, kept.
  rm -f "$env_file"
  printf "export QT_PLUGIN_PATH='%s'\${QT_PLUGIN_PATH:+:\$QT_PLUGIN_PATH}\n" "/no/suffix/here" > "$env_file"
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "non-suffix refused"
  assert_contains "unfamiliar" "non-suffix msg"
  grep -Fq "/no/suffix/here" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [non-suffix preserved]" >&2; FAIL=$((FAIL + 1)); }

  # NUL injection inside a canonical line is refused, bytes preserved.
  rm -f "$env_file"
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "nul seed"
  { head -c 30 "$env_file"; printf '\0'; tail -c +31 "$env_file"; } > "$env_file.tmp"
  mv -- "$env_file.tmp" "$env_file"
  cp -- "$env_file" "$WORK/nul-save"
  set +e
  OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "nul refused"
  assert_contains "unfamiliar" "nul msg"
  if cmp -s -- "$env_file" "$WORK/nul-save"; then PASS=$((PASS + 1)); else echo "FAIL [nul preserved]" >&2; FAIL=$((FAIL + 1)); fi

  # Swap race: live file replaced with unfamiliar content between approval
  # and snapshot copy must be refused and preserved (approval pinned).
  rm -f "$env_file"
  set +e
  OMNITILER_NATIVE_STAGE="$oldstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "swap seed old"
  rm -rf "$oldroot"
  REAL_CP="$(command -v cp)"
  mkdir -p "$WORK/p1r-fakebin"
  cat > "$WORK/p1r-fakebin/cp" <<EOF
#!/usr/bin/env bash
printf 'export EVIL_UNFAMILIAR=1\n' > "$env_file"
exec "$REAL_CP" "\$@"
EOF
  chmod +x "$WORK/p1r-fakebin/cp"
  set +e
  PATH="$WORK/p1r-fakebin:$PATH" OMNITILER_NATIVE_STAGE="$newstage" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "swap race refused"
  assert_contains "changed during setup" "swap race msg"
  grep -Fq "EVIL_UNFAMILIAR=1" "$env_file" && PASS=$((PASS + 1)) || { echo "FAIL [swap race preserved]" >&2; FAIL=$((FAIL + 1)); }
  if grep -Fq "$newstage" "$env_file"; then echo "FAIL [swap race no takeover]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi

  # Escaped paths (spaces and single quotes) roundtrip through replacement.
  local wold="$WORK/p1r-wold/a b'q/target/kwin-native-effect-stage"
  local wnew="$WORK/p1r-wnew/a b'q/target/kwin-native-effect-stage"
  rm -rf "$WORK/p1r-wold" "$WORK/p1r-wnew" "$env_file"
  mkrelstage "$wold"
  mkrelstage "$wnew"
  set +e
  OMNITILER_NATIVE_STAGE="$wold" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "escaped seed"
  rm -rf "$WORK/p1r-wold"
  set +e
  OMNITILER_NATIVE_STAGE="$wnew" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "escaped replaced"
  assert_contains "$wold" "escaped notice names old"
  assert_contains "$wnew" "escaped notice names new"
  env -u QT_PLUGIN_PATH sh -c ". \"$env_file\"; printf '%s' \"\$QT_PLUGIN_PATH\"" > "$OUTPUT" 2>&1
  if [[ "$(cat "$OUTPUT")" == "$wnew" ]]; then PASS=$((PASS + 1)); else echo "FAIL [escaped roundtrip exact]" >&2; cat "$OUTPUT" >&2; FAIL=$((FAIL + 1)); fi
  set +e
  OMNITILER_NATIVE_STAGE="$wnew" bash "$HELPER" setup >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "escaped idempotent"
  assert_contains "already current" "escaped idempotent msg"

  # Static: setup never evaluates the env file and replaces atomically.
  if grep -Eq '^[[:space:]]*(\.|source)[[:space:]]+.*ENV_FILE' "$HELPER"; then echo "FAIL [setup never sources env]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq 'mv -T -- "$tmp" "$ENV_FILE"' "$HELPER"; then PASS=$((PASS + 1)); else echo "FAIL [setup atomic replace mv -T]" >&2; FAIL=$((FAIL + 1)); fi
}

# ---- Part 2: D-Bus preflight/load/unload with fake busctl/proc ----

part2_tools() {
  mkdir -p "$FAKE_BIN/bin" "$WORK/p2/proc" "$WORK/p2/state"
  cat > "$FAKE_BIN/bin/busctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state="${FAKE_STATE_DIR:?}"
case "$*" in
  *"GetNameOwner s org.kde.KWin"*)
    if [[ -f "$state/kwin-unowned" ]]; then exit 1; fi
    if [[ -f "$state/owner-malformed" ]]; then printf '{"type":"s","data":[123]}\n'; exit 0; fi
    owner="$(cat "$state/kwin-owner" 2>/dev/null || printf ':1.99')"
    printf '{"type":"s","data":["%s"]}\n' "$owner"
    exit 0 ;;
  *"GetConnectionUnixProcessID s :"*)
    pid="$(cat "$state/kwin-pid" 2>/dev/null || printf '5151')"
    printf '{"type":"u","data":[%s]}\n' "$pid" ;;
  *"status org.kde.KWin"*)
    if [[ -f "$state/status-fail" ]]; then exit 1; fi
    printf 'CommandLine=/nix/store/fake-kwin-6.7.5/bin/kwin_wayland --wayland-fd 7\n' ;;
  *"isEffectSupported"*)
    if [[ -f "$state/supported-fail" ]]; then exit 1; fi
    if [[ -f "$state/supported-malformed" ]]; then printf '{"type":"b","data":[true,false]}\n'; exit 0; fi
    if [[ "$*" == *"omnitiler-active-border"* ]]; then
      printf '{"type":"b","data":[%s]}\n' "$(cat "$state/border-supported" 2>/dev/null || printf 'true')"
    else
      exit 1
    fi ;;
  *"isEffectLoaded"*)
    if [[ -f "$state/loaded-fail" ]]; then exit 1; fi
    if [[ -f "$state/loaded-malformed" ]]; then printf '{"type":"b","data":[true,false]}\n'; exit 0; fi
    if [[ "$*" == *"omnitiler-active-border"* ]]; then
      printf '{"type":"b","data":[%s]}\n' "$(cat "$state/border-loaded" 2>/dev/null || printf 'false')"
    else
      exit 1
    fi ;;
  *"unloadEffect"*)
    printf 'unloadEffect %s\n' "$*" >> "${FAKE_CALL_LOG:?}"
    if [[ -f "$state/unload-fail" ]]; then exit 1; fi
    printf 'false\n' > "$state/border-loaded"
    printf '{"type":"b","data":[true]}\n'
    exit 0 ;;
  *"loadEffect"*)
    printf 'loadEffect %s\n' "$*" >> "${FAKE_CALL_LOG:?}"
    if [[ -f "$state/load-fail" ]]; then exit 1; fi
    printf 'true\n' > "$state/border-loaded"
    printf '{"type":"b","data":[true]}\n'
    exit 0 ;;
  *) exit 1 ;;
esac
EOF
  chmod +x "$FAKE_BIN/bin/busctl"
  export PATH="$FAKE_BIN/bin:$PATH"
  export FAKE_STATE_DIR="$WORK/p2/state"
  export FAKE_CALL_LOG="$WORK/p2/calls.log"
  export PROC_ROOT="$WORK/p2/proc"
  export OMNITILER_NATIVE_STAGE="$WORK/p2/stage"
  mkdir -p "$OMNITILER_NATIVE_STAGE/kwin/effects/plugins" "$OMNITILER_NATIVE_STAGE/kwin/scripts/configs"
  printf 'x\0org.kde.kwin.EffectPluginFactory6.7.4\0' > "$OMNITILER_NATIVE_STAGE/kwin/effects/plugins/omnitiler-active-border.so"
  printf 'x' > "$OMNITILER_NATIVE_STAGE/kwin/scripts/configs/omnitiler-kwin_config.so"
  mkdir -p "$PROC_ROOT/5151"
  printf '5151 (kwin_wayland) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 777888\n' > "$PROC_ROOT/5151/stat"
  printf '5151\n' > "$WORK/p2/state/kwin-pid"
  printf 'true\n' > "$WORK/p2/state/border-supported"
  printf 'false\n' > "$WORK/p2/state/border-loaded"
  : > "$WORK/p2/calls.log"
}

p2_reset() {
  rm -f "$WORK/p2/state"/kwin-unowned "$WORK/p2/state"/kwin-owner "$WORK/p2/state"/owner-malformed "$WORK/p2/state"/status-fail "$WORK/p2/state"/supported-fail "$WORK/p2/state"/supported-malformed "$WORK/p2/state"/loaded-fail "$WORK/p2/state"/loaded-malformed "$WORK/p2/state"/load-fail "$WORK/p2/state"/unload-fail
  printf '5151\n' > "$WORK/p2/state/kwin-pid"
  printf 'true\n' > "$WORK/p2/state/border-supported"
  printf 'false\n' > "$WORK/p2/state/border-loaded"
  mkdir -p "$PROC_ROOT/5151"
  printf '5151 (kwin_wayland) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 777888\n' > "$PROC_ROOT/5151/stat"
  : > "$WORK/p2/calls.log"
  : > "$OUTPUT"
}

part2_tests() {
  part2_tools

  # Preflight ok.
  p2_reset
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "preflight ok"
  assert_contains "kwin_owner=:1.99" "preflight owner"
  assert_contains "kwin_pid=5151" "preflight pid"
  assert_contains "effect omnitiler-active-border supported=true loaded=false" "preflight border"
  assert_not_contains "omnitiler-drag-oracle" "preflight single effect"

  # Unsupported ABI skew is actionable without incorrectly repeating setup.
  p2_reset
  printf 'false\n' > "$WORK/p2/state/border-supported"
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 2 "preflight unsupported"
  assert_contains "does not establish a session boundary" "unsupported not only discovery"
  assert_contains "plugin load, factory, or ABI failure" "unsupported factory"
  assert_contains "active-border=6.7.4, KWin=6.7.5" "unsupported ABI versions"
  assert_contains "Setup alone cannot resolve this ABI mismatch" "unsupported ABI no repeat setup"
  assert_not_contains "just dev-native-setup" "unsupported ABI omits setup"

  # Without recognized ABI evidence, retain the generic delivery guidance.
  p2_reset
  touch "$WORK/p2/state/status-fail"
  printf 'false\n' > "$WORK/p2/state/border-supported"
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 2 "preflight unsupported generic fallback"
  assert_contains "just dev-native-setup" "unsupported generic setup"
  assert_contains "log out" "unsupported generic logout"
  assert_not_contains "Setup alone cannot resolve this ABI mismatch" "unsupported generic no ABI claim"

  # Transport failure ordinary (exit 1).
  p2_reset
  touch "$WORK/p2/state/supported-fail"
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "preflight transport fail"

  # Malformed.
  p2_reset
  touch "$WORK/p2/state/supported-malformed"
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "preflight malformed"

  # KWin unowned.
  p2_reset
  touch "$WORK/p2/state/kwin-unowned"
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "preflight unowned"

  # Load owned.
  p2_reset
  set +e
  bash "$HELPER" load omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "load owned"
  assert_contains "transient" "load transient"
  assert_contains "no persisted" "load no persist"
  [[ "$(cat "$WORK/p2/state/border-loaded")" == "true" ]] && PASS=$((PASS + 1)) || { echo "FAIL [load state]" >&2; FAIL=$((FAIL + 1)); }

  # Load unexpected refused.
  p2_reset
  set +e
  bash "$HELPER" load something-else --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "load unexpected refused"

  # Unload owned verifies false without unmapped claim.
  p2_reset
  printf 'true\n' > "$WORK/p2/state/border-loaded"
  set +e
  bash "$HELPER" unload omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 0 "unload owned"
  assert_contains "isEffectLoaded=false verified" "unload verified"
  assert_contains "does not prove the library is unmapped" "unload no unmapped claim"
  [[ "$(cat "$WORK/p2/state/border-loaded")" == "false" ]] && PASS=$((PASS + 1)) || { echo "FAIL [unload state]" >&2; FAIL=$((FAIL + 1)); }

  # Owner guard: wrong pid refuses without mutating.
  p2_reset
  printf 'true\n' > "$WORK/p2/state/border-loaded"
  : > "$WORK/p2/calls.log"
  set +e
  bash "$HELPER" unload omnitiler-active-border --expect-owner :1.99 --expect-pid 9999 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "unload owner guard pid"
  assert_contains "mutate a new owner" "owner guard msg"
  if grep -Fq "unloadEffect" "$WORK/p2/calls.log"; then echo "FAIL [owner guard no mutate]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  [[ "$(cat "$WORK/p2/state/border-loaded")" == "true" ]] && PASS=$((PASS + 1)) || { echo "FAIL [owner guard preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Owner guard: PID reuse with new start tick refuses.
  p2_reset
  mkdir -p "$PROC_ROOT/5151"
  printf '5151 (kwin_wayland) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 999999\n' > "$PROC_ROOT/5151/stat"
  : > "$WORK/p2/calls.log"
  set +e
  bash "$HELPER" unload omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "unload owner guard start"
  if grep -Fq "unloadEffect" "$WORK/p2/calls.log"; then echo "FAIL [start guard no mutate]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  printf '5151 (kwin_wayland) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 777888\n' > "$PROC_ROOT/5151/stat"

  # Owner guard: unique owner changes with same PID/start refuses without mutating (load).
  p2_reset
  printf ':1.100\n' > "$WORK/p2/state/kwin-owner"
  : > "$WORK/p2/calls.log"
  set +e
  bash "$HELPER" load omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "load owner guard unique owner"
  assert_contains "mutate a new owner" "load owner change msg"
  if grep -Fq "loadEffect" "$WORK/p2/calls.log"; then echo "FAIL [load owner change no mutate]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  [[ "$(cat "$WORK/p2/state/border-loaded")" == "false" ]] && PASS=$((PASS + 1)) || { echo "FAIL [load owner change preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Owner guard: unique owner changes with same PID/start refuses without mutating (unload).
  p2_reset
  printf 'true\n' > "$WORK/p2/state/border-loaded"
  printf ':1.100\n' > "$WORK/p2/state/kwin-owner"
  : > "$WORK/p2/calls.log"
  set +e
  bash "$HELPER" unload omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "unload owner guard unique owner"
  assert_contains "mutate a new owner" "unload owner change msg"
  if grep -Fq "unloadEffect" "$WORK/p2/calls.log"; then echo "FAIL [unload owner change no mutate]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  [[ "$(cat "$WORK/p2/state/border-loaded")" == "true" ]] && PASS=$((PASS + 1)) || { echo "FAIL [unload owner change preserved]" >&2; FAIL=$((FAIL + 1)); }

  # Preflight emits strict-validated owner; malformed owner fails closed.
  p2_reset
  touch "$WORK/p2/state/owner-malformed"
  set +e
  bash "$HELPER" preflight >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "preflight malformed owner"

  # Load missing/malformed owner fails closed without mutating.
  p2_reset
  : > "$WORK/p2/calls.log"
  set +e
  bash "$HELPER" load omnitiler-active-border --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "load missing owner fails closed"
  if grep -Fq "loadEffect" "$WORK/p2/calls.log"; then echo "FAIL [load missing owner no mutate]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  set +e
  bash "$HELPER" load omnitiler-active-border --expect-owner bogus --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "load malformed owner fails closed"
  if grep -Fq "loadEffect" "$WORK/p2/calls.log"; then echo "FAIL [load malformed owner no mutate]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi

  # Failed unload reports unresolved, no retry.
  p2_reset
  printf 'true\n' > "$WORK/p2/state/border-loaded"
  touch "$WORK/p2/state/unload-fail"
  set +e
  bash "$HELPER" unload omnitiler-active-border --expect-owner :1.99 --expect-pid 5151 --expect-start 777888 >"$OUTPUT" 2>&1
  EXIT=$?
  set -e
  check_exit 1 "unload fail unresolved"
  assert_contains "unresolved" "unload unresolved msg"
  assert_contains "do not retry unload" "unload no retry"

  # Static: helper never writes persisted enable config.
  if grep -Eq "kwriteconfig6|KWRITECONFIG6_BIN|EFFECT_CONFIG_KEY|dogfood-install" "$HELPER"; then echo "FAIL [helper no persistent enable writes]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  # Static: helper never promises hot reload.
  if grep -Eqi "hot.?reload.*(works|complete|active|applied)" "$HELPER"; then echo "FAIL [helper no hot reload promise]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "does not prove the library is unmapped" "$HELPER"; then PASS=$((PASS + 1)); else echo "FAIL [helper unmapped disclaimer]" >&2; FAIL=$((FAIL + 1)); fi
  # Static: setup publication is atomic no-clobber, never mv -n success claim.
  if grep -Fq "mv -n" "$HELPER"; then echo "FAIL [setup no mv -n]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq 'ln -T -- "$tmp" "$ENV_FILE"' "$HELPER"; then PASS=$((PASS + 1)); else echo "FAIL [setup hardlink no-clobber ln -T]" >&2; FAIL=$((FAIL + 1)); fi
  # Static: ownership compares raw bytes (cmp), never command-substitution stripping.
  if grep -Fq 'cmp -s -- "$ENV_FILE" <(expected_env_contents)' "$HELPER"; then PASS=$((PASS + 1)); else echo "FAIL [setup raw cmp ownership]" >&2; FAIL=$((FAIL + 1)); fi
  if grep -Fq 'expected_env_contents > "$tmp"' "$HELPER"; then PASS=$((PASS + 1)); else echo "FAIL [setup canonical newline write]" >&2; FAIL=$((FAIL + 1)); fi
  # Static: just dev parses/passes strict owner and fails closed if missing/malformed.
  if grep -Fq 'kwin_owner=' "$REPO_ROOT/just/linux.just" && grep -Fq -- '--expect-owner "$NATIVE_KWIN_OWNER"' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [just dev owner parse/pass]" >&2; FAIL=$((FAIL + 1)); fi
  if grep -Fq 'no valid KWin owner' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [just dev owner fail-closed]" >&2; FAIL=$((FAIL + 1)); fi
  # Static: narrow interrupt window has state-aware early trap, handed off after dev-on, never calls dev-off directly.
  if grep -Fq 'native_early_cleanup' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [just dev early trap present]" >&2; FAIL=$((FAIL + 1)); fi
  EARLY_BLOCK="$(awk '/native_early_cleanup\(\) \{/,/^\s*\}/' "$REPO_ROOT/just/linux.just")"
  if printf '%s\n' "$EARLY_BLOCK" | grep -Fq "dev-off"; then echo "FAIL [early trap never calls dev-off]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if printf '%s\n' "$EARLY_BLOCK" | grep -Fq 'dev_cleanup "$rc"'; then PASS=$((PASS + 1)); else echo "FAIL [early trap delegates to full cleanup]" >&2; FAIL=$((FAIL + 1)); fi
  if printf '%s\n' "$EARLY_BLOCK" | grep -Fq 'DEV_ON_OK'; then PASS=$((PASS + 1)); else echo "FAIL [early trap state-aware]" >&2; FAIL=$((FAIL + 1)); fi
  if grep -Fq 'DEV_ON_OK=0' "$REPO_ROOT/just/linux.just" && grep -Fq 'DEV_ON_OK=1' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [handoff flag init/set]" >&2; FAIL=$((FAIL + 1)); fi
  if grep -Fq 'publish the state-aware handoff' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [early trap handed off after dev-on]" >&2; FAIL=$((FAIL + 1)); fi
  # Static: post-dev-on handoff installs full cleanup with sequential (not simultaneous) replacement.
  if grep -Fq 'trap dev_cleanup EXIT' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [handoff installs full cleanup]" >&2; FAIL=$((FAIL + 1)); fi
  HANDOFF_BLOCK="$(awk '/dev-on succeeded; publish the state-aware handoff/,/trap .exit 143. TERM/' "$REPO_ROOT/just/linux.just")"
  if printf '%s\n' "$HANDOFF_BLOCK" | grep -Fxq '    trap - EXIT INT TERM'; then echo "FAIL [handoff no trap - gap]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq 'only invoked after' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [handoff never before success]" >&2; FAIL=$((FAIL + 1)); fi
  if printf '%s\n' "$HANDOFF_BLOCK" | grep -Fq 'atomically'; then echo "FAIL [handoff no atomic multi-signal claim]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq 'replace the minimal native-only early trap' "$REPO_ROOT/just/linux.just"; then echo "FAIL [handoff no stale atomic phrasing]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq 'are sequential' "$REPO_ROOT/just/linux.just" || grep -Fq 'sequential (one signal per trap' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [handoff sequential semantics]" >&2; FAIL=$((FAIL + 1)); fi
  # Ordering: DEV_ON_OK=1 is published before any post-success trap replacement.
  DEV_OK_LINE="$(grep -n -F 'DEV_ON_OK=1' "$REPO_ROOT/just/linux.just" | head -n 1 | cut -d: -f1)"
  TRAP_FULL_LINE="$(grep -n -F 'trap dev_cleanup EXIT' "$REPO_ROOT/just/linux.just" | head -n 1 | cut -d: -f1)"
  if [[ -n "$DEV_OK_LINE" && -n "$TRAP_FULL_LINE" && "$DEV_OK_LINE" -lt "$TRAP_FULL_LINE" ]]; then PASS=$((PASS + 1)); else echo "FAIL [handoff flag before trap replacement]" >&2; FAIL=$((FAIL + 1)); fi
  # Early INT routes post-success interrupts to full cleanup with Ctrl-C normalization.
  if grep -Fq 'DEV_ON_OK:-0}" -eq 1 ]]; then INT_RECEIVED=1' "$REPO_ROOT/just/linux.just"; then PASS=$((PASS + 1)); else echo "FAIL [early INT state-aware]" >&2; FAIL=$((FAIL + 1)); fi
  # Functional: state-aware delegation preserves rc and dev-off gating.
  DELEGATE_OUT="$(bash -c 'DEV_ON_OK=1; INT_RECEIVED=0; dev_cleanup() { local rc=$?; if [[ "${1:-}" != "" ]]; then rc="$1"; fi; printf "dev_cleanup rc=%s int=%s\n" "$rc" "$INT_RECEIVED"; }; native_unload_owned_reverse() { printf "early-only\n"; }; native_early_cleanup() { local rc=$?; if [[ "${DEV_ON_OK:-0}" -eq 1 ]]; then dev_cleanup "$rc"; exit "$rc"; fi; trap - EXIT INT TERM; native_unload_owned_reverse || true; exit "$rc"; }; (exit 42); native_early_cleanup' 2>&1 || true)"
  if printf '%s\n' "$DELEGATE_OUT" | grep -Fq 'dev_cleanup rc=42'; then PASS=$((PASS + 1)); else echo "FAIL [delegation preserves rc]" >&2; printf '%s\n' "$DELEGATE_OUT" >&2; FAIL=$((FAIL + 1)); fi
  EARLY_ONLY_OUT="$(bash -c 'DEV_ON_OK=0; dev_cleanup() { printf "dev_cleanup\n"; }; native_unload_owned_reverse() { printf "early-only\n"; }; native_early_cleanup() { local rc=$?; if [[ "${DEV_ON_OK:-0}" -eq 1 ]]; then dev_cleanup "$rc"; exit "$rc"; fi; trap - EXIT INT TERM; native_unload_owned_reverse || true; exit "$rc"; }; (exit 7); native_early_cleanup' 2>&1 || true)"
  if printf '%s\n' "$EARLY_ONLY_OUT" | grep -Fq 'early-only' && ! printf '%s\n' "$EARLY_ONLY_OUT" | grep -Fq 'dev_cleanup'; then PASS=$((PASS + 1)); else echo "FAIL [pre-success stays native-only]" >&2; printf '%s\n' "$EARLY_ONLY_OUT" >&2; FAIL=$((FAIL + 1)); fi
}

# ---- Part 3: just dev integration (isolated justfile) ----

part3_integration() {
  local jwork="$WORK/j"
  mkdir -p "$jwork"
  local isolated="$jwork/justfile.isolated"
  REPO_ROOT_SRC="$REPO_ROOT" ISOLATED_DST="$isolated" python3 - <<'PYEOF'
import pathlib, os
repo = os.environ.get("REPO_ROOT_SRC", "")
dst = os.environ.get("ISOLATED_DST", "")
def isolate(text):
  text = text.replace('REPO_ROOT="{{ justfile_directory() }}"', f'REPO_ROOT="{repo}"')
  text = text.replace('KWIN_DIR="$REPO_ROOT/kwin"', 'KWIN_DIR="${OMNITILER_KWIN_DIR:-$REPO_ROOT/kwin}"')
  text = text.replace('SOURCE_DIR="$REPO_ROOT/kwin/native-effect"', 'SOURCE_DIR="${OMNITILER_NATIVE_SOURCE:-$REPO_ROOT/kwin/native-effect}"')
  text = text.replace('BUILD_DIR="$REPO_ROOT/target/kwin-native-effect-build"', 'BUILD_DIR="${OMNITILER_NATIVE_BUILD:-$REPO_ROOT/target/kwin-native-effect-build}"')
  text = text.replace('STAGE="$REPO_ROOT/target/kwin-native-effect-stage"', 'STAGE="${OMNITILER_NATIVE_STAGE:-$REPO_ROOT/target/kwin-native-effect-stage}"')
  text = text.replace('TARGET_DIR="$REPO_ROOT/target"', 'TARGET_DIR="${OMNITILER_TARGET_DIR:-$REPO_ROOT/target}"')
  text = text.replace('/proc/', '$PROC_ROOT/')
  text = text.replace(
    'BIN="$REPO_ROOT/target/debug/omnitiler"',
    'BIN="${OMNITILER_BIN:-$REPO_ROOT/target/debug/omnitiler}"\n    PROC_ROOT="${PROC_ROOT:-/proc}"\n    START_TEST_BIN="${DEV_LOOP_START_TEST:-$REPO_ROOT/scripts/start-test.sh}"\n    DOGFOOD_BIN="${DEV_LOOP_DOGFOOD:-$REPO_ROOT/scripts/dogfood-install.sh}"',
  )
  text = text.replace('bash "$REPO_ROOT/scripts/start-test.sh"', 'bash "$START_TEST_BIN"')
  text = text.replace('bash "$REPO_ROOT/scripts/dogfood-install.sh"', 'bash "$DOGFOOD_BIN"')
  text = text.replace('/tmp/omnitiler-planner-dev.XXXXXX.log', '$RUNTIME_DIR/omnitiler-planner-dev.XXXXXX.log')
  text = text.replace(
    'for _ in $(seq 1 50); do [[ -d "$PROC_ROOT/$RECORDED_PID" ]] || break; sleep 0.2; done',
    'for _ in $(seq 1 50); do kill -0 "$RECORDED_PID" 2>/dev/null || break; sleep 0.2; done',
  )
  text = text.replace(
    'if [[ -d "$PROC_ROOT/$RECORDED_PID" ]]; then',
    'if kill -0 "$RECORDED_PID" 2>/dev/null; then',
  )
  text = text.replace(
    'for _ in $(seq 1 50); do [[ -d "$PROC_ROOT/$OLD_PID" ]] || break; sleep 0.2; done',
    'for _ in $(seq 1 50); do kill -0 "$OLD_PID" 2>/dev/null || break; sleep 0.2; done',
  )
  text = text.replace(
    'if [[ -d "$PROC_ROOT/$OLD_PID" ]]; then',
    'if kill -0 "$OLD_PID" 2>/dev/null; then',
  )
  return text
src = pathlib.Path(repo) / "justfile"
pathlib.Path(dst).write_text(isolate(src.read_text()))
# The root justfile only dispatches per OS; stage the Linux module next to the
# isolated justfile so `[linux] import "just/linux.just"` resolves, with the
# same test-path substitutions applied.
linux_dst = pathlib.Path(dst).parent / "just" / "linux.just"
linux_dst.parent.mkdir(parents=True, exist_ok=True)
linux_dst.write_text(isolate((pathlib.Path(repo) / "just" / "linux.just").read_text()))
PYEOF
  # Fake tools for just integration.
  local fbin="$jwork/fakebin"
  mkdir -p "$fbin" "$jwork/state" "$jwork/proc" "$jwork/runtime" "$jwork/fake-kwin/contents/code" "$jwork/fakebin-dir"
  cat > "$fbin/busctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state="${FAKE_STATE_DIR:?}"
case "$*" in
  *"GetNameOwner s com.omnitiler.Planner"*)
    if [[ -f "$state/planner-owned" ]]; then printf 's ":1.50"\n'; exit 0; fi
    if grep -Fq "setsid " "${FAKE_CALL_LOG:?}" 2>/dev/null; then printf 's ":1.50"\n'; exit 0; else exit 1; fi ;;
  *"GetConnectionUnixProcessID s :1.50"*)
    pid="$(cat "$state/owner-pid" 2>/dev/null || printf '4242')"
    printf '{"type":"u","data":[%s]}\n' "$pid" ;;
  *"GetNameOwner s com.omnitiler.Tray"*)
    # Native-only integration exercises pre-owned tray preservation; the
    # foreground tray launch/teardown belongs to dev-loop-split.test.sh.
    printf 's ":1.60"\n' ;;
  *"GetNameOwner s org.kde.KWin"*)
    if [[ -f "$state/kwin-unowned" ]]; then exit 1; fi
    owner="$(cat "$state/kwin-owner" 2>/dev/null || printf ':1.99')"
    printf '{"type":"s","data":["%s"]}\n' "$owner"; exit 0 ;;
  *"GetConnectionUnixProcessID s :"*)
    printf '{"type":"u","data":[5151]}\n' ;;
  *"isEffectSupported"*)
    if [[ -f "$state/supported-fail" ]]; then exit 1; fi
    if [[ "$*" == *"omnitiler-active-border"* ]]; then printf '{"type":"b","data":[%s]}\n' "$(cat "$state/border-supported" 2>/dev/null || printf 'true')"
    else exit 1; fi ;;
  *"isEffectLoaded"*)
    if [[ "$*" == *"omnitiler-active-border"* ]]; then printf '{"type":"b","data":[%s]}\n' "$(cat "$state/border-loaded" 2>/dev/null || printf 'false')"
    else exit 1; fi ;;
  *"unloadEffect"*)
    printf 'unloadEffect %s\n' "$*" >> "${FAKE_CALL_LOG:?}"
    if [[ -f "$state/unload-fail" ]]; then exit 1; fi
    printf 'false\n' > "$state/border-loaded"
    printf '{"type":"b","data":[true]}\n'; exit 0 ;;
  *"loadEffect"*)
    printf 'loadEffect %s\n' "$*" >> "${FAKE_CALL_LOG:?}"
    if [[ -f "$state/load-fail" ]]; then exit 1; fi
    printf 'true\n' > "$state/border-loaded"
    printf '{"type":"b","data":[true]}\n'; exit 0 ;;
  *"isScriptLoaded"*)
    printf '{"type":"b","data":[%s]}\n' "$(cat "$state/loaded" 2>/dev/null || printf 'false')" ;;
  *"status com.omnitiler.Planner"*) exit 0 ;;
  *) exit 1 ;;
esac
EOF
  # NOTE: there is intentionally no $fbin/cargo fake. The build-native-effect
  # recipe runs the host builder with CARGO_BIN unset, so the builder resolves
  # cargo via PATH and requires it under the (fake) store root. The logging
  # cargo fake therefore lives at $jwork/fake-store/hash-cargo/bin/cargo below
  # and jrun puts that dir first on PATH.
  cat > "$fbin/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'npm build\n' >> "${FAKE_CALL_LOG:?}"
mkdir -p "${OMNITILER_KWIN_DIR:-}/contents/code"
printf '// fake\n' > "${OMNITILER_KWIN_DIR:-}/contents/code/main.js"
exit 0
EOF
  cat > "$fbin/cmake" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'cmake build\n' >> "${FAKE_CALL_LOG:?}"
build_dir="${OMNITILER_NATIVE_BUILD:-}"
if [[ -n "$build_dir" ]]; then
  mkdir -p "$build_dir/bin/kwin/effects/plugins" "$build_dir/bin/kwin/effects/configs" "$build_dir/bin/kwin/scripts/configs"
  printf 'x' > "$build_dir/bin/kwin/effects/plugins/omnitiler-active-border.so"
  printf 'x' > "$build_dir/bin/kwin/effects/configs/omnitiler-active-border_config.so"
  printf 'x' > "$build_dir/bin/kwin/scripts/configs/omnitiler-kwin_config.so"
fi
exit 0
EOF
  cat > "$fbin/devenv" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
cargo build
EOF
  cat > "$fbin/setsid" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'setsid launch\n' >> "${FAKE_CALL_LOG:?}"
exit 0
EOF
  cat > "$fbin/systemctl" <<'EOF'
#!/usr/bin/env bash
printf 'unknown\n'
exit 0
EOF
  cat > "$fbin/tail" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'tail call\n' >> "${FAKE_CALL_LOG:?}"
for a in "$@"; do if [[ -f "$a" ]]; then echo "fake-planner-line"; break; fi; done
exit 0
EOF
  cat > "$fbin/journalctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'journalctl call\n' >> "${FAKE_CALL_LOG:?}"
echo "omnitiler:plan:cmd=x kind=admit windows=1 outcome=planned-applied"
exit 0
EOF
  chmod +x "$fbin/"*
  cat > "$fbin/nix" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'nix %s\n' "$*" >> "${FAKE_CALL_LOG:?}"
state="${FAKE_STATE_DIR:?}"
if [[ "${1:-}" == "path-info" ]]; then
  printf '%s\n' "${FAKE_DRV:?}"
  exit 0
fi
if [[ "${1:-}" == "derivation" ]]; then
  printf '{"%s":{"outputs":{"out":{"path":"%s"},"dev":{"path":"%s"}}}}\n' "${FAKE_DRV:?}" "${FAKE_STORE_PATH:?}" "${FAKE_DEV_OUT:?}"
  exit 0
fi
if [[ "${1:-}" == "build" ]]; then
  [[ "${2:-}" == "${FAKE_DRV:?}^dev" ]] || { echo "fake nix: expected exact dev output" >&2; exit 2; }
  mkdir -p "${FAKE_DEV_OUT:?}/lib/cmake/KWin"
  printf '# realized KWinConfig\n' > "${FAKE_DEV_OUT:?}/lib/cmake/KWin/KWinConfig.cmake"
  exit 0
fi
if [[ "${1:-}" == "develop" ]]; then
  shift
  shift || true
  shift
  exec "$@"
fi
exit 2
EOF
  chmod +x "$fbin/nix"
  mkdir -p "$jwork/fake-store/hash-rustc/bin" "$jwork/fake-store/hash-cargo/bin" "$jwork/fake-store/hash-cmake/bin"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$jwork/fake-store/hash-rustc/bin/rustc"
  # Logging cargo fake under the store root (see the $fbin/cargo note above):
  # build-rust (via the fake devenv) and the host builder's PATH lookup both
  # resolve this path, satisfying the builder's explicit /nix/store check.
  cat > "$jwork/fake-store/hash-cargo/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo build\n' >> "${FAKE_CALL_LOG:?}"
bin="${OMNITILER_BIN:?}"
mkdir -p "${bin%/*}"
[[ -x "$bin" ]] || { printf '#!/usr/bin/env bash\nexit 0\n' > "$bin"; chmod +x "$bin"; }
exit 0
EOF
  chmod +x "$jwork/fake-store/hash-rustc/bin/rustc" "$jwork/fake-store/hash-cargo/bin/cargo"
  cp "$fbin/cmake" "$jwork/fake-store/hash-cmake/bin/cmake"
  chmod +x "$jwork/fake-store/hash-cmake/bin/cmake"
  mkdir -p "$jwork/fake-store/hash-kwin-6.7.5/bin" "$jwork/fake-store/hash-kwin-dev-6.7.5/lib/cmake/KWin" "$jwork/fake-host"
  printf 'fake-kwin\n' > "$jwork/fake-store/hash-kwin-6.7.5/bin/kwin_wayland"
  printf '# fake KWinConfig\n' > "$jwork/fake-store/hash-kwin-dev-6.7.5/lib/cmake/KWin/KWinConfig.cmake"
  ln -sf "$jwork/fake-store/hash-kwin-6.7.5/bin/kwin_wayland" "$jwork/fake-host/kwin_wayland"
  export FAKE_DRV="$jwork/fake-store/abc123-kwin-6.7.5.drv"
  export FAKE_STORE_PATH="$jwork/fake-store/hash-kwin-6.7.5/bin/kwin_wayland"
  export FAKE_DEV_OUT="$jwork/fake-store/hash-kwin-dev-6.7.5"
  cat > "$jwork/fake-start.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state="${FAKE_STATE_DIR:?}"
if [[ "${1:-}" == "start" ]]; then
  printf 'start-test start\n' >> "${FAKE_CALL_LOG:?}"
  receipt="${CONTROLLER_OWNERSHIP_FILE:?}"
  printf '{"script_id":7,"pid":4242,"start_identity":"101010"}\n' > "$receipt"
  printf 'true\n' > "$state/loaded"
  exit 0
fi
if [[ "${1:-}" == "stop" ]]; then
  printf 'start-test stop %s\n' "${2:-}" >> "${FAKE_CALL_LOG:?}"
  if [[ -f "$state/stop-fails" ]]; then exit 1; fi
  printf 'false\n' > "$state/loaded"
  rm -f -- "${CONTROLLER_OWNERSHIP_FILE:?}"
  exit 0
fi
exit 1
EOF
  cat > "$jwork/fake-dogfood.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'dogfood %s\n' "$*" >> "${FAKE_CALL_LOG:?}"
exit 0
EOF
  chmod +x "$jwork/fake-start.sh" "$jwork/fake-dogfood.sh"

  jreset() {
    rm -rf "$jwork/state" "$jwork/proc" "$jwork/runtime"
    mkdir -p "$jwork/state" "$jwork/proc" "$jwork/runtime" "$jwork/fake-kwin/contents/code"
    : > "$jwork/calls.log"
    : > "$OUTPUT"
    printf 'false\n' > "$jwork/state/loaded"
    printf 'true\n' > "$jwork/state/border-supported"
    printf 'false\n' > "$jwork/state/border-loaded"
    printf '// fake\n' > "$jwork/fake-kwin/contents/code/main.js"
    mkdir -p "$jwork/proc/5151"
    printf '5151 (kwin_wayland) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 777888\n' > "$jwork/proc/5151/stat"
    export FAKE_STATE_DIR="$jwork/state"
    export FAKE_CALL_LOG="$jwork/calls.log"
    export PROC_ROOT="$jwork/proc"
    export OMNITILER_BIN="$jwork/fakebin-dir/omnitiler"
    export OMNITILER_KWIN_DIR="$jwork/fake-kwin"
    export OMNITILER_NATIVE_BUILD="$jwork/fake-native-build"
    export OMNITILER_NATIVE_STAGE="$jwork/fake-native-stage"
    export OMNITILER_TARGET_DIR="$jwork/fake-target"
    export OMNITILER_KWIN_DEV_CMAKE_DIR="$jwork/fake-kwin-cmake"
    export OMNITILER_HOST_KWIN_BIN="$jwork/fake-host/kwin_wayland"
    export OMNITILER_STORE_ROOT="$jwork/fake-store"
    export NIX_BIN="$fbin/nix"
    export RUSTC_BIN="$jwork/fake-store/hash-rustc/bin/rustc"
    export CARGO_BIN="$jwork/fake-store/hash-cargo/bin/cargo"
    export CMAKE_BIN="$jwork/fake-store/hash-cmake/bin/cmake"
    export XDG_RUNTIME_DIR="$jwork/runtime"
    export XDG_CURRENT_DESKTOP=KDE
    export DEV_LOOP_START_TEST="$jwork/fake-start.sh"
    export DEV_LOOP_DOGFOOD="$jwork/fake-dogfood.sh"
    export PATH="$fbin:$PATH"
    mkdir -p "$jwork/fakebin-dir" "$jwork/fake-native-build" "$jwork/fake-target" "$jwork/fake-kwin-cmake"
    printf '#!/usr/bin/env bash\nexit 0\n' > "$OMNITILER_BIN"
    chmod +x "$OMNITILER_BIN"
    sleep 300 &
    JPLANNER=$!
    mkdir -p "$jwork/proc/$JPLANNER"
    printf '%s (fake-planner) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 888111\n' "$JPLANNER" > "$jwork/proc/$JPLANNER/stat"
    ln -sfn -- "$OMNITILER_BIN" "$jwork/proc/$JPLANNER/exe"
    printf 'fake\0planner-service\0' > "$jwork/proc/$JPLANNER/cmdline"
    printf '%s\n' "$JPLANNER" > "$jwork/state/owner-pid"
  }
  jrun() {
    set +e
    FAKE_STATE_DIR="$jwork/state" FAKE_CALL_LOG="$jwork/calls.log" PROC_ROOT="$jwork/proc" OMNITILER_BIN="$OMNITILER_BIN" OMNITILER_KWIN_DIR="$jwork/fake-kwin" OMNITILER_NATIVE_BUILD="$jwork/fake-native-build" OMNITILER_NATIVE_STAGE="$jwork/fake-native-stage" OMNITILER_TARGET_DIR="$jwork/fake-target" OMNITILER_KWIN_DEV_CMAKE_DIR="$jwork/fake-kwin-cmake" OMNITILER_HOST_KWIN_BIN="$jwork/fake-host/kwin_wayland" OMNITILER_STORE_ROOT="$jwork/fake-store" NIX_BIN="$fbin/nix" RUSTC_BIN="$jwork/fake-store/hash-rustc/bin/rustc" CARGO_BIN="$jwork/fake-store/hash-cargo/bin/cargo" CMAKE_BIN="$jwork/fake-store/hash-cmake/bin/cmake" FAKE_DRV="$FAKE_DRV" FAKE_STORE_PATH="$FAKE_STORE_PATH" FAKE_DEV_OUT="$FAKE_DEV_OUT" XDG_RUNTIME_DIR="$jwork/runtime" XDG_CURRENT_DESKTOP="${XDG_CURRENT_DESKTOP-KDE}" DEV_LOOP_START_TEST="$jwork/fake-start.sh" DEV_LOOP_DOGFOOD="$jwork/fake-dogfood.sh" PATH="$jwork/fake-store/hash-cargo/bin:$fbin:$PATH" just --justfile "$isolated" "$@" >"$OUTPUT" 2>&1
    EXIT=$?
    set -e
  }
  jstop_planner() {
    kill "$JPLANNER" 2>/dev/null || true
    wait "$JPLANNER" 2>/dev/null || true
  }

  # Owned: none preloaded -> single survivor loaded owned, then unloaded, full build.
  jreset
  jrun dev
  jexit="$EXIT"
  jstop_planner
  EXIT="$jexit"
  check_exit 0 "just dev owned cycle"
  assert_contains "transient" "just owned transient"
  if grep -Fq "loadEffect" "$jwork/calls.log" && grep -Fq "unloadEffect" "$jwork/calls.log"; then PASS=$((PASS + 1)); else echo "FAIL [just owned load/unload]" >&2; cat "$jwork/calls.log" >&2; FAIL=$((FAIL + 1)); fi
  # Single survivor: exactly one owned load of the border plus its teardown
  # unload, and no oracle traffic.
  if [[ "$(grep -c "^loadEffect" "$jwork/calls.log")" -eq 1 ]] && grep -Eq "^loadEffect.*active-border" "$jwork/calls.log"; then PASS=$((PASS + 1)); else echo "FAIL [just owned single load]" >&2; cat "$jwork/calls.log" >&2; FAIL=$((FAIL + 1)); fi
  if [[ "$(grep -c "^unloadEffect" "$jwork/calls.log")" -eq 1 ]] && grep -Eq "^unloadEffect.*active-border" "$jwork/calls.log"; then PASS=$((PASS + 1)); else echo "FAIL [just owned single unload]" >&2; cat "$jwork/calls.log" >&2; FAIL=$((FAIL + 1)); fi
  if grep -Fq "drag-oracle" "$jwork/calls.log"; then echo "FAIL [just owned no oracle traffic]" >&2; cat "$jwork/calls.log" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "cmake build" "$jwork/calls.log"; then PASS=$((PASS + 1)); else echo "FAIL [just owned full build]" >&2; FAIL=$((FAIL + 1)); fi
  if grep -Fqi "kwriteconfig" "$jwork/calls.log"; then echo "FAIL [just no persistent enable writes]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  assert_contains "never hot-reloads" "just no hot reload"
  assert_contains "does not prove the library is unmapped" "just no unmapped claim"

  # Preloaded border: skip native rebuild, preserve border, own nothing.
  jreset
  printf 'true\n' > "$jwork/state/border-loaded"
  jrun dev
  jexit="$EXIT"
  jstop_planner
  EXIT="$jexit"
  check_exit 0 "just dev preloaded cycle"
  assert_contains "preserving preloaded effect omnitiler-active-border" "preloaded preserved msg"
  assert_contains "skipping native rebuild" "preloaded skip rebuild"
  if grep -Fq "cmake build" "$jwork/calls.log"; then echo "FAIL [preloaded no native rebuild]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Eq "unloadEffect.*active-border" "$jwork/calls.log"; then echo "FAIL [preloaded border never unloaded]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "loadEffect" "$jwork/calls.log"; then echo "FAIL [preloaded nothing owned]" >&2; cat "$jwork/calls.log" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi

  # Unsupported: actionable, no startup mutation.
  jreset
  printf 'false\n' > "$jwork/state/border-supported"
  jrun dev
  jexit="$EXIT"
  jstop_planner
  EXIT="$jexit"
  if [[ "$EXIT" -eq 0 ]]; then echo "FAIL [unsupported must fail]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  assert_contains "just dev-native-setup" "unsupported actionable setup"
  assert_contains "log out and log back in" "unsupported logout"
  assert_contains "factory, or ABI failure" "unsupported not only discovery"
  if grep -Fq "start-test start" "$jwork/calls.log"; then echo "FAIL [unsupported no start]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "setsid" "$jwork/calls.log"; then echo "FAIL [unsupported no launch]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "loadEffect" "$jwork/calls.log"; then echo "FAIL [unsupported no load]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi

  # Partial failure: border load fails -> no dev-on, nothing to unwind.
  jreset
  touch "$jwork/state/load-fail"
  jrun dev
  jexit="$EXIT"
  jstop_planner
  EXIT="$jexit"
  if [[ "$EXIT" -eq 0 ]]; then echo "FAIL [partial must fail]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "start-test start" "$jwork/calls.log"; then echo "FAIL [partial no start]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  assert_contains "native load failed" "partial load failed msg"

  # Absent (KWin unowned): actionable, no mutation.
  jreset
  touch "$jwork/state/kwin-unowned"
  jrun dev
  jexit="$EXIT"
  jstop_planner
  EXIT="$jexit"
  if [[ "$EXIT" -eq 0 ]]; then echo "FAIL [absent must fail]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
  if grep -Fq "start-test start" "$jwork/calls.log"; then echo "FAIL [absent no start]" >&2; FAIL=$((FAIL + 1)); else PASS=$((PASS + 1)); fi
}

part1_setup
part1_quoting
part1_canonical_and_collision
part1_relocation
part2_tests
part3_integration

echo "PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]]

# macOS recipes (CLT only, mise toolchain). Imported by the root justfile.
# Build/check/bundle entry points; only `launch`/`preflight-app` execute the
# signed bundle (bounded, via LaunchServices); `tile` is also a live entry point.
# Other recipes never launch
# live probes.

# Pass recipe arguments through as positional words: `"$@"` below carries
# each argument exactly (spaces/specials intact). `{{args}}` interpolation
# would re-split them lossily through the shell.
set positional-arguments := true

# Nonmutating preflight (default check; never launches UI/hooks).
preflight:
    mise exec -- cargo run --locked -p tiler-macos -- preflight

# TCC-trusted LaunchServices launch (required for every AX/TCC-dependent
# probe). A directly executed binary (`cargo run`, or the executable inside
# the .app) runs under the terminal's attribution and falsely reports
# ax_trusted:false; this route carries the signed bundle grant instead.
# `open -W` cannot block on this bundle, so the script polls ephemeral
# output under a bounded --timeout (never a block on the application).
# Exit-status limit: the wrapper cannot observe the probe's real exit code;
# read the JSON payload as the outcome, never the recipe exit code alone.
launch *args:
    bash scripts/macos-launch.sh "$@"

# Bounded scoped-tile dev session: explicit --user-start plus a non-empty
# --scope stay mandatory (no implicit scope/start added here); the wrapper
# timeout always covers the tile's own bounded --seconds (default 30).
# Example: just tile --user-start --scope com.apple.TextEdit --scope com.apple.calculator --input tap --seconds 120
tile *args:
    bash scripts/macos-tile.sh "$@"

# Bundle-attributed preflight (the grant under test); the plain `preflight`
# above stays nonmutating under terminal attribution (false expected there).
preflight-app:
    bash scripts/macos-launch.sh -- preflight

check:
    mise exec -- cargo run --locked -p tiler-macos -- preflight

help:
    mise exec -- cargo run --locked -p tiler-macos -- help

build:
    mise exec -- cargo build --locked -p tiler-macos

build-release:
    mise exec -- cargo build --locked --release -p tiler-macos

test:
    mise exec -- cargo test --locked -p tiler-macos

clippy:
    mise exec -- cargo clippy --locked -p tiler-macos --all-targets -- -D warnings

fmt:
    mise exec -- cargo fmt --all -- --check

# Stable signed .app: fixed bundle ID and fixed output path, persistent named
# identity only. Fails closed without the identity (no ad-hoc fallback).
app identity:
    @if [ -z "{{identity}}" ]; then echo "error: just app 'NAME' (persistent dev identity required)" >&2; exit 2; fi
    bash scripts/macos-app.sh --identity "{{identity}}"

verify:
    bash scripts/macos-app.sh --verify

# Stable signed fixture .app: fixed bundle ID
# (io.github.omnitiler.live-fixture) and fixed output path
# (target/macos/OmniTilerLiveFixture.app), persistent named identity only.
# Fails closed without the identity (no ad-hoc fallback). Artifact path is
# resolved from cargo JSON (honors CARGO_TARGET_DIR; never assumes target/).
fixture-app identity:
    @if [ -z "{{identity}}" ]; then echo "error: just fixture-app 'NAME' (persistent dev identity required)" >&2; exit 2; fi
    bash scripts/macos-fixture-app.sh --identity "{{identity}}"

fixture-verify:
    bash scripts/macos-fixture-app.sh --verify

# Foreground dogfood session: unscoped ordinary-app tiling by default
# (Ghostty is an ordinary candidate there; scoped `--scope` verification
# stays available for bounded checks).
# Unified dev entry: builds the current signed bundle first via the
# existing persistent-identity route (`scripts/macos-app.sh`), then
# launches it. The build must complete before launch (never a stale binary
# silently); `--stop` never rebuilds. Identity is the fixed configured
# default `OmniTiler Mac Dev` unless `OMNITILER_DEV_IDENTITY` overrides it.
# No installs, no TCC resets.
# Examples:
#   just dev
#   just dev verbose --exclude com.example.Skip
#   just dev trace --scope com.apple.TextEdit --seconds 30
dev *args:
    #!/usr/bin/env bash
    set -euo pipefail
    for a in "$@"; do
      if [ "$a" = "--stop" ] || [ "$a" = "stop" ]; then
        exec bash scripts/macos-dev.sh "$@"
      fi
    done
    : "${OMNITILER_DEV_IDENTITY:=OmniTiler Mac Dev}"
    bash scripts/macos-app.sh --identity "$OMNITILER_DEV_IDENTITY"
    exec bash scripts/macos-dev.sh "$@"

# Exact-owner stop from a second terminal (never broad kills).
dev-stop:
    bash scripts/macos-dev.sh --stop

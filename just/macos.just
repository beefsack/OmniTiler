# macOS recipes (CLT only, mise toolchain). Imported by the root justfile.
# Build/check/bundle entry points; only `launch`/`preflight-app` execute the
# signed bundle (bounded, via LaunchServices), everything else never launches
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

# Unsupported dev loop: fails closed on macOS Phase 0.
dev mode="":
    @echo "error: just dev: macOS full-solution loop is not implemented (use: just preflight)" >&2 && exit 1

# macOS recipes (CLT only, mise toolchain). Imported by the root justfile.
# Build/check/bundle entry points; these recipes do not launch live probes.

# Nonmutating preflight (default check; never launches UI/hooks).
preflight:
    mise exec -- cargo run --locked -p tiler-macos -- preflight

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

# OS-dispatched entry point. Platform recipes live under just/ and only the
# current OS import is loaded. Run `just <recipe>` from the repo root.

set minimum-version := "1.56.0"

[linux]
set shell := ["bash", "-euo", "pipefail", "-c"]

[windows]
set shell := ["pwsh", "-NoProfile", "-Command"]

[macos]
set shell := ["bash", "-euo", "pipefail", "-c"]

[linux]
import "just/linux.just"

[windows]
import "just/windows.just"

[macos]
import "just/macos.just"

default:
    @just --list

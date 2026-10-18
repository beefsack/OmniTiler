# AGENTS.md

## Functional Specification

- Resolve behavioral ambiguities with a minimal discriminating action-sequence row in [docs/spec/reference-outcomes.md](docs/spec/reference-outcomes.md); leave unsupported outcomes TBD for later user testing.

## Reference Sources

- Reference WM and KWin sources are cloned as sibling directories of this
  repository. Search them locally at the pin cited by the docs with
  `git show <pin>:<path>`, `git grep <pattern> <pin>`, or a worktree instead
  of web-fetching source files. Web links remain fine as citations in records.
- Add a missing source clone as a sibling directory with full history.

## Live KWin/Plasma Testing

- Before any live KWin/Plasma testing, read and follow
  [docs/live-kwin-testing.md](docs/live-kwin-testing.md). It does not grant
  mutation authorization.

## Live Windows Testing

- Before any live Windows testing (input hooks, moving/hiding/restyling other
  windows, desktop overlays, forced-crash recovery probes, or registry/policy
  writes), read and follow [docs/live-windows-testing.md](docs/live-windows-testing.md).
  It does not grant mutation authorization.

## Live macOS Testing

- Before any live macOS testing (AX control, event taps or hotkey
  interception, moving/hiding/restyling other windows, desktop overlays,
  native-Space moves or focus changes, TCC consent and reset flows, or
  forced-crash recovery probes), read and follow
  [docs/live-macos-testing.md](docs/live-macos-testing.md).
  It does not grant mutation authorization.

## Local-System Information

- Never commit local paths or information about local machines (paths, host names, hardware/resource details); describe test conditions generically.

## Repository Records

- Never record session controls in repository files or commit messages: commit/push permissions, autonomous or supervised modes, agent/model roles and routing, concurrency and approval gates, user availability, or machine-specific resource limits. Record only the work, its evidence, and product decisions.

## Dependency Management

- Linux: system and toolchain dependencies for this project are managed by
  `devenv.nix` (devenv + Nix). Do not install dependencies globally or
  ad hoc.
- When a new Linux system dependency is required, `devenv.nix` must be
  updated to add it.
- After `devenv.nix` is changed, advise the user to restart the session
  so the new dependencies are loaded into the environment. Do not assume
  the dependency is available until that has happened.
- Windows: native dependencies, their install commands and Rust policy
  (pre-1.0 track latest stable, revisit at 1.0) are listed in
  [docs/windows-dev-environment.md](docs/windows-dev-environment.md). Use
  only listed tools. A new Windows dependency is added to that list first,
  with user approval. The user performs installs; ask before installing
  anything. Never add Windows tools to `devenv.nix`.
- macOS: native dependencies and install routes are listed in
  [docs/macos-dev-environment.md](docs/macos-dev-environment.md), same
  user-owns-installs rule as Windows. Never add macOS tools to `devenv.nix`.
- Windows/macOS CLI and Rust tooling is declared in root `mise.toml` with
  OS-filtered entries (run from root: `mise trust`, `mise install`,
  `mise exec -- <cmd>`); manual OS prerequisites (Git, MSVC Build Tools +
  SDK, Xcode/CLT, host shell bootstrap) stay manual per the OS docs.
  Linux ignores `mise.toml` and stays on the `devenv.nix` route.
- Rust crate dependencies belong in `Cargo.toml`. `devenv.nix` and the
  Windows list are for system-level and toolchain dependencies only.

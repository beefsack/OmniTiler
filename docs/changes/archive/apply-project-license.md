# Apply project license

## Goal and scope

- Apply the 2026-10-11 decision: MIT, copyright Michael Alexander, except
  the native KWin effect plugin sources, GPL-2.0-or-later.
- Rust staticlib, shared FFI declarations, KWin script, Settings KCMs,
  tray, scripts, packaging and documentation remain MIT.
- No runtime changes or live testing. Dependency audit tooling and a CI
  license gate remain follow-ups, as does the full native closure per distro.

## Acceptance and approach

- Add complete license texts and explicit SPDX copyright/license headers
  to every GPL source. Rely on root LICENSE for MIT first-party sources.
- Align Cargo/npm, KPlugin, RPM/Arch/Debian and Nix metadata with the boundary;
  carry license notices in distributable package payloads.
- Keep deny.toml aligned with the researched permissive dependency inventory.
- Correct the research: MIT source may link into a GPL-covered binary;
  GPL effect source is a deliberate choice, not a linking requirement.
- Update durable decisions and outstanding audit work.

## Bounded work and verification

1. Apply license texts, source annotations and package metadata.
2. Review the build-input boundary and notice delivery; correct concrete gaps.
3. Verify Rust metadata/check/tests, isolated native CMake/build/CTest,
   KWin typecheck/bundle/metadata, Nix flake checks, Arch source metadata
   consistency and packaging shell suites.
4. Record accepted evidence and archive this note.

## Outcome and accepted evidence

- Git authorship identifies Michael Alexander; all five workspace crates now
  declare MIT. Initial JSON, shell and Nix syntax checks passed.
- GPL source list: activewindowborder.cpp, activewindowborder.h,
  activeborderlogic.h and oraclepress.h under kwin/native-effect;
  effect metadata.json also declares GPL-2.0-or-later.
- Source/build-input review confirms neither MIT KCM includes GPL effect
  headers. Shared FFI declarations and generated configuration remain MIT.
- Root LICENSE supplies MIT coverage; four GPL sources have SPDX
  license/copyright headers. Full texts are under LICENSES/. Cargo/npm,
  KPlugin, RPM/Arch/Debian and Nix metadata match the approved boundary.
- Package review corrected missing installed notices in RPM, Arch and Nix
  outputs and added LICENSE to the standalone five-file KPackage payload.
  Nix notice installation initially failed on relative source paths from
  CMake's build directory; using the absolute derivation source restored
  the check without changing product behavior. All affected builds passed
  after the correction.
- Rust: locked metadata confirms five MIT workspace crates;
  `cargo check --workspace --locked` and `cargo test --workspace --locked`
  passed with isolated target output.
- KWin: typecheck and bundle passed. Real isolated kpackagetool6 validation
  passed; archive has exactly five files, deterministic timestamps and a
  byte-identical MIT notice.
- Native: CMake configure/build and offscreen, isolated-config CTest ran in
  Nix sandboxes because standalone CMake was unavailable. Effect suite
  passed 33/33; settings-only suite passed 26/26.
- `nix flake check` passed for the native platform; script and tray package
  builds also passed. All four Nix package notices match their source texts.
- `makepkg --printsrcinfo` ran in a disposable offline Arch container;
  .SRCINFO was regenerated to match canonical field order byte-for-byte.
- Packaging shell suites passed: build-kpackage, build-source-archive,
  dogfood-install, dev-loop-split, dev-native-effect and nix-host-kwin-build.
  JSON metadata, shell syntax, deny.toml TOML syntax and diff whitespace
  checks passed. Cargo.lock and dependency versions are unchanged.
- README, installation guide, licensing research, durable decisions and
  the Licensing backlog item now describe the applied choice and follow-ups.

## Remaining release follow-ups

- Reload the declared audit-tool environment, run cargo-deny and the npm
  license inventory, prepare attribution as needed, then add a CI license gate.
- Complete native linked-library/closure and vendored attribution checks per
  distro. These remain audit work, not pending first-party license selection.

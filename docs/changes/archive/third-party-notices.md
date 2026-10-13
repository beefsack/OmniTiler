# Third-Party Notices

## Goal And Status

- Completed implementation for the pre-0.1 KDE delivery required by the
  [Licensing decision](../../decisions.md#licensing): tool-generated dependency
  notices, including full license texts, in GitHub Release binaries and
  Debian/RPM/Arch packages.
- Phase 1 investigation is complete. Phase 2 implements the accepted design:
  option B, one committed generated union file, manual `just` regeneration,
  and a CI freshness linter that never updates the committed file.
- Generation and installation wiring are implemented; representative
  verification passes. Broader verification and future binary delivery remain
  listed below and in the Licensing backlog.
- Keep the cargo-deny `licenses`, `bans` and `sources` CI gate. Notices complement
  that gate; a vendor tree containing crate licenses is not binary delivery.
- Windows binaries are outside KDE 0.1 scope. First-party licensing and the
  effect's GPL source-delivery obligations remain separate from this work.

## Shipped-Code Inventory

| Artifact | Compiled Rust coverage | Evidence |
| --- | --- | --- |
| `omnitiler` planner/tray executable in the core package | Root `omnitiler`; `tiler-protocol`/`tiler-core`, serde/serde_json, zbus, rustix, async-io/async-lock and their selected transitive dependencies | `crates/omnitiler/Cargo.toml`; all three recipes build `-p omnitiler --release --locked --offline` |
| `omnitiler-active-border.so` in `omnitiler-native-effect` | Root `tiler-kwin-effect-ffi`; MIT staticlib `libtiler_kwin_effect_ffi.a` linked into the GPL-covered effect, with tiler-core, serde/serde_json and selected transitives | `crates/tiler-kwin-effect-ffi/Cargo.toml`; `kwin/native-effect/CMakeLists.txt:105-159` |
| Both shipping settings KCMs, in the core package | No Rust linkage; Qt/KF6 only. The KCM config test's Rust linkage is test-only | `kwin/native-effect/CMakeLists.txt:181-216,286-294`; Debian settings-only build |
| KWin script and assets | First-party code only; no npm runtime imports or third-party fonts/icons identified | [Vendored inventory](../../research/licensing.md#11-vendored-attribution-inventory-2026-10-12) |
| Windows executable | `tiler-windows` and Windows-specific dependency closure; defer target-specific notices to Windows delivery | `crates/tiler-windows/Cargo.toml`; KDE-only 0.1 scope in `docs/decisions.md` |

- No compiled vendored non-Rust code was identified by the bounded research
  inventory. Release-time `cargo vendor` supplies Rust sources for offline
  builds; it does not add a separate non-Rust payload.
- Research sections 4.2-4.4 identify Qt/KWin/KF as dynamically linked system
  dependencies, supplied by the distro rather than bundled in these payloads.
  Do not add their full library texts to the Rust notice merely because they
  appear in build/runtime dependencies. The effect's GPL-covered combination
  and existing native license/source obligations still apply. Distro-transitive
  closure and static-embedding checks remain pending in that research note.
- Generate against the two shipping Rust roots, the release `Cargo.lock`, and
  actual Linux target/features. Exclude dev/test roots and unrelated Windows
  resolution. Treat proc-macro/build-only dependencies as non-shipping where
  the tool supports distinguishing them; harmless conservative extra coverage
  is preferable to omitting a compiled dependency. Confirm the tool's filtering
  behavior before prescribing its command line.
- Cover both supported Linux architectures (`x86_64-linux`, `aarch64-linux` in
  `flake.nix`), unioning target-specific normal dependencies. Do not assume a
  host-only result covers every shipped binary.

## Current Delivery Contract

- `scripts/build-source-archive.sh` exports tagged sources into
  `omnitiler-<VERSION>/`, adds the prebuilt KWin bundle, `vendor/`, `VERSION`
  and `SOURCE_REV`, verifies offline Rust builds, and creates deterministic
  `dist/omnitiler-<VERSION>.tar.gz` plus its SHA256 sidecar.
- `.github/workflows/release.yml` currently uploads only that source archive
  and checksum. No GitHub native-binary archive producer is implemented.
  Adding notices to the source archive alone does not satisfy future binary
  delivery; each binary archive must embed the notice.
- Debian/RPM/Arch and OBS consume the prepared release source, with offline
  Cargo source replacement and no npm build. They must install prepared
  notices without needing cargo-about or network access in their buildroots.
- Nix is different: `flake.nix` builds from explicit checkout filesets, not the
  release tarball. Release-generated additions do not automatically enter its
  `tray` or `native-effect` outputs.

## Accepted Design And Implementation

Accepted on 2026-10-13: option B, one scope-labelled `THIRD-PARTY-NOTICES`
covering both shipping Rust roots and supported Linux targets. Developers
regenerate manually using `just`; CI generates into a temporary location and
fails on differences. Both distro packages and the Rust-bearing Nix outputs
install the same file at the locations below. The release source archive carries
the tracked file at its root. No GitHub binary-archive producer is included in
this change.

- `about.toml` selects both Linux GNU targets, excludes dev/build-only
  dependencies and prioritizes MIT for OR-licensed crates. This retains actual
  harvested copyright text rather than cargo-about's generic SPDX fallback.
- `about.hbs` renders full harvested texts, crate/version/license metadata and
  authors without HTML escaping or local registry paths. First-party workspace
  entries are excluded. The union is not a claim that each binary links every
  listed crate; proc-macro entries are conservative coverage.
- `scripts/third-party-notices.sh` verifies harvested texts and shipping
  normal-dependency completeness, then appends in-scope top-level upstream
  `COPYRIGHT*`/`NOTICE*` files in deterministic order. Missing/unresolved texts
  fail generation. No external license service or synthesized substitution is
  used.
- `just third-party-notices` fetches locked sources and replaces the generated
  file only after validation. `just check-third-party-notices` regenerates into
  temporary storage and compares, without updating the tracked file. CI's
  `notices` job provisions tools through the existing devenv pattern, fetches
  locked sources and runs that offline linter.
- Both recipes live in `just/linux.just` and resolve through the root Linux
  import. Freshness, two-run reproducibility, archive fixtures, Debian fixtures
  and all three cargo-deny checks pass with the current lockfile; the generated
  notice content is unchanged.
- All package builders install the prepared file without cargo-about or
  network-dependent notice generation. Nix filesets include the tracked notice.
  The source-archive builder rejects tags missing the root notice.

## Install Locations

Use the same basename for both packages and retain existing first-party texts.

| Format/output | `omnitiler` | `omnitiler-native-effect` |
| --- | --- | --- |
| Prepared release source archive | `omnitiler-<VERSION>/THIRD-PARTY-NOTICES`, shared input for both packages | Same file |
| Future GitHub binary archives, outside this change | `THIRD-PARTY-NOTICES` at archive root | `THIRD-PARTY-NOTICES` at archive root |
| Debian | `/usr/share/doc/omnitiler/THIRD-PARTY-NOTICES` | `/usr/share/doc/omnitiler-native-effect/THIRD-PARTY-NOTICES` |
| RPM | `%license THIRD-PARTY-NOTICES` in `%files` | `%license THIRD-PARTY-NOTICES` in `%files native-effect` |
| Arch | `/usr/share/licenses/omnitiler/THIRD-PARTY-NOTICES` | `/usr/share/licenses/omnitiler-native-effect/THIRD-PARTY-NOTICES` |
| Nix `tray` / `native-effect` | `$out/share/licenses/omnitiler/THIRD-PARTY-NOTICES` | `$out/share/licenses/omnitiler-native-effect/THIRD-PARTY-NOTICES` |

- RPM uses its native license-directory policy; do not hard-code a distro path.
  Verify actual installed ownership/location for both RPM subpackages.
- Debian `copyright` and vendor-license comments do not replace the generated
  binary notice. Check debhelper compression/doc-directory behavior when
  validating the final package payload, including the `noeffect` profile.
- Nix settings-only and script-only outputs compile none of this Rust closure;
  they need no Rust notice solely because the shared fileset contains manifests.

## Tool Availability And Accepted Evidence

- `devenv.nix:9` already declares `cargo-about` beside `cargo-deny`; no duplicate
  addition or configuration edit is needed. The implementation-time-addition
  wording in the licensing decision/research is stale relative to this file.
- Evaluation on 2026-10-13 returned cargo-about `0.9.0` for nixpkgs revision
  `e554fab72f81915600f3f449b786fd9af40439a5`, matching `devenv.yaml` and
  `devenv.lock`. The package flake's separate pin
  `54ba4bcec4043e72a4006d825e0d7aff5562008f` also evaluated to `0.9.0`.
- Final generation has 70 unique third-party crate/version entries, matching
  the union of both roots' normal dependencies for both targets, with no missing
  or extra entries. There are 25 harvested license groups (MIT and Unicode-3.0)
  and two supplemental COPYRIGHT files (linux-raw-sys and rustix). The selected
  dual-license MIT alternative makes unselected Apache bodies unnecessary.
- No top-level upstream NOTICE files were found in the current closure, including
  a case-insensitive inventory check. Supplemental bodies compare byte-for-byte
  with their upstream files. Current final notice SHA256:
  `53aca61a39f814df1fe81a6bfc8402d58c9d605bc277ff0965a977396a69353c`.
- Two independent final renders compare identically. Fresh isolated Cargo-cache
  preparation and offline lint passed; stale-file and synthesized-fallback
  probes fail as intended. The fallback probe identifies 14 unresolved generic
  texts under an Apache-first configuration; MIT-first harvesting passes.
- Source-archive contract tests pass against both fixture HEAD variants (notice
  absent/present), with deliberate missing-notice rejection and extracted-file
  byte comparison. These use toolchain stubs, not a real release build.
- Debian packaging fixture: 30 passed, 0 failed. Shell syntax and Debian install
  recipe parsing pass. Four Debian/Arch staging destinations contain the exact
  final file; staging is not an actual package build or extraction.
- Actual x86_64-linux Nix `tray` and `native-effect` builds and install checks
  pass with the final notice. Both installed files have the hash above and
  compare byte-identically with the generated input. First-party texts remain
  alongside them.

## Remaining Verification And Delivery

- Encoding follows the [Licensing decision](../../decisions.md#licensing).
- Actual Debian/RPM/Arch package builds/extraction, including RPM's resolved
  license-directory policy and debhelper compression/doc-link behavior, are
  unverified. The representative actual-build evidence is the two Nix outputs.
- Full real-toolchain release archive generation and aarch64 binary builds are
  unverified. Notice dependency coverage includes aarch64 metadata.
- Hosted GitHub Actions execution is unverified; its fresh-cache fetch/lint
  sequence was exercised locally. Existing cargo-deny policy/jobs are unchanged;
  `cargo deny --locked check licenses bans sources` passes all three checks
  (non-fatal existing duplicate-syn warning).
- Future binary-archive notice delivery remains separate from this
  implementation's accepted scope.

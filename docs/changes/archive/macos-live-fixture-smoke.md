# macOS live fixture first-use

## Goal and acceptance

Establish reliable standalone macOS socket-driven fixture windows and a
bundle identity suitable for fixture-only scope admission. Cover every protocol
operation, distinguish
requested state from independent native observations, characterize window
identity and AX eligibility, and verify complete owned-resource cleanup.

## Scope and approach

- Standalone fixture live checks; add minimal bundle packaging/admission support
  without changing shared core or product behavior.
- Inspect build, launch, protocol and native observation paths; build offline.
- Exercise bounded protocol operations against identified fixture windows,
  retaining native geometry, visibility, identity and focus readbacks.
- Compare fixture identity with the existing macOS bundle-scope contract and
  document supported behavior, gaps and repair options at point of use.

## Verification and outcome

- Native standalone smoke exercised every protocol operation, CG/AX
  standard-window identity, geometry, focus readbacks, visibility/mode changes,
  decoration toggles, close-all/respawn and shutdown. Detailed coverage and
  limitations are in the [fixture README](../../../crates/tiler-live-fixture/README.md#macos-standalone-evidence-2026-10-20).
- Borderless/non-resizable windows exposed a readback-triggered event storm.
  Sharing the event proxy was tried but did not fix it and was removed.
  Native samples and winit source instead identified `is_maximized()`'s temporary
  style changes. The fixture skips that getter where it mutates styles and
  honestly reports unknown mode unless minimized; one regression test covers
  that mapping. The formerly stalled socket sequence then completed normally.
- The two oversized-frame failures were a test lifetime race: on macOS,
  timeout changes after peer closure returned `EINVAL`, masking `TooLong`.
  Holding the writer open until the reader finishes fixes both tests without
  weakening their assertions or changing production framing.
- Added a signed, regular-policy `io.github.omnitiler.live-fixture` bundle and
  offline build/verify recipes, using existing tools and signing patterns.
  Direct executable launch still has no bundle identity. An accessory-policy
  bundle was excluded as expected; regular policy fixed admission without any
  classifier exception. Native read-only scope inspection verified exact
  identity, launch date and two standard windows classified `Tile`, including
  a borderless/non-resizable window.
- Final gates: 121 offline fixture tests pass, strict all-target clippy and
  workspace fmt pass; bundle signature and macOS 15 deployment floor verify.
  Independent review found no blockers. No shared-core, dependency or product
  admission changes were needed.
- All created live fixture windows/processes, sockets/run directories and
  awake-helper processes were cleaned; protected-window geometry was unchanged.
  Build artifacts remain available. Fullscreen/Space behavior, physical-input
  acceptance, minimum-OS runtime and integrated live tiling remain untested.

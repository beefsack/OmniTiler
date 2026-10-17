# KDE Settings core-catalog migration

- Status: implementation in progress.
- Goal: migrate the existing four native KDE Settings tabs onto the shared Rust
  catalog through deterministic checked-in descriptors and a CI freshness gate.
- Scope: development-only exporter, KDE adapter mappings/presentation overrides,
  native descriptor consumption, compatibility tests and packaging verification.
- Non-goals: new settings (including best-effort admission), changed labels,
  placement, defaults, persistence or native operations; Windows/macOS changes;
  runtime Rust dependency for settings-only packages.

## Compatibility and acceptance

- Follow the [approved boundary](../research/shared-settings-schema.md#recommended-boundary-and-compatibility).
- Preserve both KCM identities, widget names/buddies, KConfig keys, tolerant
  decoding and changed-key-only saves; retain hidden and foreign keys.
- Keep KDE-native labels/order and native shortcut/conflict/status panels.
  Ordinary field staging and Save never invoke Fix/Force/Revert.
- The KDE adapter explicitly omits unsupported border/underlay enable controls
  and the existing non-KCM default-tiling field; it retains the KDE-only preview
  color and hidden shortcut profile. Shared catalog metadata remains unchanged.
- Settings-only builds consume checked-in artifacts without Cargo or KWin.
- Deterministic exporter check must detect stale artifacts; native lifecycle,
  fallback, cross-tab draft and operation-isolation tests must pass.

## Approach and verification

1. Export shared metadata through explicit KDE storage/widget/presentation
   mappings, with exact-once coverage and explicit unsupported-field exclusions.
2. Consume generated metadata in native controls and schemas without changing
   the existing native layout or persistence/operation ownership.
3. Add CI freshness checking and verify generated artifacts are packaged.
4. Independently review compatibility, run Rust workspace and KWin suites,
   settings-only build/CTest and drift positive/negative checks.

- All verification is offline. No live KWin/Plasma evidence is claimed.
- Native user re-check: open either KCM identity, inspect all four tabs and
  keyboard traversal; edit across tabs, Apply/reopen, Defaults/Cancel; verify
  saved values and separate shortcut/conflict operation behavior.

## Accepted evidence

- Exporter emits 14 exposed field mappings, two unsupported-field exclusions,
  one schema-only field, four tabs and native panel slots. Defaults, bounds and
  tokens derive from the catalog; native workspace option text is overridden.
- Generated schemas differ only by provenance comments. Exporter freshness and
  missing/tampered-artifact checks pass; Rust workspace tests, formatting and
  strict all-target clippy pass. Generated header syntax checked with warnings
  denied; actual KCM compilation/consumption remains the next verification gate.

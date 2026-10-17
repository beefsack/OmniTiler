# KDE Settings core-catalog migration

- Status: delivered offline 2026-10-17; native user re-check pending.
- Goal: migrate the existing four native KDE Settings tabs onto the shared Rust
  catalog through deterministic checked-in descriptors and a CI freshness gate.
- Scope: development-only exporter, KDE adapter mappings/presentation overrides,
  native descriptor consumption, compatibility tests and packaging verification.
- Non-goals: new settings (including best-effort admission), changed labels,
  placement, defaults, persistence or native operations; Windows/macOS changes;
  runtime Rust dependency for settings-only packages.

## Compatibility and acceptance

- Follow the [approved boundary](../../research/shared-settings-schema.md#recommended-boundary-and-compatibility).
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

## Accepted evidence

- Exporter emits 14 exposed field mappings, two unsupported-field exclusions,
  one schema-only field, four tabs and native panel slots. Defaults, bounds and
  tokens derive from the catalog; native workspace option text is overridden.
- Generated schemas differ only by provenance comments. Exporter freshness and
  missing/tampered-artifact checks pass (13 exporter tests). All 14 existing
  field labels, enum choices/tooltips and numeric bounds, plus four tab titles,
  are consumed by a focused native adapter over the retained Designer layout.
  KConfigXT consumes generated appearance defaults. Save and operations stay
  native; no generic operation setter was introduced.
- Native regression coverage checks field/widget/tab/buddy parity, synthetic
  descriptor-driven rendering, cross-tab drafts, Apply/Defaults/Cancel, tolerant
  fallback, exact changed keys and retained hidden/foreign configuration. Both
  KCM identities share the adapter; operation paths remain isolated.
- Independent compatibility review caught a missing workspace member in the
  Nix native-effect source fileset. Adding the exporter restores offline Cargo
  resolution; hermetic Nix settings tests pass 27/27 and effect tests 34/34.
  Source-archive checks now require the generated header and native helpers.
- Latest integrated Rust workspace tests: 2,986 passed, zero failed; formatting
  and strict all-target clippy pass; locked workspace build passes. KWin tests:
  1,773 passed, zero failed; typecheck passes. Settings-only CMake build with warnings denied and CTest
  pass 27/27. Generated-header syntax and real KCM builds pass.
- KI18n lazy source markers are supported since 5.89, below the KF6 floor.
  C-locale native text parity is tested; translated message extraction has no
  repository harness and remains unverified, as does physical native UX.
- Source-archive contract suite passes on the committed native implementation,
  including exporter sources, generated descriptors and native helper sources.
- CI run [39021538182](https://github.com/beefsack/OmniTiler/actions/runs/39021538182)
  passed seven jobs, including Rust/exporter freshness, native, Windows and
  macOS. Its sole failure was the pre-existing KWin API-guard mismatch for
  offline aliases, addressed in the [guard follow-up](kwin-66-test-alias-guard.md).
  The `bf99f3dc` fix supplies exporter workspace-lint inheritance
  and is included in this passing Rust run.

## Native user re-check

1. With rebuilt Settings installed, open
   `kcmshell6 kwin/effects/configs/omnitiler-active-border_config`, then repeat
   with `kcmshell6 kwin/scripts/configs/omnitiler-kwin_config`.
2. Visit Tiling & Workspaces, Appearance, Shortcuts and Desktop Integration.
   Compare labels, field order, values and native actions with the existing
   interface. Check Tab/Shift+Tab, label buddies and tab keyboard navigation;
   verify script Status remains below every tab. Repeat at the usual scaling.
3. Record current inner gap and border width. Change both on their respective
   tabs, switch away/back, Apply, close and reopen. Verify both values reload;
   ordinary Apply must not run shortcut or window-edge Fix/Force/Revert.
4. Change a value without applying, close with Cancel and reopen; verify the
   saved value is unchanged. Invoke Defaults, inspect across tabs, then Cancel
   without saving. Restore the two recorded values through Apply.
5. Inspect Shortcuts and Desktop Integration feedback. If checking explicit
   Apply Shortcuts/Force/Revert or window-edge Fix/Revert, use the existing
   user-owned conflict/recovery procedure and restore its exact baseline;
   these operations remain separate from ordinary Settings Apply.
6. Workspace mode retains its restart requirement. This migration does not
    require a session restart merely to inspect the rebuilt KCM.

## Bounded native shell observation, 2026-10-18

- Both KCM identities opened through the real shell with process-scoped plugin
  staging. The script-KCM follow-up proved the source-built plugin mapping.
  First-page rendering showed the expected tab titles/order, tiling controls,
  gap fallback values, Status panel and disabled Apply.
- Tabs 2-4, traversal/buddies, draft/Apply/reopen, Cancel, Defaults-without-save
  and ordinary-Apply operation isolation remain untested live. Settings-only
  CTest passed 27/27 but does not replace those interactions. Configuration,
  shortcut state, protected windows and desktop baseline were restored.
- [Acceptance continuation and limitations](../kde-live-acceptance.md).

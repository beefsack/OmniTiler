# Shared settings schema and tabbed Settings

- Selected 2026-10-16: **D, a small Rust catalog with checked-in generated native
  descriptors, staged before 0.1**. First slice: shared catalog and native
  Windows tabs; KDE exporter/generated descriptors delivered offline 2026-10-17.
- Status: options delivered 2026-10-15; route/timing approved 2026-10-16,
  superseding the post-0.1 recommendation below. Windows implementation and
  pending physical acceptance are tracked in the
  [active record](../changes/shared-settings-catalog-windows-tabs.md).
  User-approved 2026-10-16 tab placement: Tiling & Workspaces (gaps,
  workspace defaults/scope, move/admission/refill), Appearance (border,
  underlay), Shortcuts (takeover, Win+L, presets, bindings, Xbox/Game Bar),
  Desktop Integration (mouse Snap, taskbar pager). Placement is accepted;
  physical layout/accessibility acceptance remains pending.
  KDE-only tabs were [delivered independently](../changes/archive/kde-settings-tabs.md).
  Their [catalog migration](../changes/archive/kde-settings-core-catalog.md) preserves
  native presentation and storage; user-owned native re-check remains pending.
  Estimates below are historical engineering assessments, not commitments.
- Evidence snapshot: `6564e671ebe7995b32a48d50f0c356516f483d77`. Repository
  `file:line` locators below refer to that revision. Source and commit inspection,
  not new build or native UX acceptance.

## Current ownership and duplication

| Surface | Source-backed finding |
| --- | --- |
| KDE native Settings | QWidget Designer XML and C++, not QML. One vertical page has border, shortcuts, tiling/workspace, status and window-edge conflict groups (`kwin/native-effect/unifiedsettings.ui:4-7,178-182,273-277,399-403,428-432`). Both KCM identities compile the same implementation/UI through thin factories (`kwin/native-effect/CMakeLists.txt:163-217`). |
| KDE schemas/build | Effect fields use `activeborderconfig.kcfg` (colors, doubles, theme flag); KConfig compiler consumes its `.kcfgc` at build time (`kwin/native-effect/activeborderconfig.kcfg:7-39`; `kwin/native-effect/CMakeLists.txt:56-57,169-171`). Script `main.xml` declares enum tokens/defaults, `defaultTiled=true`, hidden `shortcutProfile=cosmic`, gaps `8/0..64` (`kwin/contents/config/main.xml:8-74`). The retained script Designer form repeats labels/gap bounds, but native KCM discovery replaces the generic-scripted route (`kwin/contents/ui/config.ui:7-107`; `kwin/tests/native-kcm-static.test.ts:108-134`). |
| KDE persistence/defaults | Native KCM opens `kwinrc`, group `Script-omnitiler-kwin`, loads seven owned values, stages changes and writes changed keys; other keys remain untouched (`kwin/native-effect/unifiedsettings_module.cpp:1642-1669,1692-1723,1769-1785`). Gap bounds/default and enum fallback readers are handwritten (`:50-111`); labels/options/tooltips repeat in C++ (`:179-203`) and `.ui`. |
| KWin script | `readConfig` supplies profile/mode/default-tiled (`kwin/src/plan-adapter-entry.ts:538-595`); `workspace-native.ts` repeats mode tokens, local default and tiled default, with tolerant parsing (`kwin/src/workspace-native.ts:28-75`). Gap defaults/bounds/readers repeat in `kwin/src/domain-gap.ts:10-13,20-43,56-88`. Separate same-axis/fixed/born/refill helpers also normalize configuration. |
| Rust portable core | Owns domain policy/types, not a complete settings definition. Gap bounds: `crates/tiler-core/src/bounds.rs:17,56-59`; enum wire/default helpers: `crates/tiler-core/src/directional.rs:220-241`, `crates/tiler-core/src/size_hints.rs:182-209`, `crates/tiler-core/src/output_selection.rs:280-303`, `crates/tiler-core/src/born_maximized.rs:20-55`; workspace tokens/default/parser: `crates/tiler-core/src/workspace_scope.rs:42-77`. No shared tabs/field/label catalog in these seams. |
| "Core Settings ownership" | Delivered 2026-10-11 means **distro core package**, not `tiler-core`: both KCMs link Qt/KF, remain available without the optional effect, and contain no KDE code in the portable crate (`docs/changes/archive/release-0.1-core-settings.md:32-34,40-56`; `docs/decisions.md:404-412`). Settings-only CMake explicitly avoids KWin/Cargo/effect dependencies (`kwin/native-effect/CMakeLists.txt:8-20,67-68`). |
| Windows | `settings.rs` owns schema-v1 typed JSON, defaults, labels/help/options and validation (`crates/tiler-windows/src/settings.rs:30-63,98-294,299-338,2413-2498`). `settings_ui.rs` creates handwritten native Win32 controls, edits a draft, validates/atomically saves on Apply, reloads on Revert and never saves on Close; stale edits refuse (`crates/tiler-windows/src/settings_ui.rs:1-25,1345-1402`). Storage validates bounded reads and atomically replaces files (`crates/tiler-windows/src/settings.rs:2785-2899`). |
| macOS | Phase-0 probe configuration/CLI, not a persistent tiling-settings dialog: modifier/tap/AX/window bounds and validation (`crates/tiler-macos/src/config.rs:3-19,56-107`; CLI help `crates/tiler-macos/src/cli.rs:108-170`). macOS started 2026-10-14; K2/K3 are due for consumer-driven review, not automatically extracted (`docs/decisions.md:53-56`). |

- Largest duplicated metadata: gaps `8/0..64` in script XML/form, TS, KCM
  C++/form and Windows settings/UI; border/underlay defaults in kcfg and Windows
  constants, bounds in both native UIs/validators; five enum option sets in
  script XML/TS, Rust, KCM and Windows. Labels/tooltips are separately authored
  in XML, KCM and Windows (locations above).
- Some duplication is already removed: Windows validates four enums through
  core `parse_wire`, workspace mode through core tokens, and derives born/mode
  defaults from core (`crates/tiler-windows/src/settings.rs:162-165,291-294,2428-2464`).
  This is shared value semantics, not a UI definition.

### Existing conformance and mirror tests

| Evidence | What it proves / remaining gap |
| --- | --- |
| `kwin/tests/domain-gap.test.ts:25-98`; `kwin/tests/native-kcm-static.test.ts:36-66,108-158` | Pins schema/default/bounds, retained form, native source/UI and discovery mirrors, plus TS fallback behavior. Tests contain their own expected metadata; not an authoritative all-platform catalog. |
| `kwin/tests/born-maximized.test.ts:550-578`; `crates/tiler-windows/src/settings.rs:3510-3547`; `docs/changes/archive/born-maximized-shared-policy.md:48-55` | One shared `test-fixtures/born-maximized.json` serves Rust/KDE/Windows conformance. KDE exercises actual clear/keep admission, not just parsing. This is a useful incremental pattern, not a full settings schema. |
| `crates/tiler-windows/src/settings.rs:3036-3083,3086-3123,3311-3347` | Pins KDE-parity defaults and exact core enum/workspace token mappings. Default parity is largely literal assertions, not reading KDE artifacts. |
| `kwin/native-effect/scriptconfig_module_test.cpp:1-100`; `kwin/native-effect/CMakeLists.txt:298-419` | Hermetic KCM lifecycle/load/save/default/gap/reconfigure and discovery tests. Together with static mirrors these catch many local drifts; there is no complete cross-platform tab/field/type/label/validation contract. |

### Recent commit and deferred K2

- `ecd183e` (`feat: add shared workspace scope policy and Windows mode setting`)
  adds pure workspace resolvers plus token/default helpers, Windows mode
  schema/options and three-choice controls; it changes no KDE files. Evidence:
  `git show --stat ecd183e`, `crates/tiler-core/src/workspace_scope.rs:1-18,42-77`,
  `crates/tiler-windows/src/settings.rs:247-294,2457-2464`,
  `crates/tiler-windows/src/settings_ui.rs:89-91,1384-1387`.
  **Reusable seed, not shared settings-definition machinery.** Windows runtime
  mode adoption/hotplug remain pending (`docs/decisions.md:538-557`).
- K2's 2026-09-30 audit deferred new runtime metadata, generation/build coupling
  and JS bridges pending a chosen consumer contract
  (`docs/changes/archive/portable-core-k2-settings-actions.md:3-5,25-31`;
  `docs/research/cross-platform-core/extraction.md:117-121`). The durable decision
  says revisit when macOS starts (`docs/decisions.md:1494-1499`); that trigger
  has now occurred. The post-Windows audit already identified independent
  settings/catalog duplication (`docs/research/cross-platform-core/post-windows-audit.md:61-70`).
- This item is a **bounded K2 settings-metadata slice**, not extraction of
  action catalogs, native conflict/recovery or K3. It requires a new explicit
  route/timing decision. Born-policy generation/runtime rewiring was declined
  for the immediate lift (`docs/changes/archive/born-maximized-shared-policy.md:40-44`);
  these proposals do not reverse that decision or generate behavioral policy.
- KWin-to-Rust research proposes local Rust behind a small KCM C ABI for
  validation/catalog reducers, while C++ retains Qt/KConfig/I/O
  (`docs/research/kwin-rust-migration.md:69,87,95-98`). That is compatible future
  work, not a prerequisite for tabs or declarative metadata sharing.

## Options

- Effort: rough engineer-days for existing KDE/Windows settings and offline
  verification; native user acceptance and a future macOS renderer are additional.
  Estimates are not measured or release commitments.

| Option | Complexity / layering fit | Build and packaging impact | Effort / pre-0.1 risk |
| --- | --- | --- | --- |
| **A. Rust catalog, generate at each build** | Medium-high. One definition for tabs/fields/types/defaults/validation; native renderers use generated descriptors (or generated `.ui` fragments). Good Layering if portable metadata, not toolkit layout, lives in core. Validation-only variant detects drift but still leaves manual forms. | Export tool must run before KConfig/uic and emit kcfg, script XML/TS metadata and native descriptors. Adds settings-only Cargo/generator inputs, build ordering, offline/cross-compilation and distro/Nix recipe verification. Build-host exporter must not execute target binaries. | 12-20 days; **high** release risk from build/packaging churn. |
| **B. Rust catalog via runtime FFI/JSON** | High. Native UIs construct controls from descriptors; Rust hosts consume directly, KCM needs a versioned local C ABI/JSON bridge. Core remains safe/dependency-free; serialization outside core. No daemon needed for local FFI. A remote-service variant adds availability/fallback complexity. | **Does not eliminate build-time kcfg** with today's KConfig skeleton; still generate/validate it. Local FFI adds a Rust link seam to settings-only KCM, independent of the optional effect. Sharing with plain KWin JS would separately need an async route/artifact, not a synchronous FFI call. | 10-18 days; **high** release risk from ABI/lifetime and settings build changes. |
| **C. Native tabs + shared conformance table only** | Low; strongest short-term Simplicity. Keep forms, enforce common defaults/tokens/bounds with fixtures. **Partial objective:** does not remove hand-maintained field/tab/label lists. | Existing build/package split retained; no generator or runtime seam. | KDE tabs 1-2 days; existing-host conformance 3-5 more; **low** for tabs, low-medium for broad test changes. |
| **D. Rust catalog + checked-in generated artifacts, staged** | Medium; recommended balance. Rust hosts consume catalog directly; KDE uses generated descriptors in a small native renderer. One field/tab definition without generating whole dialogs or behavioral policy. Custom native panels stay explicit. | Development exporter emits checked-in kcfg/script XML/TS value metadata/C++ descriptors; CI regeneration fails on drift. KCM builds those artifacts with existing tools: no new Cargo/runtime requirement for settings-only package. Update archive/Nix source inclusion and localization extraction; retain both KCM identities. | 8-15 days staged across KDE/Windows; macOS adapter/renderer estimate 3-5 additional once UI requirements exist. **Medium** if rushed before 0.1; low initial catalog pilot after release. |

| Option | Persisted-key migration / compatibility | Test strategy |
| --- | --- | --- |
| A | Preserve mappings, tokens/defaults and types; generator must reproduce current storage, not impose Rust names on files. No schema-v1 bump merely for tabs. New fields require explicit missing/invalid policy. | Golden kcfg/XML/descriptors, strict compiler/type checks, deterministic generator/build-order checks, offline settings-only/effect-only package builds, native lifecycle suites. |
| B | Stable field IDs plus versioned descriptor ABI, without changing saved keys. Freeze catalog for a dialog's draft; retain native stale-edit handling. Runtime absence/version refusal must leave Settings recovery available. | A's artifact parity plus ABI/version/lifetime/error cases and effect-absent KCM tests; remote variant also needs service-loss/reconnect coverage. |
| C | Pure tab regroup needs no migration. Fixtures pin separate missing/invalid behaviors and startup/live timing rather than forcing all platforms to normalize identically. | Extend existing mirror/shared-fixture coverage; native cross-tab Apply/Defaults/Cancel/Revert and discovery tests; user accessibility/size check. |
| D | Same preserved mappings as A. Migrate one consumed field family at a time; generated outputs are derivative, not second sources. Keep invalid-load policy and persistence/apply in adapters. | Catalog invariants, golden output + CI freshness, old-file roundtrips, retained hidden/foreign-key checks, native renderer/control-ID parity and existing platform suites. Verify source archives include all checked-in outputs. |

## Recommended boundary and compatibility

| Shared definition (proposal) | Native responsibility / preserved contract |
| --- | --- |
| Stable tab/field IDs; bool/int/float/color/enum type; tokens, defaults, bounds; functional source labels/help; timing category | Qt/Win32/macOS widgets, accessibility, localization/theme, layout metrics. Toolkit types and serialized JSON stay out of dependency-free core (`docs/research/cross-platform-core/extraction.md:137-141`). Source text/IDs may be shared; reference WMs remain tooltip-only (`docs/decisions.md:1367-1369`). |
| Portable validity checks returning explicit valid/invalid outcomes | KDE tolerant configuration decoding/fallback versus Windows strict file refusal/last-good are preserved (`kwin/src/domain-gap.ts:20-43`; `crates/tiler-windows/src/settings.rs:2413-2464`). Wire defaults are not configuration defaults (`docs/changes/archive/portable-core-k2-settings-actions.md:12-14,30`). |
| Field IDs mapped by platform adapters/exporter | Keep `kwinrc [Script-omnitiler-kwin]` camelCase and `[Effect-omnitiler-active-border]` keys; Windows schema-v1 nested snake_case (`settings.rs:299-338`, `:549-568`). No key rename, bulk rewrite, or new universal storage format. Tab movement is presentation only. |
| Common timing/help metadata with host capability overrides | Preserve KDE startup-only `workspaceMode`, hidden startup `shortcutProfile`, future-workspace `defaultTiled` and live/next-admission settings. Windows mode UI must not imply adoption already delivered. Native saved/requested/confirmed state stays distinct (`docs/decisions.md:423-433,538-557`; `settings_ui.rs:6-10`). |
| Tab grouping plus platform-specific fields/panel slots | Shared fields need not appear on unsupported hosts. KDE shortcut/conflict Fix/Force/Revert and Windows takeover/Win+L/presets stay native operations; no generic field setter may invoke them. Ordinary Save remains isolated (`docs/decisions.md:418-445,464-492`). |

## Recommendation, timing and smallest steps

| Decision for the user | Recommendation / exact bounded follow-up |
| --- | --- |
| Pre-0.1 usability | **Approve KDE-only tabs independently** if desired: Tiling & Workspaces, Appearance, Shortcuts, Desktop Integration. Keep operation/status feedback visible or badged outside the relevant tab. Retain widget names, `kcfg_*` bindings, both KCM discovery routes and existing Save/shortcut operations. Smallest first implementation: regroup `unifiedsettings.ui` into a `QTabWidget`, adjust navigation text only, then existing static/KCM tests plus cross-tab draft/Apply/Defaults/Cancel checks and a user native size/keyboard check. No schema work needed. |
| Full shared schema route/timing | **Choose D post-0.1**. It meets the single-definition objective with less build/runtime coupling than A/B; C is an interim guard, not completion. Simplicity before layering machinery (`docs/principles.md:12-27`); no new KDE 0.1 release gate recommended. |
| First schema slice and K2 scope | After approval, start with **inner/outer gaps only**: core metadata references existing bound, adds default/type/tab/label/validity; external exporter reproduces the two existing XML entries and supplies native descriptors; CI checks freshness and boundary/fallback/refusal fixtures. Then wire the KDE/Windows controls to those descriptors before adding enums/styles. Include TS value metadata to avoid a new synchronous JS-to-Rust route. This pilot demonstrates consumers, compatibility and packaging before extending the catalog. |
| macOS participation | Define only already-consumed portable fields now; add macOS capability/persistence/rendering when its actual Settings UI is designed. Do not extrapolate tiling settings from probe flags. Starting macOS triggers K2 review, not approval to migrate the whole action catalog or block its Phase 0 on schema machinery. |

- Verification for this research: source/history cross-checks, cited-path/line
  and document-link checks, and `git diff --check`. Implementation tests above
  are proposed acceptance, not tests run for this documentation.
- Current next action: physically accept the Windows catalog-rendered tabs and
  re-check the migrated KDE Settings. Both catalog consumers are implemented;
  the KDE exporter and checked-in artifacts have a CI drift gate. The tables above
  preserve the original options, not a still-pending route/timing decision.

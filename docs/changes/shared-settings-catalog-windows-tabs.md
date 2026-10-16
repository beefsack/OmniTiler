# Shared settings catalog and Windows tabs

- Status: implementation, offline review and CI complete; tab placement
  user-approved 2026-10-16; physical acceptance pending.
- Goal: replace the over-tall Windows Settings page with four native tabs
  rendered from a small shared settings catalog (approved route D, 2026-10-16).
- Scope: portable field/tab/section/type/default/bounds/token/label/help/timing
  descriptors and explicit validity in dependency-free `tiler-core`; Windows
  adapter metadata, native panel slots and Win32 rendering. Platform policy,
  widgets, storage and operations remain native.
- Non-goals: KDE/macOS behavior changes, KDE exporter/generated descriptors,
  storage migration, runtime adoption changes or generic native-operation setters.

## Compatibility and acceptance

- Preserve every contract in the research's
  [boundary table](../research/shared-settings-schema.md#recommended-boundary-and-compatibility):
  Windows schema-v1 nested snake_case keys and strict refusal/last-good,
  KDE keys and tolerant decoding, independent saved/requested/confirmed state,
  startup/next-admission timing and isolated native operations.
- Cover every exposed portable Windows setting; declare Windows-only fields
  and operation panels in the adapter. No toolkit types or serialized JSON
  in the core. Keep reference window-manager names tooltip-only.
- Tabs: Tiling & Workspaces, Appearance, Shortcuts, Desktop Integration.
  All content fits or scrolls within a 1080p display at 100% and 150%; operation
  and warning feedback remains visible or badged outside the active page.
- Preserve Apply/Save, Revert/reload and Close behavior, stale-edit refusal,
  stable control IDs, cross-tab draft edits and native operation isolation.
- Ctrl+Tab, reverse navigation, ordinary tab order and accessibility names work.

## Approach and bounded units

1. Catalog plus Windows typed adapter: static metadata and value validation,
   capability flags, exact-once mapping/storage round-trip tests.
2. Native tab renderer/layout and responsibility split; retained native panels,
   persistent status/conflict feedback; offline layout/navigation model tests.
3. Independent compatibility and Win32 lifecycle/layout review, corrections,
   full offline gates and cross-platform CI evidence.
4. Durable decisions/research status and a KDE-stream follow-up for exporter,
   checked-in descriptors and CI drift check.

## Implemented presentation (user-approved 2026-10-16)

| Tab | Sections and Windows-only placement |
| --- | --- |
| Tiling & Workspaces | Gaps, Workspaces (default tiling and scope), Same-axis move, separate born-maximized/fixed-size Admission groups, Migration source refill |
| Appearance | Active border and Group underlay |
| Shortcuts | Keyboard takeover, Win+L opt-in/warning, Authentic/Compatible presets, Binding editor, Xbox/Game Bar conflict details and navigation/Recheck |
| Desktop Integration | Mouse Snap prevention and Taskbar pager |

- A 924x600 logical client replaces the tall single page. All pages fit the
  layout model at 100% and 150%; long preset/rebind/workspace guidance and
  selected conflict detail scroll in readonly native edits. The binding list
  retains its scrollbars. No new operation surface was added.
- Apply/Revert/Close, save/operation status and an aggregate shortcut/conflict/
  unsaved-draft badge remain outside the selected page. Hidden pages retain
  their draft text and are disabled for keyboard traversal. Native tab arrows,
  Ctrl+Tab and Ctrl+Shift+Tab select pages; buddy labels name scalar editors.
- Core defines four tabs, seven portable sections and 17 fields; Windows adds
  six sections, four bool fields, 21 exact-once storage mappings and six native
  panel slots. Typed adapter get/stage seams are tested; native whole-draft
  collection/persistence remains unchanged rather than introducing a new
  generic Save path.

## Verification

- Windows offline gates: locked build/tests/strict all-target clippy for
  `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`;
  workspace rustfmt check and `git diff --check`.
- Shared code also needs Linux/KDE and macOS CI evidence.
- Catalog invariants: unique stable IDs, valid defaults, bounds/token checks,
  all Windows fields mapped exactly once and unchanged storage round-trips.

## Physical check plan (pending)

1. At both 100% and 150% display scaling on a 1080p display, open Settings and
   visit all four tabs. Verify every section, label, editor and action fits;
   where scrolling is required, reach the first/last controls without clipping.
2. Switch tabs with mouse, Ctrl+Tab and Ctrl+Shift+Tab. Traverse all controls
   with Tab/Shift+Tab and radio arrows; verify focus visibility, reading order
   and accessible names (including numeric/color editors and the tab control).
3. On each tab, edit a value, switch away/back and verify the draft survives;
   Apply/Save, reload/Revert, close/reopen and verify saved values. Close with
   unsaved edits via Close, Escape and Alt+F4 and verify nothing is saved.
4. Exercise shortcut pending-chord row switches, presets and invalid input;
   verify cross-tab Apply still validates the entire draft and stale edits
   refuse. Confirm save wording never claims runtime adoption or confirmation.
5. Verify conflict/warning and operation/save feedback stays visible or badged
   on every tab; Xbox/Game Bar navigation/Recheck retains its existing meaning.
   Confirm ordinary Save never invokes native remediation or policy operations.
6. Verify startup-only workspace mode and future-workspace/next-admission notes
    are truthful, and Settings can still be sent to other workspaces.
7. Verify first launch creates the native tab strip without relying on earlier
   common-control initialization. Confirm group frames remain visible, all
   page controls are clickable, and the whole window is on-screen at 150%.
   Let Recheck complete while another tab is selected; hidden gaming controls
   must stay outside keyboard traversal.

## Evidence and outcome

- Initial source inspection: the prior large Windows settings modules have
  already been split by responsibility; the tab change extends that structure.
- Interrupted work was retained, built successfully, and reconciled with the
  upstream source before completion. Catalog/adapter review confirmed unchanged
  keys, tokens, defaults, strict validation and native-operation isolation.
- Corrections: initialize tab classes before creation, leave room for the
  window origin at 150%, preserve hidden-page disable gates on every refresh
  path, and keep strip/group frames behind controls without changing editor
  sibling order. An initial anchor-chained sibling-order correction was wrong
  about Win32 insert-after direction; replaced with absolute bottom placements
  and a regression model exercising the actual direction and the failed case.
- Catalog tests cover ID uniqueness, valid defaults, bounds/tokens/capabilities/
  timing, exact-once Windows mapping and storage round-trips. Renderer tests
  cover control/section coverage, fit models, tab cycling, buddy/creation order,
  radio starters, focus transitions, badge truth and sibling ordering. Text
  fit uses conservative estimates, not native font measurement or accessibility
  proof. Production files remain below approximately 1,500 lines; renderer
  tests are separated by responsibility.
- Durable route/compatibility decision and research status updated; the backlog
  carries the separate KDE exporter/generated-descriptor/CI-drift follow-up.
- Final Windows offline gates pass with Rust/Cargo 1.99.0: locked build and
  tests for the four listed packages (2,276 passed, zero failed, one existing
  ignored test), all-target strict clippy, workspace rustfmt and
  `git diff --check`. Shared-platform CI 38876735949 (`bfc3ccbd`) passed all
  eight jobs; follow-up documentation CI 38876893770 (`8df22a01`) also passed.
- Physical checks remain pending; this record stays active until acceptance.

## Agent-live follow-up (2026-10-17)

- Single-output native Settings at `dffdf871`: all four tabs selected by mouse;
  four Ctrl+Tab steps wrapped forward and four Ctrl+Shift+Tab steps reversed.
  Per-tab edits (inner gap, border radius, takeover and taskbar pager) survived
  tab changes; Apply saved all four and close/reopen reloaded them correctly.
- Revert discarded a further edit through its confirmation/result path; Escape
  with an unsaved edit left the saved file unchanged. Unsaved/conflict feedback
  remained visible outside the selected page. Evidence: `settings-tabs-proof`
  report and `sendunit-j1/20261017-064537-23500` for Settings workspace sends.
- Original settings bytes and SPI values were restored. These checks do not
  prove the full scale/layout/accessibility matrix, every traversal control,
  stale-edit validation, all close methods or physical shortcut feel.
- Locked four-package build/test, fmt, strict all-target clippy and diff check
  passed: 2,458 passed, 0 failed, 1 ignored. No source change.

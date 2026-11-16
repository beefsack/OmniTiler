# KDE cursor-first targeting adoption

## Goal and boundaries

- Adopt the existing [output-targeting decision](../decisions/layout-and-admission.md#output-targeting)
  through KDE's current settings/catalog route and command ingress.
- Shared core already resolves cursor-first selections and three-way admission;
  sends retain subject-output authority. No core policy or protocol change.
- Preserve default-off hover, unsupported KDE cursor warp, scope and native
  safety fences. Explicit-send current-view sampling remains separate.
- No macOS or Windows adoption changes; their shared-core targeting is unchanged.

## Acceptance and bounded work

1. Directional focus with no native foreground submits the existing empty subject.
   Other commands retain their subject requirements and scope fences.
2. Normal KDE settings carry `switchTarget`, `freshPlacement` and
   `focusFollowsCursor` with catalog defaults and existing revisioned delivery.
3. Expose those three fields through the existing exporter, generated schema and
   native controls. Preserve Save/reconfigure proof and shortcut ownership.
4. Verify fixture-only [PX legs](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs):
   injected cross-output hover, separate send discriminator, empty-output
   close/hide/switch/send, sole minimize/no-focus navigation and restore.
   Unreachable routing is BLOCKED; unknown visibility is not hide proof.
   Human feel, real applications and display changes remain outside this evidence.

## Verification

- Verification coverage: command-ingress regressions, Rust format/build/strict
  clippy/tests, KWin typecheck/tests, and both launcher test suites.
- Preserve frozen world-wire v4, stage5, stage5-v4 and kde-v4 fixtures.
- Native evidence follows the [live guide](../live-kwin-testing.md) and
  [isolated settings procedure](../dev-loop.md#isolated-fixture-settings), with
  joined fixture identities, raw log citations and complete teardown receipts.

## Accepted evidence

- Source inspection: selection resolver in
  `crates/tiler-core/src/world_tiling_views.rs` and execution-cursor regressions
  in `world_logical_contract_tests.rs`; three-way resolver in
  `cursor_targeting.rs` and staged v4 admission tests in
  `world_observe_capture_tests.rs`; subject-home send source and relative ring
  in `world_tiling_views.rs` / `world_commands.rs`.
- The active KDE v4 report encoder omits legacy observer targeting declarations;
  settings use the separate revisioned connection lane. Normal runtime settings
  and native controls now carry the three approved fields through the existing
  catalog/descriptor route.
- Existing hover-motion subscription is settings-acceptance gated; this work
  adopts its normal settings surface, not another motion mechanism.

### Genuine no-focus shortcut routing

- Directional shortcuts now submit the existing empty subject only when native
  foreground is genuinely absent. A scope-filtered foreign foreground still
  refuses; other windowed commands retain their subject gates.
- The first minimal patch treated both null filtered-ID cases as absence and
  failed the existing foreign-focus ingress regression. The correction preserves
  a raw foreground-existence check for shortcuts while native toggles retain
  their scoped reference getter. No protocol or core behavior changed.
- Regressions cover the registered shortcut's exact empty-subject queue payload,
  foreign-focus refusal, in-scope implicit subjects and no-subject move refusal.
- Review found no serious issues. Final offline verification passed: Rust format,
  three-package all-target build and strict clippy; Rust tests 3,460 passed,
  3 ignored; KWin typecheck and 1,361 tests; launcher 703 and native-effect
  launcher 204 checks. Frozen fixtures and core/protocol source were unchanged.
- This establishes command ingress only, not native no-focus navigation or PX
  acceptance.

### Normal runtime settings

- Normal configuration now carries `switchTarget`, `freshPlacement` and
  `focusFollowsCursor` through the existing revisioned settings lane. Defaults
  are cursor output, cursor-output/last-focused admission and disabled hover.
  All catalog targeting alternatives are supported; invalid or throwing readers
  fall back to those defaults. Cursor warp remains unsupported.
- Regressions cover defaults, alternatives, fallback, changed-content revision
  and acknowledgement, unchanged-content suppression, and normal configuration's
  accepted-hover subscription, disable and stop. Existing generation-rotation
  coverage remains. Isolated settings still avoid normal configuration reads.
- The first full KWin run found three current-producer byte comparisons still
  expecting omitted settings. Regeneration updated five active artifacts:
  the KDE settings request plus both Stage 5 discovery requests and replies.
  Only the three existing values and their missing-key diagnostics changed;
  protocol shape and frozen historical families were unchanged.
- Focused verification passed 46 settings/motion/connection tests and 40 active
  fixture tests. Final review found no serious issues. Full verification passed:
  Rust format, three-package all-target build and strict clippy; Rust tests
  3,460 passed, 3 ignored; KWin typecheck and 1,367 tests; launcher 703 and
  native-effect launcher 204 checks.

### Native targeting controls

- The exporter now exposes the same three catalog fields through two enum
  controls and one Boolean checkbox. Generated schema and descriptor entries
  preserve the existing keys, tokens, defaults and descriptor shape. Cursor
  warp remains unsupported; no core or protocol changes were needed.
- Load, dirty tracking, Defaults and exact-key Save include all three fields.
  Save requests live reconfigure without claiming application; failures retain
  the existing retry path. Only workspace mode requires restart. Hidden,
  foreign, shortcut and host settings retain their existing ownership.
- The first native run failed two new assertions that assumed malformed stored
  Boolean text used the false default. KWin and the native control both delegate
  storage decoding to KConfig; the corrected tests preserve that existing
  behavior while separately pinning missing-key false and actual Boolean type.
  Strict protocol and isolated JSON validation remain unchanged.
- Final review found no blocking issues. Verification passed: Rust format,
  three-package all-target build and
  strict clippy; Rust tests 3,460 passed, 3 ignored; KWin typecheck and 1,367
  tests; launcher 703 and native-effect launcher 204 checks. Exporter tests
  passed 14/14, strict exporter clippy passed, and all generated artifacts
  matched the exporter. The isolated settings-only native check passed 27/27,
  including targeting persistence, Defaults/Cancel, reconfigure and retry.
- Frozen fixtures were unchanged. Fixture-only PX execution and native
  keyboard/visual acceptance remain; these offline checks do not close them.

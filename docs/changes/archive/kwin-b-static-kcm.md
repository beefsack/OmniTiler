# Reduce KWin native KCM source pinning

This record closes the KCM portion of [the implementation minimisation review's kwin-b-static item](../implementation-minimisation-review.md#decisions-and-keeps). The presentation suite remains unchanged and its audit remains open. No shipped behavior or frozen fixtures changed.

For the KCM suite, delete spelling and formatting pins; delete duplicates only with an identified enforcing test. Retain unique contracts on the ordinary gate, moving them to the existing native owner where possible.

## Verification

The ordinary gate is `.github/workflows/ci.yml`: the `kwin` job builds the protocol evaluator and runs the npm suite/typecheck; the `native` job builds `native-effect-tests` and `native-settings-tests`. Both checks configure `BUILD_TESTING=ON` and run `ctest --output-on-failure` in their `flake.nix` install checks, with Qt offscreen and an isolated config home. The full effect build also enables settings tests; the settings-only build configures effect tests out.

Baseline `dfb70175` has 449 presentation and 334 KCM assertion callsites (including helpers; loop iterations are not separate callsites). The two suites total 1,685 physical lines. The full native CMake build and its 38 CTest entries passed with the CI effect derivation's dependencies and configure flags. Both Rust evaluator examples built offline in fresh directories; Rust formatting passed and the frozen fixture roots matched `ff6eaddc`.

## KCM assertion audit

Original assertion locations refer to `kwin/tests/native-kcm-static.test.ts` at `dfb70175`. Ranges include only assertion callsites, not comments or loop iterations. A callsite with a loop may cover several values; the border-control buddy loop is split by control because only the drop-preview buddy needed a new owner.

| Original assertion(s) | Class | Disposition | Enforcing test after the change |
|---|---|---|---|
| 93–94, 98, 102–104, 108, 114–117, 125, 128, 130–134, 137–139, 142–150, 173–174, 186 | a | Delete factory/CMake spelling, registration counts, and obsolete discovery residue. | Actual installed effect and both plugin paths are checked by the native Nix install checks; discovery is validated by the CTest metadata validators. No spelling oracle. |
| 228–229, 232–253, 266–267, 269–271, 274–277, 280–287, 289–290, 297–305, 307, 309, 311–313, 315, 317, 319, 321, 327–329, 339, 342, 344–346, 353–355, 357–358 | a | Delete descriptor/helper/adapter source spellings, stale names and Cargo/Nix build text. | No source oracle. CTest descriptor and script-save legs exercise actual controls and persistence. |
| 386–389, 400, 404, 406, 413–414, 417–418, 434–436, 440–446, 449, 466–471, 495, 502–503, 513–519, 524–526, 533–538, 576, 578–579, 593–594, 596–597, 601–602, 857, 860–863 | a | Delete helper names, member storage, parser scaffolding, obsolete UI names, logging variable names and CTest text. | No source oracle. Call/save/retry behavior stays in native hot-apply, script config, and shortcut tests. |
| 83–85, 89, 91, 126, 178–179 | c | Delete duplicate effect and KCM identity, routing, disabled default, required metadata, and visible effect name. | `validate-metadata.cmake:25–69` independently checks exact ID, false default, route, required metadata and effect brand. |
| 109, 121 | c | Delete duplicate script KCM identity and discovery route. | `validate-scriptconfig.cmake:17–36` checks the exact script metadata route and exact embedded KPlugin ID. |
| 171, 223, 225, 227, 230–231 | c | Delete source tokens for appearance/script widgets. | `activeborderconfig_module_test.cpp:43–74,518–529` checks appearance, status and Apply controls; `scriptconfig_module_test.cpp:398–415,658–677,797–1100,1120–1176` finds and exercises the named script controls. |
| 198, 205 | c | Delete duplicate retired drag-oracle residue guard. | `validate-unified-lifecycle.cmake:33–41,140–145` independently checks both survivor source and build inputs; Nix install checks forbid the retired binary. |
| 217, 375–376, 486–487, 490–491, 507 | c | Delete duplicate KConfig group, theme, underlay color/extension defaults, and checkable controls. | `activeborderconfig_module_test.cpp:42–74,283–319,344–357,548–570,662–680` verifies named controls, actual `kwinrc` round trips and missing-key values. |
| 222, 261–262, 343 | c | Delete duplicate descriptor count, named script keys/default tokens, and tab identity. | `kdesettings_adapter_test.cpp:131–195,209–215` asserts expected count, IDs, keys and returned defaults including false; `activeborderconfig_module_test.cpp:449–490,511–512` checks rendered tab count, page and title. |
| 263, 268, 278–279, 306, 308, 310, 314, 316, 318, 320, 447–448 | c | Delete duplicate script write-key, hidden-profile preservation and changed-only gap or invalid-raw repair guards. | `scriptconfig_module_test.cpp:655–707,797–1103,1218–1287` checks saved keys, both gap values and hidden keys unchanged on staged edits. |
| 379–383 | c | Delete duplicate reconfigure service/path/interface/method/effect name. | `activeborderconfig_module_test.cpp:92–99` checks all five exact values (`native-effect-kcm-dbus`). |
| 405 | c | Delete duplicate theme alpha predicate. | `activeborderlogic_test.cpp:66–85` checks invalid, transparent and faint alpha behavior with and without theme selection. |
| 464, 481 (descriptor-managed controls) | c | Delete duplicate label association checks. | `kdesettings_adapter_test.cpp:366–383` walks every descriptor row control and requires a real `QLabel::buddy()` and its correct dialog label. |
| 481 (drop-preview color control) | b | Move to native widget owner. | `kdesettings_adapter_test.cpp:394–403`, `native-settings-descriptors`. |
| 546, 858 | c | Delete repeated no-effect UI text guard and duplicate own shortcut holder indication. | Retained original line 540 checks the former; `activeborderconfig_shortcut_test.cpp:1080–1106` requires `own [` and readable chords on actual rows. |
| 66, 70, 74, 111, 113, 180–181, 214, 216, 224, 226, 340, 377, 488–489 | b | Keep metadata/branding and exact shipped schema, descriptor numeric defaults/bounds, and parsing guards. | TS KCM suite. `contents/config/main.xml` is not in the native test source fileset; catalog/export byte parity alone would allow coordinated drift of expected defaults. |
| 369–370, 378, 384–385, 390–392, 399, 401, 410–411, 415–416, 419–430, 492–494, 499–501, 508–510, 520–523, 527–532, 539–545, 547 | b | Keep unique effect integration, fallback/underlay behavior, passive shortcut safeguards, operator diagnostics and UI copy. | TS KCM suite. Offline native test doubles do not exercise the complete live effect/theme/presentation hookups or check every operator message; no new live harness. |
| 552–556, 580–581, 598–599, 648, 655, 657, 659–663, 713, 722, 724, 727–729, 736–737, 739, 746–749, 752, 759, 764, 783, 795, 810–811, 816–818, 827, 829, 841, 854, 859 | b | Keep shortcut catalog and UI contracts, including actual TS registrations versus native row order/defaults and the documented mismatches. | TS KCM suite. No native test runner exercises TS registration; moving the cross-language parity would add infrastructure. |

KCM total: 168 class-a, 113 retained class-b, 52 class-c, and one split callsite (481: class-b move for one control, class-c duplicate for the others). The TS suite shrank from 865 to 463 lines (402 net); including the two added C++ assertions, KCM test code shrank by 400 lines net.

For the moved drop-preview label check, changing the label's buddy from the drop-preview color button to the border color button in an isolated source copy failed `native-settings-descriptors` at the new equality check. Restoring the buddy and rebuilding passed that test, the full effect-plus-settings CTest gate (38/38), and the settings-only CTest gate (27/27). KWin typecheck passed; `npm test` passed 1,485 of 1,486 tests, with the one existing skip.

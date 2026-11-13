# KDE splash fixture and capacity-spec correction

- Goal: add creation-only splash typing to the live fixture and close native
  splash classification/focus and utility/splash switcher-property gaps.
  Correct retired capacity-policy wording without changing decisions.
- Scope: safe winit X11 creation attributes, protocol enum, fixture tests and
  README; native KDE readbacks; REQ-MIN-04b/f/g and directly adjacent wording.
  No product behavior, settings, visual switcher or physical-input changes.
- Acceptance: KWin reports the fixture class and `splash:true`; core retains an
  ordinary float without classification hold/allocation, tiled neighbours unchanged; record
  observed focus and native utility/splash task/skip-switcher properties.
- Approach: use existing X11/XWayland creation route, retain honest unsupported
  reporting on native Wayland and other platforms; sequential Rust gates,
  fixture-only native checkpoints, saved raw evidence and independent audit.
- Implementation: `X11WindowType::Splash` maps to safe winit
  `WindowType::Splash` through the existing Linux-gated creation path. No new
  dependency, FFI or native Windows/macOS code changed. The protocol test covers
  accepted X11 creation, unsupported Wayland/Windows/macOS creation, and refused
  runtime changes; the mapping test covers the native splash variant.
- Spec correction: REQ-MIN-04b now always floats on geometric exhaustion;
  REQ-MIN-04f records removed capacity-overlap projection and retained strict
  geometry/safety holds; REQ-MIN-04g records both removed settings, fixed
  largest-splittable/always-float policies and unchanged Apply timing. Existing
  [D3/D9/D10 decisions](../../decisions/layout-and-admission.md#best-effort-d3-d4-no-space-result-and-focus)
  and [divergence inventory](../../decisions/tier1-divergences.md) are unchanged.
  Requirement counts remain 184 total, 183 NORMATIVE, 1 OPEN, 0 PROVISIONAL.
- Switcher interpretation: KWin `v6.8.1`
  [`Window::wantsTabFocus`](https://invent.kde.org/plasma/kwin/-/blob/v6.8.1/src/window.cpp#L661)
  requires normal/dialog/applet-popup type and input eligibility; the
  [`TabBox` filter](https://invent.kde.org/plasma/kwin/-/blob/v6.8.1/src/tabbox/tabbox.cpp#L164)
  separately checks that predicate and `!skipSwitcher()`. Therefore a false
  native `skipSwitcher` flag alone never proves inclusion. Visual switcher
  behavior is outside this check.

## Verification and native evidence (2026-11-14)

- Sequential gates passed with full logs retained externally:
  `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler -p tiler-live-fixture --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler -p tiler-live-fixture --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  `cargo test --locked -p tiler-live-fixture`.
  Shared suite: 3,400 passed, zero failed/ignored, one filtered. Fixture:
  123 passed, zero failed/ignored. The README now documents the same fixture
  test form already used by CI and earlier fixture records.
- Evidence bundle: externally retained KDE splash native evidence;
  `*-commands.jsonl` holds raw calls/replies, `*-helper.js` and
  `*-helper-receipt.json` bind each helper source/plugin/returned ID,
  `*-native.json`, `*-fixture.jsonl` and `*-presentation.json` retain exact
  sentinel/PID/UUID joins and the relevant native, fixture and core state.
- Readiness: `readiness-journal.jsonl` and `readiness-summary.txt` retain
  post-cursor current-owner `ready`, `discovered`, accepted
  `src-1-3jvss6` dispatch and matching revision-1 applied result. The
  unfiltered `readiness-kwin-scripting.jsonl` error check is empty.
  Checks used exact fixture-class scope on an appended test workspace.

| Leg | Observed evidence | Qualification |
| --- | --- | --- |
| Tiled baseline | `stage-settled`, revision 8: two normal fixtures have native frames matching their core allocations; native/core/fixture focus agrees on the second | Direct-geometry tiling, not native custom-tile association |
| Utility | `util-settled`, revision 10: native `utility:true`, `normalWindow:false`; core `floating:true`, no allocation, unchanged tiled neighbours. Utility focused in all three observers | Observed admission focus in this run, not a universal host focus guarantee |
| Splash | `pre-splash`, revision 12, then `splash-settled`, revision 14: native `splash:true`, exact `OmniTilerTestWindow` class; core ordinary float, no allocation, untouched frame/member/visibility authority and no classification/frame/member hold. Tiled neighbours retain allocations and native frames | Applied revision 14 reports `held=0`; a separate retained `above` state target has `auth:hold`, `reason:unreadable`. This is not evidence that every state lane is hold-free |
| Splash focus | `splash-settled`, `splash-after-native-activate` and `splash-after-fixture-focus`: splash stays unfocused; second tiled neighbour remains native active/core foreground/fixture focused after opening, native activation and accepted fixture `focus:true` | Accepted focus request is not focus success; no focus-policy change |
| Native switcher flags | `util-settled` and `splash-settled`: both report `skipTaskbar:false`, `skipSwitcher:false` | Native flags only; separate KWin type/input filtering above still applies. No visual switcher claim |
| Wayland unsupported | `wayland-splash-spawn`: `x11_window_type` unsupported; `wayland-splash-native` joins that fixture to a normal, non-splash window, which tiles | Only X11/XWayland supplies a real splash type |

- Lifecycle: all 12 helpers were unloaded with exact name/object absence;
  only the pre-existing script object remains after teardown. Both fixture
  shutdowns completed, with owned windows, servers, sockets and private run
  directories absent. Combined dev, planner and tray logs were saved before
  teardown. Controller/planner shutdown plus the exact foreground wrapper's
  cleanup trap removed all execution children. Three top-level command
  wrappers remained exited zombies at the recorded process checkpoint;
  a later `lead-wrapper-states.txt` check finds all three absent.
  `final-dev-status.txt` and `lead-dev-status.txt` are DOWN,
  `final-ydotoold.txt` and `final-scripting-errors.jsonl` are empty.
- Limits: Windows/macOS native builds were not run; changes outside the
  Linux-gated mapping are the additive portable enum and headless tests.
  No real media-app flags, physical shortcut delivery or visual switcher
  behavior is established.
- Independent complete-diff review and saved-evidence audit accepted all
  qualified rows with no code, protocol, spec or native-evidence defects.
  `audit-report.md` retains the review. It corrects an understated initial
  execution-report test total to the 3,400 raw-log sum above.
- The audit narrowed the separate splash `above` hold to ordinary retained
  post-apply behavior, not an admission defect: the saved dev log shows
  `property=above target=1 rev=13 setter=accepted pre=0 post=1`, then revision
  14 retains a non-writable target. `crates/tiler-core/src/world_above.rs:338-367` withholds controlled
  authority once the binding is spent or native above is already true.
  The existing [all-float persistent-above policy](../../decisions/contract.md#persistent-above-float-policy)
  applies to both types; no type-specific stacking rule was introduced.
- Outcome: creation-only splash fixture support and the requested qualified
  native checks are complete; capacity-spec rows match existing decisions.
  No product defect or failed semantic approach was found.

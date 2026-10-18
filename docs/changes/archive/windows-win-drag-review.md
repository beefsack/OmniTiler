# Windows Win+drag keep-or-drop review

- Status: complete, archived 2026-10-18. Windows Win+drag and its exclusive
  interception/state are removed; native title-bar drag remains. KDE unchanged.
- Goal/acceptance: remove the Windows modifier-drag producer and exclusive
  mouse-hook, Start-mask, queue and owner state without changing native drag,
  keyboard shortcuts, shared Escape, gaming or recovery behavior. Existing
  settings must still load. No shared-core/protocol behavior changes.
- Approach: remove producer-only code and tests, reconcile platform requirements
  and Windows gate legs, run locked Windows build/test/fmt/strict clippy and
  whitespace gates, independently review behavior preservation, then run bounded
  owned-window native-drag/Win-drag/Start smoke with exact restoration. Physical
  shortcut acceptance remains distinct from injected-input evidence.
- Bounded units: Windows implementation and deterministic gates; platform docs;
  independent diff review; scoped live verification; final outcome/archive.
- Source baseline: `9fa32420`; counts below include comments, blanks and tests,
  not executable LOC, deletion estimates or engineering-time estimates.

## Implementation evidence (2026-10-18)

- Removed the Win+drag classifier, mouse hook/trampoline, mouse input-pump leg,
  queue/snapshot/owner arm and focus state, exclusive Start masking and tests.
  No `WH_MOUSE_LL`, `MSLLHOOKSTRUCT` or mouse-hook installation remains in the
  Windows crate. Keyboard hook and chord Start masking remain.
- Windows Rust diff: 43 files, 471 lines added and 5,104 removed (net 4,633
  removed, including tests/comments/blanks); three producer-only files deleted.
- Follow-up caller audit removed the orphaned `ffi::point_root` wrapper and its
  exclusive imports; all other remaining input wrappers have shared callers.
- Retained native START capture and Escape sequence helpers in `windrag.rs`
  (the existing filename avoids unrelated module churn), native gesture
  classification and same/cross-output Engine resolver, preview, render hide
  acknowledgement and drop paths. This removal changes no shared core/protocol
  or KDE behavior.
- No persisted Win+drag setting/catalog binding existed; no schema migration
  or version bump is needed. Settings structs continue ignoring unknown fields.
- Locked four-package build and tests, formatting, strict all-target clippy,
  unsafe allowlist and whitespace gates pass. Tests: 2,844 passed, 0 failed,
  1 ignored versus baseline 2,883/0/1; 39 producer-only tests removed, none added.
- Independent full-diff review accepted native title-bar preview/drop/Escape,
  keyboard held/release/chord masking and suspend/recovery preservation; no
  blocking findings. Focused input-handoff and keyboard tests corroborated it.
- Native title-bar, gaming, recovery, inactive-resize and DPI gate obligations
  remain; only modifier-drag-specific legs are retired. No user-facing README
  or shortcut page promised Windows Win+drag.
- Removed obsolete Win-drag stages/helpers from `windows-mouse-drag.ps1`
  (30 added, 951 removed); six native title stages and shared preview/cleanup
  helpers remain. Its mock contract passes. Focused shortcut/input suites:
  140 passed, 0 failed.
- Initial fast and slow/dwell native smoke attempts failed the old preview-log
  oracle. Initial preflight captured only a settings hash/taskbar visibility,
  not full restorable preimages; these attempts are not acceptance evidence.
- Baseline discriminator: clean detached `5950b795` fails the original oracle
  identically, despite valid `unchanged` rows carrying mover, rect, tick and
  prior. The renderer contract permits these steady-state resolutions. The
  corrected oracle accepts them while retaining visible-carrier, mover/tick,
  prior and preview/drop rectangle equality checks; hidden-preview assertions
  reject them symmetrically. The corrected baseline completes native drop,
  preview agreement, topology change and plan/readback checks. No pre-existing
  native-preview product defect was established.
- Final source/helper and harness review passes; its one stale hook-count
  comment finding was corrected. Native stages, shared preview/cancel/recovery
  checks and keyboard shortcut paths remain; the harness mock passes.
- The corrected removal build also passes: native `drag-drop-applied`, visible
  preview-to-Engine and preview-to-drop rect/prior agreement, actual plan/frame
  match and topology change. Win+client drag leaves frames unchanged with no
  gesture or preview resolution. Synthetic bare Win opens Start and an
  owned-window click dismisses it; Start is closed at cleanup.
- Fixture coverage initially blocked the corrected removal journey. The final
  run temporarily raises only exact-owned tagged helpers, restores their normal
  band before owner launch, and verifies foreground/root-at-point before input.
  Earlier pre-guard fallback clicks hit Terminal without injected keys;
  Terminal remained running. Those attempts are not acceptance evidence and were not
  repeated after coverage guards were installed. No unrelated window is moved,
  restyled, minimized or closed in the accepted journey.
- Corrected journeys capture settings bytes to a restorable copy, taskbar
  auto-hide state and raw SPI before effects. Final cleanup verifies exact-owner
  stop/restore, helpers closed, no actors/overlays and a clean ledger; executes
  `mise exec -- just stop`, then repeats restore/ledger checks and byte-exact
  settings restore/readback plus taskbar/SPI preimage comparisons. Earlier
  skipped/probe-only final-stop attempts are superseded by this exit-zero run.
- Independent final code/harness and live-evidence reviews accept the result;
  no remaining removal blocker. Physical shortcut delivery and native feel,
  gaming and multi-output/DPI acceptance remain user-owned, not inferred from
  injected input. Pixel blending is not claimed for the moving native source.
- Scope choice: retain the existing native-helper module name and shared
  preview/drop/fence interfaces rather than refactor them. A stale future
  Win+drag comment in shared `visual.rs` is outside this Windows-only change;
  no shared runtime correction is needed.
- Outcome: removal complete; the queued removal item is closed. No native
  preview product defect was established by the baseline discriminator. Next
  implementation action: none; physical shortcut acceptance remains pending.

## Historical review (before the removal decision)

The remaining sections preserve the pre-decision analysis, not current requirements.

## Historical behavior and implementation cost

- Either Win key + left-button press on an eligible managed tile can start from
  its client area, including an unfocused tile; there is no extra-modifier gate.
  The initiating Down and paired Up are consumed; motion passes to Windows.
  Floating/sticky/unmanaged subjects pass through; published origins also exclude
  maximized/fullscreen members and floating workspaces. The real source and
  siblings stay put while the target-slot preview moves. Release uses the shared
  Engine drop, not a native move loop ([classifier][mouse],
  [origin publication][origins], [preview][preview]).
- Current focus is **at press**, not at drop: fresh identity/token/member-tag
  checks precede existing focus authority; already-foreground skips the setter;
  suspend/elevated-foreground fences refuse the arm. No-move/Esc retain that
  press focus without a drop plan. A failed focus attempt may have primed/sent a
  setter before refusal, so it is not a promise of zero focus effects
  ([owner arm][arm], [press evidence][press]).
- Esc, zero movement, self/unsupported-centre/outside targets and invalidation
  cancel/refuse rather than reorganize. Pointer-at-release and Esc sequence are
  callback-bound; same-process HWND reuse needs member-lifetime proof. Capacity
  loss must fail closed while retaining the consumed-click Up obligation
  ([mouse], [settle], [arm]).

| Footprint at baseline | Whole-file lines | Interpretation |
| --- | ---: | --- |
| `crates/tiler-windows/src/win_mouse.rs` | 1,323 | Classifier, bounded queue, hook policy/installation and embedded tests |
| `crates/tiler-windows/src/tiling_sys/windrag.rs` | 357 | Arm/focus/terminal cleanup; also shared START/Esc helpers used by native drag |
| `crates/tiler-windows/src/tiling_sys/tests_windrag.rs` | 477 | Owner refusal, lifetime and scoped cleanup fixtures |
| `crates/tiler-windows/src/snapkey/tests_windrag_mask.rs` | 81 | Keyboard Start-mask/both-Win/synthetic-state fixtures |

- About **1,680 lines in the two primary implementation files**, plus **558
  separate test lines** (2,238 total file footprint). Embedded tests are already
  included. This is not 2,238 removable lines: shared START/Esc helpers must
  survive. Conversely, the total excludes distributed integration and shared
  tests. Complexity is lifecycle/cross-thread coupling, not new drop geometry.
- Input thread owns both low-level hooks and their message pump. Mouse callbacks
  do Win-held/root-at-point reads, eligibility lookup, bounded Down/Up/Cancel
  queueing and coalesced motion; no Engine, geometry or log I/O. Queue capacity
  is 64, owner dispatch cap eight per iteration. Keyboard installation and
  current snapshot policy gate fresh Downs; terminal edges remain paired across
  cancellation/suspension ([mouse], [input backend][input]).
- Owner thread publishes eligible identity snapshots, drains/validates arms,
  focuses and settles; render-owned carrier displays the shared read-only Engine
  preview. Start suppression couples mouse consumption to the keyboard reserve
  and an E8 `SendInput` pair on physical Win release. Existing focus actuation
  can wait up to 500ms on a miss; recorded 6-10ms press samples are observations,
  not a bound, and do not run in the hook ([arm], [mask], [press], [preview]).
- Unsafe boundary: business modules forbid unsafe code; `ffi/llhook.rs` owns
  `SetWindowsHookExW(WH_MOUSE_LL)`, raw `MSLLHOOKSTRUCT` copying, consume/pass
  trampoline and explicit unhook/retry. Panics pass through instead of unwinding
  across FFI. Keeping this producer keeps an additional system-wide mouse-hook
  surface, alongside the still-needed keyboard hook ([FFI][ffi], [input]).
- Removal is not deleting one module: `input_handoff.rs`, `input_pump/`,
  `snapkey/sys/`, `tiling_sys/run_loop_intake.rs`, `focus.rs`, gesture maps and
  producer branches in `gesture_tick.rs`, `drag_preview.rs` and `cross_drag.rs`
  participate. Preserve native START/END, shared Esc, preview, Engine resolver,
  render clear-before-drop, focus/recovery and Mouse Snap handling. These are
  shared dependencies, not all Win+drag-owned costs ([input], [origins], [settle],
  [preview], [native][native], [settings][settings]).

## What title-bar drag already covers; what removal loses

- Windows already observes native `EVENT_SYSTEM_MOVESIZESTART/END`, captures
  release pointer/Esc at the callback, and classifies move versus resize from
  actual frames. Native title-bar movement follows the pointer; both producers
  feed the same target-slot preview/Engine drop with retained sibling layout,
  cancellation and refusal handling ([native], [settle], [preview]).
- Same-output title-bar drop/cancel/zero/self/centre/outside and sibling-freeze
  native readbacks were accepted in the original [drag delivery][delivery].
  Both producers also have occupied/empty cross-output routing and fixtures;
  physical multi-output/mixed-DPI acceptance remains pending ([foundation]).
- Dropping loses **client-area Win+left reordering without finding a title bar**,
  the stationary-source/moving-preview presentation, and its explicit press-focus
  contract. A window with no usable native title-bar move region loses this
  particular initiation route; actual custom-frame coverage is still unproved.
  It does not remove target-slot reordering, nested/group-edge placement,
  cancellation, cross-output drop implementation, or native float/resize paths.
  Title-bar input is not identical UX, even when the final drop is identical
  ([mouse], [preview], [foundation], [delivery]).
- KDE's selected reference is host-native Meta+drag with pointer-following frame
  and target-slot preview, not a custom stationary Windows producer. KWin script
  connects native Started/Stepped/Finished and sends validated pointer-domain
  drops to the Engine ([KDE entry][kde-entry], [KDE adapter][kde-adapter]). Exact
  KDE press-focus timing and physical preview/no-jump pause remain TBD in
  [R-DRAG-07/08][reference]; do not turn selected policy into observed evidence.
- macOS explicitly will not add modifier+drag: the approved 2026-10-17 direction
  is passively observed native title-bar drag, shared insertion resolver/preview/
  Escape, with the earlier modifier implementation diagnostic-only until replaced.
  That decision explicitly leaves Windows unchanged pending this review
  ([platform decision][macos]).

## Evidence, defects and ongoing risk

- Deterministic coverage includes queue saturation/terminal delivery,
  identity/reuse, arm/refusal/cleanup, pointer settle, keyboard mask, configuration
  races and pump install/retry/stop seams: [mouse], [owner tests][owner-tests],
  [mask tests][mask-tests], `input_pump/tests.rs`,
  `input_pump/native/core_tests.rs`, `snapkey/sys/sys_tests.rs`.
- Historical failures were corrected, not current open defects: click passthrough,
  late identity, late release-pointer capture, ineffective proof oracle and a
  duplicate arm. Attempts to delegate through SC_MOVE/non-client messages failed
  and were abandoned; native-loop delegation is not an established cheap option
  ([delivery], review and accepted Win-slice sections).
- Accepted bounded synthetic/native journeys establish stationary hold,
  preview/drop agreement, unfocused press/no-move/Esc focus and clean teardown.
  The press-focus record's optional title-bar whole-suite oracle failed; accepted
  primary Win journeys do not imply that suite passed. The 2026-10-17 offline
  revalidation is not a new desktop run ([press]).
- Still pending: physical click/Start containment for both Win keys, fast Esc,
  preheld/modifier ordering and feel, custom frames, inactive resize/float/sticky
  behavior, cancellation/suspend/stop races, hook coexistence, mixed-DPI/output
  journeys and visual pixels after thread refactoring. Owner-threading burst/idle
  success is not those drag journeys ([delivery], [press], [thread acceptance][threads]).
- Hook-chain consumption and Start-mask reentry/pairing are maintenance risks
  supported by the implementation, not proof of a currently reproduced third-party
  hook conflict. Keeping the producer extends every input-pump/lifetime/configuration
  change's regression matrix. There is no dedicated Win+drag setting in the
  reviewed implementation; keyboard takeover and Mouse Snap prevention are
  existing shared controls, not a default-off feature switch ([mouse], [input],
  [mask], [settings]).
- Gaming/Xbox: dropping the mouse producer removes one interception/Win-mask path,
  **not** keyboard shortcuts, fullscreen ownership, suspension, hidden-window
  recovery or the Win+G/F11 collision problem. [Gaming records][gaming] document
  a fullscreen-key incident and unresolved Xbox-return visibility causes, not a
  causal Win+drag defect. Compatible rebinds need physical acceptance; active/
  suspended incremental game cost (M16) remains unmeasured. Neither keeping nor
  dropping establishes game/anti-cheat compatibility ([gaming], [threads], [gate]).

## 0.1 gate impact (proposal, not an exception)

| Proposed Windows check | Keeping, including optional/default-off | If removal is approved |
| --- | --- | --- |
| 6, held-key/release | Preserve shared keyboard holds plus Win-drag mask/pairing coverage | Retain keyboard repeat/release checks; retire only Win-drag-specific pairing/mask cases |
| 17, must-pass R-DRAG-08 | Retain press-focus physical/native-edge acceptance when enabled | Explicitly amend Windows modifier-press assertion; retain native title-bar and inactive-resize noninterference, not a false Win+drag pass |
| 25/26, same-output/cancel edges; 47, cross-output | Cover both delivered producers; optional mode still needs enabled-path safety | Remove Win-only legs; retain native drop/preview/cancel/source-refill/refusal assertions |
| 23, drag visuals; 28/N1, failure/unavailable visuals; WX3, DPI | Include enabled Win-preview and terminal/teardown cases where relevant | Retain shared renderer/native-preview/failure/DPI assertions; do not waive whole checks |
| 34/N3/WX1, gaming/input; WX7, recovery | Mouse-hook cost/coexistence and keyboard/owner safety remain | Narrow producer-specific matrix only; game cost, Xbox routing/return and hidden-window recovery remain |

- [Windows 0.1 gate proposal][gate] currently requires both delivered producers
  and expressly says removal is only a discussion. Dropping/default-off requires
  an explicit user decision and later reconciliation of REQ-DRAG-03/07/08,
  R-DRAG-03/07/08 and gate wording. No gate count/classification is changed here.

## Options and recommendation for discussion

| Option | Benefit | Cost / risk |
| --- | --- | --- |
| Keep as-is | No capability loss or new setting; delivered familiarity | Full hook/lifetime/threading/physical matrix for a non-native input idiom |
| Keep behind a setting, default-off | Native default; opt-in client-area/stationary drag survives | New catalog/persistence/UI/snapshot/install/disable-during-hold semantics and tests; enabled path keeps maintenance/gate burden. Merely ignoring Downs while leaving WH_MOUSE_LL installed does not remove hook exposure |
| Drop Win+drag, retain title-bar | Removes dedicated mouse interception, paired-click/Start-mask coupling and producer-only branches/tests; aligns default initiation with macOS direction | Loses client-area/stationary-source UX; requires careful shared-path preservation, explicit spec/gate amendment and title-bar regression acceptance |

- Recommendation: **drop**, unless the Windows owner identifies client-area
  initiation or stationary-source presentation as an important supported use
  case. The shared drop capability survives; default-off adds product surface
  without retiring most maintenance. This is a judgment from the footprint and
  retained alternative, not measured user demand, a promised performance gain
  or a claim that Win+drag is currently broken.
- Tentative analytical choices only: whole-file footprint rather than a deletion
  estimate; separate producer safety from shared drag/game/recovery acceptance;
  treat historical native/synthetic evidence as scoped, not physical proof.
- Verification/outcome: source/record and line-count review, relative-link and
  whitespace checks; documentation only, no new code tests or live evidence.
  This record remains active for discussion; no product decision is promoted.
- Exact next action: Windows owner/user decides keep, default-off or drop.
  Only after that decision reconcile the affected requirements/gate legs and
  scope implementation plus retained title-bar regression acceptance.

[mouse]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/win_mouse.rs
[arm]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/windrag.rs
[origins]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/focus.rs
[input]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/input_pump/native/mod.rs
[mask]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/snapkey/sys/mod.rs
[ffi]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/ffi/llhook.rs
[native]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/observe.rs
[settle]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/gesture_tick.rs
[preview]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/drag_preview.rs
[settings]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/settings.rs
[owner-tests]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/tiling_sys/tests_windrag.rs
[mask-tests]: https://github.com/beefsack/OmniTiler/blob/9fa32420/crates/tiler-windows/src/snapkey/tests_windrag_mask.rs
[kde-entry]: ../../../kwin/src/plan-adapter-entry.ts
[kde-adapter]: ../../../kwin/src/plan-adapter.ts
[reference]: ../../spec/reference-outcomes/mouse.md#r-drag-07-dragged-frame-vs-retained-allocation
[macos]: ../../decisions.md#scope-and-platform-goals
[delivery]: windows-mouse-drag.md
[press]: windows-drag-press-focus.md
[threads]: windows-owner-threading-acceptance.md
[foundation]: ../windows-multi-output-foundation.md
[gaming]: ../windows-gaming-coexistence.md
[gate]: ../windows-0.1-gate-proposal.md

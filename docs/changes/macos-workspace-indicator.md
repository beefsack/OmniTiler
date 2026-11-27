# macOS workspace indicator

## Goal and acceptance

Add a small, event-driven menu-bar indicator to the bounded macOS tiling preview.
Show native workspace numbers in native order, distinguish current and occupied
workspaces, and select existing workspaces through the existing guarded Mission
Control AX selector. Reuse retained off-current membership and layouts; do not
introduce a second workspace manager, idle polling, focus-taking UI, or overlays.

Acceptance: offline projection/config/lifecycle tests; live appearance/current,
Space-switch update, off-current occupancy, exact-hit-tested click selection and
teardown; verified restoration of the original one-Space desktop. macOS tests,
strict clippy, format, signed build/verify and hosted CI must pass. Physical
appearance, fullscreen/game behavior and unavailable capture remain explicit
acceptance limits rather than inferred results.

## Selected approach

- One native status item shows native Space numbers in per-display groups,
  ordered left to right with a separator between groups. Each group has its
  own current marker. Retain the core-designated spare; a native unbound
  spare is disabled when several displays are attached.
- The status button shows one image containing numbers, occupancy, miniature
  bands and optional representative app icons. Left-click selects an eligible
  number directly; disabled and unknown painted slots do nothing.
  Right-click or Control-click opens the Tiled toggle and mirrored
  workspace menu. The button describes the groups to accessibility clients,
  and the native menu mirrors the available selection actions.
- Use core pager rows with proven native output attribution, never pointer
  or focus guesses. Unknown or held state is group-local. Numbers beyond 9
  remain visible but disabled. Native fullscreen and games receive no extra
  window or input layer.
- Refresh from existing observation/engine events on AppKit's main thread.
  Selection reuses fresh session/topology gates and verified arrival.
- Coalesce screen-parameter notifications at a 50 ms trailing edge,
  then recheck once after one normal 250 ms cadence interval. The live
  native row determines which retained Space bindings are still on that
  display. Vanished, rehomed and orphan unbound rows drop out; surviving
  core identities and ordinals are not rewritten.

Offer current-only mode for space-constrained menu bars. Representative
application icons remain optional and compact; the item does not add a
desktop overlay.

## Bounded work and dependencies

1. Research AeroSpace, cloned bars, Windows/KDE semantics and macOS seams.
2. Implement projection, settings, native UI, guarded dispatch and offline tests.
3. Independently review native lifecycle/input and selection boundaries.
4. Run signed, bounded live proofs; restore and verify all owned effects.
5. Verify final gates/CI, record differences and physical appearance procedure.

## Research checkpoint

AeroSpace refreshes its tray model with layout events; its ordered i3 style
shows nonempty-or-visible workspaces and dims inactive entries. Its experimental
style menu is persisted in UserDefaults, not TOML. No SketchyBar clone was found.
Windows has per-output taskbar cells with miniature windows/icons and a
focused-output tray menu; KDE relies on Plasma's pager rather than a bespoke
strip. Both other adapters have fuller workspace lifecycles than macOS's
select-only preview. Existing objc2 AppKit bindings can host a status item without
a new crate. The exact occupancy and application-icon projection, unsafe baseline
and multi-display rendering rule were resolved by implementation inspection.

The click carrier follows the single-image and temporary-menu lifecycle in
WhichSpace v1.5.0 (`WhichSpace/AppDelegate.swift`, `statusItemLocation(in:)`
and `menuDidClose`). On macOS 27, status-item events may come from another
process: hit-test the screen pointer converted through the button window,
and read the global modifier state as well as the event flags. AltTab
v11.10.2 (`src/Menubar.swift:238–240`) documents the layout-recursion risk
of status-button subviews.

WhichSpace v1.5.0 (`WhichSpace/SpaceUpdateCoordinator.swift:23–39,50–66`)
coalesces topology notifications at a 50 ms trailing edge. yabai v7.1.25
(`src/space_manager.c:1143–1193`) carries existing views across SID changes
using Space UUIDs; rift v0.8.6 (`src/actor/spaces.rs:762–823`) only remaps a
unique same-display replacement. The observed returning group already
adopted its new SID and selected correctly. The phantom rows were on the
surviving display, so the indicator fix does not add a Space UUID cache or
infer a replacement from order.

## Implementation and review checkpoint

The indicator uses retained engine-domain membership, including hidden domains,
with revision-fenced unknown fallback. Optional icons use a deterministic member
application (focused member preferred); missing artwork leaves the number intact.
Settings are `workspace_indicator.mode` (`all`/`current`/`off`) and
`workspace_indicator.icons` (false by default), with CLI overrides.

Clicks carry immutable SID, UI generation and topology revision through
admission and native drain. The directly painted tiles use the same slot
geometry for drawing and hit testing. Offline regressions cover pre-click
rebuild and post-admission reorder refusals.
The independent global occupancy scanner was removed from production; a read-only
diagnostic scanner is explicitly not retained-state evidence. Live appearance,
selection and teardown still require a bounded fixture and independent review.

Current styling: bordered/bold current number; occupied bold at full alpha; empty
regular and dimmed; unknown has an explicit `?` suffix and tooltip.
Application icons are optional alongside, never instead of, numbers. These
appearance choices remain tentative pending physical review.

## Live checkpoint

The first signed live proof stopped at retained occupancy before rendering or
synthetic input: the scoped observer published zero admitted windows and one
classifier omission for the owned helper throughout the bounded wait. A separate
main-thread inspection classified an equivalently launched helper as tileable;
the exclusion mechanism is not yet established. No click, Space-switch update,
appearance or teardown acceptance is claimed.

Cleanup independently read back the original one-Space/current layout and removed
the temporary Space; helper, runtime and display-sleep companion stopped. The
protected-window snapshot comparison failed despite equal counts, so exact
protected-state restoration is not accepted until that discrepancy is explained.
Further live mutation is blocked on read-only diagnosis of the discrepancy and
the worker-path exclusion. Capture and physical appearance remain pending.

### Blocker diagnosis and continuation

The protected-window snapshots were already sorted, so ordering does not explain
the mismatch. The failed fixture compared the full records, including transient
`onscreen`, immediately after Mission Control cleanup, unlike the established
settled frame/membership gate. Unfortunately its failure JSON retained only counts,
not the before/after records. A transient visibility/readback difference and a
real protected-state change cannot be distinguished retrospectively. Do not claim
exact restoration or repeat the live proof on this evidence.

The classifier's omission reason was also absent from the retained logs. A
behavior-preserving observability addition now carries seven bounded exclusion
tokens from the scoped backend through the runtime; aggregate counts and snapshot
admission remain unchanged. Offline regressions cover unknown-subrole exclusion
and count consistency. No speculative classifier eligibility change was made.

Final local evidence: **842 tests** (708 library plus all integration suites),
strict all-target macOS clippy and workspace format pass. Signed app rebuild and
separate verification pass after the logging-only addition; final offline audit
found no additional serious issue. Main artifact SHA-256:
`ae9899dd34981789ae2fbc0aee01825f12f3b4f5073e2c1a64e6fe52d2521d9f`.
Unsafe blocks are 111 → 114 and unsafe impls
7 → 8, confined to `ffi.rs`; no new crate or shared-core change. No hosted CI or
publication claim is made for the uncommitted indicator.

Next: obtain read-only protected-state reverification and a decision on the
unrecoverable historical baseline gap. Preserve raw vectors in any revised
fixture and use the established bounded exact frame/membership settlement gate;
do not drop protected identities or loosen geometry tolerances. Only then name
the actual scoped exclusion with the new tokens, repair the proven cause, and
repeat the full indicator proof. Screenshot grant changes remain out of scope.

### User quick physical check (2026-10-17)

In an unscoped dogfood run on a single-Space desktop the indicator appeared and
appeared to work, showing a single `1`. No trailing empty workspace is shown,
as expected: trailing lifecycle is not implemented on macOS. Click-to-select,
Space-switch updates and teardown remain unaccepted.

### Two extended displays (2026-11-27, before the click-model fix)

In signed fixture-only runs with separate Spaces and SIP on, both groups
painted their own current markers, in arrangement order, with a separator
and occupied miniature bands. AXPress and six physical clicks on *menu
mirror items* selected the exact requested display's Space without changing
the other display's current. Session-only rearrangement reversed the groups
and restored their original order; selection still used the correct group.
Native fullscreen did not get an extra bar; the tracked fullscreen group
showed unknown markers and recovered exact highlights after exit. Native
recent-use reordering was reflected in the indicator.

Disconnect removed the departed group and separator. Reconnect restored
that group and its exact current; mirror selection still targeted it even
though a native Space ID changed. However, three unknown rows stayed on the
remaining display after its native list had already shrunk. During settle,
ambiguous attribution temporarily hid the item as intended. The runtime had
already adopted the restored native inventory while the affected pager rows
persisted, so a missed notification alone cannot explain this failure.
An enabled trailing unknown row was also observed before disconnect. The
grouped projector disabled the unbound spare, but AppKit's default automatic
menu validation disregarded the explicitly disabled item state.
Direct number clicks failed: AppKit's permanently attached menu intercepted
the status-button click before its number subviews could act.

Open under multi-display scoping: a fixture window crossed displays during
the arrangement change (native clamping versus stale placement is not
established), and the SIP-on Mission Control/Dock switch took roughly
1.0–1.1 seconds. Neither is addressed by the indicator fix.

### Signed check after the single-image change

With two extended displays and a scoped, signed fixture window, the menu-bar
capture showed two groups, two current outlines, one separator and a
miniature under the occupied current Space. Accessibility named the same
groups and identified the trailing `5?` as unavailable. Inline AppKit menu
presentation held the AXPress request open and blocked bounded shutdown.
Menu presentation is deferred to the main run loop so the request can return
before menu tracking; the menu still detaches only when tracking closes.

On the deferred build, AXPress opened the status menu without timing out.
The native menu reported the trailing `5?` disabled. Pressing one workspace
mirror in each display group changed only that display's current Space.
Session-only rearrangement reversed the groups in the capture and AX
description; a third mirror press selected the reordered group without
changing the other display. Reverting the arrangement restored the
original group order. The run reported clean shutdown and zero topology
failures. Native readback confirmed both starting currents and the
original arrangement; the fixture, product process and socket were gone.

### Bounded physical session

Before input, the indicator showed the native three- and four-Space groups
plus the disabled spare. In the 120-second fixture run, four indicator
selections reached the primary display through the existing pinned route.
Two changed its current Space (1,165 ms and 1,013 ms) without changing the
external display; two selected the already-current Space and made no
change. No external selection, visible context menu or disconnect was
captured in this run. The last settled AX counts still matched the native
counts plus the disabled spare, but this was not a reconnect readback and
there was no post-reconnect capture. Both starting currents and the
arrangement were restored; the fixture, runtime and socket were gone.
Those first clicks were navigation to recover the terminal, not the
physical acceptance procedure. They do not establish a wrong-group hit.

The instrumented rerun confirmed direct left-clicks on workspace 1 in
each display's own menu bar. Click coordinates resolved to that display's
group and workspace; only its current changed. Observed switches took
1,024 ms and 1,060 ms. Control-click opened the context menu; unmodified
right-click was not physically tested.

Unplug collapsed the display groups. After replug, the capture and AX
readback agreed with the native inventories: three and four desktops, plus
one disabled trailing spare (`7?`) on the four-desktop group. The spare was
the only unknown row; no stale native rows remained. Shutdown was clean
with no topology failures, the original arrangement and primary current
were restored, and the fixture, runtime and socket were gone. The former
external current's native ID disappeared on disconnect. Exact-ID
restoration refused rather than selecting a guessed replacement; the
returning display was already on its replacement first desktop.

## Explicit platform differences and remaining limits

- One native status item, not Windows' per-output external taskbar overlay or
  Plasma's native pager. The item can be crowded out by the native menu bar.
- Retained managed-window occupancy is not a global application census;
  unknown membership is visibly `?`, not empty. Off-current domains remain
  retained. Without a coherent core pager reply the item deliberately hides
  rather than fabricating rows. In the fixture leg, it first appeared once
  a managed window seeded a coherent reply; an empty scope is not itself a
  separate visibility rule.
- The [grouped native selector](macos-per-output-indicator.md) validates the
  target output and observes exact arrival with the other currents unchanged.
  Keyboard, transfer, and trailing creation do not gain multi-display scope.
- Icons default off, one deterministic representative app per workspace rather
  than Windows' per-window miniature artwork. Missing artwork preserves numbers;
  refresh follows existing observations, not Windows' timed artwork refresh.
- Preview-lifetime native UI/settings, not Windows' persistent settings/hot-apply
  surface or Plasma configuration. `all`, `current`, `off` and icon CLI/file
  overrides are the current configuration surface.
- No new overlay or input hook; native menu-bar/fullscreen behavior, game safety,
  crowding, icon recognizability and physical focus/appearance remain unaccepted.

Both new source modules exceed the production soft size cap (the proof is roughly
3,000 lines); split by responsibility before treating the diagnostic as settled
infrastructure. This is a maintainability follow-up, not a reason to relax the
blocked live evidence gates.

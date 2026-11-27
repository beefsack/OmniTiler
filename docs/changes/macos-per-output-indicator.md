# macOS shared grouped workspace indicator

## Goal and acceptance

Render every physical output's core pager in one native menu-bar item and
prove under SIP that each click switches exactly the clicked group's native
Space on that output alone. This two-display proof blocks 0.1 separately
from single-display MX6. Keep the core-designated trailing row and existing
selection authority. No virtual workspace implementation, reduced SIP,
pointer warp, inferred occupancy/current, or desktop strip. Unknown or held
state is group-local and does not create click authority; ambiguous
output-to-Space mapping remains safe-hide, not acceptance.

## Design discriminator

Windows uses one taskbar overlay per output, rather than its focused-output
tray stopgap; KDE keeps each pager in core's logical order. A replicated or
pointer/focus-driven macOS pager that swaps in only one output's rows
would make a display's pager depend on focus or pointer position. The
shared item instead represents all outputs simultaneously. See [Windows settings](../decisions/settings.md#windows-taskbar-pager-placement-and-geometry)
and [KDE pager backing](../decisions/workspaces.md#dynamic-workspace-lifecycle-and-kde-pager-backing).

The current `NSStatusBar.systemStatusBar()` is system-managed. The public
AppKit SDK (`NSStatusBar.h`, `NSStatusItem.h`) exposes `systemStatusBar` and
`statusItemWithLength:`, but no screen argument or per-replica content or
placement control. Multiple items would still be system-placed; a status
button window's `screen` is a read-only observation, not an assignment API.
An app-owned `NSPanel` with `NSWindowStyleMaskNonactivatingPanel` can instead
be placed in a specific `NSScreen`'s coordinates using `NSWindow` geometry.
It is a separate window, not a menu-bar slot. It cannot assume reserved room
in the native menu bar or that macOS will hide it with the menu bar.

The [tagged prior art](macos-0.1-native-spaces-prior-art.md#multi-display-indicator)
separates placement from selection. SketchyBar v2.24.1 (`src/window.c`) and
spacebar v1.4.0 (`src/bar.c:543–558`) draw their own display-local bars via
SkyLight. SketchyBar's `src/display.c:240–266` documents why `visibleFrame`
alone is not a safe secondary-bar inset on Tahoe and uses private menu-bar
queries. WhichSpace v1.5.0 (`README.md:71`, `WhichSpace/AppDelegate.swift:416`)
retains one mirrored status item. Hammerspoon 1.1.1
(`extensions/spaces/spaces.lua:740–800`) targets Mission Control's AX
display group, but does not prove exact arrival. OmniTiler's original
single-display projection and product drain refused multi-row scope.
Core pager rows already carry output/workspace identity. The existing
[Mission Control primitive](../../crates/tiler-macos/src/mc_lifecycle.rs#L842-L913)
can locate a unique SID and press in its exact `AXDisplayID` group after a
fresh SID/CG display and list-count check. It does not itself prove that a
product click still names that output or that its arrival leaves other rows
unchanged. Public placement alone does not remove either requirement.

## Selected design and implementation scope

On 2026-11-27 the user selected one shared `NSStatusItem` and explicitly
rejected an app-owned nonactivating strip and the desktop intrusion it would
create. This is a [cross-platform consistency exception](../decisions/settings.md#macos-workspace-indicator):
Windows and KDE keep per-output pagers; public AppKit cannot bind the native
item to one output. The cost is width: on a notched display, macOS can hide
an item that does not fit without offering overflow. No compact mode in
this change.

The shared presentation applies in both [workspace modes](macos-virtualised-workspaces-mode.md).
This implementation covers native Spaces, the only mode shipping in 0.1.
Virtualised workspaces become the fresh-install default after 0.1 only once
the [safety gates](../decisions/platforms.md#macos-01-release-scope) pass.
Their logical-workspace click routing is implemented and proven separately;
native Spaces become opt-in with documented limitations.

Groups are ordered by global frame x, with the primary first on a tie.
Each group has its own current highlight; separators appear only between
groups. The single-display rendering and existing number/spare rules are
unchanged. Join each core row to its exact native output and revalidate
clicks against current identity and topology before selecting. Verify the
target SID on that output and that other outputs remain unchanged. Never
infer targeting from pointer or focus, fall back to another display, or
replay when attribution becomes ambiguous.

The initial pin in the diagnostic selector left the product owner refusing
every multi-display claim. The product now binds the clicked core output
and workspace in the serialized owner and presses the exact Mission
Control display group. It rechecks full topology before pressing and
requires exact arrival with every other current unchanged. The global
Dock swipe and single-display MX6, keyboard and transfer routes are
unchanged. A native unbound spare renders disabled with several displays;
this change does not add multi-display Space creation or a tiling-toggle
target. A known group held behind stale current evidence shows unknown,
not a guessed highlight or stale click authority.

## Verification

- The separator test was red before implementation (four columns, five
  expected) and passed with the grouped carrier. Production snapshot and
  context tests cover independent currents, output pinning, order and
  primary tie-break, regrouping, held and rehomed rows, single-display
  layout and ambiguous attribution. Locked macOS tests passed: 1,789
  library (three ignored) and 152 integration. Scoped formatting and
  whitespace checks passed. The dependency-inclusive strict Clippy run
  stopped on the nine known Windows log-sink diagnostics. With `--no-deps`
  it instead stopped on a pre-existing `type_complexity` error in
  `fixture_restart/tests.rs:819`; no changed Rust file emitted a diagnostic.
- The signed single-display fixture completed occupied → empty →
  occupied (3 → 4 → 3). Fresh native readbacks matched indicator rebuilds
  at generations 2 → 3 → 4: four rows, one occupied, zero unknown.
  The controller restored the starting Space and original topology,
  reaped the product and left no recovery receipt or residue. This
  matches the [pre-change baseline](macos-core-trailing-pager.md#signed-fixture-retest-2026-11-27)
  for native switching and rebuilds. No physical click or painted pixels
  were observed; the existing fixture supplies no verifiable status-item
  capture target. Physical click and appearance remain open for MX6.

## Bounded physical check (after implementation)

User-run two-display acceptance is still required.

Build and verify the signed product and fixture using the
[live macOS guide](../live-macos-testing.md#fixture-first-live-tests). After
the fixture starts and the owned desktops are ready, launch the bounded
product from the repository root:

```sh
mise exec -- just tile --user-start --scope io.github.omnitiler.live-fixture --input off --workspace-indicator all --seconds 120
```

Let the product stop on its own; use another bounded session for a later
leg if needed. Do not extend the leg to ordinary application windows.

1. Use two extended physical displays (A and B), SIP enabled, and an already
   enabled “Displays have separate Spaces” setting. Record the display/Space
   arrangement and current Space on each before making changes. Follow the
   [signed fixture procedure](../live-macos-testing.md#fixture-first-live-tests):
   signed fixture only, `io.github.omnitiler.live-fixture` scope, and test only
   on separately identified run-owned desktops. Give A and B unequal current
   ordinals with at least one alternate owned desktop per display; leave an
   alternate on B empty. Verify one menu-bar item, groups in left-to-right
   arrangement order with the primary first in a tie, a separator between
   them, and both exact current Spaces painted at once.
2. Focus a fixture window on A and, without first activating B,
   click B's alternate workspace in the B group of the shared item.
   Verify independently in Mission Control that the expected desktop on B
   became current and A's desktop did not change. Compare the painted
   markers, including on the empty destination. Repeat in reverse (focus
   on B, click A). A changed highlight alone or landing on a same-numbered
   desktop on the wrong output is a failure.
   Read-only corroboration is available with
   `mise exec -- cargo run --locked -p tiler-macos -- mc-inspect`:
   compare `layout.current` before and after. It is a sorted set, not a
   display map; it cannot replace the per-display Mission Control check.
3. With the pager idle, disconnect B, allow topology to settle, and reconnect
   it. Once rehome settles, only A's group must remain, without a separator
   or a B target. After reconnect, verify the returning group and then
   change display arrangement; the groups must reorder and use fresh
   targets. Repeat one click and check both native currents again.
   While topology is unproven, no stale target may be actionable. If no
   ambiguous transient is visible, rely on the offline refusal regression.
4. In a separate user-run fullscreen check with an owned fixture window,
   verify the native item does not intrude on the fullscreen surface;
   after exit verify fresh current markers and no stale click targets.
   Check native menu usability and note if crowding hides the status item.
   This does not replace separate real-game acceptance.
5. Stop the scoped owner and fixture, restore the arrangement and the
   starting desktop on each output, and remove only the owned test desktops.
   Verify no pager, owned window, helper, or recovery residue remains. Report
   both outputs' before/after desktop identities for each click, the painted
   results and any failed leg; stop on scope, permission or restore failure.

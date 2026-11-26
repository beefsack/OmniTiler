# macOS per-output workspace indicator

## Goal and acceptance

Render each physical output's core pager and independently prove that a click
switches only that output's intended native Space with SIP enabled. This is a
0.1 consistency blocker, separate from the single-display MX6 journey. Keep the
core-designated trailing row and existing selection authority; no virtual
workspaces, reduced SIP, pointer warp, or inferred occupancy and current.
Unknown or held facts never create click authority; ambiguous output scope
remains safe-hide, not parity.

## Design discriminator

Windows uses one taskbar overlay per output, rather than its focused-output
tray stopgap; KDE keeps each pager in core's logical order. A replicated or
pointer/focus-driven macOS status item does not satisfy that behavior: changing
one physical display's pointer or focus cannot change what another display's
pager represents. See [Windows settings](../decisions/settings.md#windows-taskbar-pager-placement-and-geometry)
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
display group, but does not prove exact arrival. In OmniTiler, the projection
and the product drain explicitly refuse multi-row scope
([projection](../../crates/tiler-macos/src/workspace_indicator.rs#L319-L359),
[drain](../../crates/tiler-macos/src/workspace_select.rs#L746-L764)).
Core pager rows already carry output/workspace identity. The existing
[Mission Control primitive](../../crates/tiler-macos/src/mc_lifecycle.rs#L842-L913)
can locate a unique SID and press in its exact `AXDisplayID` group after a
fresh SID/CG display and list-count check. It does not itself prove that a
product click still names that output or that its arrival leaves other rows
unchanged. Public placement alone does not remove either requirement.

## Decision required before implementation

| Option | Consistency | Safety and user-visible trade-off | Rough cost |
| --- | --- | --- | --- |
| A. App-owned, nonactivating strip per display below the menu bar | Can meet parity if the strip and exact SIP-on click pass on both displays. | Keep the current native item when the primary is the **only** display. It cannot be pinned to the primary in an extended setup; keeping it alive there can mirror or duplicate it. Do not overlay native bar hits. Prove a safe inset under crowding/auto-hide or suppress, plus fullscreen/game suppression and hotplug/reanchor. Visible Mission Control has to select the exact display group without disturbing other currents. | High: carrier lifecycle, selector and two-display proof. |
| B. One native item tracking pointer/focus (or labelling all outputs) | Not per-output parity; requires a gate amendment if shipped instead. | Keeps system crowding, auto-hide and fullscreen behavior; focus/pointer changes can replace the visible target. Gate stale clicks and game/non-interference; a view-only item avoids new actuator risk but cannot replace selection. Clickable multi-display selection still needs the exact Mission Control join and arrival proof. | Low for view-only; medium or higher for truthful click selection. |
| C. Native item bound to each display's menu bar, SIP on | Would match parity; no such supported route was found. | Native crowding, auto-hide and fullscreen behavior would be desirable, but neither the public SDK nor the tagged examples above provide persistent placement. Drawing a separate SkyLight bar is not binding to the menu bar. Selection would still need the Mission Control proof. | No supported implementation to price. |
| D. Defer parity past 0.1 | Not parity; requires an explicit gate amendment. | Retain the single-display native item and multi-display safe-hide. No new crowding, auto-hide, fullscreen/game or Mission Control surface; multi-display users get no pager under ambiguous scope. | Low now; postpones A's implementation and proof. |

**Conservative recommendation:** Keep the blocker and safe-hide unchanged
until the user explicitly authorizes A (the only identified route that could
meet parity). If a desktop strip is unacceptable, prefer an explicit D
amendment over marketing B as parity. Do not add unused projection plumbing
before choosing a carrier.

No carrier is selected and no runtime code, MX6 rules or durable decision
changes until that visible trade-off is resolved. If panels are approved, use
accepted core rows keyed by exact output; positively join AppKit/CG display
identity to managed row and target Space. Revalidate that join and the AX
Mission Control display group and order before one press. Verify the target SID
became current on that row and other rows did not change. No fallback to
another display or replay after ambiguity. In shared native scope, never
present one native selection as multiple independent selections.

After the carrier decision, cover the real snapshot/context builders offline:
independent currents on two outputs; remove, rehome and reconnect without
keeping stale targets; holds/unknown occupancy not read as empty; and
ambiguous identity/refused or stale clicks with no native effect. Keep MX6's
numbers and spare rules unchanged. Red/green first, then locked macOS library
and integration tests and strict Clippy on the touched crates once.

## Bounded physical check (after implementation)

No new signed fixture run was performed. The [existing baseline](macos-core-trailing-pager.md#signed-fixture-retest-2026-11-27)
proved a single-display native 3 → 4 → 3 switch, matching rebuilds and clean
restoration, not physical click or painted markers. A two-display run is still
required; do not use these steps to test the current safe-hidden preview as if
it were the proposed carrier.

1. Use two extended physical displays (A and B), SIP enabled, and an already
   enabled “Displays have separate Spaces” setting. Record the display/Space
   arrangement and current Space on each before making changes. Follow the
   [signed fixture procedure](../live-macos-testing.md#fixture-first-live-tests):
   signed fixture only, `io.github.omnitiler.live-fixture` scope, and test only
   on separately identified run-owned desktops. Give A and B unequal current
   ordinals with at least one alternate owned desktop per display; leave an
   alternate on B empty. Establish that each strip shows only its output's
   workspaces and paints its current marker independently.
2. Focus a fixture window on A and, without first activating B, physically
   click B's alternate workspace on B's pager. Independently verify in Mission
   Control that the expected native desktop on B became current and A's
   desktop did not change. Compare the painted markers, including on the
   empty destination. Repeat in reverse (focus on B, click A). A changed
   highlight alone or landing on a same-numbered desktop on the wrong output
   is a failure.
   Read-only corroboration is available with
   `mise exec -- cargo run --locked -p tiler-macos -- mc-inspect`:
   compare `layout.current` before and after. It is a sorted set, not a
   display map; it cannot replace the per-display Mission Control check.
3. With the pager idle, disconnect B, allow topology to settle, and reconnect
   it. After rehome, the removed pager must not appear on A or retain a
   clickable target; A must show its accepted rows. After reconnect, change
   display arrangement or scale and repeat one click: each pager must
   reanchor to its own display and use fresh targets. While a topology join
   is unproven, there must be no actionable stale rows. An unobserved
   transient is not proof of fail-closed behavior; cover it offline.
4. Put an owned fixture window into native fullscreen on one output, check
   that the pager does not cover or take input from that fullscreen surface,
   and leave fullscreen to verify fresh return. Check the approved auto-hide
   and placement behavior, including that native menus remain usable. This
   does not replace the separate real-game acceptance legs.
5. Stop the scoped owner and fixture, restore the arrangement and the
   starting desktop on each output, and remove only the owned test desktops.
   Verify no pager, owned window, helper, or recovery residue remains. Report
   both outputs' before/after desktop identities for each click, the painted
   results and any failed leg; stop on scope, permission or restore failure.

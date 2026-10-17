# macOS group underlay

## Current outcome (2026-10-17)

Parked at a diagnostic-only milestone. Scoped SkyLight ordering of the existing
AppKit carrier did not prove below-member placement. No product underlay is
enabled. A different renderer/window-ownership shape would require a new bounded
evaluation; the present evidence does not establish that all Tier 2 routes fail.

## Goal and acceptance

Reuse shared active-group projection and visual policy plus the active-border
AppKit foundation to render the focused tiled group's underlay. Match KDE and
Windows geometry, fill, configuration and trigger semantics where macOS permits;
explicitly document host differences. Drop overlays are out of scope.

Accept below-member ordering with a foreign-process fixture before
production integration. Prove group creation/dissolution, projected reflow,
member/nonmember ordering, click-through, Space/fullscreen suppression and exact
teardown restoration. Appearance alone remains a short physical check.

## Approach and bounded units

1. Establish reference semantics and accept below-target carrier evidence.
2. Integrate shared policy, observation/fences, configuration and regression tests.
3. Exercise production live acceptance and independent safety review.
4. Verify tests, strict clippy, formatting, signed build/verification and hosted CI;
   record differences, decisions and evidence, then archive this note.

## Established semantics

The group is the focused tiled leaf's immediate parent, including recursive
descendant leaves; bounds come from Engine projection, not observed client
rectangles. Shared visual policy expands the union by border gap, border width
and underlay extension (-1 follows border width). Default fill is `#662a82da`,
without a separate radius setting. Visibility requires eligible focused-group
context plus the modifier chord or a classified matching interactive move.
Fullscreen/maximize, stale projection, focus/scope drift and missing anchors hide.
The fill belongs below the lowest member and the active border.

## Ordering verdict and delivered diagnostic milestone (2026-10-17)

Production integration is blocked. Two foreign-process public-ordering approaches
failed the required global CG ordering gate: relative-Below from the initial
state left the carrier at index 12 versus member indices [45, 2]; front ordering
followed by relative-Below left it at index 0 versus members [40, 38]. Smaller
indices are nearer the front. Neither public route establishes placement below
all members. The subsequent private-ordering evaluation is recorded below.

Delivered only: a main-thread filled, nonactivating, click-through carrier;
shared-core outer-geometry adaptation and strict `#aarrggbb` parsing; and the
bounded `group-underlay-session` diagnostic. The fixture fails closed on ordering.
It is not wired into tiling, configuration or product visibility triggers.

The first observed report ends at a refused click (carrier below the receipt);
it proves reflow and helper cleanup, not later fullscreen/fence legs. The second
reports successful mouse-down and mouse-up receipt deltas (+1 each), pointer
restoration, focus isolation, reflow, explicit fence hide/return, native fullscreen
entry/hide/exit/return, original membership, helper teardown and zero onscreen
residue. Its overall verdict is false solely because ordering failed. The fence
leg is not an actual native Space switch. Neither group creation/dissolution nor
Engine reflow is proven by this carrier-only fixture.

Recovered original tool-output excerpts have truncated heads; the second JSON's
detail string is also truncated, while its verdict fields remain available.
SHA-256:

- First route: `dd3e5fb113d72d25a016bc5b49ac64adca28587c426c5fe7e51e5e88de2613fc`.
- Second route: `7550bfa3404837e22a1b31c8c975594e0c733297a57d816ef53d981e5eb37fe2`.

Independent review found and corrected an alpha/channel grammar error: the live
legs used RGBA `(0x66,0x2a,0x82,0xda)`, not the intended
`(0x2a,0x82,0xda,0x66)`. Pure regressions now pin strict ARGB input and RGBA output.
Ordering and input readbacks do not depend on fill colour; corrected appearance
has not been exercised live. No appearance-only physical procedure is offered
for a product effect that does not yet exist.

Fresh restoration diagnostics reported one original Space/current Space, Mission
Control absent, no recorded helper/controller/display-awake processes remaining,
and unchanged frontmost application. The protected terminal was not targeted;
its exact geometry/membership comparison is unavailable because no baseline was
captured. Do not describe that missing comparison as verified restoration.

## Verification and differences

At the repaired milestone, locked macOS tests (721), strict all-target clippy,
workspace formatting, signed development app build and signature verification
pass. Bundle identity remains `io.github.omnitiler.macos.probe`. Independent
review accepts retention only as an explicitly unaccepted ordering diagnostic,
not production acceptance.

Published diagnostic milestone: `a175451e`. All local gates were repeated after
integration with concurrent changes. Hosted CI
[38984864225](https://github.com/beefsack/OmniTiler/actions/runs/38984864225)
passed macOS, Rust, Windows, KWin, native, licenses and notices. Native passed on
a single failed-job rerun after an unrelated shortcut-selection allocator abort.
Shell retains the pre-existing KWin API guard failure for unaudited
`fullscreen`/`maximized`/`sticky` symbols; no Shell/native source changed here.

Unsafe accounting in `ffi.rs`: lines matching `unsafe` 171 → 176; matching
`unsafe {` 103 → 104. All new unsafe remains confined to that module.

Differences from KDE/Windows at this milestone:

- No production underlay, settings/CLI configuration, modifier/move trigger,
  ActiveGroup integration or change-driven re-anchoring is delivered.
- Public AppKit below-member ordering fails; KDE parents below its lowest member
  and Windows orders below its lowest member/border. The diagnostic's front-first
  route can cover unrelated windows and must not be used in production.
- Fixture geometry uses AppKit points and shared expansion, but is a synthetic
  union rather than an Engine-projected group. No underlay radius is introduced.
- The corrected default/grammar matches the references offline, not yet in a
  corrected live render. Chord/move parity, native Space switching, maximize,
  multi-display, physical appearance and game journeys remain unproven.

## Parked questions and exact next action

The tentative scoped SkyLight evaluation replaced the earlier evaluate-versus-
defer question and is complete with a rejected ordering contract. Defer product
integration. Exact next action: review whether a separate SkyLight-created
carrier evaluation is worthwhile; do not repeat the two tested AppKit-carrier
routes without a changed hypothesis. No such renderer is selected or implemented.
Only accepted below-member, above-lower-nonmember and focus/raise stability
evidence can unblock integration. Dock injection, SIP reduction and wider TCC
remain outside this route.

Cmd+Shift level polling is an unselected chord candidate. The move arm still
needs a classified native interactive-move source; AX movement notifications
alone do not distinguish titlebar movement from resize or programmatic movement.

## Initial risks

Public AppKit ordering above foreign windows is established by the active-border
work; below-member ordering is not. Do not substitute above-target rendering or
silently introduce private rendering APIs. macOS modifier and classified move
sources require explicit evidence and a documented selection. Unsupported
behavioral discriminators remain TBD in reference outcomes.

Unsafe baseline in `crates/tiler-macos/src/ffi.rs`: 171 lines matching `unsafe`,
103 matching `unsafe {`. No other unsafe module is permitted.

## Scoped SkyLight evaluation (2026-10-17)

**Tentative (pending user review):** evaluate private relative ordering on the
existing carrier within Tier 2, with SIP enabled and no scripting addition or
Dock injection. This is not a selection of a new private renderer. Both tested
approaches failed; retain only the fail-closed diagnostic and defer integration.

Local source inspection found:

- JankyBorders `src/border.c` orders its SkyLight-created windows using
  `SLSTransactionCreate`, `SLSTransactionOrderWindow` and commit mode 0, matching
  level/sublevel and reasserting ordering on window updates. Its window creation
  differs from this AppKit-owned panel.
- yabai declares `SLSOrderWindow` in `src/misc/extern.h` and uses it in the Dock
  scripting-addition payload; that authority path is not used here.
- AeroSpace supplied no comparable relative-order renderer precedent.

The local headers declare order/commit results as `CGError`; independent review
checked argument order, transaction release and connection selection against
those sources. A successful return alone is never the placement oracle.

Two complete foreign-process reports were retained. The first build tested
direct `SLSOrderWindow`; the second added the transaction discriminator. Direct
ordering returned 0 on all 16 calls across the two runs but left settled global
indices unchanged. Transaction ordering returned `-1698579600` (`0x9ac1bb70`)
before commit and did not change placement. These are observed outcomes for this
connection/carrier shape, not proof that a different SkyLight renderer cannot work.

Representative second-run indices (smaller is nearer the front):

| Stage | Carrier | Members | Lower nonmember | Below every member |
| --- | --- | --- | --- | --- |
| Before/direct/transaction | 0 | 3, 2 | 16 | No |
| Focus and reflow | 0 | 3, 2 | 16 | No |
| Click armed, then fence | 4 | 7, 6 | 20 | No |
| Fullscreen return | 11 | 14, 13 | 28 | No |

The above-lower-nonmember relation alone passes these rows; the required complete
ordering conjunction never does. Stability below members is therefore unproven.
No third ordering approach, production integration or border ordering change was
attempted. Synthetic reflow here is carrier geometry, not Engine reflow.

Both runs reported focus isolation, reflow, mouse-down/up receipt deltas +1/+1,
pointer restoration, explicit fence hide/return, owned native fullscreen
entry/hide/exit/return, original Space return, reaped helpers and zero onscreen
residue. The fence is not a native Space-switch test. All are diagnostic-carrier
observations, not product lifecycle acceptance. One offscreen CG record was
reported separately until process teardown.

Baseline/restoration limitation: a pre-run foreign geometry baseline contained
39 windows; each run re-found 38 with no drift. The absent baseline window was
not identified in those reports. There is no retained per-window protected-
terminal comparison: complete restoration must not be claimed. Original Space
and frontmost application returned; final process checks found no test helpers,
controller or display-awake child remaining. Unrelated existing processes were
preserved.

Raw report SHA-256:

- Direct-era build: `9c22fad6d71834099de908902fffe5471554d3a493615ea00ea007d6cdbfc28b`.
- Route-discrimination build: `8b30b2590dcde20be2f9772abd870305dcf89b720a11762399ce16cf53498931`.

Source was `f1abf858` plus the diagnostic diff. Per-build executable hashes were
retained separately. The original signed binaries were subsequently rebuilt;
the reports do not embed source/artifact/signing metadata, so their external
provenance is incomplete. Fresh signed-build verification is not retroactive
live evidence.

Review corrections expose vanished baseline IDs in future reports, distinguish
drift stability from complete restoration, remove stale public-only wording and
apply the above-nonmember requirement only when that window is below the group.
Missing indices still refuse, and below-every-member remains mandatory. Pure
regressions cover these cases; these corrections were not rerun live and cannot
repair the historical restoration gap. Independent correction review accepts
diagnostic-only retention, not ordering or product acceptance.

Fresh post-correction gates: 725 macOS tests, locked strict all-target clippy,
workspace fmt, signed development build and `just verify` passed, with retained
logs. Original gate logs were unavailable; these fresh checks supersede the
earlier unretained gate claims. Unsafe accounting for this evaluation in
`ffi.rs`: lines matching `unsafe` **176 → 191**; matching `unsafe {`
**104 → 111**. No unsafe was added outside that module.

Published diagnostic evaluation: `b60fc07b`. Full local gates were repeated before
publication. Concurrent Windows documentation updates were rebased without source
changes. Hosted CI [38988368422](https://github.com/beefsack/OmniTiler/actions/runs/38988368422)
passed macOS, Rust, Windows, KWin, licenses and notices; native passed all 26 tests
on one targeted rerun after the previously seen shortcut-selection allocator
abort. Shell alone remains red on the pre-existing KWin API guard for
`fullscreen`/`maximized`/`sticky`; no Shell/native source changed here. This is a
verified diagnostic milestone, not completed product-underlay acceptance. Keep
this note with the parked item rather than archive unfinished integration.

Remaining KDE/Windows differences, explicitly:

- No product underlay or group creation/dissolution lifecycle; no `ActiveGroup`
  integration or Engine-projected reflow/re-anchoring.
- No underlay settings/CLI, modifier chord or classified interactive-move arm.
- No accepted below-members/border ordering or stable stacking repair; no Windows
  band matching or KDE target parenting. The front-first diagnostic may cover
  unrelated windows and is not a product fallback.
- AppKit point/backing geometry uses a synthetic union, not Engine projection;
  strict ARGB parsing/shared expansion exist only in the diagnostic foundation.
- No product Space-switch/fullscreen/maximize suppression acceptance. Diagnostic
  fullscreen and fence evidence do not establish these product behaviors.
- Multi-display, physical appearance, chord/move feel and game journeys remain
  unproven. No appearance-only dogfood step is offered for an absent product.
- Active border remains above-target at normal level, unlike KDE/Windows
  below-target rendering and Windows band matching. This evaluation does not
  prove a small safe border switch; parity is a separate parked follow-up.

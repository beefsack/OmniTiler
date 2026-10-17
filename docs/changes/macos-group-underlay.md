# macOS group underlay

## Goal and acceptance

Reuse shared active-group projection and visual policy plus the active-border
AppKit foundation to render the focused tiled group's underlay. Match KDE and
Windows geometry, fill, configuration and trigger semantics where macOS permits;
explicitly document host differences. Drop overlays are out of scope.

Accept public below-member ordering with a foreign-process fixture before
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
indices are nearer the front. Neither route establishes placement below all
members, and no third ordering attempt or private renderer was added.

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
not production acceptance. Publication and hosted CI evidence follow below.

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

Select whether to evaluate a scoped private SkyLight ordering route, using local
reference sources and an owned foreign-process fixture with complete artifacts,
or defer the underlay. This is an ordering-route decision, not permission to
inject into Dock, lower SIP or widen TCC. Only after accepted below-member and
nonmember ordering evidence should production integration resume.

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

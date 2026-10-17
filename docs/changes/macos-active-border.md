# macOS active window border

## Current outcome (2026-10-17)

Implemented and published. Live checks now prove empty-Space hiding and return,
native fullscreen entry/exit and border return, and synthetic mouse-down delivery
through a painted border region to an owned window underneath. The
[bounded physical procedure](archive/macos-dogfood-hardening.md#6-active-border-appearance-check-pending-2026-10-17)
is now only for appearance/perceived tracking. Group underlay and drop overlays
have not been implemented; broader float/zoom, multi-display and real-game
acceptance remains separate.

## Goal and scope

Deliver the active-window border with shared KDE/Windows visibility, geometry,
defaults, and configuration semantics. Establish a main-thread rendering carrier
reusable by later group-underlay and drop-overlay work; do not implement those
effects here. Preserve focus, input, native Spaces, and fullscreen/game isolation.

## Acceptance

- Reuse `tiler-core::visual` rather than duplicate its visibility/geometry policy.
- Match eligible active floating and tiled windows, configured border geometry,
  and fullscreen/maximize/minimize suppression; document host differences.
- Prove owned-window focus, movement/resize, fullscreen suppression, click-through,
  Space transitions, and restoration. Distinguish API evidence from physical
  visual acceptance and measure tracking latency where possible.
- Extend the existing physical dogfood procedure with a short appearance check.
- Pass macOS tests, strict clippy, formatting, signed app verification, and hosted
  CI; account separately for unrelated failures.
- Keep unsafe code confined to `ffi.rs`; report a reproducible before/after count.

## Approach and bounded units

1. Establish reference semantics and inspect local rendering precedents.
2. Accept a bounded AppKit carrier fixture before production integration: public
   relative window ordering, nonactivating click-through windows, main-thread
   drawing, ownership and verified teardown. Test native-Space/fullscreen fences.
   Escalate rather than silently add SkyLight rendering if public ordering fails.
3. Integrate the accepted carrier with observation, shared core policy, scoped
   runtime, configuration, regression coverage, and the physical procedure.
4. Exercise live acceptance and restoration, correct concrete defects, and obtain
   independent review of focus/input/Space isolation and unsafe boundaries.
5. Record decisions, evidence, differences, and verified outcome.

## Initial findings

Shared policy lives in `crates/tiler-core/src/visual.rs`: expand the inner rectangle
by the border gap; suppress absent/deleted/minimized/fullscreen/maximized/excluded
targets. KDE and Windows border eligible active windows independently of tiling
membership. Defaults are width 3, radius 6, gap 0, fallback `#2a82da`, theme enabled.
Tiling gaps remain independent. Neither reference has blanket move-animation
suppression. Windows additionally excludes cloaked/hidden-workspace/shell windows.

Local JankyBorders uses private SkyLight windows and transactions for exact
relative ordering; current yabai delegates to it. AeroSpace supplies observation
precedents but no active-border renderer. First evaluate public AppKit relative
ordering: it is the smaller API/lifetime surface and fits the existing main-thread
architecture. Actual Space/fullscreen isolation is an acceptance condition, not
an assumption from upstream source.

## Verification and open questions

Carrier review found the initial fixture insufficient for integration: same-app
ordering cannot establish foreign-window ordering, flag readbacks cannot prove
physical click-through, and the initial fullscreen exit membership check sampled
an asynchronous transition. Two exit experiments did not establish synchronous
restoration; do not repeat that oracle. Correct the fixture to use an independent
owned helper process and explicit final teardown/membership readbacks, and extract
the reusable create/update/hide carrier API before production wiring. The measured
approximately 54 ms included a deliberate 50 ms event-pump wait and is not display
latency. Physical appearance remains separate from input-delivery readbacks.

The corrected foreign-process fixture passed twice with globally observed carrier
ordering above its target, unchanged foreign focus after showing, fullscreen
hide, and final window/process/Space teardown. A held-child lifetime repair keeps
the helper output reader alive; otherwise periodic helper output encountered a
closed pipe and the helper exited. Independent review accepted the corrected
carrier contract. Native accent resolves through `NSColor.controlAccentColor` in
sRGB; drawing uses AppKit points with target backing-scale readbacks.

Production integration now reuses observed owner snapshots and shared policy,
one main-thread carrier, scoped target gating, and startup border configuration.
Review corrections now use exact engine-plan membership rather than classifier
eligibility for placement, and suppress full-display coverage regardless of
placement. Rejected float operations correctly retain existing plan truth; no
speculative float-state cache was added. Actual production timing confirmed a
253 ms maximum with the initial cadence. A border-only dirty fast path, throttled
to 33 ms, now consumes owner publications without topology/tiling/AX work at drag
frequency. Later run maxima were 22, 32, 24 and 23 ms. These are
snapshot-to-application samples, not display latency. Transition logs now include
hide reasons and bounded timing counters.

Use locked macOS cargo tests and strict clippy, workspace formatting, and
`mise exec -- just app 'OmniTiler Mac Dev'` followed by `mise exec -- just verify`.
Live runs use the signed LaunchServices bundle and disposable identified targets.
Physical appearance, shell behavior, multi-display transitions, and real games
must not be claimed from synthetic or API-only evidence.

Unsupported behavioral discriminators remain TBD in the reference outcomes rather
than being inferred. No screen-capture consent change was made; any new capture
grant remains an open question, not a silently introduced prerequisite.

## Delivered foundation and configuration

`active_border.rs` holds shared-core adaptation and the observation mapper;
`border_runtime.rs` holds facts, fences and change-only decisions. `ffi.rs`
exposes the main-thread `BorderCarrier` lifecycle, native accent, point/backing
geometry and drawing. `border_session.rs` is its bounded foreign-helper fixture.
`native_tile_backend.rs` publishes existing observations; `tile.rs` owns one
carrier with teardown cleanup. Later effects can reuse this lifecycle,
observation, colour and geometry foundation; their drawing/placement is not
implemented.

Startup settings: `active_border.{enabled,width,gap,radius,color,use_theme}`.
CLI: `--no-active-border`, `--active-border-width`, `--active-border-gap`,
`--active-border-radius`, `--active-border-color`, `--active-border-theme`,
`--no-active-border-theme`. Defaults on/3/0/6/`#2a82da`/theme-on; bounds match
Windows (width 0–32, gap/radius 0–64).

## Final evidence and remaining gaps

Initial verification on base `c5e7c392` plus implementation diff passed 711 macOS
tests, strict locked all-target clippy, package formatting, signed development
app build and `just verify`. The implementation was published as `7c45cadd`,
followed by portable-stub fixes `162785d6` and `cd4c7652`. Hosted CI on `cd4c7652`
passed Rust, Windows, macOS, KWin, native, licenses and notices; only the unchanged
pre-existing Shell KWin API guard failure remained. The live follow-up on
`cd4c7652` plus fixture changes passed 714 macOS tests, strict clippy and fmt.
Bundle identity:
`io.github.omnitiler.macos.probe`. Pre-notarization Gatekeeper rejection is an
expected development diagnostic, not a bypass or shipping claim.

Existing raw fixture reports and production logs were independently inspected
after the live work. Global carrier/target indices were 3/28 and 0/5; showing
left foreign focus on the target; native fullscreen entry hid the carrier; final
teardown reaped helpers, removed their records, left zero onscreen owned windows
and restored topology/focus. Closed offscreen CG records were reported separately.
Production exercised two same-bundle windows, focus reordering and observed
geometry updates. Fixture move/resize API samples of 108–164 ms include AX/pump
waits and are not display latency. Final readback found recorded test processes
gone; unrelated stale records were preserved.

Two float-input discriminators posted and merged inputs but did not establish a
successful float or a maximize transition. Their artifacts cannot distinguish a
refusal from actual floating; do not repeat the same discriminator or infer a
border defect. Pure float/zoom coverage passes. `spaces-move` no-effect concerns
transfer, not selection; the later switch check below uses native selection and
does not depend on that parked transfer primitive.

### Live follow-up: native switching, fullscreen return and input delivery

- **Production Space switch:** a scoped, input-off product run showed the border
  on an owned helper. Native selection created a temporary empty Space and
  switched there. The carrier was absent with no eligible focused target;
  transition logs reported `revision-stale`, then `out-of-scope` on the repeat.
  Selecting the original Space and focusing the owned helper restored its carrier
  with `reorder=true`. Both runs completed cleanly with zero border errors. This
  exercises the native selection command path, not physical tap-to-select input.
  Per-switch arrival/carrier readbacks were retained in the execution transcript;
  product transition logs and outcomes are file-backed.
- **Fullscreen exit/return:** the foreign-helper fixture entered native fullscreen,
  hid its carrier, exited with AX fullscreen readback false, and returned to a
  visible carrier and the exact focused helper. Original membership was restored.
  The exit now uses the same-holder unreaped `Child`, executable/record identity,
  fresh CG joins and post-write liveness; it does not weaken the title-proof path
  used by unrelated commands. This replaces the title-withheld refusal, not the
  rejected synchronous fullscreen-Space equality oracle.
- **Synthetic click:** a sample in the painted straight border band had the
  carrier above an owned receipt window in unfiltered CG order. Fresh AX and
  WindowServer point hits joined that receipt window; a guarded HID down/up pair
  produced its mouse-down count delta +1 (baseline 1, then 2), proving delivery
  beneath the ring rather than merely successful posting. A foreign-window pixel
  candidate was refused without clicking. The prior count's origin is unknown;
  only the controlled delta is claimed. The mouse-up edge was posted but its
  received count was not retained in this report; later reports expose both
  counters. Pointer restoration passed. No event tap or capture grant was needed.
- **Restoration:** temporary Space removal completed. Fresh final readbacks
  corroborated exactly one original Space/current Space, Mission Control absent,
  no owned windows/processes, and unchanged protected-window geometry/membership.
  Each bounded display-awake child was reaped; unrelated stale records were left
  untouched. Helpers and their records were removed; zero onscreen owned carrier
  residue remained.

The independently inspected fullscreen/click report has SHA-256
`70d13e2d2597e5b10c99ded80595a5b52584fb1ba73294e38b63d545b776c532`.
Input-receipt and held-exit instrumentation is fixture-only. The fixture also
observed that cross-process relative ordering can leave a panel buried: its
click leg uses front ordering plus mandatory actual CG-order checks. Production
already uses front plus relative ordering; this is not a claim that all foreign
stacking variants are solved. Appearance/perceived lag, successful float/zoom,
multiple displays, stacking variants and real games remain follow-up coverage.

## Host differences and review

The [tentative visual decision](../decisions.md#visuals-border-underlay-and-grouping)
records: native accent and point/backing conversion; startup-only configuration
instead of Windows live Apply; work-area axis equality for floating zoom rather
than native maximize flags; extra full-display safety suppression even for placed
targets; scoped frontmost eligibility; macOS transient/accessory exclusions; and
a normal-level panel above the target rather than KDE/Windows below-target
rendering or Windows band matching. Managed collection behavior and observed
membership fences replace host workspace parenting. No blanket move/animation
hiding is added, matching the references.

Unsafe accounting, base to implementation in `ffi.rs`: lines matching `unsafe`
165 → 171 (includes comments/attributes); lines matching `unsafe {` 102 → 103.
New unsafe remains confined to `ffi.rs`; no new unsafe module. Independent review
found no remaining code blocker, separately from the acceptance gaps above.

## Exact next action

The [group-underlay SkyLight evaluation](macos-group-underlay.md#scoped-skylight-evaluation-2026-10-17)
did not establish below-target ordering for the existing AppKit carrier. No
border switch was made; below-target rendering and Windows band parity remain a
parked follow-up in the backlog. This is not proof against a different renderer.

Run the linked appearance-only procedure and record the visual verdict. Any
separate float follow-up must observe an actual successful core verdict rather
than merely input-post/merged counters. Group underlay and drop overlays remain
subsequent effects; native switch/fullscreen/input proof no longer awaits a
physical procedure.

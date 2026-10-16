# macOS active window border

## Current outcome (2026-10-17)

Implemented and offline gates pass; acceptance is incomplete. In particular, the
required cross-Space switch has not been observed. Keep this note active. The
[bounded physical procedure](archive/macos-dogfood-hardening.md#6-active-border-appearance-and-space-check-pending-2026-10-17)
covers appearance, click delivery, fullscreen return and switching without window
transfer. Group underlay and drop overlays have not been implemented.

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
latency. Physical appearance and input acceptance remain separate.

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

Final verification on base `c5e7c392` plus implementation diff: 711 macOS tests
passed, strict locked all-target clippy, package formatting, signed development
app build and `just verify` passed. Bundle identity:
`io.github.omnitiler.macos.probe`. Pre-notarization Gatekeeper rejection is an
expected development diagnostic, not a bypass or shipping claim. Hosted CI has
not evaluated this diff.

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
transfer, not selection, and does not prove the required switch. `spaces-select`
exists, but no confirmed disposable Space pair was available for this acceptance.
Fullscreen entry/hide and final teardown are proved; normal exit/return is not.
Physical appearance/perceived lag, click delivery, fullscreen return, successful
float/zoom, Space switching, multiple displays and real games remain unproven.

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

Run the linked short physical procedure, especially cross-Space absence/eligible
return, and record each cell independently. If no adjacent Space exists, first
establish a disposable owned pair with an already-verified lifecycle fixture
rather than retrying transfer. Any float follow-up must observe an actual
successful core verdict, not merely input-post/merged counters. After acceptance,
refresh offline gates and hosted CI on the integrated source and close this note.

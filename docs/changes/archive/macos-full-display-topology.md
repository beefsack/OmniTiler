# macOS FULL display topology

## Goal and scope

Report FULL native display rectangles to the core-owned topology introduced by
`f9bd2df1`, independently of menu-bar/Dock-excluded work areas. Preserve output
identity, UUID-exact Space homing, coordinate conversion and native capability
refusals. No new adjacency authority, native-Space transfer capability, protocol
change or geometry inference is introduced.

## Approach and acceptance

1. Populate world topology from retained `NativeDomain.bounds`; leave domain
   bounds sourced from `visible`, with existing outer-gap handling unchanged.
2. Keep deprecated wire adjacency empty. Exercise the actual producer conversion
   and homing path with synthetic, canonical display UUIDs and AppKit-point
   fixtures, including FULL/work-area divergence, shared-edge rounding,
   negative/above-primary origins and a missing primary.
3. Pass macOS tests, strict clippy, formatting, signed development app build and
   bundle verification. Review the diff independently and run a bounded scoped
   single-display smoke. Record native two-display arrangement validation as
   pending rather than infer it from fixtures or Space order.

## Delivered behavior and decisions

- `world_builder.rs` now emits FULL `NativeDomain.bounds` as
  `OutputTopology.geometry`; visible domain bounds and empty adjacency are
  unchanged. `world_runtime.rs` documents that distinction.
- No new product decision or behavioral ambiguity: this implements the existing
  [core-owned topology contract](shared-core-owned-output-topology.md).
- Ordinary implementation choices: reuse retained FULL geometry rather than
  derive it again; test producer-shaped values through `build_snapshot` and
  `NativeContext::derive`; assert validated FULL reports and core acceptance
  without claiming private core-topology readback. No new dependencies or unsafe
  code. Native Space/transfer refusals remain unchanged.

## Accepted evidence (2026-10-18)

- The discriminating FULL-versus-visible assertion failed against the old
  builder: visible `(0,25,1440,875)` was reported instead of FULL
  `(0,0,1440,900)`. It passes after the fix.
- Fixtures use canonical uppercase, dashed CFUUID-shaped synthetic identities,
  shuffled native Space rows and the real AppKit bottom-left to CG top-left
  conversion. A fractional shared edge rounds to exact contact independently of
  scale; left/above-primary origins remain negative. A missing primary yields
  skipped displays and a valid empty report, not guessed placement.
- Stacked FULL rectangles touch while visible rectangles have a gap; FULL
  topology, separate visible bounds, empty adjacency and engine acceptance are
  asserted. Independent review found no blocking defects. A test name was
  narrowed to describe report acceptance rather than unobserved private state.
- `mise exec -- cargo test --locked -p tiler-macos` passed all targets, including
  759 library tests. The final targeted `--lib world_builder` run passed 15 tests.
- `mise exec -- cargo fmt --all -- --check` and strict all-target clippy for
  `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi` and `tiler-macos` passed.
- `mise exec -- just app 'OmniTiler Mac Dev'` and `mise exec -- just verify`
  passed, including a rebuild immediately before the live smoke. Development
  signing's expected `spctl` rejection is not distribution-signing acceptance.
- Single-display scoped TextEdit smoke:
  `mise exec -- just tile --user-start --scope com.apple.TextEdit --input off --seconds 10`.
  Terminal outcome was `completed`, `shutdown_clean:true`, with one topology
  update, zero topology failures and three disposable documents stably tiled.
  AX/CG frame readbacks agreed and a second bounded run did not drift. Insets
  were consistent with menu/Dock-excluded visible tiling plus the existing outer
  gap, not FULL-display tiling; exact native FULL/visible rectangles were not
  exposed by the existing readback probes, so this is supplementary evidence.
- Lock/Secure Input checks passed before each live leg; no synthetic input was
  used. Test-owned processes exited and the bounded display-sleep inhibitor was
  stopped by exact PID. Existing documents and temporary Space were retained.
- Unsafe remains **114 `unsafe {` blocks**, all in unchanged `ffi.rs`; delta 0.

## Pending native two-display arrangement check

Only a single-display smoke was available. Offline fixtures do not establish
native two-display arrangement, adjacency or cross-display interaction.

1. Follow [live macOS testing](../../live-macos-testing.md), establish disposable
   TextEdit-only scope, check lock/Secure Input before every leg, and record the
   original arrangement for restoration. Run `mise exec -- just preflight-app`,
   then the documented `scope-inspect --scope com.apple.TextEdit` and
   `spaces-diagnose` probes to record window/Space membership.
2. Attach a second display. Arrange FULL rectangles edge-adjacent with different
   menu/Dock work-area insets. Capture each display's FULL/visible rectangles and
   the complete producer world report; compare topology to FULL, domain bounds
   to visible, empty adjacency and UUID-exact display/Space homing. Existing
   readback probes do not expose these rectangles: obtain a rect-bearing
   diagnostic capture before claiming this comparison passed.
3. Run the bounded TextEdit-scoped tile command above with disposable documents
   on both displays. Read back frames/membership before and after; verify visible
   work-area containment with the existing gap exactly once. Do not infer display
   arrangement from native Space order or focused display.
4. Repeat with the secondary left of primary (negative x), then above primary
   (negative CG y), with FULL edges touching despite visible-area gaps. Exercise
   supported directional focus/move actions at the shared edge; verify FULL-based
   target selection where supported and unchanged refusals for unsupported
   native Space/transfer operations. No new capability is expected.
5. Restore the original arrangement and disposable-window baseline, stop only
   test-owned exact PIDs, and retain the report/geometry/frame comparison as the
   verdict. Never target the session-host window or run unscoped tiling.

## Outcome

Implementation, offline gates and supplementary single-display smoke passed.
Native two-display validation remains the bounded follow-up above; no further
implementation change is identified.

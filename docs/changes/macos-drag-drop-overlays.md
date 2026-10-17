# macOS drag/drop overlays

## Goal and acceptance

Deliver the KDE/Windows drag/drop contract through shared core targeting and the
existing above-target AppKit filled carrier. Both native titlebar and Meta+left
move producers should share insertion previews and synchronous drop application.
Centre stacking/swap is not selected. Below-target rendering remains out of scope.

Acceptance requires offline regressions, live drag start/target change/drop/cancel,
Engine layout and applied-frame agreement, click-through/input release, fullscreen
suppression, and per-leg baseline restoration. Appearance and perceived feel alone
remain a short physical dogfood check. Pass macOS tests, strict clippy, formatting,
signed build/verification and hosted CI; distinguish the existing Shell API guard
failure. Keep unsafe confined to `ffi.rs` and account for count changes.

## Contract and approach

Reuse shared drag resolution: edge/group/interior insertion slots, unsupported
centre, self/zero/outside snap-back, Escape cancellation, single-flight finish and
cross-output joins. Preview defaults to `#402a82da`, is nonactivating and
click-through, and clears before drop/cancel mutation. Native frame presentation
remains host-specific. See Pointer/Drag/Drop in `../decisions.md` and requirement
rows REQ-DRAG-01..08 / REQ-MOU-01..03.

Tentative trigger direction: use the existing configurable macOS Meta mapping
(Control by default, Command opt-in) for Meta+left; observe native titlebar moves
without treating arbitrary AX movement as an interactive drag. Accept a concrete
classification contract before native integration; park material ambiguity.

## Bounded units

1. Establish reference semantics and current adapter/core seams (complete).
2. Implement pure drag state/core adaptation, scoped input producers, carrier and
   settings integration with offline regressions; stop on an unresolvable native
   classification or shared-core contract issue.
3. Accept a bounded owned-window fixture, then exercise signed product live legs
   with exact baselines, target/layout/frame readbacks and restoration.
4. Independently review live/input safety, layering and evidence; resolve concrete
   findings, repeat relevant gates and publish verified milestones.
5. Record differences, tentative decisions, evidence and remaining questions;
   update the backlog and archive only on completed acceptance.

## Initial evidence and risks

Starting revision `4b8dd4ad` is clean. macOS currently has keyboard-only taps and
no native interactive-move classifier. AX moved/resized notifications alone are
not a classifier. The existing border carrier establishes public above-target
ordering; group-underlay below-member ordering remains unaccepted and parked.
No new renderer or below-target fallback is selected.

Unsafe baseline in `crates/tiler-macos/src/ffi.rs`: 191 lines matching `unsafe`,
111 matching `unsafe {`; no unsafe outside that module.

## Current outcome (2026-10-17)

**Parked diagnostic milestone, not delivered product overlays.** The Meta+left
pipeline is integrated offline with the current Engine, desired-frame application,
startup `drag_preview` configuration, and reusable filled above-target carrier.
The hard `DRAG_LIVE_ACCEPTED=false` gate prevents product mouse-tap, channel and
drag Escape-watch installation even with configuration enabled. No live drag
start, changing target, preview/drop agreement, cancel, click-through/release or
fullscreen behavior has been established for this feature. Keep this note active;
do not archive incomplete acceptance or offer an appearance-only dogfood check.

### Delivered offline

- `drag_drop.rs`: pure controller, shared Engine preview/drop adaptation, ARGB
  parsing, sticky hover, final-pointer requests and stale/cancel/no-op policy.
- `drag_session.rs`: scoped producer/carrier/runtime scaffold, current Engine
  handoff, exact off-tap AX/WindowServer hit joins, revision and membership checks,
  verified above-target CG ordering, fullscreen fences and explicit parked arm.
- `drag_native.rs`: classifier contracts only; no native move producer.
- `drag_fixture.rs`: offline fixture only, not evidence of signed product effects.
- LaunchServices helper fixture launch uses ordinary launch-date identity;
  production identity validation was not weakened to accept direct-spawn helpers.
- Config: `drag_preview.enabled` / `.color`, CLI `--no-drag-preview` and
  `--drag-preview-color`; default fill `#402a82da`. Settings resolve but cannot
  override the hard live gate. Shared core targeting is unchanged.

### Corrections and accepted review

Initial review found consume-before-proof, lock-contention event loss,
frame-containment mistaken for point proof, and missing fullscreen/order/revision
fences. Corrections introduced a pre-proven lock-free tap gate, independent
captured-button lifecycle, lossless terminal events, conservative observed-
fullscreen suppression, verified carrier ordering and press-revision validation.
The gate now requires an exact integer point, monotonic proof age at most 500 ms
(two tile turns), and a coherent single-use publication; mismatches pass through.
These are offline contracts, not demonstrated live input behavior.

Review also found session-global synthetic posting with no verified exact-owned
release primitive: refusing the up after a posted down can strand input.
`owned-drag-post` now hard-refuses all edges, including Escape, before live reads
or posts. `post_mouse_click_guarded` likewise refuses before any down; the border
and underlay diagnostic click legs are consequently parked. Existing border
product rendering is unaffected. The pre-existing owned keyboard-post command
was not changed or revalidated by this work. Final independent review accepts
diagnostic-only retention, not the originally requested feature acceptance.

### Live attempts and restoration

1. **Helper identity setup failure:** direct-spawn helpers lacked a LaunchServices
   launch date, so workers refused them; no facts/Engine admission, drag or overlay.
   All mouse edges passed through. This was a fixture failure, not feature proof.
   A LaunchServices helper route repaired the ordinary identity precondition.
2. **Stale-coordinate fixture failure:** the next attempt used pre-tiling points
   after tiling had moved the helper. Tap counters reported three deliveries,
   zero matches/suppression, zero previews/drops/cancels. Three Control-modified
   mouse edges probably reached the protected terminal at those points. No keys
   were sent there. Possible transient input effects are unverified; unchanged
   geometry is not proof of no input effect. Live testing stopped.

Final readbacks found all nine protected-terminal CG entries unchanged in bounds,
layer, onscreen state and IDs; original pointer, frontmost application and native
Space topology returned. The helper was exactly stopped, no owned probe/helper
processes or carriers remained, and the bounded display-awake child was reaped.
Owned helper geometry before/after the gesture was unchanged; destroying the
fixture helper is not a successful drag-cancel verdict. The initial after-click
subcheck was inconclusive. No further synthetic repetition was performed.

The stale-point shape is now regression-tested through exact point-owner guards,
but not live-reproven. Hard parking removes exposure from the new diagnostic
routes rather than claiming a balanced-release solution.

### Every current KDE/Windows difference

- No enabled product drag producer/overlay/drop lifecycle; no live acceptance.
- Intended Meta trigger follows existing macOS Control-default/Command-opt-in,
  unlike KDE Meta and Windows Win; physical press/feel is unproven.
- Native titlebar movement remains unclassified and parked; AX movement alone
  is not accepted. Resize gestures are not implemented by this move scaffold.
- No cross-output or cross-workspace drop integration; outside-domain pointers
  snap back offline. No native-Space transfer is delivered here.
- Insertion semantics/default fill reuse core; centre drops are refused, not
  swaps/stacks. Group/wrap/interior behavior relies on core coverage rather than
  a completed macOS host matrix.
- AppKit point/backing conversion and normal-level above-target carrier replace
  KDE parenting/outline placement and Windows layered-window band matching.
  Unknown/unverified target ordering hides; no below-target route is selected.
- Conservative any-observed-fullscreen suppression may hide on unrelated
  displays. Pointer-display precision, Space switching and fullscreen product
  behavior remain unproven, as do real-game and multi-display journeys.
- Startup-only configuration rather than Windows live Apply; settings cannot
  enable the parked arm. Preview cadence is the 250 ms tile loop with a bounded
  Engine query, not a demonstrated low-latency visual path.
- Exact-point cached proof can miss jittering presses; topology revision changes
  conservatively cancel/refuse. These fail-safe differences are not parity proof.
- New synthetic drag/paired-click diagnostics refuse before posting. Border
  product remains operational, but its old synthetic click leg cannot be rerun
  through this parked helper until balanced release is proven.
- No appearance-only physical step is offered for an absent product effect.

### Verification and unsafe accounting

Latest offline verification: 655 macOS library tests plus 134 integration tests
(789 total), strict locked all-target clippy, workspace formatting, signed
development app build and signature verification pass. Bundle identity remains
`io.github.omnitiler.macos.probe`. The development Gatekeeper diagnostic is not a
shipping/notarization claim.

Published diagnostic foundation: `5eff7c32`. Hosted CI exposed missing portable
atomic imports and a non-macOS fail-closed hit-test stub; repaired in `cf796fa8`
without changing macOS behavior. All local gates were repeated at that revision.
Independent review confirmed the stub-only fix, unchanged hard park and clean
tree. Hosted CI [39002083688](https://github.com/beefsack/OmniTiler/actions/runs/39002083688)
passed Rust, Windows, macOS, KWin, native, licenses and notices. Shell alone failed
the pre-existing KWin API guard for `fullscreen`/`maximized`/`sticky`; no Shell or
native source was changed by this work. No failed-job rerun was needed.

Current signed main executable SHA-256:
`9436916258f308f6c5bd5a28f36dac9fb7d4fa8f159ef2bb10f3a793ccc3f84d`.
Helper-app executable:
`6ab433927cd18c325b3bf260880eb9945b3d2de8f622edc5b788029f8efe614a`.
Both bundles pass deep strict signature verification; designated requirements use
their stable bundle identifiers and certificate leaf
`040f96377aaf110a77905d453bb76681e7133319`. These are current parked-build
identities, not retroactive provenance for the earlier live attempts.

Retained failed-leg outcome SHA-256:

- Initial helper-identity failure:
  `289c646acd41bdbaf5f3206b341308b73448699f8bb83f785df36fd5a35ed8e1`.
- Stale-coordinate attempt:
  `0a95a08d62e2f8219d6bb9d17e523a64335f33e0b152a8144b109929b8ca2164`.
- Stale-coordinate diagnostic log:
  `20474625b27949e661b0904e4fcf3bae631285edddcaaaceab6a16bc0ed3dce1`.

These hashes bind content only. The ephemeral logs contain no embedded source or
binary identity; source/artifact provenance is incomplete. They do not establish
successful product acceptance, physical appearance or an absence of transient
input effects.

Unsafe in `ffi.rs`: matching lines **191 → 195**, matching `unsafe {` blocks
**111 → 111**. Added matching lines are comments/documentation; no new unsafe
block or unsafe outside that module. No new dependency.

## Parked questions and exact next action

First establish an exact-owned balanced synthetic release contract (or a different
bounded fixture that cannot post to unrelated windows), with independent review.
Only then prepare a new drag leg using post-tiling stable geometry and fresh
point joins before every action. Capture baseline/restore and Engine/overlay/frame
readbacks in the fixture before enabling the hard arm for bounded acceptance.
Do not repeat the old stale-coordinate driver. Native titlebar classification,
cross-domain integration and pointer-display fullscreen attribution are separate
parked work. Below-target ordering stays parked with group underlay.

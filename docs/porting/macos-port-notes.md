# macOS port notes (world contract v2)

How the macOS adapter moves to the strict v2 contract in
[world-adapter-guide](world-adapter-guide.md). Status below is
implemented vs approved-not-implemented vs open as of `01a73798`.
The [v2 migration plan and abstraction-fit review](../changes/macos-world-v2-port.md)
confirm that inventory at `a0f54105` and record current build failures and gates.
Update 2026-10-25: the strict-v2 compile cutover is complete offline; macOS
build/tests/strict clippy/fmt pass. The inventory below describes the starting
point; policy/native settlement/presentation and live acceptance remain port work.
Live work stays under [live-macos-testing](../live-macos-testing.md)
with the fixture-first rules and the signed bundle in
`crates/tiler-live-fixture/README.md`.

## Current macOS inventory (port or delete)

- `crates/tiler-macos/src/world_builder.rs` (Unit A, additive,
  offline): pure report builder only, still on the retired
  native-domain vocabulary (topology/domain keys, trailing-creation
  proof rows). Port to v2 screens/facts/closes/view receipts plus
  `host_owned_workspace_domains` observations; no production caller
  may keep the old shape.
- `crates/tiler-macos/src/world_runtime.rs`,
  `world_dispatch.rs`, `world_sequencer.rs`, `world_command_map.rs`
  (Unit B scaffolding): adopt replies for reconciliation and commands
  through world commands and the reply presentation. Delete every
  consumer of the retired internal `Engine`/`CoreEvent`/`CoreReply`
  boundary; the legacy drag consumers cannot reach it anymore.
- `crates/tiler-macos/src/classifier.rs`,
  `workspace_select.rs`, `workspace_send*.rs`, `drag_drop.rs`,
  `space_drag_transfer.rs`, `dock_swipe_select.rs`,
  `input_session.rs`, `runtime*.rs`: move classification,
  selection/send/trailing and drag policy to core. AX/CG/Space facts,
  coordinate/lifetime mechanics stay native. Core publishes native-to-logical
  bindings in replies; macOS uses live lookups, never a durable mapping table.
  Publish observed Space membership as `native_workspace` (`nativeWs`), not
  logical `observed_workspace` or view receipts; core alone translates it and
  derives logical membership/views. Opt into equal counts only if the host
  requires it.
- Report inability through truthful feedback (refusal, unknown,
  hold): never fabricate backing, arrival or capability.

## Native facts that stay macOS-owned

- AX facts (roles, `AXSize` settability, minimized/hidden/fullscreen,
  modal/sheet identity) and CG frames/ownership/levels feed normalized
  `WindowFact`s. CG offscreen alone implies neither exclusion nor
  destruction; per-window AX subscriptions plus the deduplicated
  complete-reread backstop repair missed notifications.
- Space facts: per-display native order and current Space ride
  `native[]` under `host_owned_workspace_domains`; per-window Space
  membership rides `native_workspace`; logical receipts are for tiler-owned
  domains only. The product send route (named
  window identity, dual hit-test point proof, posted Desktop-N chord,
  exact membership/current readback) keeps its existing proof gates;
  only the policy and settlement vocabulary move to core.
- Coordinates: CG/AX frames use logical points; the live fixture's size/position
  protocol uses physical pixels. Convert at that boundary using the display
  scale factor and distinguish client from outer bounds. Normalize screens,
  frames, hints, cursor and style dimensions into the same adapter space before
  reporting; do not mix physical fixture hints with logical-point frames.
  Document the port's integer rounding and native/render conversion explicitly.
  macOS frame-attempt DPI fields are `None`: logical points have no per-window
  basis. Write whole points; round fixture-derived minima up and maxima down.
- Input stays native (tap pump, eligibility caches, consume-first
  drain, health recovery); only the targeting and settlement policy
  moves to core commands. Focus arrival needs the SLPS/NSWorkspace
  oracle plus per-app `AXFocusedWindow` joined through
  `_AXUIElementGetWindow`, never focus intent alone.

## Host styling on macOS

- Two independent host styling colors, never ring-coupled (see the
  [generic rule](world-adapter-guide.md#8-border-underlay-drop-preview-and-pager)).
  The active-window ring keeps its existing border color/theme tokens
  (`--active-border-color`, `--active-border-theme` default on: live
  system accent with the `#2a82da` fallback). The menu-bar workspace
  indicator highlights the current Space number independently of
  occupancy styling. When this port renders pager miniatures, their
  focused-window highlight gets independent highlight color/theme
  tokens defaulting to the system accent, explicitly overridable
  without touching the ring.

## Approved core changes and deferred questions

- Host-owned native adoption (user-approved 2026-10-24, core delivery
  `c7f6af40`, native acceptance separate): macOS reuses the core change the KDE
  port delivers (adopt native Spaces in native order per display,
  membership preserved, no moves at launch; outside moves/creations
  adopted later). Do not implement a macOS-side equivalent.
- Only startup adoption and its live acceptance wait on that delivery; other
  stages and offline translation/executor/readback tests proceed. Core owns
  native bindings and creation correlation; hosts execute desires with raw
  attempts. Removal remains eligibility plus fresh native checks.
- User decision 2026-10-25: Cmd+H uses option B, explicit minimized facts with
  lifetime/close retention. Diagnostics keep the actual hidden/minimized cause.
  The shared minimized-comment clarification is deferred pending agreement with
  KDE's inactive-Activity representation; this adapter mapping stays reversible.
  The [stage-2 evidence](../changes/macos-world-v2-port.md#stage-2-hideunhide-evidence-2026-10-25)
  now compares those sources and omission/minimized world reports. The tested
  geometry agrees, but hide-then-terminate lacks explicit-close plumbing under
  the superseded omission path. B repairs that gap; no core state is added.
- Fullscreen/system Spaces are omitted from desktop `native_order`; fullscreen
  windows report their flag with no logical receipts. Tentatively fullscreen,
  sticky/all-Spaces and unreadable membership use `native_workspace=None`;
  shared discriminator tests must confirm this. Sticky requires positive
  all-Spaces proof; per-display scope still needs checking.
- KDE stage 1b owns creation-attempt correlation, optional native current and
  authoritative native order; it landed in `4bd4aca9` (native-only view
  settlement `28c3cff2`). Consume those shapes, never a separate macOS equivalent.
- Stage-2 policy removal is incomplete. The approved D2 hint-loss correction
  `0b65cad5` preserves automatic floats in core without a native floating echo;
  the separate macOS checkpoint removes its automatic assertion cache and
  first-sight hint-policy copy. The unchanged D2 integration test passes.
  Synthetic born-fullscreen floating and adapter slot/member inference are also
  removed; native fullscreen facts now use core's own slotless/first-exit policy.
  Shared classifier integration remains open: selected transient float members
  cannot simply become shared eligibility refusals without changing behavior.
  Per-app native rosters now retain unknown/hidden lifetimes and incompleteness
  independently of other apps; closes are flat and homing-independent. Fresh
  full-CG liveness corroborates missing AX windows before close, including
  move-to-hidden-with-stay when topology is unchanged. This remains offline
  evidence, not native hide/reveal acceptance.
  [Mechanical coverage, blocker and sticky verification plan](../changes/macos-world-v2-port.md#stage-2-mechanical-evidence-and-policy-blocker-2026-10-25).
- Activities default stays tentative pending KDE evidence (see
  [decisions](../decisions.md#port-deferred-investigations-2026-10-24)):
  inactive-activity windows stay retained unmanaged (never tile, move,
  focus or lose their slots) until active; all-desktop maps to sticky;
  subset-of-desktops floats and never rehomes. The port may propose an
  evidence-backed alternative, but any core change needs user
  approval. Do not claim macOS multi-Space or accessibility variants
  prove Activities semantics; affected-ports notes (shared broker,
  startup adoption, async settlement) apply generically.
- Async configure: only if a KDE/Wayland pending-configure failure is
  reproduced may the port
  propose a neutral settling representation; the default stays the
  current convergence without a new core signal, and any core change
  needs user approval. Distinguish this from the Windows resize lane:
  the shipped `01a73798` fix prefers a later staged frame over the
  immediate readback (exact immediate target plus eligible later
  same-origin growth learns minima; the producer reports prior
  attempts then fresh facts). Later facts are not native completion,
  and no settling-protocol redesign is in scope.

## Build, check and gate commands

- macOS toolchain comes from `mise.toml` (`mise trust`, `mise
  install`, `mise exec -- <cmd>`); the user owns installs per
  [macos-dev-environment](../macos-dev-environment.md). The `just`
  recipes below run through `mise exec -- just ...`.
- Nonmutating preflight: `mise exec -- cargo run --locked -p
  tiler-macos -- preflight`. Offline gates: `mise exec -- just build`,
  `mise exec -- just test`, `mise exec -- just clippy`,
  `mise exec -- just fmt`.
- Signed bundle (persistent dev identity required, fails closed
  without it): `mise exec -- just app 'NAME'`, then
  `mise exec -- just verify`. A plain `cargo build` outside the
  script does not assemble or sign the bundle and does not enforce
  the deployment-target floor. Fixture bundle:
  `mise exec -- just fixture-app 'NAME'` plus
  `mise exec -- just fixture-verify`.
- Scoped live tile route: `mise exec -- just tile --user-start
  --scope ... --seconds N`; agent live legs use it fixture-only under
  [live-macos-testing](../live-macos-testing.md).
- The starting macOS crate failed Linux/Windows CI on pre-existing
  errors (`AX_ATTRIBUTE_UNSUPPORTED`, `take_trailing_proof`); the port
  repaired those cfg leaks in stage 1 without masking them; cross-platform CI
  must still verify the complete port.
- Native macOS compilation also failed at `a0f54105`: 31 errors from retired
  world-report/feedback/binding fields, changed command shapes and private
  engine-boundary consumers. These are separate from the non-macOS cfg leaks;
  see the [confirmed state and gates](../changes/macos-world-v2-port.md#confirmed-starting-state).
- Core-only gates stay host-independent (see the generic guide
  section 9); macOS live acceptance additionally needs the native
  hide/reveal checkpoints and the narrow checker ordering, never
  logical-receipt-only claims.

## Material rough edges (no behavior change selected)

- Legacy workspace/send/drag modules still coexist with
  `world_runtime.rs`; the v2 cutover deletes one side, not both in
  parallel.
- Trailing lifecycle, send-stay binding and the Dock-swipe/MC-AX
  fallback matrix are capability-gated product work on top of v2, not
  part of the contract move.
- Synthetic-chord and synthetic-drag diagnostics never substitute for
  physical input, focus arrival or shell acceptance; keep that
  separation in every port leg.

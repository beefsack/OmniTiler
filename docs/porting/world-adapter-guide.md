# World-contract platform adapter guide (v2)

Durable generic guide for every platform adapter (Windows reference,
KDE and macOS ports). The remote contract is strict v2 with no
compatibility route: v1 payloads, field names and discovery paths
refuse and are never shimmed.

KDE's paired cutover remains parked, not an accepted implementation of this
guide. Its [port record](../changes/kde-world-v2-port.md#stage-2-implementation-boundary)
tracks partial offline evidence, command wake transport and native backing
correlation; native settlement must not be inferred from wire acceptance.

Source roots below are relative to the repository root. Core symbols
live in `crates/tiler-core/src/`; the JSON codec lives in
`crates/tiler-protocol/src/world_protocol.rs`.

## 1. One core model and policy

- Exactly one `WorldEngine`
  (`crates/tiler-core/src/world_engine.rs`) owns the logical workspace
  model: stable logical workspace ids, per-screen rings, views, members,
  history, lifecycle and trailing. Native workspace domains are
  observations only, never mapping authority, commands or membership
  owners ([decision](../decisions.md#logical-workspace-authority)).
- The adapter never decides tiling, order, membership, cleanup, focus
  policy, gesture outcome or presentation eligibility. It captures facts,
  observes native state, actuates exactly what the reply authorizes, and
  renders the published presentation.
- Platform split: capture/observe/actuate/render stay native;
  reports carry facts, replies carry desires. Raw attempts ride the
  report; every verdict lives in core.

### Runtime integration

Create one `WorldEngine::new(core_generation)` and call
`handle_report(&WorldReport)` for the serialized stream. Validate remote payloads
through the strict protocol codec before invoking the engine. Apply only the
reply paired to the current owner/incarnation/generation/request and live
observation watermark. `Accepted` is fresh; `Duplicate` is cached, not another
effect opportunity; `CommandConsumed` incorporates fresh observations without
re-executing the consumed command. A rejected report authorizes no effects.
Capture native facts after dispatch, retain raw attempts for the next report,
and repeat on native events/invalidation. Do not create another Engine or a
native policy owner to fill a host gap.

## 2. Report: complete normalized facts every time

Every report carries a full snapshot envelope: screens, window facts, closes,
view receipts and presentation ingress. Completeness flags can truthfully be
false; an incomplete snapshot holds affected authority rather than inventing
missing facts. Shape:
[`crates/tiler-core/src/world_report.rs`](../../crates/tiler-core/src/world_report.rs)
(`WorldReport`); wire form in
[`crates/tiler-protocol/src/world_protocol.rs`](../../crates/tiler-protocol/src/world_protocol.rs).

- `contract_version` is `WORLD_CONTRACT_VERSION` (2). The codec constant
  `WORLD_PROTOCOL_VERSION` equals it. Anything else refuses.
- Screens (`ScreenReport`): stable output id, FULL rectangle and work
  area in one coherent adapter coordinate space, completeness and
  primary. `None` rectangles stay representable for explicitly
  unreadable observations; a report claiming complete topology carries
  valid FULL and work rectangles for every screen.
- Window facts (`WindowFact`): observed output affinity, frame, flags
  (floating, fullscreen, maximized, sticky, fixed, minimized), size
  hints and `observed_workspace`. `rect=None` is explicitly unreadable:
  the homed scope holds instead of authorizing geometry. `minimized`
  homes the lifetime on its retained logical workspace with no managed
  geometry. Never silent absence, never inferred close.
- Explicit `closed` ids are always honored, even with incomplete
  inventory. Only implicit missing-equals-closed needs a provably
  complete inventory.
- Views (`ScreenView`): the independently observed applied logical view
  per screen. Empty means unknown: never a copy of a desired view or a
  reply acknowledgement. Transfer and migration settlement needs a
  receipt exactly naming the expected logical id.
  Host-owned adapters instead publish native current views in `native[]` and
  omit logical view receipts; core derives those receipts from its bindings.
- Membership evidence (`observed_workspace`) is an adapter-observed
  logical id (facade readback or mechanical application receipt),
  never authority by itself. Under `host_owned_workspace_domains`,
  `WindowFact.native_workspace` (optional wire `nativeWs`) instead carries the
  observed opaque native desktop id; core derives the logical receipt through
  its per-output binding. This bootstraps before logical ids exist without
  adapter-minted ids. The default native-free contract needs no native field.
  Host-owned adapters leave `observed_workspace` empty; logical receipts are
  only for tiler-owned domains.
  Unknown/conflicting native evidence proves no arrival; adopted resident
  scopes hold write authority, and unhomed newcomers await evidence. `None`
  means unknown, never proof of absence.
- Capability opt-ins (off by default, never platform-name checks in
  core): `host_owned_workspace_domains` carries `native[]`
  (`NativeObservation`: observed native order plus current native view
  per screen, pure observation); `equal_workspace_count` pads core
  rings neutrally for hosts that need equal counts. Carrying `native[]` or
  non-null native window membership without the capability refuses. A present
  native list is complete only when its screen `complete` and report
  `inventory_complete` are both true; missing rows never imply native deletion.
  Duplicate output rows/within-output ids refuse; native ids can repeat across
  outputs. Separate views bind independently; linked views share logical ids
  but only genuinely observed output/native pairs publish bindings.
- Presentation ingress (`PresentationIngress`): genuinely observed
  foreground lifetime (proof-verified adapter-side, `""` when none),
  its live frame (or `None`), host-scaled style numbers in the same
  coherent adapter units as screens and fact frames (physical pixels on
  Windows, KWin logical pixels on KDE; DPI conversion stays
  adapter-side), chord/move facts, the focused-fullscreen-game `paused`
  lane, one native top-to-bottom stacking snapshot (`pager_order`) and
  one bounded frame-source row per window (`pager_frames`: live, then
  retained float, then retained tile snapshots).
- Cursor: normalized position in adapter units (same FULL-rect space)
  plus the natively hit-tested managed lifetime (`cursor_hit`, exact
  managed binding, never a rectangle guess). Absent cursor or a point
  outside every screen falls back through focused output to
  primary/first readable. Focus-follows-cursor and cursor-warp are
  core-actuated settings; an adapter with no warp API reports
  unsupported and never claims an applied warp.
- Command fields: at most one command per report; observation-only
  reports carry none. Command vocabulary is `WorldCommand` in
  `crates/tiler-core/src/world_report.rs`; planner-resolved selection
  (`SelectNumbered`, `SelectRelative`, `SelectPrevious`,
  `SelectTrailing`) carries `output: None` for core-resolved
  cursor/focused targeting or `Some` for a genuinely semantic explicit
  output. Direction-only commands name direction (and window/mode)
  only; core resolves the target from retained FULL rects. `SendSource`
  (`HomeThenAction`, `HomeThenCachedLive`) resolves from frozen
  pre-observation retained homing plus the action context
  (`action_foreground`, `action_cached`, `action_outputs`); the adapter
  never preselects.
- Clocks: `monotonic_ms` is an arbitrary-epoch adapter millisecond
  clock (never wall-clock), one sample per report, seating retry
  backoff together with per-attempt stamps.

### Host-owned adoption amendment (tentative, 2026-10-25)

- Host-owned adapters supply only native membership (`nativeWs`) and native
  desktop order/current. Core derives logical member/view receipts; adapters
  read output-specific `native_binding` and keep only live native-id/handle
  lookups, never their own native-to-logical table. Tiler-owned domains keep the
  logical receipt route. This amends the earlier macOS receipt assumption.
- `current_native: Option<String>` is proposed (`current: null` on the wire):
  a non-desktop current surface keeps the retained logical view while its desktop
  order is still used. Fullscreen/system surfaces omitted from the order are
  never adopted; unknown membership holds its retained home, never defaults to
  the current desktop. KDE reports a desktop current id.
- Native order wins after host-owned creation/reorder. Identical spanning lists
  form one logical workspace set: shared mode is fully supported, separate-view
  modes are degraded and require an adapter warning. Missing/partial lists never
  prove deletion; complete vanished ids are removals, never renames. Generations
  re-adopt from fresh observation. Sticky stays float-only over all managed
  workspaces of its owning output, not all outputs.
- Stage 1b is an unaccepted draft, not a consumable new API. Its proposed
  `NativeCreationDesire` / `NativeCreationAttempt` needs stronger correlation:
  an accepted asynchronous setter and a unique later native-order addition do
  not prove which operation created that desktop. Hold ambiguous creations,
  spend each operation at most once, and never infer native arrival from counts.
  See the [counterexample and next design choice](../changes/kde-world-v2-port.md#stage-1b-creation-correlation-blocker).

### Classification ingress and remote transport rough edge

The shared policy entry points also include
[`window_policy.rs`](../../crates/tiler-core/src/window_policy.rs): adapters
normalize `SafetyFacts` (visible/minimized/cloaked, privileged, shell, tool,
owner/dialog and no-activate) plus fullscreen/maximized state, then invoke
`classify_geometry`/`classify_focus`. Fixed-size, born-state and admission
predicates stay in core too. Native role/type, ownership/modal relationships,
controllability and client size hints must be grounded before normalization;
app identity for rules stays truthful, not a policy verdict fabricated by JS.

Current `WorldReport::windows` is already the projected `WindowFact` roster;
the v2 codec does not carry raw `SafetyFacts`, richer native role/app-rule facts,
or a remote classification request. Windows calls the shared policy in-process.
A remote broker must expose a fact-capture transport to its Rust policy owner
and run these same core entry points there, not copy them into the observer.
That wrapper transport is port work; a richer shared ingress would be a larger
interface change to review, not a compatibility route or an API already shipped.

## 3. Identity, sequencing and dedup

The broker responsibilities below also apply to macOS and future remote
adapters, not only KDE. They share the host-adoption and asynchronous-settlement
concerns in sections 5 and 7.

- One logical adapter/broker per platform, not transparent
  multi-client RPC. Sole report identity (`owner`,
  `adapter_incarnation`) and sequencer: one complete stream, one
  request/observation namespace, one monotonic clock.
- `observation_seq` advances on any invalidating native input,
  including during an outstanding request. Merely numbering requests
  is insufficient. `request_seq` is monotonic per
  (incarnation, generation); a reused `request_seq` with different
  input is invalid, older requests get a bounded superseded error.
- `command_seq` is bound to the adapter incarnation/core generation
  high-watermark and consumed exactly once; commandless reports never
  move it. Duplicates return the cached reply with no new revision.
- Generation changes cancel flights, ordering and retained world
  state. Incarnation resets state. Independent counters from multiple
  clients collide: fold every other client's cursor, gesture and
  presentation facts into the one coherent report, preserving
  invalidation, lifetime and one gesture identity.
- Timeout never mints a new numbered duplicate. A lost acknowledgement
  redelivers the same attempt stamp; the core consumes attempts
  idempotently by `attempt_ms`.
- Core command dedup is one high-watermark; retained transfers and
  clears use `op-<command_seq>` identities correlated to their
  dispatching command. One-shot effects (`fx-<seq>`) mint once and are
  spent before first native dispatch; repeated diffs never reapply.
  Subscribers never replay one-shot effects or terminal results.

## 4. Desired world state: what the reply authorizes

One full snapshot per accepted report
([`crates/tiler-core/src/world_publish.rs`](../../crates/tiler-core/src/world_publish.rs)). Omission inside a held
scope authorizes nothing; every known scope gets an explicit
`ScopeStatus` row (`Controlled`, `Untouched`, `Hold` with a
`HoldReason`: unreadable, incomplete-inventory, gesture-barrier,
refused-target, ambiguous-creation, occupied-removal,
constraint-clamp, minimized).

- Window rows (`WindowDesire`) carry separate frame and membership
  authority: floats, excluded and game rows may authorize membership
  without authorizing frame changes. A row with `floating` true and
  untouched frame authority converges float intent; the adapter never
  infers float from a missing allocation. Ordinary floats keep their
  frames with no authority; capacity floats are functionally ordinary
  (`float_origin` is diagnostic only, never a behavior branch).
- `frame_operation` (`op-<cmdSeq>`) authorizes exactly one explicit
  float-placement geometry write for that window; `None` on a float row
  authorizes no float write (ordinary floats keep their frames, and a
  bare `Controlled` frame authority never writes float geometry).
  Repeatable tiled reconcile is separate: a `Controlled` tiled scope
  with an allocation actuates mismatch corrections with no operation
  token, under the clamp/suppression lane (`world_frames.rs`).
  `clear_operation`
  authorizes exactly one nonactivating maximize clear. Both clear on
  confirmation, proven refusal, supersession, close, newer intent or
  generation reset. Never re-actuated.
- Workspace rows publish the stable logical id, owning output, scope,
  order, planner-owned tiling mode and removal eligibility. The adapter
  never deletes logical workspaces; `existence=false` is removal
  eligibility only (re-enumerate occupants, pins and minimum count;
  unknown occupancy keeps). `native_binding` carries an adopted native id only
  under `host_owned_workspace_domains`, and only for that observed output.
  Unbound trailing spares, mirrored/migrated rows without observed native
  backing and all default native-free rows carry `None`, never guessed backing.
- Output rows publish the retained logical view target (informational
  retained view, never a focus command) and the planner-owned tiling
  mode of the current view.
- Transfers (`TransferTarget`) stay outstanding until confirmed by
  observation, superseded by newer external input, or settled by proven
  refusal. Possibly-executed evidence holds the target without
  re-actuation. Focus/raise/stack are one-shot effects, never retained.
- Results: the correlated per-command outcome rides the issuing reply
  only. The requesting client gets transient terminal results
  (drag-preview, toggle-float, migration provenance); subscribers
  consume the snapshot, transfers and effects. A cache is insufficient
  for terminal results; route them to the requester (issuing-reply-only
  results).
- Reconnect gets the current accepted presentation; no replay of
  one-shot state. Separate presentation/query/command authorization
  from exclusive report/actuator authority.

## 5. Raw attempts and settlement

The adapter writes, re-reads and reports; the core owns every verdict
([`crates/tiler-core/src/world_settle.rs`](../../crates/tiler-core/src/world_settle.rs),
[`crates/tiler-core/src/world_frames.rs`](../../crates/tiler-core/src/world_frames.rs)). Attempt rows are mechanical
facts only, at most one latest-wins entry per window/operation/phase:

- `FrameAttempt`: attempted frame (echoed desire as correlation),
  immediate post-write readback (`None` unreadable), DPI before/after
  (`None` unreadable, or the adapter coordinate space has no per-window
  basis; never guessed; a changed basis invalidates the placement),
  `attempt_ms`, optional `operation_id` (`Some("op-<cmdSeq>")`
  only for explicit float-placement writes; `None` never settles a
  float) and `prewrite` (live frame before the write; required with an
  operation id). Only an explicit native refusal with an unchanged
  readable preimage proves refusal of an explicit float-placement
  attempt; anything else latches ambiguity. Repeatable tiled-sweep
  retries are governed by the clamp/suppression lane, not by this
  refusal latch.
- `MemberAttempt` / `ViewAttempt` / `ClearAttempt`: same
  write/re-read/report split for membership homing, exact-output view
  and maximized-state proof. Setter success never proves arrival by
  itself; readable post states plus staged observations settle.
- Acknowledgement never copies desires into observations. Incorporation
  receipts bind to the incorporating generation; foreign-generation,
  foreign-adapter and wrong-request replies apply nothing.
- Logical receipts do NOT prove native hide/reveal. Settlement needs a
  fresh native readback (visibility, frame, foreground) joined to the
  managed lifetime. Awaiting native proof is a hold, not a failure;
  fabricating arrival is a defect.
- Carry completed dispatch attempts into the next freshly captured report:
  its window observations must not predate the attempts' immediate readbacks.
  Core prefers those later readable facts for ordinary frame suppression and
  learned minima. "Later" does not mean a native asynchronous configure has
  completed; report unknown honestly and never manufacture settled evidence.

## 6. Lifetime, freshness and scope vetoes

Native lifetime, replacement and exact recovery ownership stay
adapter-mechanical, with last-moment freshness checks before every
native effect:

- Unreadable frames, incomplete topology/inventory, gesture barriers,
  minimized departures and held properties authorize no writes through that
  held authority. Retained allocations may remain as context (for example a
  constraint clamp); their presence is not write permission. Evaluate frame
  and membership authority independently, including explicit transfer lanes.
- The adapter rechecks fresh native identity, foreground, visibility,
  band and fullscreen facts at dispatch; stale or mismatched proof
  refuses that effect narrowly without touching the rest of the reply.
- Host limitations ride explicit fences: minimized slots, foreign or
  unmanaged foregrounds (presentation-eligible, never members),
  game/fullscreen lanes and unreadable facts. Report inability through
  truthful feedback (refusal, unknown, hold), never fabricated backing,
  arrival or capability.
- Generation/incarnation resets clear all retained flights, ordering
  and desired caches. Old effects, flights and desired rows never leak
  across the replacement: the adapter re-discovers and re-observes.

## 7. Host limits, coordinates, DPI and gaming

### Host-owned adoption (core delivered; native acceptance pending)

Under `host_owned_workspace_domains`, a fresh core generation must adopt each
display's observed native desktops/Spaces in native order and preserve observed
window membership. Do not move windows between workspaces at launch. Continue
tiling through the existing startup arrangement inference from size/position.
After startup, adopt outside host membership moves and newly created native
desktops. This is an approved core change for the KDE port to deliver and accept
against real native observations; macOS reuses it, not a separate policy copy.
The discriminator is four occupied native desktops, fresh generation, four
native-bound logical workspaces in native order with members preserved and no
launch workspace moves. Tentative count clarification retains the ordinary
trailing spare under existing lifecycle rules; it does not require exactly
four total rows. Core coverage is delivered; native outcome remains TBD.
See [decision](../decisions.md#native-adoption-2026-10-24).

Native-bound empty desktops retain identity; missing native rows never prove
removal. Linked views use one compatible logical order; fresh native projections
whose first-appearance union reverses a display's order hold without adoption
rather than choosing a new merge policy. General native-order merging and
correlation of empty native creation to an unbound desired spare remain
[parked neutrality concerns](../changes/kde-world-v2-port.md#decisions-and-core-neutrality-concerns),
not extra fields or platform-side policy to invent during a port.

### Coordinates and safety

- One coherent adapter coordinate space per report (screens FULL,
  work areas, fact frames, cursor, presentation shapes all agree).
  The adapter owns all DPI conversion before the report and all
  native conversion when actuating/rendering. Never mix client, outer
  and visible-frame spaces; join them explicitly per platform.
- Size hints are client sizes; convert to decorated frames with the
  platform's own frame facts (KWin window `clientGeometry` versus
  `frameGeometry`), never with another platform's compensation. KDE still
  needs this window-client capture; its current `readClientArea` instead
  reads `workspace.clientArea`, the desktop work area. Learned minima ride the
  shared clamp/suppression lane: the shipped Windows fix (`01a73798`) prefers
  a later staged frame over the immediate readback (exact immediate
  target plus eligible later same-origin growth learns minima; the
  producer reports prior attempts then fresh facts), and later facts
  are not native completion. A KDE/Wayland pending-configure risk
  remains source-derived, not a reproduced failure. The KDE port first
  reproduces it and checks whether KWin exposes pending/completion evidence;
  only a reproduced failure justifies proposing a neutral core settling
  representation for user approval. The Windows log never showed a clamp Hold,
  so it does not establish that pending resize was misread as a clamp.
- Gaming safety wins over every overlay and input route: pause border,
  underlay, pager and cursor/overlay work while a focused fullscreen
  game holds the session. Never add game-costing hooks or polling; the
  `paused` lane hides presentation without projecting. Explicit
  commands are honored; no unsolicited fullscreen writes and no
  activation/focus stealing.

## 8. Border, underlay, drop preview and pager

- Core publishes uniform per-reply presentation
  ([`crates/tiler-core/src/world_presentation.rs`](../../crates/tiler-core/src/world_presentation.rs), `WorldPresentation`:
  foreground token, border shape, underlay shape, pager content).
  Eligibility, reasons and complete bounds are core-owned; the adapter
  keeps only native facts (proof-verified foreground identity,
  liveness, scope/proof gates, host-scaled style numbers, native order,
  drawing, final freshness vetoes).
- The renderer consumes core bounds only: delete any adapter-side
  geometry/policy derivation and consume the published shapes. Native
  styling stays native (colors, anchoring, artwork,
  click-to-command): never derive those from core either. It
  revalidates the live foreground against the reply token and never
  paints stale shapes. Native anchoring plus immediate hide-only vetoes
  stay renderer-side (unknown/unreadable foreground or scope frame
  hides instead of guessing).
- Host styling is two independent colors on every port, never
  ring-coupled. The active-window ring keeps its own border
  color/theme tokens (default: live system accent with the `#2a82da`
  fallback); the pager focused-window highlight keeps independent
  highlight color/theme tokens (Windows: `core.pager_highlight.*`;
  default: live system accent with the same fallback). An explicit
  override on either side never touches the other. The focused
  miniature paints a full-strength accent border over a dimmer accent
  fill of the same color; unfocused miniatures are unchanged. The
  pager active workspace cell keeps the system accent with no theme
  switch.
- Drop preview and group highlight come from the same reply; the
  adapter reports the gesture lifecycle (started/updated/released/
  cancelled with the stable gesture id) and applies the commit.
  A future pager/plasmoid consumes cached presentation plus
  notifications with its own client ids; it never invents authority.

## 9. Core-only acceptance baseline and the honest native gap

- Baseline (accepted, core-only):
  `test-fixtures/world-actions.json`, guide
  `test-fixtures/world-actions.md`, runner
  `crates/tiler-protocol/examples/world_acceptance.rs`
  (base `83d77f5c`). One single-output, tiled-only, zero-gap case:
  bootstrap, admission, select-away, select-previous return. It proves
  desired roster/homing/authorities, actual allocations with work-area
  coverage, oracle tiles, views, rings, retained view flights, effects
  and report/reply pairing. Out of scope by design: float, sticky,
  fullscreen, maximize, minimize, constraints, gaps, host-owned
  workspaces, gestures, cursor/warp, migration, sends, presentation.
- The away interval proves only the LOGICAL view receipt and stable
  homing while away is selected. Nothing in the core baseline claims
  native hide/reveal.
- Native acceptance needs a separate leg: fresh caller/lifetime/output/
  operation/provenance joins per run plus mandatory independent native
  hide/reveal checkpoints (visibility, frame, foreground readbacks).
  Controlled-scope and effect fields must follow the actual trace
  vocabulary of the producer; no new diagnostic is added solely to
  mirror a retired draft.
- The next narrow checker must FIRST reject missing readback, pending
  operations, omissions/fences, coverage defects and skipped
  intermediate hide/reveal before any broader claim. No new connector
  is built here.

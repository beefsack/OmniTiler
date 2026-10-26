# World-contract platform adapter guide (v2)

Durable generic guide for every platform adapter (Windows reference,
KDE and macOS ports). The remote contract is strict v2 with no
compatibility route: v1 payloads, field names and discovery paths
refuse and are never shimmed.

KDE's paired cutover and observe/actuate boundary are complete offline. Its
[port record](../changes/kde-world-v2-port.md) tracks accepted evidence and
remaining classification, renderer and host-integration work. Fresh backing,
source-view supersession and raw attempts pass offline; native settlement must
not be inferred from wire acceptance.

Source roots below are relative to the repository root. Core symbols
live in `crates/tiler-core/src/`; the JSON codec lives in
`crates/tiler-protocol/src/world_protocol.rs`.

## Shared stress acceptance connectors

- Reuse `test-fixtures/world-stress-*.json` and the strict contract in
  `test-fixtures/world-stress.md`; the core-only example runs ordered, batched
  and reordered host schedules. Do not fork expected worlds per port.
- Native connectors map actions to existing production routes, support added
  inter-action delay including zero, and query fixture identity, native frames,
  visibility and foreground independently of queue acceptance. Settle only at
  explicit observation barriers; report transport overhead separately.
- Join the host's real membership/view and renderer evidence without inventing
  freshness or silent focus repairs. KDE uses its native Plasma pager, not a
  replacement OmniTiler pager. User-approved 2026-10-27: hidden native
  geometry is corroborated against retained raw frame proof, not the logical
  tile; require exact tile geometry on reveal. Preserve the hidden-write fence.
  [Current status](../changes/platform-neutral-stress-acceptance.md).

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
- No-focused-window directional focus: send `FocusDirectional` with an empty
  window (`{"op":"focus-directional","win":"","direction":"right"}` on
  the wire), not a remembered window or a local fallback. Encoding is tentative
  (pending user review); behavior is User-approved 2026-10-26. Report genuinely
  absent `focused_global` and normal action-context facts. Core crosses from
  the current output by existing selection, otherwise no-ops; a contradictory
  nonempty observed focus refuses. Missing/null `win` still refuses. Move/send
  still require a subject. Observation-only host focus loss needs no recovery
  command or focus setter. [Decision](../decisions.md#workspaces).
- Tiling-mode actions: `ToggleWorkspaceTiling { output, workspace }` (wire
  `toggle-workspace-tiling`, `out/ws`) flips core-retained mode at execution.
  Never negate cached reply mode into `SetWorkspaceTiling`; keep explicit set
  only for a genuine requested mode. Both target stable logical workspace ids.
- Automatic core birth-clear is opt-in: `WorldReport.birth_clear`, wire
  `birthClear`, missing defaults false; a present non-bool refuses. KDE opts in;
  Windows keeps its existing native birth route and macOS does not opt in.
  Disabled reports drop automatic bindings/marks and publish no automatic
  token; explicit directional/unfloat clears are unaffected.
- `WorldReport.born_maximized` (wire `bornMaximized`, missing/invalid defaults
  `tile-on-launch`) selects subsequent-admission mode. Commandless
  `op-born-<serial>` tokens use existing `clear_operation`/raw `ClearAttempt`.
  Tentative independent property authority permits a proven maximize mode
  transition while geometry convergence stays held, never bypassing safety
  fences. Setter acceptance is not completion; fresh native facts settle.
  [N6 is accepted offline](../changes/kde-world-v2-port.md#n6-accepted-offline-outcome),
  not native-accepted. KWin lifecycle facts normalize before `WindowFact`:
  deleted means close, application-handled unmanaged means absent. This does
  not select N5 retained-membership policy or new core controllability facts.
- Clocks: `monotonic_ms` is an arbitrary-epoch adapter millisecond
  clock (never wall-clock), one sample per report, seating retry
  backoff together with per-attempt stamps.

### Host-owned adoption amendment (tentative, 2026-10-25)

- Host-owned adapters supply only native membership (`nativeWs`) and native
  desktop order/current. Core derives logical member/view receipts; adapters
  read output-specific `native_binding` and keep only live native-id/handle
  lookups, never their own native-to-logical table. Tiler-owned domains keep the
  logical receipt route. This amends the earlier macOS receipt assumption.
  Explicit logical window/view receipts are ignored under the capability,
  including for settlement and external-supersession tracking; they cannot
  substitute for unknown native membership/current or veto genuine native proof.
- `current_native: Option<String>` is delivered (`current: null` on the wire):
  a non-desktop current surface keeps the retained logical view while its desktop
  order is still used. Fullscreen/system surfaces omitted from the order are
  never adopted; unknown membership holds its retained home, never defaults to
  the current desktop. KDE reports a desktop current id. Missing/empty `current`
  refuses, rather than meaning `None`. Prefer `native_workspace=None` for a
  fullscreen Space outside `native_order`, unreadable membership or an all-desktop
  window with no single membership. An unlisted, unmapped id also yields unknown,
  never adoption; a still-retained binding can resolve on a partial snapshot
  until complete removal prunes it.
- Native order wins after host-owned creation/reorder. Identical spanning lists
  form one logical workspace set: shared mode is fully supported, separate-view
  modes are degraded and require an adapter warning. Missing/partial lists never
  prove deletion; complete vanished ids are removals, never renames. Generations
  re-adopt from fresh observation. Sticky stays float-only over all managed
  workspaces of its owning output, not all outputs (`world_directional.rs`,
  `effective_domain`, lines 86-104): routing changes the view, never the output.
- `WorldReply.native_creations: Vec<NativeCreationDesire>` (wire
  `nativeCreations:[{op,ws,out}]`) alone authorizes native creation, at most once
  per stable operation id. An unbound row is not authorization; an ordinary
  empty unselected spare requests no creation.
- `WorldReport.creation_attempts: Vec<NativeCreationAttempt>` (wire
  `creationAttempts:[{op,ws,out,setter,before,after,created,attemptMs}]`) carries
  raw execution evidence, not a binding. Rust fields: `operation_id: String`,
  `logical_workspace: String`, `target_output: OutputId`, `setter: FrameSetter`,
  `order_before/order_after: Vec<String>`, `created_native: Option<String>`,
  `attempt_ms: u64`. Wire stamps are decimal strings; `created` is optional
  (`null`, missing or empty means uncorrelated). Missing attempt array means `[]`.
- Tentative hybrid correlation: a host-correlated `created` binds exactly that
  fresh native id once, including raced before/after lists. Without it, one
  outstanding creation on the output can bind the next single new native id.
  Several additions or outstanding creations hold `AmbiguousCreation`; no blind
  re-create. This deliberately accepts an outside-first asynchronous race: the
  outside singleton binds, and the later requested desktop adopts independently.
  Setter acceptance alone is not causal evidence. Review after KDE/macOS land.
- An attempt withdraws execution work; unchanged redelivery is idempotent and
  changed evidence, refusal or invalid timing holds. A correlated id not yet
  observed stays pending, never falls back to an unrelated singleton. Only fresh
  complete native order proves backing. Adapters retain native lookups, not maps.
  `before` is sampled immediately before the write; `after` is the first
  successfully observed full post-call order. When unavailable, keep execution
  evidence locally until readable, preserve the original write stamp, and do not
  invent an empty after-order. A later-turn list delta is uncorrelated (`created`
  null); it cannot supply KDE's same-turn causal proof.
- Host-hidden windows (KDE inactive Activity, macOS Cmd+H) use existing
  `WindowFact.minimized=true`, no new state (tentative pending native Activities
  testing). Activities support remains later port work.
- `minimized=true` is not "never move": frame/clear work and ordinary focus/send
  refuse, and occupancy retains the workspace, but whole-workspace migration,
  output displacement and in-flight membership transfers can relocate it. Do not
  claim strict retained-unmanaged semantics: the qualified Activities promise
  permits workspace migration/swap, host display removal and completion of a
  transfer pending before hide.
  Unknown float-sticky membership can omit its reply row (not a close); unknown
  newcomers remain unhomed. Strict sticky slot retention is parked, not proven.

Minimized path audit (core behavior preserved, 2026-10-25):

| Path | Existing behavior | Source under `crates/tiler-core/src/` |
| --- | --- | --- |
| Frame/maximize clear | No allocation, placement, frame or clear token | `world_publish.rs:736-764,829-888` |
| Focus-by-id/pager click | Refuses `focus-unknown`; minimized absent from live lookup/snapshot | `world_commands.rs:60-99`, `session/world.rs:727-729` |
| Direct send/directional move | Refuses unknown/stale subject, no new transfer | `world_sends.rs:201-216`, `world_operations.rs:378-387` |
| Close-reflow/automatic focus | Excludes minimized candidates | `world_observe.rs:333-348`, `world_model.rs:947-971`, `session/world.rs:1869-1900` |
| Trailing/native desktop removal | Minimized occupancy retains logical workspace, even if binding disappears | `world_model.rs:36-51`, `world_native.rs:288-303` |
| Whole-workspace migration/global swap | Includes non-sticky minimized members; can move membership | `world_commands.rs:790-823,1071-1105` |
| Output removal | Displaced homing follows survivor; can move membership | `world_observe.rs:136-151,498-506,639-647` |
| Transfer already pending at minimize | Publishes target membership, no geometry/unminimize | `world_publish.rs:743-750,837-855` |
| Restore | Host-observed restoration reuses stored neighborhood; not a core restore command | `session/world.rs:1498-1575,1871-1877` |

`tests/world_minimized_native_pinning.rs` pins ordinary refusal, close-reflow,
pager click, occupancy/removal and unknown membership. It does not change the
relocation paths or establish strict retained-unmanaged semantics.

### Classification ingress and remote transport rough edge

The shared policy entry points also include
[`window_policy.rs`](../../crates/tiler-core/src/window_policy.rs): adapters
normalize `SafetyFacts` (visible/minimized/cloaked, privileged, shell, tool,
owner/dialog and no-activate) plus fullscreen/maximized state, then invoke
`classify_geometry`/`classify_focus`. Fixed-size, born-state and admission
predicates stay in core too. Native role/type, ownership/modal relationships,
controllability and client size hints must be grounded before normalization;
app identity for rules stays truthful, not a policy verdict fabricated by JS.

Tentative shared classification A (2026-10-25) adds in-process
`classify_membership(&SafetyFacts, transient)` and `MembershipVerdict` (tile,
float member, leave unmanaged). The single neutral transient fact describes a
dialog/sheet/modal rather than a standalone document; existing safety exclusions
win. macOS uses this verdict and projects float membership into the existing
`WindowFact.floating` flag. Windows still uses its unchanged geometry/focus
entry points: both `Dialog` and `OwnedDialog` remain exclusions, not float
members. No raw classification field or wire version is added.

Current `WorldReport::windows` is already the projected `WindowFact` roster;
the v2 codec does not carry raw `SafetyFacts`, richer native role/app-rule facts,
or a remote classification request. Windows calls the shared policy in-process.
A remote broker must expose a fact-capture transport to its Rust policy owner
and run these same core entry points there, not copy them into the observer.
KDE's wrapper now carries raw candidate facts and calls the existing shared
fixed predicate in Rust. Role/control normalization and the remaining policy
deletions are still [partial port work](../changes/kde-world-v2-port.md#stage-4-partial-outcome-and-deletion-boundary);
a richer shared ingress would be a larger interface change to review, not a
compatibility route or an API already shipped.

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
- Core command dedup is one high-watermark; command-driven retained transfers
  and clears use `op-<command_seq>` identities correlated to their
  dispatching command. Opt-in commandless birth clears instead use core-minted
  `op-born-<serial>`, monotonic through incarnation rebinds within a generation.
  One-shot effects (`fx-<seq>`) mint once and are
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
  generation reset. Never re-actuated. A birth clear may carry its independent
  property token while frame/membership authorities remain held; this grants
  no frame/member/focus write and never bypasses a window safety fence.
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
- Member/view attempt logical pre/post/baseline name logical workspaces.
  Host-owned adapters leave those fields unknown, report real setter disposition
  with the exact desired-target correlation, then capture fresh native facts.
  Never encode native ids as logical ids or rebuild an adapter binding owner.
  The separate native fields below establish host-owned attempt evidence without
  translating through an adapter table; see the
  [B2 discriminator](../changes/kde-world-v2-port.md#b2-source-view-rewind-during-migration).
- Delivered core B2 extension (tentative product contract, later review pending):
  `MemberAttempt.native_pre/native_post: Option<String>` and
  `ViewAttempt.native_pre/native_post/native_baseline: Option<String>`, wire
  `nativePre/nativePost/nativeBase`. Non-null without host-owned capability
  refuses; missing/null/empty native values mean unknown. Host-owned settlement
  ignores logical attempt evidence and translates native ids only through core
  bindings; tiler-owned domains keep logical preimages. Capture real native
  reads around writes and the observation-time view baseline, never desires or
  an adapter binding table. A matching outstanding change settles; other input
  is adopted. The released-source rule below supplies the identity boundary.
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
eligible existing-native reuse remain
[parked neutrality concerns](../changes/kde-world-v2-port.md#decisions-and-core-neutrality-concerns),
not extra fields or platform-side policy to invent during a port.
Migration keeps the stable logical id. Existing backing for another logical
workspace is not eligible reuse; fresh creation desires/correlation supply
missing backing. Tentative released-source option 1 supersedes dormant return
backing: release the source binding on migration; adopt its unbound desktop as a
fresh empty logical workspace whenever observed current or otherwise listed
without a logical workspace, at authoritative native-order position. The
migrated logical stays at its destination; the user's switch wins and the paired
source refill retires permanently. No reverse provenance or eligible-spare reuse;
ordinary trailing/equal-count lifecycle is unchanged. This is platform-neutral,
including macOS Spaces. Core releases after positive arrival or explicit
close/newer off-output proof, never an absent-occupant guess; refusal rollback
retains backing. Incomplete native lists or pending creations defer cleanup,
retried independently after transfer retirement. Fresh source adoption stages
its echo on the next report, so the first proven-rewind reply retains the
migration parent; the paired refill retires without re-application and the
parent then settles against observed views. Retained transfer targets survive
trailing lifecycle while their correlated backing is still pending.
Core-published actuatable scopes and desired window rows, including a complete
empty set, drive actuation without requester-only results;
the adapter never selects a replacement source refill by ordinal.

### Coordinates and safety

- One coherent adapter coordinate space per report (screens FULL,
  work areas, fact frames, cursor, presentation shapes all agree).
  The adapter owns all DPI conversion before the report and all
  native conversion when actuating/rendering. Never mix client, outer
  and visible-frame spaces; join them explicitly per platform.
- Size hints are client sizes; convert to decorated frames with the
  platform's own frame facts (KWin window `clientGeometry` versus
  `frameGeometry`), never with another platform's compensation. KDE captures
  both window rectangles and adds their rounded per-axis extent difference to
  meaningful client hints. `readClientArea` instead reads `workspace.clientArea`,
  the desktop work area. Conversion stays exact even above the current core
  hint-sanitization bound; that constraint-loss concern is parked in the
  [port record](../changes/kde-world-v2-port.md#stage-3-outcome-2026-10-26).
  Learned minima ride the
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
- Neutral fullscreen presentation suspension is a separate, default-off
  construction opt-in:
  `WorldEngine::with_focused_fullscreen_presentation_suspension()`.
  KDE enables it; Windows and macOS construction remains unchanged. Current
  `focused_global` plus the corresponding `WindowFact.fullscreen/output` selects
  the suspended output, with no game identity or geometry inference. No matching
  fact means no invented output verdict. Hosts keep fullscreen facts truthful:
  KWin `fullScreen`; macOS native fullscreen when that adapter opts in later.
  This does not reuse or broaden global `PresentationIngress.paused`.
- `WorldPresentation.suspended_output` encodes as optional `suspendedOutput`,
  omitted when inactive so default publication bytes stay unchanged. Core hides
  border/underlay targeting that output and omits only its pager content. A
  renderer drops preview paint whose `DragPreview.domain.output` matches this
  verdict, not whose mover-home source matches; other-output previews remain
  eligible. Command results and gesture lifecycle are unchanged. Fresh native
  mode/focus invalidation must fence both pre-entry and pre-exit paints; an exit
  never authorizes restoration from stale presentation. Native hide-only vetoes
  remain. These API/wire names are tentative technical vocabulary.
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

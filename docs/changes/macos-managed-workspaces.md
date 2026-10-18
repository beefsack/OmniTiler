# macOS managed native workspaces

## Product send-follow partial live checkpoint (2026-10-19)

**Outward numbered product send-follow works; TextEdit roundtrip acceptance is
blocked by input ownership, not runtime readiness.** No product cause or fix is
established. Do not yet treat numbered sends as dogfood-ready.

- At `7641167b`, outward product `send-2` again receives admission and settles
  `Applied`: exact helper membership `[1]` -> `[277]`, current 277, focus follows,
  indicator current=2 and border updates agree. No scoped image was inspected;
  visual layout/border/indicator acceptance remains pending.
- Follow-up runs dispatch no inverse: one exceeds the required lifetime margin;
  another starts on 277 and `send-2` settles `Applied` without moving. Current
  native send ordinals follow SLS order `[1,277]`, even when current is 277.
  The previous current-first scoped-ring explanation of an earlier refusal is
  withdrawn as unproven: that engine mapping is not the current native send
  ordinal resolver. Same-current `Applied` no-op behavior is observed, not a new
  product decision.
- The earlier inverse refusal still names only `engine-facts`, with helper
  observer teardown 264 ms after admission and no anchored holder age. Expiry
  remains possible. Reviewed diagnostics `c052ae08`/`bbf7eb00` distinguish
  `engine-facts-unmanaged` from `engine-facts-noreply`, preserving zero-post
  refusal. **Neither split token has yet been observed on a live inverse.**
- Earlier helper runs falsely abort on settings-backup and helper-record mtime
  predicates. Those predicates are rejected; no further helper roundtrip is a
  prerequisite to TextEdit acceptance.
- **TextEdit input blocker, independently source-verified at `ec9a2083`:**
  `owned_control.rs` constructs `OwnedPostTarget::from_spawn_record` and requires
  fresh `focus::read_agreement` before each key edge. `synth.rs` authenticates a
  helper spawn record and joins its exact CG/AX window. A helper-targeted chord
  therefore refuses while TextEdit is focused; focusing the helper changes the
  admitted product subject. TextEdit has no accepted owned-post target proof.
  The separate managed-send permit does not authorize diagnostic input posting.
  No existing TextEdit chord-post route was found, and no live run, ownership
  bypass or new action arming was attempted in this investigation.
- Last macOS source gates remain **1010 tests**, strict all-target clippy,
  format and signed build/verify. No production code or dependencies change in
  this follow-up. [CI 39135233669](https://github.com/beefsack/OmniTiler/actions/runs/39135233669)
  passes at `428d53ac`; the intervening change is KDE/core work, not macOS runtime
  code. Unsafe remains **114 blocks**, all in unchanged `ffi.rs`.
- Current Space is restored through the existing guarded select route. Final
  topology is `[1,277]`, current 1; holders match baseline with no preimage.
  Owned helpers/runtimes/display-awake children are gone. Preexisting Space 277
  and disposable TextEdit documents remain. Final raw protected-window records
  exactly match the baseline below, including onscreen state:

  ```json
  [
    {"bounds":{"h":1139.0,"w":1150.0,"x":0.0,"y":30.0},"layer":0,"onscreen":true,"pid":31863,"wid":3675},
    {"bounds":{"h":64.0,"w":64.0,"x":0.0,"y":760.0},"layer":0,"onscreen":false,"pid":31863,"wid":3676},
    {"bounds":{"h":1139.0,"w":1150.0,"x":0.0,"y":30.0},"layer":0,"onscreen":false,"pid":31863,"wid":3908},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4064},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4068},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4070},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4073}
  ]
  ```

Next: resolve TextEdit input authority: either use user physical chords or define
and review an exact-disposable TextEdit input proof without weakening helper
ownership. Then execute procedure 1 under one scoped 120-second runtime, using
observed runtime startup/admission lines, never file mtimes. Require outward and
inverse membership/focus, both layouts, indicator, border and scoped visual
evidence. Same-current honest no-op semantics follow that acceptance; shared
KDE/Windows alignment is tentative pending source comparison and a discriminating
spec row. Send-stay, trailing lifecycle and per-display work remain parked;
two-display live checks remain pending. Display-handoff acquisition remains
tentative.

## Effect-owner display handoff checkpoint (2026-10-18)

**The offline snapshot defect is repaired; live send-follow acceptance remains
pending.** This supersedes the acquisition blocker below, not its live outcome.

- `5e523000` removes AppKit display reads from the effect owner. The tentative
  acquisition choice is a latest-wins plain-data handoff: CoreGraphics bounds
  cannot reproduce AppKit visible geometry, scale and separate-Spaces inputs
  without changing the existing topology fingerprint.
- `5226de28` gates publication on startup and screen-parameter notifications;
  steady-state topology polling does not overwrite the slot. `202afae6` restores
  an invalidated/initially absent slot from the next good main-thread snapshot,
  preventing permanent send refusal after a transient read failure. No new poll
  or synchronous main-thread wait is introduced. Existing topology polling is
  unchanged; notification publication uses that loop's already-read snapshot.
- The effect owner joins published display legs with fresh native Space rows and
  types under the identical fingerprint. Engine-managed authority, per-edge
  revision/ordinal checks, AX/CG/focus checks and unconditional release remain.
  Spawned-thread regression coverage exercises the real native reader; gate and
  invalidation/recovery tests cover publication behavior.
- Final macOS tests pass (855 library tests plus integration suites), strict
  all-target macOS clippy and workspace format pass, signed app build and separate
  verification pass. Unsafe remains **114 blocks**, all in unchanged `ffi.rs`;
  no dependencies added. Independent review accepted the owner boundary and found
  the subsequently corrected invalidation recovery gap.

Next: independently accept the recovery correction, then run the owned-helper
product-tap roundtrip before TextEdit procedure 1. Physical shortcut feel remains
user-owned; synthetic transfer cannot establish it.

## Product-tap refusal checkpoint (2026-10-18)

**Numbered send-follow is armed but not ready for physical send dogfooding.**
The owned-helper product tap reaches engine admission; native execution refuses
before posting. TextEdit acceptance procedure 1, layout/focus/border/indicator
transfer acceptance, and send-stay remain pending.

- CI at `5950b795` passes all eight jobs
  ([39122511678](https://github.com/beefsack/OmniTiler/actions/runs/39122511678)),
  verifying the off-macOS unused-mut repair. Diagnostic-only `93d2b7f9` adds one
  bounded effect-owner refusal line with stage, reason and per-operation post
  counts, without changing admission, effects, completion or release semantics.
  Independent review accepts it; 854 library tests plus integration suites,
  strict all-target macOS clippy, format, signed build and separate verification
  pass. Its [CI 39124331455](https://github.com/beefsack/OmniTiler/actions/runs/39124331455)
  also passes all eight jobs. Unsafe remains **114 blocks**, all in unchanged
  `ffi.rs`; no dependency changes.
- Two bounded helper-only Authentic runs consume the owned `send-2` chord and
  report `send-accepted`, `send-enqueued` and `submitted`, then settle `Refused`.
  The first run's subsequent `send-1` is an expected same-current-Space refusal,
  not an inverse transfer. The diagnostic run isolates the actual failure:

  ```text
  stage=snapshot
  reason=fresh row unreadable (product send refuses: fresh row unreadable (send topology unreadable: display snapshot (display snapshot requires the main thread)))
  dposts=md:0 mu:0 kd:0 ku:0
  ```

- `OsSendReaders::fresh_topology` calls main-thread-only
  `ns_display_snapshot` on the dedicated effect owner. An offline spawned-thread
  reproduction returns the same refusal. No transfer or inverse occurs; helper
  membership stays `[1]`, current Space stays 1. This is a product defect, not
  another dispatch or holder-delivery failure.
- The topology revision includes visible display geometry, scale, ordered native
  Space rows/current and separate-Spaces mode. Replacing AppKit data with CG
  bounds or unknown mode would change the fingerprint, not repair the proof.
  Next implementation requires a reviewed main-thread snapshot acquisition or
  handoff contract, retaining fresh per-edge native row/ordinal validation and
  engine-managed authority. No cached-snapshot authority substitution or weaker
  revision domain is selected here.
- Both signed runs report completed/Clean teardown. The launch wrapper exits 1
  because diagnostic stderr is nonempty; its status is not the signed child's
  outcome. Each Revert restores all nine disabled holders with unchanged chords,
  no external edits/failures, and removes the owned preimage. Final topology is
  `[1,277]`, current 1. Owned helpers, runtime owners and display-awake children
  are gone. Existing Space 277 and the three reusable TextEdit documents remain;
  no Space is created or removed.
- Seven title-free protected-window records compare byte-identical before/after
  each run. The following raw records are identical in the diagnostic run's
  before and after responses:

  ```json
  [
    {"bounds":{"h":1139.0,"w":1150.0,"x":0.0,"y":30.0},"layer":0,"onscreen":true,"pid":31863,"wid":3675},
    {"bounds":{"h":64.0,"w":64.0,"x":0.0,"y":760.0},"layer":0,"onscreen":false,"pid":31863,"wid":3676},
    {"bounds":{"h":1139.0,"w":1150.0,"x":0.0,"y":30.0},"layer":0,"onscreen":false,"pid":31863,"wid":3908},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4064},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4068},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4070},
    {"bounds":{"h":30.0,"w":2240.0,"x":0.0,"y":0.0},"layer":0,"onscreen":false,"pid":31863,"wid":4073}
  ]
  ```

- No screenshots are taken: no transfer visual state is reached. Raw native and
  terminal responses remain separate temporary evidence, not committed content.
  Diagnostic-run main executable SHA-256:
  `848201dcfc1b27888f8233b5dec530aaf608dc7822a0a0fc104561a52045bcb5`;
  helper: `ee0ac6574157589ec390cecc6753838c4641661b68225db9ff899afe316255c5`.
  Bundle IDs are `io.github.omnitiler.macos.probe` and
  `io.github.omnitiler.macos.probe.helper`, signed by `OmniTiler Mac Dev` with
  designated identifier plus certificate leaf
  `H"040f96377aaf110a77905d453bb76681e7133319"`.

Next: settle the fresh topology acquisition contract, add a regression exercising
the real effect-thread reader boundary, implement and review the fix, then repeat
one owned-helper product-tap roundtrip before TextEdit procedure 1. Keep send-stay,
trailing-Space lifecycle and multi-display work parked.

### Dogfood checklist

- **Current status:** basic actions and existing-Space selects can be checked;
  outward helper product send-follow passes, but TextEdit roundtrip/visual
  acceptance is pending. Existing owned-post requires helper foreground and
  cannot deliver a chord with TextEdit focused. Resolve input authority before
  synthetic TextEdit acceptance; no helper roundtrip prerequisite remains. No
  new actions are armed. Earlier refusals and readiness aborts are not successful
  roundtrip acceptance.
- The usual user entry is `mise exec -- just dev trace`. It builds/signs before
  launch, is unbounded and unscoped, and can tile ordinary terminal windows.
  For this disposable TextEdit check, use the scoped, explicit Authentic/Ctrl
  form instead. Start on Desktop 1 with two ordinary native Spaces, two
  disposable TextEdit windows on Desktop 1 and one on Desktop 2. Require an
  unlocked session, inactive Secure Input and the existing Accessibility grant.
  Run these individually, inspecting each response before continuing:

  ```text
  mise exec -- just preflight-app
  mise exec -- just launch -- spaces-desktop-status
  mise exec -- just launch -- spaces-diagnose
  mise exec -- just tile --user-start --scope com.apple.TextEdit --input tap --preset authentic --seconds 120
  ```

- Authentic automatically enables standard Desktop-1..9 holders with unchanged
  chords and durable owned preimages. It must not replace customized chords.
  Compatible does not write holders. Record status before the run; do not
  manually Apply settings just to hide a startup failure.
- Try Ctrl+2 then Ctrl+1; check the current indicator and each window's retained
  slot. Try Ctrl+H/J/K/L for focus, Ctrl+Shift+H/J/K/L for movement, and Ctrl+G
  to float then re-tile. The border should follow the focused managed window;
  the indicator should track the current native Space.
- **Procedure 1, physical chords or reviewed TextEdit input proof:** focus one
  disposable Desktop-1 document, press Ctrl+Shift+2, wait for settlement, then
  Ctrl+Shift+1 to return it. Require exact membership and focus-follow, reflow on
  both Spaces, retained slots on revisiting, current indicator and following
  border. Physical shortcut feel and Mission Control jank require this manual
  observation; synthetic posting is not a substitute.
- Send-stay, trailing empty-Space selection/send and multi-display transfers are
  not ready. Use one display; do not try Ctrl+0 or Ctrl+Shift+0 as acceptance.
- Wait for bounded teardown, or press Ctrl+C once for graceful stop; a second
  terminal can run `mise exec -- just dev-stop`. Stop leaves window frames in
  place. Then restore owned holders and inspect final state:

  ```text
  mise exec -- just launch -- spaces-desktop-revert
  mise exec -- just launch -- spaces-desktop-status
  mise exec -- just launch -- spaces-diagnose
  ```

- Report the commit, exact chord/action sequence, source/target desktop numbers,
  expected versus actual membership/focus/layout, and whether the border or
  indicator disagreed. Include the wrapper's printed log locations and relevant
  `send effect`, `send settled` and terminal shutdown JSON, without application
  content. Preserve a failed Revert response/preimage; do not delete recovery
  data or repeatedly retry uncertain effects.

## Holder ownership live checkpoint (2026-10-18)

**Automatic holder ownership passes; product transfer remains unproven.**

- At fb87897a, signed LaunchServices preflight reports AX trusted, unlocked
  session and inactive Secure Input. A bounded TextEdit-only Authentic run
  completes cleanly with 61 active chords and enables Desktops 1-9 without chord
  changes. Baseline has no enabled Desktop-N holders or preimage; explicit Revert
  restores all nine disabled preimages, with no failures or external edits, and
  removes the backup. A subsequent Compatible run completes cleanly with 48
  chords and byte-identical holder status. No input chord was delivered.
- Three disposable TextEdit windows retain their exact AX/CG frames and Space 1
  membership. Topology remains [1,277], current 1. No Space is created or removed;
  the preexisting extra Space is retained. Exact title-free protected-window
  records before Authentic, after Authentic, after Revert and finally all match:

  ```json
  {"bounds":{"h":1139.0,"w":1150.0,"x":0.0,"y":30.0},"layer":0,"onscreen":true,"pid":31863,"wid":3675}
  ```

- Both bounded runtime owners exit cleanly and the display-awake child is reaped.
  No screenshots are taken: no transfer occurs and visual acceptance is pending.
  Main executable SHA-256:
  `2856f34441ae05ca3e7af204a82e317a9c36c5cffdfb77ebec575b8140f09fc1`.
- Post-rebase offline gates pass 996 macOS tests (846 library + 150 integration),
  strict macOS clippy/format and signed build/verify. Hosted CI exposes an
  off-macOS unused-mut binding in the new AX reader; the cfg-specific binding
  repair preserves behavior. Its diagnostic transport adds only send-1..9 names
  to existing owned-helper posts, preserving nonce/CG/focus checks and untagged
  session delivery. No ordinary-application ownership bypass is added. Updated
  offline tests pass 999 (849 library + 150 integration).

Next: exercise the actual product tap send using an owned helper with the existing
spawn-record proof; keep TextEdit procedure 1 pending until actual input delivery
to its exact disposable identity is available. Helper results cannot establish
TextEdit or physical shortcut acceptance.

## Numbered send-follow implementation checkpoint (2026-10-18)

**Numbered send-follow 1-9 is armed; native acceptance procedure 1 is next.**
This supersedes the select-only implementation boundary below, not its pending
live acceptance. Send-stay, trailing lifecycle and per-display attribution remain
pending in that order.

- Ctrl+Shift+1-9 now enters the tap catalogue and engine-managed admission lane.
  A dedicated effect owner performs fresh engine-facts queries, joined native
  topology/ordinal reads, AX/CG frame and foreground checks, dual input hit tests,
  consumed exact point/chord permits, tagged titlebar drag and Desktop-N delivery.
  Post-release OS membership/current readback settles transfer feedback and
  previous-workspace history; native observations reconcile the retained world.
- Current managed authority is a narrow engine control query over RetainedCore,
  not native membership or static scope. Unknown, excluded, unhomed, relaid-out,
  stale-generation or revision-disagreeing subjects refuse. Native evidence only
  supplements the engine facts. No effect proof or settlement runs on the engine
  or AppKit threads.
- Authentic startup enables standard Desktop-N holders with durable owned
  preimages and existing explicit conditional Revert. Compatible startup never
  touches the holder store. Other send shapes remain unsupported.
- Deleted the obsolete fake-only SendBackend/OneLeg/drain path and its duplicate
  tests, unused posting helpers, and panicking effect-owner Drop. Shutdown signals
  cancellation and hands an unfinished exact owner to a narrow off-main terminal
  reaper; release attempts bypass target gates and uncertain results never replay.
- Independent review accepted the native path before arming. Producer-shaped
  tests include real runtime hotkey admission and transfer feedback, injected
  native readers, engine removal despite matching native evidence, mid-leg drift,
  unconditional release, cancellation and terminal ownership. Latest targeted
  gates: 846 library tests and 32 scoped runtime tests, strict all-target clippy,
  workspace format, named signed app build and separate verification pass.
  Unsafe remains **114 blocks**, all in unchanged ffi.rs; no dependencies added.
- Residual limits: sequential OS checks are not atomic; exact AX/CG frame equality
  can refuse conservatively; process termination during held input is not proven
  crash recovery. Physical shortcut feel and two-display behavior are unproven.

Next: execute procedure 1 on disposable TextEdit windows, retain raw protected
before/after records, inspect and delete scoped visual evidence, restore owned
settings/current Space, and record exact outcomes before extending the slice.

## Managed posting simplification checkpoint (2026-10-18)

**Product remains select-only. Native backend, runtime wiring and acceptance
items 1-4 remain incomplete.**

- Simplicity review retained the admitted identity, per-edge fresh proof,
  serialized effect lane and release-first/no-replay contract. Removed the
  discarded-permit ceremony, redundant pre-leg proof and separate optional
  revision accessor; the required per-edge revision proof remains.
- Each noncopyable permit is now consumed by the backend and HID boundary,
  carrying its proven grab point and resolved Desktop-N chord. The posting
  boundary no longer accepts replacement point/chord arguments. The first grab
  point is pinned for the leg; later point drift refuses before posting and
  still releases held input.
- Independent review accepted the unarmed change. Its point-stability finding
  was corrected with semantic before/after-hold drift tests. Snapshot/ordinal
  and proof revision must still be coherently joined by native integration;
  the former optional revision accessor did not establish that property.
- Final offline gates: **979 macOS tests** (831 library + 148 integration),
  including exact point/chord routing, dual-hit disagreement, point drift and
  unconditional release. Strict all-target clippy, workspace format/diff checks,
  named signed app build and separate verification pass. Unsafe remains **114
  blocks**, all in unchanged `ffi.rs`; no dependencies were added.
- Native integration review rejected a prototype that treated cached topology
  plus scoped native membership as fresh managed-window authority. No native
  prototype or optional row-revision bypass is retained. Passing fake adapter
  tests did not establish its production evidence readers.
- Current managed window classification, generation and homing live in
  engine-owned `RetainedCore`; no existing plain-data accessor exports the
  window-level facts to the effect owner. `NativeContextCell` supplies topology,
  not that authority. Select a narrow publication/access contract rather than
  substituting admission pins, static scope or native membership alone. Native
  arrival readback must query the OS, not repeatedly read a cached context.
- Exceptional teardown ownership remains a separate integration decision:
  designate who retains and reaps the effect owner off engine/AppKit threads,
  including the terminal policy if it does not finish. A blocking shutdown loop
  or signal-only live-owner detach does not resolve that contract. The existing
  panicking scaffold Drop is unchanged and remains unacceptable for product use.

Next: select the managed-state publication and nonblocking teardown ownership
contracts, supply fresh runtime facts with coherent topology/ordinal evidence,
then implement the
native backend/tile and Authentic holder enable/Revert wiring. Do not arm sends
until those contracts are independently reviewed. Send-stay, trailing lifecycle
and per-display attribution follow; native acceptance items 1-4 remain pending.

## Identity and asynchronous-controller prerequisite checkpoint (2026-10-18)

**Native integration is incomplete. Product remains select-only: no send
bindings, no automatic Authentic holder writes, and no new live proof.**
This checkpoint supersedes the unresolved pure focus-swap and admission-lifetime
questions in the earlier preparation checkpoint, not native acceptance items 1-4.

### Accepted offline scope and evidence

- `e23d03fb` pins the admitted subject's app generation and source domain, adds
  a pure managed-target proof and single-flight completion lane. `3fa2e92b`
  requires a known revision and retains inflight ownership on cancellation.
- `47e59266` adds an injectable release-first send controller and effect owner;
  it is **not a native API implementation**. `c5beb523` replaces idle polling
  with event wakeup, adds nonblocking shutdown request/finished-only reap and
  corrects per-operation count classification for a reused backend.
- Producer-shaped offline tests prove pure focus-swap, lifetime/revision drift,
  unknown evidence, unusable holders, busy/stale completion, unconditional
  release attempts, cancellation and uncertain-effect feedback without replay.
  The thread factory and all fake actuation run off the submitting thread.
  No runtime AX proof, native post or tile-to-effect integration is established.
- Independent review found no blocker for the **unarmed prerequisite**. It
  identified permit consumption as a prerequisite to native integration; the
  adapter currently validates then discards the permit before its fake post.
- Full final gates at `c5beb523`: **973 macOS tests** (825 library + 148
  integration), strict all-target clippy, workspace format/diff checks, named
  signed app build and separate verification pass. Unsafe remains **114 blocks**,
  all in unchanged `ffi.rs`; no dependencies were added. Hosted
  [CI 39112487727](https://github.com/beefsack/OmniTiler/actions/runs/39112487727)
  was still in progress: seven jobs passed, native settings tests pending.

### Native integration blockers and next unit

- Consume a one-use permit at the actual posting boundary, binding the exact
  selected point and switch chord rather than only PID/window ID. Select the
  grab point on the effect owner from fresh geometry and both hit-tests; never
  substitute focus, raise a window or reuse a fixture-owned target as authority.
- Supply current runtime scope, app generation, homing/current-domain and
  topology facts through a narrow plain-data accessor. Admission pins are not a
  replacement for this fresh comparison. Unknown snapshot revision must refuse.
- Settle nonpanic, nonblocking release ownership through exceptional teardown
  before connecting the effect owner to AppKit. The scaffold's live-owner Drop
  currently requests cancellation then panics; it is not accepted product
  recovery. A new reaper/lifetime mechanism is not selected by this checkpoint.
- Implement the actual native `ManagedSendBackend` and tile submission,
  completion/history/feedback and teardown wiring. These remain absent; no
  native implementation claim follows from the injectable controller's tests.

Next: resolve those bounded posting/facts/lifetime integration contracts, then
implement and independently review the native backend. Only afterward wire
Authentic enable/Revert and deliberately arm numbered send-follow. Send-stay,
trailing selection/send/indicator slot and per-display attribution remain
unimplemented. Execute pending acceptance items 1-4 below only after their
respective implementations; no native outcome is inferred from offline gates.

## Managed product-post contract (2026-10-18, tentative)

Native implementation and acceptance remain pending. The prerequisite checkpoint
above does not expand the select-only executable capability boundary.

- **Target identity:** a send targets only the focused managed window admitted
  by the engine, pinned by PID, window ID, app-lifetime generation, source domain,
  target SID, topology revision and operation identity. Never substitute the
  subsequently focused window. Admission-time generation must survive the
  handoff; sampling a replacement app generation at dispatch is not proof.
- **Pre-post proof:** require current scoped managed membership, ordinary visible
  nonfullscreen state, unchanged lifetime/topology, fresh CG ownership/geometry,
  the selected SLPS/direct-AX/NSWorkspace focus agreement and both positional AX
  and WindowServer hits naming that exact target at the grab point. Repeat the
  applicable fresh proof immediately before each actuation edge. Unknown,
  unreadable, stale or disagreeing evidence refuses; sequential proof is not an
  atomic OS transaction. No raise, focus substitution or raw-post bypass repairs
  a failed proof.
- **Nonblocking handoff:** retain the serialized claim lane and run fresh
  create/use/drop AX proof plus the bounded native send on a dedicated effect
  thread, not the engine, AppKit or per-app observation threads. Main-thread
  submission and completion observation never wait for AX or native settlement.
  Fence requests/completions by the same operation and lifetime/revision; reject
  stale results without replay. Keep cancellation and exact thread ownership
  explicit through shutdown. No extra periodic AX proof query is introduced.
- **Release and result:** pending key-up/mouse-up bypass fresh target gates.
  Release precedes effect readback and any switch-back. Applied requires exact
  subject membership and target current-SID readback; pre-actuation refusal is
  proven refusal, while uncertain posts, release residue or unreadable arrival
  remain possibly executed and never replay. Existing bounded post-release
  settlement is not a fresh-proof polling loop.

Rationale: managed scope is product authority, unlike a fixture's held child;
generation and focus checks prevent delayed commands from grabbing a replacement
or newly focused window. A serialized effect thread reuses the existing guarded
drag sequence without stalling AppKit or the AX owners' observation cadence.
These are tentative technical contracts, not new native-transfer evidence.

Next: implement the identity/focus refusal and asynchronous effect handoff with
producer-shaped tests, then connect the native backend and Authentic ownership.
Acceptance procedures 1-4 below remain pending in their stated order.

## Unarmed numbered-send preparation checkpoint (2026-10-18)

**Step 2 remains incomplete. No product send is armed, no startup holder enable
is wired, and no new native transfer proof is claimed.** The product retains
34 basic actions and 18 existing-Space selects. All send rows remain unsupported,
including when an over-armed input fixture supplies a send chord.

### Accepted offline scope

- Added a fake-backed numbered send-follow drain with pinned subject and target,
  single-display/session/membership fences, fresh Desktop-N holder checks,
  release-first drag execution, arrival/history and honest transfer feedback.
- Added planner command/claim preparation and an Authentic-only holder-ownership
  helper over existing durable Apply/Revert transactions. These are not connected
  to a working native send backend or startup settings mutation. Compatible's
  helper path performs no store access. Existing explicit Apply/Revert is unchanged.
- Full macOS tests: 940 passed, zero failed or ignored. Strict all-target clippy,
  workspace format, named signed app build and separate signature verification
  pass. Independent correction review reran all 792 library tests, the unarmed
  runtime integration refusal and targeted send/input/world suites. Unsafe remains
  **114 blocks**, all in unchanged `ffi.rs`; no dependencies were added.

### Blocking integration contracts

- `synth::OwnedPostTarget` proves fixture ownership using a held helper child or
  spawn record. A product send needs a separate scoped managed-window identity
  contract; bypassing that proof with raw global HID posts is not an integration.
- Fresh positional AX and WindowServer point hits must name the pinned subject
  at the drag boundary without blocking the tile main loop on AX work. The
  AX-owner handoff, freshness fence, live focus revalidation and effect ownership
  remain unresolved. The fake drain does not prove refusal on a pure focus swap
  that leaves the pinned subject otherwise managed and readable.
- Only after those contracts and a native backend exist should Authentic holder
  enable be wired and numbered send-follow rows armed. Owned preimages and
  conditional Revert remain required by the existing approved product decision.
- Send-stay, trailing lifecycle and per-display/multi-display implementation have
  not advanced at this checkpoint. No new product or architecture decision is
  selected. The retained send lane is deliberately unreachable preparation.

### Pending native acceptance, in order

All procedures below require their implementation first. Use a disposable
TextEdit-only scope, fresh identity/lock/Secure Input checks per leg, exact raw
protected-window records before and after, and settings/topology preimages.
Never direct input or window operations at the controlling terminal. After
Mission Control closes, resample geometry; stop on genuine protected drift.
Capture only freshly verified owned-window/indicator regions, inspect them,
then delete them. Synthetic results do not establish physical shortcut feel.

1. **Numbered product send and Authentic ownership: pending.** Build/sign/verify,
   run signed preflight, then `spaces-desktop-status` and `spaces-diagnose`.
   With disposable A on O and an existing adjacent T, launch
   `mise exec -- just tile --user-start --scope com.apple.TextEdit --input tap --preset authentic --seconds 60`.
   Require automatic standard Desktop-N enable with unchanged chords and durable
   preimages. Focus A; press Ctrl+Shift+2, then Ctrl+Shift+1. After each release,
   require A's exact target membership, current Space and focus, retained layout,
   balanced input release and unchanged protected geometry. Stop the bounded
   runtime; run `spaces-desktop-revert`, then fresh status/diagnose. Compare exact
   owned settings preimages and original current/topology. Repeat Compatible
   startup and require zero holder mutation. Launch diagnostic subcommands only
   through `mise exec -- just launch -- <subcommand>`.
2. **Send-follow and send-stay: pending.** Repeat A/B on O and C on T. Send B
   with follow and require target current/focus and both retained layouts. Bind
   the currently unbound numbered stay action after native support exists;
   repeat O->T and require B on T but current/focus returned to O. Revisit both
   Spaces and compare per-window slots. Repeat the inverse, restore B and the
   original current Space, and record physical switch-back cost separately.
3. **Trailing empty Space: pending.** On O press Meta+0; record created T and
   current/membership. Return to O; select T through the indicator trailing slot;
   press Meta+0 again on empty T and require reuse, not another SID. Return to O;
   press Meta+Shift+0 on disposable A and require append/reuse plus send-follow.
   Repeat after T becomes occupied, check the new trailing target, restore A/O,
   and remove only newly created, proven-empty Spaces. Unknown occupancy must
   still refuse removal; do not exempt foreign chrome to force cleanup.
4. **Per-display attribution and two displays: pending.** Record stable display
   IDs, ordered user-Space SIDs and current SID for each display, including the
   separate-Spaces mode. Put disposable A/B on different displays; focus each
   in turn and select/send to its numbered target. Require attribution to that
   display's order, unchanged other-display current/memberships, exact focus and
   retained layouts. Repeat trailing selection/send through each indicator and
   compare native SIDs to displayed numbers. Test refusal on unsupported shared
   Space topology. Restore both original currents, windows/settings and only own
   proven-empty temporary Spaces. Offline attribution coverage is still pending;
   no two-display success is inferred from single-display evidence.

Next: settle the scoped product-post target identity contract and nonblocking AX
proof handoff, implement the native backend and Authentic enable wiring, then
deliberately re-arm numbered send-follow and execute acceptance item 1.

## Tagged native-holder delivery checkpoint (2026-10-18)

**The transfer discriminator passes; the managed-workspace product remains
incomplete and no additional product actions are armed.** Tagged Control+Fn
arrows and tagged Control-only Desktop-N digits each carried an owned helper
outward and back, with exact post-release membership/current-Space readbacks.
NumericPad was unnecessary. These runs supersede the delivery uncertainty below,
not the pending product integration or physical acceptance.

### Placement and fixture

- Title-free CG/AX measurement confirmed the placement refusal: the protected
  terminal and three disposable TextEdit windows left no clear 480x384 helper
  rectangle. Existing foreign-window frame commands correctly refused without a
  spawn record; no ownership gate was weakened and no window was repositioned.
- Selecting existing ordinary Space 277 through the proven MC AX route left no
  onscreen layer-0 obstacles. The unchanged helper placement then succeeded.
  Running from 277 toward 1 exercises Left outward and Right inverse, as derived
  from SLS order `[1,277]`. Original current Space 1 was restored afterward.
- Explicit `spaces-drag-transfer --numbered` resolves both SIDs to their Desktop-N
  ordinals and reads enabled standard native holders before spawning or posting.
  Disabled, custom, unreadable and unmappable holders refuse. The fixture never
  enables settings; default mode remains arrows. Both key edges and pending
  releases retain the selected digit/flags and own-switch tag.
- Numbered proof used explicit `spaces-desktop-apply --count 2` and Revert.
  Holders 118/119 changed from disabled to enabled with unchanged Control+1/2
  chords, then returned to disabled; the owned recovery preimage was removed.
  This proves session-effective holder delivery, not automatic Authentic startup.

### Accepted native evidence

Raw title-free transfer reports, preflight, CG, scoped AX, topology and settings
before/after responses were retained separately. `legF-transfer.json` reports the
arrow roundtrip; `transfer-numbered.json` reports `numbered:true`. Each has:
outward membership/current `[1]`/1, inverse `[277]`/277, balanced one mouse-down/up
and key-down/up per leg, exact helper frame, restored topology/pointer, exact
helper exit/reap/record removal/absence and display-sleep-child reap. Synthetic
roundtrip readback times were 1125 ms and 1115 ms respectively, not visual latency.

Raw protected-window records selected from the arrow baseline/final CG responses
(`legD-cg-inspect.json`, `legG-cg-inspect2.json`) and numbered baseline/final
responses (`cg-before.json`, `cg-after.json`) are identical in all four samples:

```json
{"bounds":{"h":1139.0,"w":1150.0,"x":0.0,"y":30.0},"layer":0,"onscreen":true,"pid":31863,"wid":3675}
```

The arrow leg's intermediate samples retain those bounds with `onscreen:false`
while another Space is current. Final samples were taken after MC closed;
baseline/final topology is `[1,277]`, current 1. The three TextEdit identities,
frames and Space memberships match, and no owned helper remains. Space 277 is
still retained because foreign chrome blocks proven-empty removal; no Space was
created or removed. No screenshot was taken: helpers exit within the fixture,
and no stale-region capture or visual-jank claim was made. Natural focus return
inside the helper fixture remains unproven; final independent scope inspection
observed the original protected foreground PID.

### Verification and next action

Independent review and its regression-coverage correction pass: shared real/fake
chord selection, both tagged edges, Control-only digits versus Control+Fn arrows,
pending release/error paths and pre-post holder refusals. Library tests: 771;
integration suites, strict all-target macOS clippy, workspace format/diff checks
and named signed app build/separate verification pass. Unsafe remains **114
blocks**, all in unchanged `ffi.rs`. Live numbered main executable SHA-256:
`8563a3137661ef564e8c08f67175924ae8e66d694968d9f59c5c6db83d441e1b`.

Next: integrate numbered product sends with Authentic Desktop-N enable and owned
Revert, then send-follow/stay, trailing lifecycle and per-display attribution.
Tap coexistence, ordinary-window product transfer, physical feel, multi-display
and minimum-release acceptance remain pending. No new product decision is made.

## Relative-arrow offline checkpoint (2026-10-18)

**Step 3 remains incomplete; no additional product actions are armed.** Relative
arrow posting now carries Control plus SecondaryFn (`0x840000`) on down, up and
pending-release paths, including the normal drag diagnostic. Digits remain
Control-only. NumericPad is not added without delivery evidence. Regression tests
pin executor/diagnostic flags and both tagged tap edges; provenance matching and
the 18-select capability boundary are unchanged. Independent review found no
blocking code issue.

The live transfer fixture refused at helper placement before spawning a helper
or posting synthetic input: no clear placement rectangle was available among the
protected terminal and three disposable TextEdit windows. Reported topology was
unchanged, with no settings writes or Space creation/removal. Raw output was not
retained and a final protected-window CG snapshot was not collected, so this is
**not accepted live delivery, protected-window stability or restoration evidence**.
No screenshot was taken. The existing Space 277/TextEdit residue is not cleared.

Offline gates pass: 761 library tests plus 148 integration tests across ten suites,
strict all-target macOS crate clippy, workspace format and diff checks, named
signed app build and separate verification for both bundles. The earlier transient
`native_plans_land_inside_actual_work_areas_divided_per_domain` failure did not
repeat in independent verification; its original panic output was not retained.
Unsafe remains **114 blocks**, all in unchanged `ffi.rs`. Final main executable
SHA-256: `8b313a9d68bc90aa5e67430fba81e7553cc0fc4a4753464f9bc3e59fac147bf3`.
This rebuilt artifact has not been live-tested.

Next: establish a clear disposable target placement, then prove tagged Ctrl+Fn
arrow and tagged Desktop-N delivery with fresh retained raw protected-window
records before and after each leg. Use the existing user-run discriminator if
placement remains impractical; do not infer holder delivery from posting or tap
bypass tests. Numbered sends, follow/stay, trailing lifecycle and multi-display
remain pending. No new product decision is selected by this checkpoint.

## Current prerequisite checkpoint (2026-10-18)

Managed workspace implementation has not advanced beyond the select-only fallback,
but its [world-state prerequisite is complete](archive/macos-full-state-replies.md#final-outcome-2026-10-18):
runtime tap→engine→drain `Ctrl+2` and `Ctrl+1`, inspected ring/indicator screenshots
and clean overlay teardown pass. A live-discovered empty hidden AX snapshot bug
was corrected and reverified: each TextEdit identity now retains its exact slot
through native round trips, not merely the same set of frames. Scoped screenshots
work; the [reusable process](../live-macos-testing.md#scoped-screenshots) requires
fresh target identity/liveness/frame before every crop, inspection then deletion.

Reused Space 277 remains because foreign chrome prevents proven-empty removal;
three disposable TextEdit documents remain available for testing. No test-owned
processes, taps, helpers, overlays or display-sleep children remain. Inspect the
Space's contents before manual cleanup; do not relax unknown-occupancy guards.

Next: relative chord
Control+Fn/tag delivery, numbered sends and Authentic Desktop-N enable, follow/stay,
trailing lifecycle, then per-display attribution and multi-display coverage. The
successful untagged runtime input is not evidence for tagged native holder delivery.

## Drag/chord discriminator preparation (2026-10-17)

**Offline preparation only; native transfer remains parked.** The one-shot
`drag-diag-chord`, `drag-diag-hold` and `drag-diag-full` commands isolate the
two synthetic input halves without creating/removing Spaces, changing settings,
raising windows or enabling product transfers. Physical results remain TBD.
The existing NoEffect observations below are not superseded by fake-backed tests.

### Scope, implementation and acceptance

- Default countdown: 5 seconds (`--countdown 0..30`). Chord-only posts exactly
  one Ctrl+Right down/up through the transfer executor's shared safe construction
  path: null/default event source, HID tap, Control+SecondaryFn flags on both key events and
  the own-switch provenance tag. `--untagged` omits that tag.
- Hold/full capture the real cursor once after the countdown. Positional AX
  `(PID, window ID)`, WindowServer hit-test and a complete CG owner join must
  identify one standard window. Ghostty, the probe's process/bundles, protected,
  ambiguous and known-fullscreen targets refuse before the drag. These are
  user-run ordinary-window diagnostics, not a replacement for the owned-helper
  restrictions on automated experiments.
- The executor's mouse sequence is move/down/one dragged event at the same point,
  not a sweep across the title bar. Hold-only waits 6 seconds by default
  (`--hold-secs 1..15`); full waits 50 ms, posts the chord, holds another 400 ms,
  then releases. No drag events continue during or after the chord.
- Release bypasses target/session gates. Explicit cleanup and unwind guards
  cover pending key and mouse edges; SIGINT/SIGTERM request release-first
  cancellation. Failed release takes precedence over the triggering error.
  Reports retain partial posts, identity, points, elapsed timestamps and posting
  latency; dispatch is not delivery or transfer evidence. SIGKILL, power loss
  and abort cannot be covered by in-process cleanup.
- Work units: source comparison; shared-posting/CLI and fake regressions;
  independent cleanup review and correction; signed build/verification and
  publication. Offline acceptance covers parsing, target refusals, event order,
  hold cap, injected errors/interruption/unwind and honest residue reports.
  Live acceptance is the walkthrough below, not an automated gate.

### Amethyst comparison

Source pins: Amethyst
[`5cfe2fe12b1a933139010da5760aaa7e44033e9b`](https://github.com/ianyh/Amethyst/tree/5cfe2fe12b1a933139010da5760aaa7e44033e9b),
whose `Window.move(toSpaceAtIndex:)` delegates to Silica; its project references
Silica's drifting `master` branch. The inspected implementation is pinned to
Silica [`ebead82a28486e7719b7ed69633ecfed8a4fe2d2`](https://github.com/ianyh/Silica/tree/ebead82a28486e7719b7ed69633ecfed8a4fe2d2),
[`SIWindow.m`](https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SIWindow.m)
and
[`SISystemWideElement.m`](https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SISystemWideElement.m).
Behavior is reimplemented, not copied.

| Detail | Silica / Amethyst | Our transfer and diagnostic sequence |
| --- | --- | --- |
| Down point | Minimize-button horizontal midpoint, halfway through top chrome; fallback x = window-left + 5 | Transfer chooses guarded frame-derived titlebar points; diagnostics use only the captured cursor. The variant deliberately retains this cursor-anchor substitution. |
| Event order | MouseMoved → LeftMouseDown → LeftMouseDragged → 50 ms → key-down → key-up → 400 ms → LeftMouseUp | Same order, event count and delays; one mouse point throughout; no continuing drag moves |
| Source/state and tap | NULL source (default state, no explicit source-state ID); HID for all posts | Identical shared construction path; no explicit source-state ID |
| Mouse flags | Move/down/up explicitly zero; dragged event retains creation defaults | All mouse flags explicitly zero, including dragged; variant retains this explicit zeroing |
| Chord | Configured Desktop-N symbolic hotkey, ID `118 + N - 1`, N = 1..16 | Normal mode uses Ctrl+Right; `--amethyst --desktop 2` reads the configured Desktop-2 chord |
| Modifier delivery | Key-down has configured flags; key-up flags zero; no separate Control-key events | Normal mode now has Control+SecondaryFn flags on both edges, no separate modifier events; variant matches configured-down/zero-up |
| Provenance | Untagged | Normal posts carry own-switch tag; variant is untagged |
| Disabled shortcut | Enables it before posting | Variant refuses disabled/unbound/unreadable shortcuts; never writes settings |
| Surrounding behavior | Manager switches back after about 1 second unless follow is enabled | One drag sequence only; no inverse or automatic switch-back. Countdown, cursor/identity/session guards, bounded holds, cancellation and release/residue reporting are additional diagnostics. |

Related notes: [yabai's Space machinery](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c)
uses native gesture selection and scripting-addition window moves, not this
drag/chord path. Its
[input hit-test](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L961-L975)
is relevant to target proof, not evidence that synthetic carry works.
[AeroSpace](https://github.com/nikitabobko/AeroSpace) manages virtual workspaces
rather than implementing this native-Space transfer; its
[AX point lookup](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/mouse/focusFollowsMouse.swift#L55-L64)
is a related identity precedent only. Neither supplies missing transfer proof.

### Walkthrough: 2x2 drag/chord test, plus Amethyst variant

Use one display, an unlocked session, no Secure Input, games or fullscreen apps.
Stop the tiler normally and wait for its teardown; no tap should be running.
Do not stop unrelated processes or use name-based kills. If a diagnostic needs
an exact-PID stop, stop only the verified process started for that trial.
In Mission Control manually create one extra ordinary Space: Desktop 1 and
Desktop 2. Put one disposable, windowed TextEdit document on Desktop 1, outside
Ghostty's rectangle, with a clear title bar. The cursor must be on TextEdit,
never Ghostty or the probe itself. Keep other windows out of that point.

Build/verify and inspect preflight through the signed app. Run these
shell-neutral lines individually (also valid in fish):

```text
mise exec -- just app 'OmniTiler Mac Dev'
mise exec -- just verify
mise exec -- just preflight-app
mise exec -- just launch -- spaces-diagnose
```

Require Accessibility trust through this LaunchServices route; do not substitute
direct execution of the bundle binary. If Ctrl+Right does not already work,
stop and correct the manual setup. None of these diagnostics configures shortcuts.
For every cell record (a) whether the current desktop changes and (b) whether
TextEdit follows onto Desktop 2. A desktop switch without the window isolates
carry failure from chord failure. Retain the complete JSON and streamed posts;
`*-complete` means the event sequence completed, **not** that transfer succeeded.
After each trial release physical input, wait for the diagnostic to finish, then
manually return TextEdit and the current view to Desktop 1 before continuing.

| Cell | Launch and physical action | Observe |
| --- | --- | --- |
| Real drag / real chord (baseline) | No diagnostic: hold TextEdit's title bar, make a small real drag, press real Ctrl+Right while holding, then release | Desktop 2 and TextEdit should follow together; stop if this baseline fails |
| Real drag / synthetic chord | Run the chord command below; during the 5-second countdown physically grab and slightly drag TextEdit's title bar; keep holding until the pair posts, then release | Two posted key events; distinguish no desktop switch from switch without TextEdit |
| Synthetic drag / real chord | Run hold below; during countdown move the real pointer onto TextEdit's title bar **without pressing the mouse**. After synthetic down/drag is announced, press real Ctrl+Right during the 6-second hold; do not move/click | Correct captured identity and point, four mouse posts including final up; whether the real chord carries the synthetic drag |
| Synthetic drag / synthetic chord | Run full below; position the real pointer during countdown, then leave mouse and keyboard untouched | Six posts, final mouse-up, desktop switch and TextEdit carry independently |

```text
mise exec -- just launch --timeout 25 --stream -- drag-diag-chord --allow-untested
mise exec -- just launch --timeout 40 --stream -- drag-diag-hold --hold-secs 6 --allow-untested
mise exec -- just launch --timeout 25 --stream -- drag-diag-full --allow-untested
```

Optional provenance discriminator: repeat the real-drag/synthetic-chord cell with
`mise exec -- just launch --timeout 25 --stream -- drag-diag-chord --untagged --allow-untested`.

For the Amethyst comparison, first verify manually that the already-configured
Switch to Desktop 2 shortcut works. If disabled/unbound, the command refuses;
any manual configuration change is separate from this test and must be restored
afterward. Reset to Desktop 1, position the cursor as in the full cell, then run:

```text
mise exec -- just launch --timeout 25 --stream -- drag-diag-full --amethyst --desktop 2 --allow-untested
```

Observe the same six posts and native effects, accounting for the documented
cursor-anchor/explicit-drag-flags substitutions. This is a sequence comparison,
not an exact reproduction of Amethyst's anchor or automatic return policy.

Ctrl+C in the terminal stops the **launch wrapper**, not its LaunchServices
child. Wait for the child's bounded release/exit before another trial; the hold
defaults to 6 seconds and cannot exceed the requested 15-second cap except host
scheduling delay. Direct SIGINT/SIGTERM to the verified diagnostic PID requests
release-first cancellation. Do not force-kill during a held input. On refusal or
residue, preserve JSON and stop rather than retrying blindly. In-process cleanup
cannot guarantee release after SIGKILL, abort or power loss.

Cleanup: return TextEdit to Desktop 1, wait for all started diagnostics to exit,
then manually remove only the disposable **empty** Desktop 2 in Mission Control.
Do not remove an occupied Space. Run
`mise exec -- just launch -- spaces-diagnose` and compare the original topology.
Restore any separately changed shortcut setting. No probe performs cleanup Space
operations or changes settings on the user's behalf.

### User-assisted discriminator results (2026-10-17)

Run with the user at the desktop: tiler stopped, one display, Desktop 1 plus one
manually created Desktop 2, a disposable TextEdit window on Desktop 1.

| Cell | Result |
| --- | --- |
| Real drag + real Ctrl+Right (baseline) | **Pass**: the view switched while the window was still held; on release TextEdit stayed on Desktop 2. Real drag + Ctrl+Left returned it |
| Real drag + synthetic Ctrl+Right (`drag-diag-chord`, tagged, flags `0x40000`) | **No effect**: two posts, no desktop switch |
| Synthetic Ctrl+Right alone, no drag (same command, tagged) | **No effect**: two posts, no desktop switch |
| Synthetic drag + synthetic Ctrl+2 (`drag-diag-full --amethyst --desktop 2`, after the user enabled Switch to Desktop 2) | **Pass**: the view and TextEdit both moved to Desktop 2. Six balanced posts; both identities joined TextEdit's `AXStandardWindow` at the cursor; mouse-down to mouse-up 469 ms, including the 400 ms settle delay |

The untagged Ctrl+Right variant and the synthetic-drag/real-chord cell were
skipped once the cause was isolated.

**Cause.** The read-only `spaces-bindings` table registers every Mission
Control arrow chord with Carbon modifiers `0x21000`, which is Control plus Fn.
On Apple keyboards the arrow keys carry the Fn flag. The executor's chord sets
only Control (`0x40000`), so it never matches. This is consistent with the
earlier no-effect evidence in the
[routes research](../research/macos-port/native-spaces-routes.md). Desktop-N
holders are registered as Control only (`0x1000`), so they match the existing
construction. The synthetic title-bar drag itself works.

**Consequences.**
- The numbered send route (synthetic drag plus Desktop-N chord) is proven on a
  real app.
- Relative sends need the arrow chord posted with Control plus SecondaryFn
  (`0x840000`), possibly plus NumericPad (`0x200000`), as hardware arrows carry.
  This needs a fix and a re-test.
- The working Desktop-N run was untagged, with key-up flags 0. Confirm that the
  tagged chord also works, because the tiler's tap needs the provenance tag to
  pass its own chords.
- Transfer is unparked. The earlier occlusion and identity work stands.

Cleanup: the user returned TextEdit, closed it and unticked Switch to Desktop 2.
A read-only `spaces-diagnose` afterwards still showed two Spaces on the display,
with Desktop 1 current. The disposable Desktop 2 had not yet been removed.

### Offline verification

Final local gates pass: **862 tests** (728 library plus eight integration suites),
locked all-target macOS clippy with `-D warnings`, workspace format and diff checks,
`just app 'OmniTiler Mac Dev'` and separate `just verify` for both signed bundles.
Independent review found no serious remaining issue after cleanup corrections.
Unsafe blocks **114 → 114**, all confined to the unchanged `ffi.rs`; the maintained
`signal-hook` dependency avoids new unsafe signal handling. Static launcher-flow
verification caught and corrected a two-layer `--stream` mismatch: prompts now
always go to probe stderr, and the wrapper alone controls live streaming.
No live discriminator cell has been run; managed workspace completion and product
transfer capability remain outside this diagnostic delivery.

## Select-only fallback handover (2026-10-17)

**Transfer remains parked; only existing-Space selection is armed in the bounded
tiling preview.** The clear-helper fixture resolved the earlier positional
identity blocker without weakening either hit-test. Two synthetic outward
attempts then reported **NoEffect**. Numbered transfer, TextEdit transfer,
send-follow and send-stay are not proven or enabled. This supersedes the next
action and unarmed-select status in the earlier checkpoints below.

### Accepted outcome and evidence limits

- The helper now positions itself at creation outside the protected terminal's
  rectangle. Live CG and AX frame readbacks verify the placement; after public
  self-join O→T→O, positional AX and WindowServer hit-tests both joined the helper
  at the titlebar. No terminal move was needed. Placement search and chrome slack
  are diagnostic-fixture policy, not production window-placement policy.
- Two initial zero-post fixture refusals exposed content-versus-frame chrome and
  a stale pre-Mission-Control helper identity. Frame readback and fresh exact
  authorization repaired those faults before the synthetic attempts.
- Both subsequent outward attempts posted one mouse-down/up and one key-down/up,
  then read original membership/current instead of the temporary target. The
  second explicitly corroborated native Space ordering and right-arrow keycode
  124. **No inverse ran.** These are two NoEffect observations, not two successful
  round trips and not proof of a SIP or native-route limitation. Further synthetic
  transfer tuning is stopped; chord effectiveness versus held-drag interaction
  remains the precise unresolved discriminator.
- Surviving terminal-history excerpts report exact one-Space/current restoration,
  helper frame and pointer restoration, unchanged protected frame/membership,
  helper exact stop/reap/absence and display-sleep-child reap for both attempts.
  Separate read-only signed-bundle inspection subsequently corroborated one Space,
  Mission Control absent, unlocked/trusted session, unchanged protected frame and
  no probe/helper residue. No settings writes occurred in these runs.
- **Evidence gap:** raw transfer JSON was removed after cleanup; only parsed
  terminal-history excerpts survive. Its latency arrays were not retained, so
  there is **no end-to-end transfer timing claim** and no physical visual-jank
  acceptance. Final unsuccessful-transfer main artifact SHA-256:
  `8a156840d034515fc1f0e491fa4b148c4acf60e7f43dc18d78dafc4f7a14538e`.

### Delivered fallback

- Exactly 18 product workspace bindings: existing numbered Spaces 1–9, eight
  relative-select rows and previous toggle. Numbered actions keep Meta (default
  Ctrl); auxiliary actions use Ctrl+Cmd with default Ctrl Meta. Compatible keeps
  established native unshifted Ctrl digits/arrows as pass-through; Authentic uses
  the MC AX controller. Index 0/trailing, all sends/stays/output sends and
  migration remain capability-disabled. Send-stay remains unbound in the
  catalogue, but executable bindability is still outstanding.
- A serialized engine-to-main-thread claim reuses the native selector, keeps
  per-display previous history for the preview lifetime, and advances it only on
  verified arrival. Busy, stale topology, unknown gates and failed effects refuse
  narrowly without action replay. Native drain refuses unless exactly one display
  is observed; it never guesses the first of several displays.
- Select eligibility does not require a focused window. Separate per-chord
  basic/select caches prevent basic-chord suppression or hidden-window dispatch
  on empty Spaces. Existing session, scope, Secure Input and lock protections
  remain; fresh gates run before native dispatch and at MC leg boundaries.
  A 30-second effect budget preserves the existing 15-second MC cleanup reserve;
  a bounded display-sleep child is reaped by its exact process handle.
- Existing off-current domain retention is reused, not redesigned. Regression
  coverage adds empty-Space arrival/return with exact retained layout and zero
  hidden writes. Automatic trailing creation/deletion and current-empty
  classification remain unaccepted and are not armed.
- Independent review found and corrected shared-cache basic-chord interception,
  first-display guessing and stale pre-dispatch gates before accepting the bounded
  select test route. Final review found no serious unresolved issue. Atomicity
  across host state changes is not promised; the main pump may block within the
  bounded native effect while the tap and engine remain independent.
- No live tap→engine→drain selection proof is claimed. Existing guarded synthetic
  producers cannot emit an untagged armed select chord, so no new fixture was
  built merely to replace a physical check. Read-only signed-bundle preflight,
  topology and membership evidence was retained separately. The physical procedure
  below is the next acceptance action, not a completed result.

### Verification and remaining differences

- Local macOS gates: **670 tests** (536 library plus eight integration suites),
  strict all-target macOS crate clippy, workspace format, signed app build and
  separate verification pass. Independent review reran all 536 library tests and
  all 26 scoped-tile tests. Unsafe blocks **102 → 102**, confined to `ffi.rs`;
  no dependency or shared-core change.
- Published fallback `ba422719`; hosted CI exposed non-macOS dead-code warnings
  for the live-gated drain. Narrow target-conditional lint annotations in
  `920d218d` repair that portability issue without changing macOS behavior.
  [CI 38923718185](https://github.com/beefsack/OmniTiler/actions/runs/38923718185)
  passes all eight jobs: macOS, Windows, Rust, native, KWin, shell, licenses and
  notices. Concurrent unrelated changes were retained. Hosted success does not
  establish physical acceptance.
- Differences from KDE/Windows and Tier 1 tilers: select-only bounded preview,
  not a complete managed workspace manager; no send/follow/stay/output/migration
  execution; no trailing lifecycle; one-display-only product selection; explicit
  Ctrl+Cmd auxiliary arm; visible MC transitions rather than a proven lower-jank
  route; history lasts one preview run, not restarts; explicit session-effective
  Desktop-N Apply/Revert rather than automatic settings takeover. The intended
  native send-follow and send-stay switch-back remain unimplemented. Cross-display,
  fullscreen, minimum-release and physical shortcut/jank acceptance remain TBD.

### User physical select-only procedure

This is **not** the final send/follow/stay dogfood script: those actions remain
unsupported. Use disposable TextEdit windows, one display, an unlocked session,
no Secure Input or game/fullscreen context, and two native user Spaces. Start
TextEdit on Desktop 1; leave Desktop 2 empty. Create the second Space manually in
Mission Control if necessary; remember which empty Space to remove afterward.
Run each shell-neutral line separately, inspecting its JSON before continuing:

```text
mise exec -- just preflight-app
mise exec -- just launch -- spaces-desktop-status
mise exec -- just launch -- spaces-desktop-apply --count 2
mise exec -- just launch -- spaces-desktop-status
mise exec -- just tile --user-start --scope com.apple.TextEdit --input tap --preset authentic --seconds 60
```

At the start of the bounded run, inspect the menu-bar workspace numbers: all
existing desktops should be visible in order, the current one highlighted, and
unknown occupancy marked `?`. Check that the strip is legible and unobtrusive;
optional `--workspace-indicator-icons` adds one representative app icon alongside
each available number. Report crowding, missing numbers or unwanted focus changes.
This appearance check remains pending; see the
[indicator record](macos-workspace-indicator.md) for live-proof limitations.

During the bounded run press **Ctrl+2** (empty Desktop 2), **Ctrl+Cmd+J**
(wrap to Desktop 1), **Ctrl+Cmd+Tab** (previous Desktop 2), then **Ctrl+1**
(return). Wait for each transition and inspect the `dev select` outcome and
`caffeinate_reaped` readback. Report visible MC jank, unexpected focus/input,
failed empty-Space navigation and any shortcut conflict. Do not try send chords
as acceptance tests: they are unarmed. Wait for the 60-second preview teardown
and inspect terminal JSON; stopping its wrapper does not stop the signed child.
Always finish, including after a failed selection:

```text
mise exec -- just launch -- spaces-desktop-revert
mise exec -- just launch -- spaces-desktop-status
mise exec -- just launch -- spaces-diagnose
```

Revert must restore the owned preimages and remove the recovery preimage. If it
does not, retain its JSON and recovery data; do not delete them or repeatedly
Apply. Return to Desktop 1 and manually remove only the disposable empty Space;
rerun `spaces-diagnose` to verify the original single-Space layout. Do not remove
an occupied Space. Send-stay switch-back jank cannot be judged yet; that question
remains parked with transfer rather than silently accepted.

### Exact continuation

Run the physical select-only procedure and assess its empty-Space navigation and
MC transition cost. Transfer stays parked unless new evidence distinguishes
native chord delivery from the held-drag interaction; do not repeat the same
Ctrl+arrow fixture. Numbered/helper/TextEdit transfer proof, send integration,
trailing lifecycle, multi-display attribution and the final full dogfood script
remain open. The whole managed-workspaces item is **not** awaiting final physical
acceptance; only the select-only fallback is awaiting its physical check.

## Current discriminator and continuation (2026-10-17)

The display-spanning layer-20 Dock surface is **not an input occluder at
the tested owned-helper titlebar points**. Unfiltered, title-free CG lists
show the membership-less surface before MC and after closing it; both
`AXUIElementCopyElementAtPosition` (titlebar static-text element joined to
the helper window) and `SLSFindWindowAndOwner` return the exact helper at
all five points. Input-reaching identity, not rectangle stacking, is the
appropriate guard; no Dock-name exemption is justified.

Transfer is nevertheless **parked, not proven**. After independent helper
self-join O→T→O, focus agreement names the helper but positional AX identity
names the protected terminal at all five points. A bounded read-only settle
did not change it; an exact-helper AXRaise before SLPS dispatch did not change
it either (the product's dispatch-then-raise sequence was not tested here). All
legs made zero synthetic posts. Do not repeat these approaches. Next transfer
question: why does the recovery self-join produce a helper with exact membership
and focus agreement but no input-reaching titlebar, and can a user-arranged
helper outside that rectangle demonstrate the synthetic route without changing
the focus/identity guard? No TextEdit or numbered-transfer proof is claimed.

MC AX disappearance precedes CG thumbnail-transform settlement: immediate
snapshots changed protected bounds with zero posts, while later read-only
snapshots returned exactly to baseline. A bounded read-only exact-restoration
wait repairs this observation gap; a persistent mismatch still refuses.
The final refusal restored original single-Space/current topology, exact helper
frame, protected frame/membership, pointer and full foreground agreement; exact
helper exit/reap/record removal, absence and bounded display-sleep-child reap
all passed. No protected-window writes occurred.

Delivered a select-only, explicit MC AX controller/CLI: numbered/relative
selection, previous toggle and trailing creation, keeping send routes
unsupported. Product bindings remain unarmed: the preview's foreground/window
eligibility gate cannot navigate back from an empty Space, and no main-thread
workspace-effect handoff exists. The 19-row select-only binding subset is exposed
without claiming interception or empty-Space product support. Full managed
trailing cleanup and physical select/send dogfooding remain outstanding.

### Select-only checkpoint and verification

- `spaces-select --display N --action ID [--action ID ...]` executes existing
  numbered selects, wrapping relative selects and previous toggle. History is
  per display for one controller/batch lifetime, advances only on verified
  arrival and is not persisted across CLI launches. Index 0 creates or selects
  trailing through shared minimum/ordinal/reuse helpers. Exact topology drift,
  unknown membership and unverified effects refuse without action replay.
  Requested creation/selection persists; adopted SIDs and final layout are
  reported even on partial failure. This is not an automatic managed lifecycle.
- A native seven-action batch proved two-view toggle, numbered selection and
  relative wrap with exact current-SID readback: selection **652–677 ms**;
  create-and-select **1158/1181 ms**. These are native readback timings, not
  physical visual-jank acceptance or a gesture comparison.
- The second trailing request appended another SID rather than reusing the
  apparently empty current Space. Its occupancy cause was not captured at the
  decision boundary. The scanner's conservative candidate treatment remains;
  a new zero-area/unreadable-candidate refusal prevents that class from becoming
  an append trigger. This refusal is **offline-covered only**. Active empty-Space
  classification/reuse and complete trailing cleanup remain open, not accepted.
- `mc-recover` correctly refused the three-Space shape instead of broadening its
  original two-Space contract. New `mc-remove-empty --space SID --allow-untested`
  exposes existing exact-SID, noncurrent/nonlast user-Space, complete-emptiness
  and exact-removal readback guards for explicitly requested cleanup. It never
  selects, guesses ownership, replays a removal or removes an occupied Space.
  Both created SIDs were independently removed; protected bounds/membership and
  the original one-Space/current layout read back exactly restored. No settings
  writes occurred; final status has no enabled digit holders or recovery preimage.
- `cg-inspect` provides a title-free, unfiltered read-only snapshot;
  `spaces-grab-diagnose` owns a bounded disposable helper and compares AX plus
  WindowServer input hit identity before/after MC without synthetic posts.
  Source precedent: [yabai input hit-test](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L961-L975)
  and [AeroSpace AX point query](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/mouse/focusFollowsMouse.swift#L55-L64).
  Neither source establishes a blanket Dock-rectangle occlusion rule.
- Final local gates: **648 tests** (519 library plus eight integration suites),
  strict all-target macOS crate clippy, workspace format, signed build and
  separate verification pass. Unsafe blocks **100 → 102**, all in `ffi.rs`:
  optional hit-test symbol resolution and its checked call. No new dependency
  or shared-core change. Final signed main SHA-256:
  `d1a592b40cfe0599617eaef256e41d704bd91cdba1b28b52781dac2416a24072`.
  This artifact includes the later offline-only zero-area refusal; it is not a
  successful live-transfer artifact.
- Source checkpoint `ff0ebb00`: [CI 38908954339](https://github.com/beefsack/OmniTiler/actions/runs/38908954339)
  passes macOS, Rust, Windows, KWin script, shell, license and notice jobs.
  The KDE native job fails test `native-effect-kcm-shortcut-selection`: first
  a `shortcutDisabledIds` assertion, then a subprocess abort on one failed-job
  rerun (32/33 native tests pass in both attempts). The rerun's failed-step log
  does not identify the abort cause. No KDE/shared source changed in this
  checkpoint; causality is not established and the overall hosted gate remains
  **not green**. Native KCM investigation is separate from this macOS milestone.
- Read-only final inspection independently confirms one original Space/current,
  Mission Control absent, trusted/unlocked session, no settings preimage and no
  enabled digit holders. No helper, probe or display-sleep companions remain.
- Consistency limitations: defaults retain Ctrl Meta and Ctrl+Cmd auxiliary
  arms, but no workspace shortcut is intercepted. Batch-only history and
  explicit select/create/cleanup are development seams, unlike KDE/Windows'
  integrated persistent workspace manager. MC AX has visible transitions;
  lower-jank gesture selection is not selected on unmeasured evidence. Native
  drag-follow/send-stay switch-back and session-effective Desktop-N ownership
  remain the previously documented host differences; no transfer ships here.
  Fullscreen/cross-display/floor acceptance and physical jank remain TBD.

The final physical select/send/follow/stay/Ctrl+Cmd/enable-Revert script is not
written: those product actions are not executable yet. This item is **parked on
transfer input identity and select-only integration**, not awaiting final physical
acceptance. Next: resolve the post-self-join focus-versus-input mismatch with a
changed, independently accepted helper setup; separately diagnose current-empty
occupancy before arming empty-Space selection or implementing automatic deletion.

## Goal and acceptance

Implement the approved native-Space route with SIP enabled, no scripting
addition, no Dock injection and no virtual workspaces. Keep the shared core's
workspace policy and the KDE/Windows defaults and semantics.

- Verify owned-window synthetic title-bar drag plus native switch chord and its
  inverse, with exact membership readback and guaranteed input release.
- Select through Mission Control AX or the proven same-display Dock gesture;
  choose the simpler, lower-jank route on evidence rather than posting time.
- Authentic enables Desktop-N holders with owned preimages and explicit Revert;
  synthetic switch chords bypass interception without bypassing user ownership.
- Map native Spaces per display to independent core workspace layouts, apply
  trailing-empty lifecycle, and retain off-current window membership.
- Provide numbered/relative select, send-follow and send-stay with explicit
  unavoidable host differences and a short shell-neutral physical procedure.
- Pass macOS tests, strict clippy/format, signed build/verify and hosted CI.
  Physical acceptance remains pending until the user runs the procedure.

## Approach and bounded units

1. Implement/review a bounded owned-window drag transfer fixture and synthetic
   input provenance, then verify an eligible scoped native round trip.
2. Implement Authentic Desktop-N enable/Revert with conditional owned restoration
   and fake-backed settings coverage; real preference writes require verified
   backup and a permitted recovery procedure, otherwise defer to physical checks.
3. Integrate per-display mapping, independent layouts, workspace actions/defaults,
   trailing lifecycle and non-current-Space observation using existing core APIs.
4. Review native mutation/recovery boundaries, run final gates and publish the
   physical acceptance guide and durable route decisions.

## Evidence and open questions

- The approved manual drag routes and existing lifecycle/select evidence are in
  [the route survey](../research/macos-port/native-spaces-routes.md).
- MC AX selection has settled readback evidence around 628-660 ms; Dock gesture
  selection works both ways but its end-to-end latency and visual jank are not
  measured. Direct SkyLight transfer and the absent bridged dispatcher are not
  implementation candidates.
- Multi-display, macOS 15 floor, native fullscreen transfer and physical
  switch-back jank remain distinct acceptance questions; no unsupported outcome
  is silently selected.

## Diagnostic milestone (2026-10-16)

**Incomplete: synthetic transfer is not established and product workspaces are
not enabled.** The user-approved route is unchanged. No native preferences were
written, and the current tiling preview's bindings remain unchanged.

Delivered a bounded `spaces-drag-transfer` diagnostic, with either two existing
adjacent user SIDs or `--temporary` for an exactly adopted disposable Space.
One executor serves the native implementation and failure-injected tests:
title-bar grab, 50 ms grab settle, tagged HID switch chord, 400 ms held transition,
mouse-up, then bounded exact membership and current-SID polling. Every
post-down exit attempts release; unresolved input halts further Mission Control
actions and reports residue. Owned-Child identity, the selected focus oracle,
fresh AX positional identity, conservative CG occlusion checks and finite budgets
fence the posts. Pointer restoration uses an eventless warp, not input into the
original application.

The tap passes tagged exact Ctrl+Left/Right pairs without consuming, forwarding
or disturbing user-chord pending state. Untagged identical chords still follow
normal ownership. This is offline-proven only; HID-to-tap tag survival and
Desktop-N bypass remain unverified/unimplemented respectively. The tag is an
event-routing marker, not authentication against other local processes.

### Native evidence and limitations

- Signed LaunchServices runs observed Accessibility trust and unlocked session
  state before each leg. Bounded display-sleep companions were reaped by exact
  PID. The fixture did not arm a tap.
- The first temporary-mode run created a Space but refused before helper spawn
  because adjacency used the pre-create snapshot. Passing the adopted
  post-create snapshot repaired that fixture error. Exact-SID recovery removed
  the temporary Space and restored the original single-Space layout.
- The corrected run created one temporary Space (479 ms), verified independent
  public helper self-join O->T->O, then refused at the positional guard because
  above-normal-layer chrome covered the grab point. **Zero synthetic drag/key
  posts; no inverse dispatched.** This is a safety refusal, not a native
  transfer failure or SIP rejection.
- Exact helper stop/reap/record removal and absence passed. The temporary Space
  was removed (521 ms); final readback matched the original single-Space layout
  and current SID, with Mission Control absent. No helpers, probes, taps or
  display-sleep companions remained from these runs.
- Helper frame equality and protected-window stability read back false despite
  zero synthetic posts. Their causes are unresolved; no protected window was
  written back or focused. Foreground readback did not establish natural return
  after helper exit. Do not claim complete frame/focus restoration.
- Subsequent offline cleanup repair removed Mission Control closure from the
  unresolved-input branch, retained the display-sleep companion through final
  removal, made successful companion reap idempotent, and corrected early helper
  cleanup and synthetic-post reports. No further live run followed that repair.

### Route and consistency status

Selection has **not yet been chosen**: Mission Control AX has settled timing
evidence while Dock-swipe has selection evidence but no end-to-end timing or
visual-jank comparison. Existing evidence favors evaluating the simpler gesture
route without claiming it is already lower-jank. Create/remove still use the
proven Mission Control AX route.

No new macOS workspace defaults or semantics ship in this milestone. The
approved unavoidable host difference remains native drag-follow transfer and a
visible switch back for send-stay; its acceptability is for physical dogfooding.
Numbered/relative bindings must still be compared with KDE/Windows before
enabling them. Per-display numbering/chord attribution, native fullscreen and
multi-display transfer outcomes remain TBD, not inferred from this fixture.

The original Ctrl-as-Meta default-binding question was resolved by the user's
2026-10-16 Ctrl+Cmd auxiliary-arm choice, documented below and in the
[minimal discriminator](../spec/reference-outcomes.md#macos-native-workspace-discriminator).
The already-approved independent layouts, per-display mapping, trailing
lifecycle and send-stay route do not need to be re-decided.

### Verification

- macOS crate: 456 library tests and all eight integration suites pass; strict
  all-target clippy with `-D warnings` and workspace formatting pass.
- Unsafe-block count in the existing FFI module: **93 -> 96** (keyword
  occurrences 147 -> 151, including documentation). The three new blocks are
  maintained-binding AX hit-test/PID acquisition and retained-value ownership.
  No unsafe blocks occur in the new executor, runner or tests; no new dependency
  was added (only a feature on the existing Core Graphics binding).
- Final signed build and separate `just verify` pass for both bundles and the
  macOS 15 deployment floor. Final main SHA-256:
  `47c3ea8ac0718d2ab14d6b553db764269283cee9b835c534558c5ad0c07dd5ac`.
  The live artifact predates the final offline cleanup repair; live main SHA-256:
  `cd17a01575d5ec985e9ce65229027e28bf11df0457d083309a3485d031bf87cc`.
- Diagnostic source milestone `977463e` passed all eight hosted jobs:
  [CI 38866437557](https://github.com/beefsack/OmniTiler/actions/runs/38866437557).
  A preceding revision's Windows portable run exposed an existing fake-backend
  race: the observation-pump fixture released its worker before configuring the
  only readback. The test-only correction configures truth before release;
  production runtime and the existing oracle are unchanged. Targeted and full
  macOS tests, strict clippy and format pass after that correction.

## Continuation and physical acceptance

Do not repeat the blocked automated fixture unchanged. First resolve its
frame/protected-stability diagnostic gaps and arrange a provably clear
owned-helper grab point, or use a short user-run physical discriminator instead
of further automated tuning. The existing
[manual transfer check](../research/macos-port/native-spaces-routes.md#user-manual-transfer-check-2026-10-16)
already establishes the ordinary physical route; it does not prove synthetic
delivery.

Next implementation units remain Authentic Desktop-N owned preimages/explicit
Apply and Revert (real writes deferred to user-run checks unless separately
safe), workspace mapping/independent layouts, lifecycle/actions/bindings,
off-current tracking, and final physical acceptance. No product workspace
command or enable/Revert command exists yet; do not substitute diagnostic
commands for the requested dogfood procedure. The final shell-neutral physical
script must be added here once those commands exist, covering select,
send-follow, send-stay, enable/Revert and the user's switch-back-jank verdict.
Backlog item 1 remains open, not awaiting final physical acceptance yet.

## Ownership, catalogue and retained-layout milestone (2026-10-16)

**Still incomplete: native workspace actions remain unarmed.** This milestone
delivers settings ownership, action plumbing and independent layout retention,
not successful synthetic transfer or managed trailing-Space lifecycle.

### Delivered

- Explicit `spaces-desktop-apply [--count N]`, `spaces-desktop-revert` and
  read-only `spaces-desktop-status`. Apply enables the observed Desktop-N
  holder without rewriting its chord (default coverage 1–9, supported 1–10).
  The enable-only SkyLight API follows local Silica usage; no plist is written.
  Native enable effect and logout/reboot behavior remain unverified.
- Exact observable enabled/chord preimages are saved in a private durable
  Application Support backup before any write, with file and parent-directory
  synchronization. Safe standard-library file locks serialize the full
  transaction; fresh publication cannot replace an existing backup. Retry
  preserves original preimages. Revert restores only still-owned values,
  preserves external edits and retains unresolved recovery records. A rebound
  holder is preserved and warned unusable for the default transfer chord.
- Tagged exact Ctrl+digit switch pairs now bypass the tap; identical untagged
  user chords remain consumed. Offline-proven only; native tag delivery and
  numbered synthetic posting remain unverified/unimplemented respectively.
- Workspace catalogue/action plumbing: numbered select/send, relative
  select/send, previous toggle and output-send-follow; numbered/relative stay
  rows are bindable but unbound. Every workspace row is capability-disabled;
  the product does not intercept these new chords or pretend to execute them.
- User-selected auxiliary arm is Ctrl+Cmd with default Ctrl Meta, otherwise
  shared Meta+Ctrl. Numbered actions retain plain Meta and Shift sends. Static
  conflict checks avoid known Ctrl+Cmd+F/Q/Space/D; custom global/common-app
  conflicts still require physical checking. Compatible leaves established
  unshifted Ctrl arrows/digits to macOS without inventing replacements.
- Core domains now retain each native Space's independent layout across manual
  switches. Off-current windows remain members; hidden domains reconcile without
  frame/focus emission or desired-geometry writes. Current-only native effect
  fences remain. Deleted Spaces/displays retire their domains; transient type
  gaps retain hidden domains, and unprovable display mapping/geometry parks
  existing sessions rather than destroying them. Native row order is retained.

### Corrected drag fixture: safety refusal, not transfer proof

The changed fixture checks five titlebar points with fresh CG occlusion and AX
identity, without exemptions for foreign chrome. One bounded native run again
refused before posting: **all five points were covered, zero mouse/key posts,
no inverse dispatched**. Together with the earlier fixed-center refusal,
automated grab-point tuning is stopped pending a changed fixture or physical
discriminator; no production transfer path depends on this failed fixture.

The run's terminal JSON reports independent helper O→T→O recovery first
(selection 673/672 ms, self-join 321/220 ms), temporary create/remove 491/496 ms,
exact temporary-Space removal and original single-Space/current restoration.
Post-run `mc-inspect` reports one original Space, Mission Control absent and an
unlocked trusted session. Exact helper exit/reap/record removal, helper absence
and display-sleep companion reap are reported. These are cleanup evidence, not
synthetic transfer acceptance.

New diagnostics identify a protected-window bounds change despite zero posts,
an unreadable final helper AX frame, and split foreground sources after helper
exit. No protected window was written or focused. Frame stability and natural
foreground return are **not established**. Stop live work under these conditions;
do not relax occlusion or focus guards to get a passing result. The live artifact
SHA-256 was `66b336f70f4d14869159a19474108e8bdad78401a0c64bd6225df92482b578b6`;
it predates the later retained-layout changes.

### Verification and exact continuation

- Fake-backed settings tests cover lock contention, first-preimage retention,
  partial failure/retry, durable-path derivation, sync refusal, external edits,
  malformed recovery data and rebound chords. Catalogue tests cover all Meta
  roles/presets, exact auxiliary arms and duplicate freedom.
- Native-layout fake tests cover nontrivial O→T→O frame preservation, hidden
  close, no hidden effects/focus, deletion, display removal, unreadable type,
  skipped-display parking/recovery and native row ordering.
- The input fixture intermittently submitted focused sequence 2 before worker
  startup sequence 1; this changed core admission versus focus projection and
  made grow-at-edge a legitimate no-op. Waiting for startup observation
  publication fixes the causal test race without changing the geometry oracle.
  The failing target passed 32 consecutive runs after repair.
- Retained-layout tests now distinguish superseded worker frame records from
  the current engine plan, including revision-stamped hidden-write checks.
  This repairs an asynchronous fixture race without weakening exact layout
  preservation or zero-hidden-write oracles.
- Final local macOS gates: **636 tests** (507 library plus all eight integration
  suites), strict all-target crate clippy and workspace format pass. Signed
  build and separate verification pass for both bundles and the deployment
  floor. No full-workspace macOS clippy claim (the known core portability
  blocker remains outside this change).
- Source milestone `c1655e9d` passed hosted macOS, Linux Rust/strict clippy,
  KWin, native, shell and dependency gates in
  [CI 38893345659](https://github.com/beefsack/OmniTiler/actions/runs/38893345659).
  Windows exposed an incorrect Unix-only refusal in the new transaction-lock
  helper. The causal repair uses portable standard-library locking while
  preserving Unix no-follow/private-mode safeguards; the same fake-backed
  ownership assertions run unchanged, rather than being skipped.
  Follow-up `8d445ba1` passes Windows tests too; its Windows strict clippy
  requires explicitly non-truncating lock-file opens. Both platform branches
  now state that existing lock-file contents are preserved.
  `b9338c38` passes Windows and all Linux gates; macOS exposed an existing
  ambient-PID test assumption (a supposedly mismatched PID was identical and
  happened to be alive). That test now exercises the exact pure PID/bundle
  ownership boundary; production authorization is unchanged.
- Unsafe blocks in the existing FFI module: **96 → 100** (four narrowly scoped
  dynamic hotkey SPI resolution/read/enable blocks); no unsafe elsewhere and no
  new dependency. Standard-library locking avoided an additional unsafe block.

Next: resolve the failed owned-helper fixture with a clearly unobstructed
user-arranged helper or a short physical synthetic-transfer discriminator before
building product transfer on it. Do not repeat the unchanged automated run.
Then implement selection (MC AX versus gesture remains an evidence-led choice),
digit posting, native action execution, trailing-Space lifecycle and actual
bindable send-stay configuration. Cross-display attribution/transfer and native
fullscreen remain unsupported outcomes, not guessed policy. Existing independent
domains require no core redesign.

### User-run settings smoke check (not the final workspace dogfood script)

Use only while unlocked, with disposable apps and no game/fullscreen context.
First inspect the JSON; if SPI is unavailable, stop. Run commands one at a time
and wait for each bounded launch to exit (all are shell-neutral):

```text
mise exec -- just launch -- spaces-desktop-status
mise exec -- just launch -- spaces-desktop-apply --count 2
mise exec -- just launch -- spaces-desktop-status
mise exec -- just launch -- spaces-desktop-revert
mise exec -- just launch -- spaces-desktop-status
```

Expected: Apply reports verified per-holder enabled readbacks with a saved
preimage; Revert restores unchanged owned preimages and removes the backup only
when complete. Do not manually edit those holders between Apply and Revert unless
deliberately testing external-edit preservation. On any failure retain the backup
and report JSON; do not repeatedly Apply or manually delete recovery files.
Native switch behavior itself is not proven by these settings readbacks.

The requested final real-app select/send-follow/send-stay/Ctrl+Cmd/jank procedure
cannot truthfully be supplied until those product commands work. Backlog item 1
remains active rather than awaiting final physical acceptance.

## Native settings and occlusion diagnosis checkpoint (2026-10-16)

**Incomplete: synthetic transfer remains unproven; workspace actions stay
unarmed.** This checkpoint establishes session-effective Desktop-N ownership,
not managed-workspace completion or physical shortcut acceptance.

### Accepted evidence

- Five fresh signed LaunchServices invocations performed status, Apply for two
  Desktop-N holders, status, Revert and status. Both holders changed from disabled
  to enabled with unchanged chords and successful SPI readback. The intervening
  fresh process independently corroborated both enabled Ctrl+digit chords.
- Revert restored both observed preimages, reported no failures or external
  edits, and removed the preimage JSON. Final status matched the original
  effective state. The symbolic-hotkey preferences were unchanged after Apply
  and Revert: this proves persistence across processes in the same login session,
  **not logout/reboot persistence or native chord delivery**. The empty lock file
  remains intentionally; no recovery preimage remains.
- The settings artifact SHA-256 was
  `26ffc2521deb6f3600a451a6c93298241339271621217ba889a813afb770294e`.
  The persistent probe/helper bundle IDs and designated requirements were
  verified; every leg observed an unlocked session and reaped its bounded
  display-sleep companion by exact PID.

### Transfer invariant and failed hypotheses

- The fixed helper spawn overlapped an ordinary terminal window. Moving only the
  helper through alternative desktop positions did not produce an eligible grab:
  an onscreen, display-spanning layer-20 Dock surface covered every candidate in
  the conservative CG rectangle test. No foreign window was moved or exempted.
- Explicit Mission Control closure followed by a bounded ten-second observation
  did not resolve that condition: Mission Control AX reported absent while the
  same Dock surface remained. Thus neither spatial arrangement nor waiting for a
  presumed Mission Control tail solved the fixture. No further drag attempts
  followed; **all three diagnostic runs made zero synthetic posts**.
- A separate causal fixture fault was identified: LaunchServices can publish the
  bundled helper identity after spawn, invalidating the early authorization
  snapshot. Final frame observation now re-authorizes just like per-leg setup;
  refusal still reports unknown, never frame equality. The corrected final-frame
  path has not reached live acceptance because the synthetic legs remain blocked.
- Retained source changes are limited to occluder IDs/layer/geometry in refusals,
  reporting Mission Control close shortfalls and fresh final-frame authorization.
  The unsuccessful arrangement search and overlay-wait heuristics are **not**
  retained. Occlusion, exact ownership and input-release guards are unchanged.
- The last transfer diagnostic artifact SHA-256 was
  `303b3f4964af96c4bcb315873dc1a5b40a7e55dfe1b98a382e4ddbc068870cce`;
  it includes those subsequently removed diagnostic heuristics. Its failure report
  names the exact temporary SID; separate narrow recovery reports verify removal
  after each run. Final readback shows the original single-Space layout/current,
  Mission Control absent, unchanged protected-window frames/membership and full
  foreground agreement. Helpers exited/reaped with records removed, and no probe
  or display-sleep companions remain. No Ghostty input or window mutation occurred.

### Verification and next discriminator

- Final retained tree: **637 tests** (508 library plus eight integration suites),
  strict all-target macOS crate clippy, workspace format, signed build and separate
  verification pass. Unsafe blocks remain **100 → 100**, all in `ffi.rs`; no new
  dependency or shared-core change. Final signed main SHA-256:
  `a61315d83a3cff77bd51c9725f36f2ea075bf481cc0ea74a3f76cbdfde3d1871`.
  This final trimmed artifact has no new live-transfer acceptance claim.
- **Open technical question:** does the persistent Dock CG rectangle actually
  intercept pointer input, or is it non-interactive/transparent backing? Current
  observations cannot distinguish them. First obtain a read-only unfiltered CG
  snapshot plus AX point identity before opening Mission Control, then compare
  after closure. Membership-filtered snapshots omit membership-less surfaces and
  cannot answer this. Do not add a Dock exemption or retry transfer unchanged.
- Route choices remain unchanged: synthetic title-bar drag plus native chord for
  transfer; MC AX or Dock gesture selection is not yet chosen. Create/remove stay
  MC AX. No new tentative product decision. Existing host differences remain
  Ctrl+Cmd auxiliary arms, native drag-follow and visible send-stay switch-back,
  plus explicit session-effective Desktop-N Apply/Revert rather than plist writes.
  Cross-display/fullscreen/floor acceptance and physical jank remain TBD.
- Product select/send/follow/stay, trailing lifecycle and the final real-app
  physical script remain outstanding; the settings-only procedure above is not
  that script. Keep backlog item 1 active, not awaiting final physical acceptance.

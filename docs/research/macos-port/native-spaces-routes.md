# Native Spaces: non-SA routes and bounded probe plan

Initial source survey, 2026-10-14. Policy: native Spaces first, yabai primary reference,
tier 2, SIP enabled, no scripting addition (SA) or Dock injection; macOS 15
floor. **Native window moves are not established impossible:** yabai 7.1.25
claims SIP-on recovery, with a reported bridged-operation success on Tahoe.
Enumeration/membership and bounded mutations now have
[owned-fixture evidence][phase0]. The [live verdict below](#live-verdict--2026-10-16)
established selection and lifecycle routes, but not ordinary-window transfer
at that checkpoint. Subsequent managed drag-send acceptance is recorded in the
[managed-workspace checkpoints](../../changes/macos-managed-workspaces.md).
Source routing and observed effects remain separate.

FACT = inspected pinned source; CLAIM = upstream report, not our experiment;
EVIDENCE = our observed readback; UNKNOWN = not established. Source was read
from existing checkouts; public issue and OS-help links support upstream claims.
Pins match the [availability snapshot][pins], including its newer Amethyst,
rift, glide and paneru pins; historical plan citations are not advanced.

## Trailing lifecycle findings (2026-10-20)

- **FACT, yabai first:** `dd845723` [space.c:17-70][Y19] queries
  `SLSCopyWindowsWithOptionsAndTags` with options `0x2` (`0x7` including
  minimized), zero set/clear tags, then filters parent IDs, levels 0/3/8 and
  attributes/tags. Managed-window eligibility additionally excludes sticky,
  hidden, minimized, floating and nonstandard windows
  ([window_manager.c:272-281][Y20]). This is not a complete destructive-emptiness
  proof. Its [destroy/create handlers][Y21] check user/last-Space/animation
  eligibility, not emptiness, and dispatch through the scripting addition.
  They provide no safe tier-2 shortcut for creation or empty-only removal.
- **FACT, ordered remainder at [snapshot pins][pins]:** rift `8114ccc`
  [window_server.rs:426-514][R3] uses `0x7` membership, defines sticky as more
  than one Space, and enumerates onscreen CG windows excluding desktop elements.
  AeroSpace `74a1bf1`, Amethyst `5cfe2fe`/Silica `ebead82`, FlashSpace `9680153`,
  paneru `8a21331` and glide `ef3b30f` provide no faster native create/remove
  route in the inspected sources. Hammerspoon `23e387e` [MC AX lifecycle][H2]
  remains the SIP-on create/remove precedent. Its
  [window-list API][H4] warns that overlays/tooltips/offscreen windows need
  downstream filtering. No surveyed reference has an AccessibilityUIServer
  exemption matching our destructive proof; do not attribute ours upstream.
- **EVIDENCE, deliberate divergence:** retain full CG inventory plus fresh
  per-window `0x7` membership, including unmanaged/hidden user windows, rather
  than transplanting yabai's management filter or unchecked destroy. Exact system
  AccessibilityUIServer, offscreen layer 3, 0x0, had partial membership
  `[434,1]` over user Spaces `[1,277,434]` and no AX windows. The existing
  every-Space sticky check therefore produced a false unknown. The
  [user-approved exact-owner subset exception](../../decisions.md#scope-and-platform-goals)
  is narrower than ignoring a bundle, layer or all zero-area windows.
- **EVIDENCE and limit:** positive trailing selection reuse confirms in 122 ms;
  MC create plus immediate selection confirms in 900-944 ms. Inherited 434
  removal and runtime-owned 445/455 shutdown removal succeed. ControlCenter
  5559 initially blocked 467 removal. The subsequent exact-system offscreen
  layer-25 120x30 exception additionally requires fresh readable empty owner AX
  inventory; its one verified removal restores `[1,277]`/current 1. Other shapes,
  normal-layer windows and unknown AX/owner evidence still block. Two fresh
  create/automatic-remove cycles and user-run opened/pinned ControlCenter contrast
  remain unverified.
- **FACT, chrome versus content:** Amethyst `5cfe2fe`
  [ignores ControlCenter wholesale][A4]; that management filter is too broad for
  destructive emptiness. AeroSpace `74a1bf1` captures an accessory ControlCenter
  [AX system-dialog popup][AS2]. It is user-visible content, not grounds to
  ignore the owner: our stronger empty-owner-AX-inventory check excludes it.
  The captured CG layer-2005 352x149 panel remains a candidate independently.
  No surveyed reference establishes a generic safe ControlCenter removal rule.
- **FACT, missing send step:** yabai's [native-send tail][Y22] moves, then calls
  `SLSSpaceSetFrontPSN` for the subject application before later selection;
  [the event handler][Y13] pins front then gestures. The [ABI][Y23] takes a
  by-value `ProcessSerialNumber` and returns `CGError`. Our earlier bridged
  follow copied move plus swipe but omitted this destination-front pin.
  In the captured sole-visible-subject fixture, exact membership/current arrives
  in 65 ms from native dispatch while Finder remains frontmost: focus fails and
  the outcome is honestly `PossiblyExecuted`.
- **Implementation, not live acceptance:** front pin now precedes the one swipe
  with fresh session/owner/anchor checks, complete capability gating before
  bridged-route choice and exact final subject-focus proof. No uncertain
  post-arrival AX raise, replay or second selector. Remainder survey offers no
  competing native-send focus contract: rift/paneru/glide have focus primitives
  but no equivalent native send; AeroSpace parks; Amethyst/Silica drag; FlashSpace
  activates apps; Hammerspoon's compatibility move has no post-move focus proof.
  Reused/new-target hardware follow acceptance and timing remain TBD.
- **Fixture rule:** `mc-members.layout.space_ids` is sorted, not native order.
  Application activation can change Space order during return. Retain full
  ordered `spaces-diagnose` rows and prove the actual last SID is the empty
  target before claiming reuse; do not infer a stale row or lost binding from
  a second append after such a reorder. [Live outcome](../../changes/macos-managed-workspaces.md#trailing-lifecycle-investigation-2026-10-20).

## Send latency findings (2026-10-20)

- **FACT, corrected discovery:** [Y18] uses [Y16] to resolve the internal mangled bridged
  dispatcher through loaded Mach-O symbols, not `dlsym`. The previous unmangled
  `dlsym` miss was a false-negative lookup, not proof of absence. The corrected
  read-only lookup finds dispatcher, operation class and init selector; one
  owned-helper bridged move changed exact membership to the inactive target,
  kept current unchanged and passed independent public self-restore after the
  mover exited. Its 3.157 s settle was a full observation window, not move time.
- **FACT, reference default:** [Y1] allocates/initializes
  `SLSBridgedMoveWindowsToManagedSpaceOperation`, dispatches asynchronously and
  releases it. On macOS 15+, [Y2] bypasses the old direct API if this dispatcher
  is absent, then tries SA before the compat triple. The earlier old-direct
  no-effect result tested neither bridged nor compat. Compat still has no exact
  tag-preimage getter in the surveyed sources: reset-to-zero is not restoration.
  No compat write, SA attempt, injection or reduced SIP is added.
- **FACT, retiling:** [Y4] moves then tiles the destination in the send handler;
  [Y17] refreshes application windows, validates the visible user Space and
  updates/flushes invalid/dirty views on `SPACE_CHANGED` and `DISPLAY_CHANGED`.
  Our adapter follows the event-driven shape with complete owner-thread AX
  rereads on topology changes and verified arrival, not blind frame writes.
  Unlike direct upstream retiling, existing current-Space/revision fences remain:
  a fast stay return can precede destination geometry; revisit triggers tiling.

| Ordered reference survey at [snapshot pins][pins] | Relied-on finding; reason for divergence |
| --- | --- |
| yabai `dd845723`, [Y1], [Y2], [Y4], [Y16], [Y17] | Bridged-first move, synchronous layout update and event-driven visible-view validation. Copy move ABI/discovery and event-driven reread; retain our exact effect proof, current-only setters and no-replay policy. |
| rift `8114ccc`, [R1], [R2] | Native membership queries and versioned switch gestures, no native move binding found. No evidence for a faster native transfer. |
| AeroSpace `74a1bf1`, [MacWindow.swift:123-183][AS1] | Corner parking/unparking, not native Space sends. Not selected because workspace semantics differ. |
| Amethyst `5cfe2fe` / Silica `ebead82`, [A1]-[A3] | Drag fallback's 50 ms grab and 400 ms hold; Amethyst adds a 1 s switch-back delay. Retain unmeasured fallback holds as reference values, not OS minima; use verified Dock-swipe return, not the fixed switch-back sleep. |
| FlashSpace `9680153`, [WorkspaceManager.swift:180-230][FS1] | App hide/show/activate changes app-level workspace assignment, not per-window native membership. Not selected. |
| Hammerspoon `23e387e`, [H1], [H2] | Same compat triple without effect/preimage proof; MC selection defaults to a 300 ms wait. Neither beats the proven bridged move under our restore contract. |
| paneru `8a21331`, [skylight.rs:144-256][P1], [windows.rs:906-960][P2] | SkyLight reads/SLPS focus plus virtual strips; no native move found. Not a replacement for native sends. |
| glide `ef3b30f`, [window_server.rs:215-284][G1] | SLPS focus and native-Space integration, no native move found. No faster transfer contract established. |

- **EVIDENCE, synthetic scoped TextEdit:** bridged-first sends now dispatch and
  immediately select via Dock-swipe, without a serial 200 ms membership poll.
  Final exact membership/current/focus gates success; late completion only gets
  bounded readback, never a second move/select or drag fallback. Baseline
  admission-to-native-arrival 547-575 ms becomes 88-98 ms. Comparable empty-target
  first exact subject geometry becomes 270 ms from 1,000 ms; inverse is 218 ms.
  These are a few controlled samples, not a distribution or visual/input-ready
  measurement. Floor-release, multi-display and live post-create fast sends are
  unproven. [Checkpoint and limits](../../changes/macos-managed-workspaces.md#send-latency-work-2026-10-20).
- **Remaining waits:** absent bridged capability selects the original drag
  before any effect. Its 50/400 ms constants cite [A1]; lowering them is unproven.
  Post-select/readback deadlines bound asynchronous completion, not stabilization
  sleeps. The 1 s observer backstop remains only for lost/unsupported events.

## Dogfood latency investigation (2026-10-19)

- **Subsequent route choice:** the user selected the yabai-informed Dock-swipe
  selector for dogfood, then approved it as the default on 2026-10-20 with MC AX
  automatic fallback and user-selectable routes still to implement (backlog
  item 3b/3c). At the original selector checkpoint, transfer,
  MC trailing create/remove, send waits and observation/reconcile timing remain
  unchanged. The [implementation checkpoint](../../changes/macos-managed-workspaces.md#dock-swipe-selector-dogfood-checkpoint-2026-10-20)
  has corrected pre-post order drift and terminal ownership/cleanup. The selector
  was available for UX dogfood; conservative unknown occupancy then refused
  positive reuse and retained a test-created Space. The later lifecycle findings
  above supersede that reuse/cleanup blocker, not the owed follow-focus checks.
  The comparison recommendations below describe
  the preceding investigation, not an outstanding route-choice requirement.
- **Pinned mechanics rechecked:** [Y7] computes absolute target/current native
  ordinal distance, posts that many immediate Began/Ended session-tap pairs
  (progress +/-1, velocity +/-9999), and relies on later Space-change handling
  rather than synchronous arrival verification. Full native order, including
  intervening fullscreen Spaces, determines distance. Same-display needs no
  cursor warp or click. [R1]'s separate >=27 payload remains unvalidated and
  refused by the existing payload gate. Exact current SID is effect evidence,
  not visual completion or usable-input latency.
- **Disposable product checks, 2026-10-20:** existing numbered/relative/previous
  selections confirmed exact native current in 43-62 ms, settled non-adjacent
  selection in 54 ms, and numbered send-stay's return in 46 ms. Main-loop
  observation was roughly 338-371 ms. These boundaries differ from prior MC
  goto's 628-660 ms end-to-end; neither establishes usable-input/visual latency.
  Immediate post-create gestures twice posted without arrival. A later successful
  settled target does not prove that Space age is causal or justify a fixed wait.
- **Missing source contract identified:** pinned yabai `src/mission_control.c`
  registers Dock application `AXExposeExit` and clears MC-active state on that
  notification. This is not equivalent to AX-group absence. The product now arms
  that notification before create and awaits exit before swipe, off-main with a
  genuine deadline; no selector sleep/retry/fallback. The reviewed observer passed
  live post-create acceptance: dispatch/posted/confirmed 915/917/945 ms including
  MC creation; main-loop observation 1178 ms. Empty-target Finder focus is not
  document-focus acceptance. Subsequent trailing reuse refused unknown emptiness
  before a gesture, preserving current and count. Test-created SID 434 remains
  inactive residue; no foreign-chrome exemption or removal loop is introduced.
- **Conclusion:** Mission Control exposure is a cost of the current selector,
  not a demonstrated SIP-on native-Spaces requirement. Keep native workspaces
  and tier 2; recommend comparing the existing same-display Dock-swipe diagnostic
  before choosing a product selector. No route change is selected here.
- EVIDENCE: the user reports slow MC selection and slow destination tiling after
  send on one display. Earlier MC goto legs measured 628-660 ms each; gesture
  15/131 ms measured **focus/posting only**, not completed animation or usable
  input. No retained macOS phase-timing trace establishes the current bottleneck.
- FACT below means static source, not measured elapsed time. The historical
  "transfer unresolved" verdict and unresolved Desktop-N probe are superseded
  by numbered send-follow/stay acceptance in the managed-workspace record;
  physical feel, minimum-release and multi-display acceptance remain separate.

### Current cost and retile chain

| Stage (FACT, repository source) | Cost or trigger; what it does not prove |
| --- | --- |
| Select: `crates/tiler-macos/src/tile.rs:2407`, `workspace_select.rs:740,941`, `mc_lifecycle.rs:418-425,508-526,934-974` | Main-loop synchronous MC open, stabilize, fresh SID/index join, thumbnail AXPress, close-if-present, current-SID verification. Blocks that main-loop turn; native visual completion is not measured. |
| MC waits: `mc_lifecycle.rs:243-297`; `mc_probe.rs:53-67` | Fixed **300 ms** stabilization. Open/close each poll up to 2 s (100 ms slices); goto effect up to 3 s (200 ms slices after an immediate read). AX messaging timeout 2 s per element. Bounds are not mandatory sleeps; goto normally auto-closes MC. |
| Send: `workspace_send_managed.rs:563-730`; `space_drag_transfer.rs:508-651` | Dedicated effect owner; fresh identity/focus/dual-hit proof before down-edges, fresh ordinal/holder resolution, mouse down/drag, switch chord, release, exact membership/current readback. Fixed **50 ms grab + 400 ms animation hold**, then immediate read or up to 3 s polling at 200 ms. Native slide overlaps the hold; proof/read costs and any remaining transition are not independently timed here. |
| Send proof: `workspace_send_native.rs:140,177-182,743-820`; `workspace_send_managed.rs:262-386` | Engine-facts query has a 200 ms timeout, not a 200 ms fixed delay. Repeated fresh AX/CG/SLS and ordinal checks may add time; do not remove safety gates without evidence. |
| Stay: `workspace_send_native.rs:950-964`; `workspace_send_managed.rs:779-862` | Forward transfer follows the destination, then released MC AX selection returns to the pinned source. Thus stay adds another visible selection leg; changing standalone selection alone does not automatically change the return selector. |
| Logical move: `crates/tiler-core/src/world_sends.rs:103-125,137-335`; `world_engine.rs:2078-2145` | Admission retains the transfer and shields the mover into the target layout **before native ack**. No universal "wait for Applied before planning" fence. |
| Native completion: `tile.rs:2348-2395`; `select_feedback.rs:104-123`; `runtime_thread.rs:1269-1286` | Polled on the 250 ms main-loop cadence, then feedback staged for a later world report. Not an immediate scoped reread or layout dispatch. Adds up to a cadence under a responsive loop, not a fixed 250 ms minimum. |
| Space change: `ffi.rs:8222-8324`; `tile.rs:2214-2258`; `runtime.rs:779-806` | Notification only marks dirty. Main-loop topology read publishes current-SID/revision; publication invalidates stale replies but queues **no reconcile job**. Active-Space-change alone therefore does not retile. |
| Observation: `native_tile_backend.rs:616-665,1151-1220`; `runtime_native.rs:167-182,331-375` | Per-app AX notifications trigger membership rereads/reconcile; otherwise 1 s silence backstop (failures may delay longer). No AX Space-change subscription among `ffi.rs:483-507` watched notifications. A Space hop may emit focus/move events, but prompt delivery is UNKNOWN. |
| Frame apply: `world_dispatch.rs:852-937`; `native_tile_backend.rs:216-295,298-418,1461-1520` | Only current-scope controlled frames emit. Each AX size/position/size leg rechecks live membership/current Space, revision and show-state. A different engine dispatch after topology refresh can tile the shielded mover before its own observation; its scoped reread is the reliable convergence path, not a proved prerequisite for first tile. |
| Retained settlement: `crates/tiler-core/src/world_engine.rs:1106-1180` | Observed target homing settles the window transfer. `Applied` alone does not. Settlement and first successful geometry write are distinct milestones. |

- **Likely fixable gap, not diagnosed latency:** topology and completion both
  stage facts without dispatch. If no relevant AX event or other dispatch
  arrives, destination layout convergence can wait for the 1 s reread. An
  event-driven, coalesced reread on the affected AX owners after active-Space
  change or verified send arrival is a candidate; preserve full observations,
  revision fences, current-Space-only writes and fail-closed behavior.
- Existing event/readback `t_ms` series lives in
  `workspace_send_native.rs:599-614,865-945`; observation/revision outcomes use
  metadata-only runtime events. Needed measurement joins: admission -> proof ->
  chord -> release -> exact membership/current -> topology publish -> notified
  or backstop reread -> reconcile -> first successful frame readback. Current
  traces do not identify the read trigger or visual/input-ready completion.

### Does yabai have the same limitation?

| Operation at pinned yabai source | SIP-on / no SA | With SA; consequences |
| --- | --- | --- |
| `space --focus` [Y7] | Fallback posts Dock-swipe Began/Ended per ordinal step, progress +/-1, **velocity +/-9999**. No MC thumbnail traversal. [#2780](https://github.com/asmvik/yabai/issues/2780) claims near-instant switching; initial input activation lag was reported, then claimed fixed. A later release-family breakage report reinforces version coupling. | First tries Dock-owned direct Show/Hide/current-Space update [Y5], avoiding the ordinary slide/MC route. Upstream #2780 contrasts its lack of input lag; no comparative local benchmark. SA/reduced SIP is outside the selected tier. |
| `window --space` [Y1], [Y2], [Y12] | Bridged asynchronous operation if available; else old direct API only below workaround thresholds; else SA attempt then compat-tag fallback. No drag or fixed 450 ms hold in these move helpers. 7.1.25/[#2788][move-issue] claims bridged success on 26.4/26.4.1, not the macOS 15 floor or every fallback. Our earlier dispatcher was absent; direct call had no effect. | Fallback SA moves from Dock's connection. A send without follow need not switch desktops. A direct membership move is different from desktop activation or animation; source cannot establish end-to-end tiling speed. |
| Cross-Space window focus [Y10], [Y13] | SLPS make-key/AXRaise; optional `skip_window_focus_animation` races native activation with gesture focus. [#2781](https://github.com/asmvik/yabai/issues/2781) and 7.1.21-23 [Y3] describe suppression and focus/gesture conflicts; default off at the pin. **Not a general move-animation toggle.** | Do not confuse this with `space --focus` or move authority. #2788 reports send+follow interaction and recommends separate move then Space focus. |
| `space --switch` [Y7] | Same-display branch has **no gesture fallback**; cross-display branch swaps window sets. Not the command to copy for ordinary selection. | Same-display is SA focus. |

- Other references (FACT): rift [R1] uses a directional gesture, legacy 15 ms
  end delay with velocity 200 times progress magnitude, and a separate >=27
  payload/velocity family. This supports a versioned gesture route, not blanket
  compatibility. Amethyst/Silica [A1]-[A3] share 50/400 ms drag waits; Amethyst
  adds a 1 s delayed switch-back. Hammerspoon [H2] exposes MC, waits for AX UI
  and documents Reduce Motion as minimizing, not removing, the visual shift.
  AeroSpace corner parking and FlashSpace app hide/show avoid native Space
  transitions by changing workspace semantics (per-window parking versus
  app-level assignment), not by finding a faster native setter.

### Ranked candidates (recommendations, not selected changes)

| Rank / candidate | Expected effect and evidence status | Tier, risk and decision consequence |
| --- | --- | --- |
| 1. Same-display high-velocity Dock-swipe selection | Removes MC open/traversal and 300 ms stabilization; aims to suppress slide. **EVIDENCE:** exact SID round trip, no warp/click. **CLAIM:** visually instant upstream. **UNKNOWN:** local end-to-end/input readiness. Velocity is already the suppression mechanism, not a separate proven fix. | Tier 2/SIP-on; undocumented event fields and multi-step ordinal drift/version coupling. Already allowed by 2026-10-16 route; replacing 2026-10-17's explicit MC fallback still needs a user choice. Keep exact target readback/no replay and same-display restriction. Does not fix drag-send hold or automatically accelerate stay return. |
| 2. Measure, then targeted event-driven observation/reconcile | Could remove near-1 s silent-observation tail after send; **FACT:** notification/dispatch gap above; **UNKNOWN:** share of reported delay. Use active-Space event plus verified arrival to request coalesced complete rereads, not blind frame writes or a faster global poll. | Tier 2/SIP-on; observation races/stale-revision loops must be covered offline. Consistent with notification-primary recovery and existing fences. Measure before selecting exact trigger/owners. |
| 3. Desktop-N chord selection | Avoids MC overview/traversal and fixed 300 ms; ordinarily leaves native slide. **EVIDENCE:** chords transfer windows in current managed sends; standalone posted select speed/physical feel remains UNKNOWN. | Tier 2/SIP-on. Authentic already owns/enables Desktop 1-9 with preimage/Revert; reuse verified holders/provenance, no additional settings writes. Index-bound, collision/reorder risk, <=9 range; relative/previous need resolution and >9 fallback. New selector choice needs user decision. |
| 4. User-owned Reduce Motion | Reduced movement / possible crossfade rather than slide; not an animation-free guarantee or retile fix. [Apple](https://support.apple.com/guide/mac-help/mchlc03f57a1/mac) documents reduced desktop-switch motion; Hammerspoon [H2] says MC shift remains. Exact crossfade/duration/input effect here is UNKNOWN. | Public OS setting, SIP-on; system-wide visual consequence, user changes/restores it only if desired. Does not remove our 300/450 ms fixed waits. Settings location varies by release (Accessibility Display or Motion). |
| 5. Tune holds/proof cost; offload synchronous select | At most the measured unnecessary part of 50/400 ms holds/proof/loop blocking. **FACT:** explicit waits and blocking exist; safe lower thresholds UNKNOWN. Does not by itself suppress OS animation. | Tier 2/SIP-on; early release can break transfers, weakened identity gates can misdirect input. Keep safety proof; no speculative sleep removal. MC AX/AppKit thread requirements need design review before moving work. |
| Deferred: bridged move / SA / virtual workspaces | Bridged move could avoid drag/follow and help stay if available; local symbol/restore gate remains. SA direct activation avoids normal transitions; virtual workspaces avoid native switching entirely. | Bridged remains tier 2 but version/authority/restore-unproven; off critical path by recorded route. SA changes SIP/security tier; parking/app hiding changes user-visible semantics. Neither route change is justified before tier-2 selector/observation investigation. |

- **Inherent to the selected drag method:** follows destination, requires a
  release/readback boundary, and stay switches back. Not proved inherent to all
  SIP-on native moves. The 400 ms hold is an implementation choice inherited
  from Silica, not a documented OS minimum. Native transitions are asynchronous
  and APIs/version behavior constrain guarantees; neither the MC overview nor
  a 1 s retile delay is established as unavoidable.

### Minimal user-run discriminator (not executed)

- Follow [live macOS guidance][live]. One display, two existing ordinary
  Desktops, only disposable TextEdit documents, no terminal target. Keep exact
  original topology/current, document memberships/frames and optional setting
  preimage; restore before ending. Use the existing bounded scoped TextEdit
  procedure in the managed-workspace record, not an unscoped product launch.

| Sequence | Discriminator / required result |
| --- | --- |
| Current MC select A -> B -> A; with tiler stopped, physical native Desktop-N chord A -> B -> A only if independently already enabled (otherwise skip) | Separates MC overview from native slide. Note visual completion **and usable input**, exact current SID and unchanged memberships. Authentic Revert may disable its previously owned chords; do not enable extra shortcuts for this comparison. |
| Product send-follow A -> B -> A, with a resident document at each end; wait for settlement after each action | Note whether movement ends before the first correct tile. Join existing event `t_ms`, exact membership/current, observation/reconcile and frame outcome. If logs cannot distinguish event/backstop, add metadata-only phase instrumentation in a separately chosen implementation unit. |
| Optional same two pairs after user toggles Reduce Motion, then restore its preimage | Tests crossfade/visual preference versus actual tile delay; no agent setting write. Stop on wrong target, focus disagreement or restoration ambiguity. |
| Separate reviewed Dock-swipe diagnostic A -> B -> A | `spaces-switch-probe` exists but is a combined setup/recovery/arrow/gesture harness, not a safe one-line gesture benchmark. Review its current ownership/preflight and isolate the comparison before offering commands. Require exact SID/focus restoration, no warp/click, and visual/input-ready timing; do not call old posting-only times end-to-end latency. |

- Exact next decision: whether to prioritize the bounded Dock-swipe selector
  comparison and metadata-only send phase measurement. No product code, native
  test, settings, SIP policy or route decision changed in this investigation.

## Route classification

“No” in the SA column describes the particular route, not every yabai branch.
Public AX UI automation and synthetic system input are tier-2 candidates;
visible transitions, scope and unproven recovery constrain their probes.
AppleScript/System Events is an AX-automation wrapper, not another native API.

| Operation | Candidate route / yabai SA routing (FACT) | SIP-on status, version caveat and evidence |
| --- | --- | --- |
| Window → Space, same display | Bridged `SLSBridgedMoveWindowsToManagedSpaceOperation.initWithWindows:spaceID:` + `SLSPerformAsynchronousBridgedWindowManagementOperation`; **no SA**, yabai first branch [Y1] | CLAIM: [#2788][move-issue] reports 26.4/26.4.1 success without disabling SIP; 7.1.25 changelog says moves work again [Y3]. Not proof for 15 or every newer family. EVIDENCE: dispatcher absent, class present; no dispatch. |
| Window → Space, same display | `SLSMoveWindowsToManagedSpace`; **no SA** on old direct branch [Y1] | FACT: yabai bypasses this branch on 12.7+, 13.6+, 14.5+, and every major ≥15 [Y2]; Hammerspoon independently changes route at 14.5 [H3]. Diagnostic direct call is not the source-recommended floor-15 route; it returns void. EVIDENCE: one dispatch, membership unchanged through delayed readback; no rejection status. |
| Window → Space, same display | Compat ID + `SLSSetWindowListWorkspace`; yabai **tries SA first**, then non-SA compat fallback [Y1]; Hammerspoon uses compat directly [H1] | FACT: set destination compat tag → set window workspace → reset tag to zero, without exact tag-preimage restoration or effect readback. These sources establish a workaround, not universal efficacy. The 7.1.25 recovery issue specifically identifies bridged operation; do not transfer that runtime claim to compat. Our setters remain disabled. |
| Window → Space, other display | Same three move helpers; yabai send path passes destination SID [Y4] | FACT: no separate cross-display authority API in this send path. UNKNOWN: membership, geometry/display adoption and focus side effects. Same-display success is insufficient evidence. Requires later distinct owned-display leg. |
| Window → Space, same/other display | SA client [Y15] → Dock payload `SLSMoveWindowsToManagedSpace` [Y12] | Excluded. Calling from Dock's connection is not evidence that an ordinary-process call is entitled to the same effect. |
| Window → Space, same/other display | Silica/Amethyst titlebar mouse-down + posted system Space-switch chord + delayed mouse-up [A1] | FACT: non-SA synthetic drag, not direct AX Space setter; visible and timing-sensitive. Silica **enables a disabled shortcut** before posting [A2]; that settings write must be omitted. Disabled/missing target chord blocks this route. Cross-display Amethyst additionally adjusts the frame [A3]. |
| Window → Space | `SLSProcessAssignToSpace` [Y6]; AX position/size, raise, app activation | Process assignment is process-wide, not a proven per-window move/inverse; do not substitute it. Plain AX frame/raise has no Space-assignment attribute in surveyed code. Mission Control window-thumbnail drag is a UI/manual candidate, not a surveyed low-level setter. |
| Switch/focus Space | `SLSShowSpaces`/`SLSHideSpaces` + `SLSManagedDisplaySetCurrentSpace` and Dock state update, **inside SA** [Y5]; yabai focus tries SA first [Y7] | Excluded direct SA route. These calls alone outside Dock are not a proven activation route. `space_manager_switch_space` is a different operation: same-display SA focus, cross-display window-set swap [Y7], not an ordinary desktop selection. |
| Switch/focus Space | Non-SA synthesized Dock-swipe; yabai fallback [Y7], rift directional implementation [R1] | CLAIM: yabai 7.1.19/20 SIP-on focus [Y3]. EVIDENCE: same-display pair switched both ways; no warp/click. Cross-display route expands scope. Rift has a separate ≥27 payload branch: source version coupling, not evidence of 27 acceptance. |
| Switch/focus Space | Hammerspoon Mission Control + AX `gotoSpace`/`AXPress`; System Events equivalent [H2] | FACT: public AX, no SA, visible UI; index must be freshly joined to target SID. EVIDENCE: exact same-display current-SID round trips through public-app toggle + Dock AX, without System Events. Visual animation/jank unobserved. |
| Switch/focus Space | Synthetic enabled Ctrl+Left/Right or Desktop-N chord; Silica resolves symbolic ID `118+N−1` [A2] | No SA. Arrow route is relative; digit route is index-bound. EVIDENCE: corroborated enabled Ctrl+Right candidate had no effect after an exact-helper pair. Desktop-N unresolved, not posted. Silica's enable-if-disabled behavior is not adopted. No physical delivery/collision evidence implied. |
| Create Space | Yabai `scripting_addition_create_space`: Dock `ManagedSpace` object + internal add function [Y5], [Y8], [Y11] | SA-only yabai command; excluded. No proven ordinary-process SkyLight equivalent in survey. |
| Create Space | Hammerspoon Mission Control `mc.spaces.add` AXPress [H2]; System Events or physical + button | FACT: non-SA visible UI route. EVIDENCE: one temporary user Space created by AX, exact ordered before/after difference, never a guessed desktop number. |
| Destroy Space | Yabai SA internal remove function [Y5], [Y8], [Y11]; Hammerspoon AX `AXRemoveDesktop` [H2]; physical thumbnail close button | Yabai command SA-only; AX alternative non-SA. EVIDENCE: uniquely created inactive, user-window-empty SID removed once and exact original ordered topology/current restored. System backing chrome exclusions were identity-checked. |
| Move Space between displays | Yabai SA internal move function + desktop-picture-manager bookkeeping [Y5], [Y9], [Y11] | SA-only in yabai; no surveyed non-SA implementation. Mission Control thumbnail drag is an unverified manual/UI candidate, not part of the initial probe. Keyboard/AX window focus does not move a Space. |
| Focus window on another Space | Yabai default SLPS front-process/make-key + AXRaise [Y10], plus optional `SLSSpaceSetFrontPSN` and gesture event handling [Y13]; send-path interaction [Y4] | Default window focus **not SA** (SA focus in disabled branch). CLAIM: animation suppression 7.1.21–23 [Y3]. Cross-Space transfer/animation differs from same-Space focus; issue #2788 separately warns about send-and-follow interaction. Require Space-current + AX focused WID/PID + CG owner + NSWorkspace agreement, not status 0. |
| Focus window on another Space | AXRaise + app activation, optionally followed by explicit UI/chord switch | No SA; OS/app settings can switch Spaces or bring other same-app windows forward. No general exact cross-Space result proved by surveyed source. Our latest SLPS fixture has incomplete full-agreement evidence [phase0]. |
| Window-to-Space membership | `SLSCopySpacesForWindows(cid,0x7,[wid])`, joined to owned CG PID/WID [H1], [R2]; private ABI [Y14] | No SA. EVIDENCE: works through Child-held helper [phase0]. Sticky windows may have multiple memberships; fullscreen/system transitions need type checks. Readback must be exact `[target]`, not merely contain target. |

### What other non-SA tools do

| Tool / source pin | Actual alternative (FACT unless marked CLAIM) |
| --- | --- |
| AeroSpace `74a1bf17e82d70e0a21945ba04bb4f590bf19f83` | AX corner parking for virtual workspaces, not native Space moves: [hide/restore](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacWindow.swift#L123-L183). |
| Amethyst `5cfe2fe12b1a933139010da5760aaa7e44033e9b`, Silica `ebead82a28486e7719b7ed69633ecfed8a4fe2d2` | Native Spaces via drag+resolved shortcut [A1], [A2], [A3]; Amethyst's direct `move(toSpace: CGSSpaceID)` is [empty](https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Model/Window.swift#L392-L397), unlike its indexed mover. |
| rift `8114ccc534b5c9a7e6a77ce27568d8739be9216f` | Queries native membership [R2], directional gesture selection [R1]; no native window-move binding found. Its virtual workspace machinery is not evidence of native mutation authority. |
| glide `ef3b30f88445fe4b517eb4c60023011f6d3725dd` | Per-native-Space integration and [SLPS focus](https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/window_server.rs#L215-L284); no native window-move implementation found. |
| paneru `8a213313538a352537b26e25131cfe75f1e00724` | Virtual strips/slivers and [SLPS focus](https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/manager/windows.rs#L906-L960); [SkyLight queries](https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/manager/skylight.rs#L144-L256), not native move. |
| FlashSpace `968015308d8d284a98831f90f365538a63456ee9` | App-assignment workspaces using [hide/show/activate](https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/Workspaces/WorkspaceManager.swift#L180-L230), not native per-window Spaces. |
| komorebi-for-mac `bdf10452f10ebf15ec1978cb05d0aac79ef5611f` | [Corner parking with saved geometry](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L506-L577). |
| Hammerspoon `23e387e2805a9890066366e0ac96c71b27f0cfd5` | Private membership/move dispatch [H1] plus animated AX Mission Control create/goto/remove [H2]; dispatch returns success without move-effect readback. |

The 14.5+/15 workaround is not a blanket private-API/SIP failure. Nor does
upstream Tahoe success certify the 15 floor. Recheck each supported release
family; keep floor and newer-family results separate. The 2026-10-15 decision
retains native Spaces and tier 2: an unresolved transfer route blocks the MVP;
it does not select parking or a SIP-policy change.

## Live verdict — 2026-10-16

Physical native graphical-session evidence on a release newer than the macOS 15
floor, SIP enabled, persistent signed bundle, observed Accessibility trust. One
temporary user Space was created, identified by exact ordered per-display SID
difference, used throughout the comparisons, and removed. All windows affected
by move/focus/input experiments were disposable helpers. This is neither
ordinary-app acceptance nor minimum-supported-release acceptance.

**Phase 0 Spaces verdict: selection and create/remove are viable; general window
transfer remains unresolved, so the workspace MVP is blocked.** The direct move
had observed no effect; bridge dispatch was unavailable; other move candidates
were ineligible or lacked exact restoration. This does **not** establish that
all SIP-enabled native window moves are impossible, or that SIP caused any result.

| Route | Observed result | Latency and rejection evidence |
| --- | --- | --- |
| MC AX create | Works: exactly one new user SID, preexisting order/current unchanged | 448 ms; AX action accepted and SID effect observed |
| MC AX goto / return | Works: exact current-SID readbacks both ways; owned membership retained | Approximately 628–660 ms per leg, including MC traversal and settled readback; no explicit rejection |
| MC AX remove | Works: created SID inactive, user-window-empty, then absent; original ordered layout/current restored | 515 ms; one remove dispatch, effect verified |
| Public AppKit helper self-join | Works **only as an independent recovery fixture**: owned window `[O] → [T] → [O]`, exact frame and collection-behavior restoration | 112–317 ms self-action reports, followed by membership readbacks; no general foreign-window authority implied |
| Direct `SLSMoveWindowsToManagedSpace` | **Observed no effect**: positively dispatched once; immediate and delayed membership stayed exactly `[O]`, current stayed O, frame unchanged | Mover child 180 ms; delayed observation 3,139 ms over a 3-second poll window. Void API, no rejection status; SIP cause UNKNOWN. No no-op inverse dispatched |
| Bridged asynchronous move | Unavailable: operation class present, dispatcher function absent; no dispatch | N/A; symbol availability, **not rejection** |
| Compat ID + workspace setter | Blocked: exact compat-tag preimage/readback/restore remains unproven; no setters called | N/A; safety prerequisite, **not rejection** |
| Silica drag + Desktop-N chord | Ineligible: outbound and return Desktop-N bindings unresolved; private resolver header absent from surveyed source | N/A; no mouse-down, no shortcut enabling, **not rejection** |
| Enabled Ctrl+Right switch candidate | **Observed no effect**: effective Carbon table corroborated the enabled chord; exact-helper down/up delivered, current stayed O; no inverse/retry | 164 ms **focus + posting only**, then bounded 3-second current poll; no authority error. Chord-to-action attribution remains unproved; not physical-input evidence |
| Same-display yabai Dock-swipe | Works: began/ended pairs switched O→T and T→O, memberships unchanged, no warp/click | 15/131 ms **focus + posting only**; exact current effects within the 3-second polling window. End-to-end switch latency was not captured |
| Desktop-N switch chord | Ineligible: no resolved enabled outbound/return binding; Carbon entries have no symbolic IDs | N/A; no post, **not rejection** |
| Cross-display window/Space moves; cross-Space ordinary-app focus | Not tested by this single-display fixture | TBD; no non-SA Space-to-display route established |

**Visual jank is unobserved for every route.** MC UI is necessarily exposed by
the AX route; neither posted-event timing nor current-SID readback proves whether
the desktop transition animated or looked smooth. Gesture payloads remain
version-coupled; the comparison refuses the newer augmented-payload family.

### Recovery, restoration and limitations

- Independent public helper recovery passed before direct SLS dispatch, on the
  same helper. The primary direct mover ran as a separate short-lived process
  and exited while the exact helper Child remained held. Off-current title
  withholding is handled by the documented Child/pipe-nonce/executable/CG join;
  separate controllers without that Child still require current nonce proof.
- The first integrated `mc-lifecycle` run incorrectly checked a Space switch
  immediately. A later snapshot showed the asynchronous effect. Corrected
  bounded **readback polling**, without redispatch, proved return; the corrected
  integrated create/goto/remove command has not itself had a fresh full pass.
- AX error decoding had omitted two SDK error constants, misnaming `NoValue`.
  Maintained-binding regression coverage now pins the numeric/name mapping.
- Removal initially parked on system backing windows. Fresh user-window
  emptiness now records narrowly verified compositor and zero-area, offscreen,
  all-user-Space accessibility chrome exclusions. A versioned WindowServer
  executable alias is canonicalized to the same system file; unreadable or
  different identity still blocks. Ordinary sticky windows and Ghostty block.
- The switch comparison did not naturally restore foreground focus. A single
  identity-verified public app activation restored the captured Ghostty target,
  with full selected-oracle agreement and unchanged frame/membership; no
  keystrokes, SLPS events, AXRaise or window setters targeted it. One offscreen,
  non-Space Ghostty chrome surface self-resized during setup. Ordinary document
  frames/memberships were unchanged; the chrome was not written back.
- Final B2 equals B0 for ordered per-display SIDs/current, display topology,
  separate-Spaces state and shortcut overrides. Temporary SID absent and with
  no members; helpers/probes/taps/display-sleep assertions released and reaped;
  Ghostty frontmost by the full focus oracle. No native preferences changed.

### User manual transfer check — 2026-10-16

User-run, physical input, tiler stopped, one display, one disposable TextEdit
document window and one temporary Desktop created and later removed through
Mission Control. All three native routes moved the window from Desktop 1 to
Desktop 2, with the view following:

| Route | Result |
| --- | --- |
| Hold title-bar drag, press enabled Ctrl+Right, release | Moved |
| Hold title-bar drag, press temporarily enabled Ctrl+2 (Switch to Desktop 2), release | Moved |
| Drag the window onto Desktop 2's Mission Control thumbnail | Moved |

Evidence is the user's visual observation plus a before readback
(`spaces-diagnose --wid`, membership `[1]` on a two-Space display); per-route
after readbacks were not captured. Final cleanup readback showed the original
single-Space topology. The user reported normal native behavior with no
unusual jank. This confirms the Amethyst-style drag route is available with SIP
enabled on the development host; the minimum-supported release, multi-display
and synthetic (agent-posted) drag remain unverified.

User decision 2026-10-16: build managed native workspaces on these routes
([decision](../../decisions.md#scope-and-platform-goals)).

The subsequent [synthetic drag diagnostic](../../changes/macos-managed-workspaces.md#diagnostic-milestone-2026-10-16)
verified independent helper recovery but refused before any synthetic post on a
positional safety check. Original single-Space topology and helper absence were
verified; frame/protected-stability diagnostics remain unresolved. This adds no
synthetic-transfer acceptance and changes neither the manual result nor the
approved route.

### Exact continuation

**Superseded 2026-10-16 by the user manual check above:** implement synthetic
title-bar drag plus native switch chord as the transfer route (relative via the
enabled Ctrl+arrow holders, numbered via Authentic-enabled Desktop-N chords with
owned preimages and Revert), with membership readback and a drag-back inverse.
The compat-tag and bridged-dispatcher investigations are no longer on the
critical path. Original continuation, retained for history:

Find and review a **safe ordinary-window transfer candidate with exact inverse**:
first investigate an authoritative compat-tag getter/preimage, or a release with
the bridged dispatcher actually present. The drag alternative still needs
resolved already-enabled Desktop-N bindings and independently recoverable input.
Do not repeat the direct no-effect call without a changed hypothesis. Native
per-display mapping, numbered/relative select and send/follow/stay, trailing
creation and background-frame actuation remain pending that transfer gate.

## Read-only Mission Control binding map

Read overrides with `defaults read com.apple.symbolichotkeys AppleSymbolicHotKeys`.
Standard parameters are character/keycode/CG modifier mask (Ctrl `0x40000`,
Shift `0x20000`, Option `0x80000`, Command `0x100000`). **Missing `value` or ID
does not mean disabled**: preferences are incomplete overrides, not the
effective hotkey table. Keep actual personal bindings outside repository records.

| Conventional symbolic IDs to inspect | Meaning / expected default chord | How to establish the actual binding |
| --- | --- | --- |
| 32 / 34 | Mission Control / slow variant; Ctrl+Up / Ctrl+Shift+Up | If absent, inspect effective system chords and read the System Settings label. This conventional attribution is a lookup hypothesis, not verified ID metadata from the surveyed clones. |
| 33 / 35 | Application windows / slow variant; Ctrl+Down / Ctrl+Shift+Down | Same; do not equate absence with inactivity. |
| 79 / 80 | Previous Space / slow variant; Ctrl+Left / Ctrl+Shift+Left | Enabled-without-value needs effective-chord corroboration. |
| 81 / 82 | Next Space / slow variant; Ctrl+Right / Ctrl+Shift+Right | Same. |
| 118–133 | Switch to Desktop 1–16 | Silica explicitly derives these IDs [A2]. Read actual key/modifiers, not assumed Ctrl+digit; reject disabled or unresolved targets. |
| 60 / 61 | Previous input source / next source; commonly Ctrl+Space / Ctrl+Option+Space | Decode explicit values; relevant neighbouring Ctrl collisions, not Space switching. |

Public Carbon `CopySymbolicHotKeys` gives effective **chord/enabled** dictionaries,
but no symbolic IDs. Hammerspoon [matches chords and strips the automatic Fn
bit](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/hotkey/libhotkey.m#L208-L241).
Its modifiers are **Carbon**, not CG: Ctrl `0x1000`, Shift `0x200`, Option
`0x800`, Command `0x100`; `0x20000` here is the automatic Fn bit. Array index
is not symbolic ID. A read-only public-table query can corroborate collision
chords, but cannot certify that a chord activates Desktop N.

Silica reads IDs through `CGSGetSymbolicHotKeyValue`/
`CGSIsSymbolicHotKeyEnabled`; its private-header submodule is unavailable in
the surveyed checkout. No guessed ABI resolver was added. Effective chord
observation and ID attribution must stay separate; unresolved Desktop-N
bindings block the digit fallback, rather than triggering an enable/write.
Settings reads are not delivery, interception, suppression or physical evidence.

## Minimal live sequence (initial readiness plan)

The verdict above supersedes unimplemented/physical-only setup statements in
this initial plan. Reviewed MC AX automation and public owned-helper self-join
now provide the measured setup and independent restore paths. Preserve its
identity, exact-readback and restoration requirements for subsequent experiments.

Follow [live-macos-testing.md][live]. Use one identified disposable window and
one newly created **user** Space on the same display; do not touch Ghostty or
any other controlling terminal. No display rearrangement, fullscreen Spaces,
SA, shortcut changes, or unrelated windows. UI actions below may initially be
user-performed physical Mission Control actions; the equivalent AX implementation
is not present in the probe. A plan must not be reported as an existing recovery
command. Keep run-specific identities/preimages in the ephemeral test record.

1. **Baseline/preflight.** Rebuild/sign/verify the stable bundle; record source,
   executable hashes, bundle/requirement and arguments per the guide. Require
   unlocked ordinary session, LaunchServices `ax_trusted:true`, Secure Input
   inactive and posting consent for any synthetic leg. Record SIP read-only.
   Establish the disposable helper through
   `mise exec -- just launch -- probe-spawn --seconds 120` and its fresh identity
   joins; the user physically gives it focus. Finish one route pair and restore
   before its finite lifetime expires; insufficient remaining time blocks a
   new action. Record pre-fixture focus separately
   for user-owned physical restoration, never an agent write to the terminal.
   Run `mise exec -- just preflight-app` and
   `mise exec -- just launch -- spaces-diagnose`. Snapshot B0: ordered user-SID
   list and current SID **per display**, separate-Spaces mode, topology,
   frontmost/AX focus and relevant window identity/geometry/visibility. Preserve
   display associations; flattened sorted IDs cannot prove ordered restoration.
   Capture hotkey overrides plus effective chords. Never change settings to fit
   the fixture. The user puts focus on the owned fixture before any synthetic
   leg; all synthetic down/up pairs target only that verified fixture.
2. **Create exactly one temporary Space.** Open Mission Control physically;
   on the chosen display press its + once (future AX: `mc.spaces.add` AXPress).
   Close Mission Control. Read SLS again: B1 must differ from B0 by exactly one
   new user SID T on that display, preserving all previous order/memberships
   and current SID O. Stop on any other difference. Never identify T solely
   as “Desktop 2”, a thumbnail coordinate, or the largest SID.
3. **Independent recovery first.** Revalidate the existing owned-helper
   nonce/CG/AX join flow; capture fresh PID/start/executable/nonce, WID,
   membership `[O]`, frame, visibility and owned focus. Prove graceful stop and
   exact-identity stop. The user can physically drag **only the owned window**
   thumbnail O→T→O in Mission Control, with membership and frame readbacks at
   each boundary. Repeat return through an independent control path after the
   primary mover process exits, keeping the fixture owner alive. This manual
   restore is the initial independent fallback; do not claim the untested SLS
   inverse proves recovery. If this proof is unavailable, no API move follows.
   Space removal is not a substitute for verified window restoration.
4. **Move/back route order.** Fresh symbols/type/session/identity checks first:
   (a) bridged operation only if available **and** a reviewed bounded dispatch,
   asynchronous readback and independent recovery implementation exists;
   otherwise record unavailable/unimplemented, no dispatch. (b) One direct-SLS
   diagnostic round trip, despite upstream's ≥15 bypass, after step 3:

   ```sh
   mise exec -- just launch -- spaces-move --pid "$PID" --nonce "$NONCE" --wid "$WID" --space "$T" --allow-untested
   mise exec -- just launch -- spaces-diagnose --pid "$PID" --nonce "$NONCE" --wid "$WID"
   # Only if fresh identity/membership permit restoration:
   mise exec -- just launch -- spaces-restore --pid "$PID" --nonce "$NONCE" --wid "$WID" --space "$O" --allow-untested
   ```

   Inspect JSON, not wrapper exit alone. At each boundary poll read-only, at
   most 3 seconds, for exact `[T]`/`[O]`, current SID, CG/AX identity and frame;
   record dispatch separately. Existing command reads immediately, so preserve
   its report and collect delayed diagnosis separately. On no effect `[O]`,
   do not run a no-op inverse or retry. On ambiguity use the independent path,
   verify restoration, then stop. (c) **Compat remains blocked** until exact
   compat-tag preimage/readback/independent restoration exists; never reset an
   unknown tag to zero. (d) Silica-style drag+digit is a later eligible comparison
   only when both destination and return chords are already enabled, have SID
   mappings and a reviewed owned-window input guard; otherwise skip. No blind
   mouse-down or shortcut enabling. Cross-display moves need a later distinct
   recovery leg; one temporary same-display Space does not cover them.
5. **Switch/back separately.** First establish physical Mission Control O→T→O
   current-SID readbacks as independent recovery. Compare only implemented and
   scoped routes, returning to O after each: AX Mission Control goto (fresh
   SID→display/index join); then enabled Desktop-N down/up chords; then relative
   Ctrl+arrow only if O and T are adjacent and the chord is enabled; finally a
   reviewed same-display Dock-swipe route. Stop after one failed/ambiguous pair.
   No cross-display cursor warp/click. Require current `[T]` then `[O]` within
   3 seconds; owned window membership must remain unchanged, and owned focus
   transfer/return must agree across NSWorkspace, AX and CG. No tap needed to
   test a posted chord. Carbon/tap collision legs are separate, not implied by
   this switch result. Current `spaces-switch` only reports unsupported.
6. **Remove only T and compare.** Restore `[O]`, exact geometry/visibility and
   baseline focus first; confirm T inactive, still the uniquely created SID,
   and no unrelated windows assigned there. Re-open Mission Control, freshly
   map T to the display's thumbnail; use its close control (future AX:
   `AXRemoveDesktop`). Never remove active/last Space or guess a stale index.
   Stop/reap only the verified helper; check process/record/CG residue. Snapshot
   B2 must equal B0 in ordered per-display SIDs/current, topology, separate-Spaces
   mode and shortcut overrides. Compare the owned-window preimage before its
   exit; after exit require owned-window absence. User restores any original
   protected-terminal focus physically. Uncertain restoration blocks cleanup
   expansion: preserve residue/evidence and report exact remaining state.

### Implementation readiness and verdict oracle

`spaces-session` remains read/lifecycle-only even with `--allow-untested`.
`spaces-move`/`spaces-restore` require nonce/record and fresh CG ownership,
one-Space preimage, known target, **user source/target types**, readable
non-loginwindow frontmost context, **unambiguous same-display mapping** and
explicit opt-in. Cross-display move/restore are refused pending their distinct
recovery leg. Session observation is
not an atomic lock detector; recheck at each action. No new write route is
enabled by this preparation. Bridged dispatch, AX MC and input writers still
need bounded recovery implementation before automation; the physical setup and
independent restore must be demonstrated first. Do not launch the pending focus
fixture with a protected terminal as a writable restore target.

An omitted/disabled symbol, consent shortfall, refused topology, unsupported
CLI, or no dispatch is **not SIP rejection**. A dispatched void API with stable
unchanged membership is **observed no effect**, not a causal SIP finding.
Preserve exact OS errors if a route returns them. “SA unavailable” can describe
missing excluded infrastructure; “SIP caused rejection” needs an explicit
authority diagnostic identifying that cause, not a generic failure. Without
such evidence, causal attribution remains inferred/UNKNOWN; no reduced-SIP
comparison is run. Accept native move/select only after observed effects and
independent exact restoration, separately for the floor and newer families.

[phase0]: ../../changes/macos-phase-0.md
[pins]: prior-art.md#source-checkout-snapshot-2026-10-14
[live]: ../../live-macos-testing.md
[move-issue]: https://github.com/asmvik/yabai/issues/2788
[Y1]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
[Y2]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/workspace.m#L17-L26
[Y3]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L12-L43
[Y4]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L2092-L2122
[Y5]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/osax/payload.m#L460-L593
[Y6]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1074-L1077
[Y7]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035
[Y8]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1037-L1072
[Y9]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L905-L923
[Y10]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1269-L1335
[Y11]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L442-L482
[Y12]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/osax/payload.m#L906-L930
[Y13]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L359-L375
[Y14]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h
[Y15]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L604-L621
[Y16]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/macho_dlsym.h#L55-L80
[Y17]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L994-L1080
[Y18]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/yabai.c#L148-L149
[Y19]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space.c#L17-L70
[Y20]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L272-L281
[Y21]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1037-L1072
[Y22]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L2093-L2123
[Y23]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L55
[AS1]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacWindow.swift#L123-L183
[AS2]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/axDumps/scenario_firefox_google_meet_share_window/05_apple_controlcenter.json5
[FS1]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/Workspaces/WorkspaceManager.swift#L180-L230
[P1]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/manager/skylight.rs#L144-L256
[P2]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/manager/windows.rs#L906-L960
[G1]: https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/window_server.rs#L215-L284
[H1]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L116-L199
[H2]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L689-L882
[H3]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L16-L21
[H4]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L100-L130
[A1]: https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SIWindow.m#L210-L262
[A2]: https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SISystemWideElement.m#L29-L65
[A3]: https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Managers/WindowManager.swift#L949-L974
[A4]: https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Categories/NSRunningApplication%2BManageable.swift#L18-L36
[R1]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/space_switch.rs#L170-L223
[R2]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/window_server.rs#L426-L476
[R3]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/window_server.rs#L426-L514

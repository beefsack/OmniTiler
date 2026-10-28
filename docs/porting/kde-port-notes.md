# KDE port notes (world contract v2)

How the KDE adapter moves to the strict v2 contract in
[world-adapter-guide](world-adapter-guide.md). Status below is
implemented vs approved-not-implemented vs open. The v1 inventory below
describes the delivered baseline, not an accepted v2 KDE runtime.
The staged implementation and parked boundaries are tracked in
[KDE world-v2 port](../changes/kde-world-v2-port.md). Stages 0/1/1b/2/3 are
complete offline, including command wake, source-view rewind settlement,
raw attempts and native presentation ingress. Stage 4 is partial: required raw
candidate capture and Rust fixed-size wire classification are delivered, with
role/local-fixed/gesture policy still outstanding. N6's neutral tiling toggle
and opt-in world-owned birth-clear are accepted offline, including simultaneous
births and native deleted/unmanaged lifecycle normalization. Existing-core shortcuts now
submit commands only; the native float-navigation policy is deleted. Stage 5
renderer derivation removal and causal freshness are accepted offline. Native acceptance remains
outstanding; no logical receipt or offline test is native arrival proof.

KWin source is read from the local sibling clone at the documented
`v6.7.4` pin; port work must not web-fetch KWin sources. Mechanical
API checks use `git show <pin>:<path>` and `git grep <pattern>
<pin>` inside that clone.

## Recommended topology

- The external Rust service (`crates/omnitiler/src/`, planner) extends
  the existing Planner and owns exactly one `WorldEngine`. The plain
  KWin JS observes, captures and actuates through the async
  session-bus `callDBus`; the native C++ effect
  (`kwin/native-effect/`) is a pure renderer of the serialized core
  presentation. It retains native anchoring plus immediate hide-only
  vetoes.
- A future pager widget/plasmoid consumes cached presentation plus
  notifications with its own client ids; it never invents authority.
- The service side is one logical adapter/broker, not transparent
  multi-client RPC: sole report identity and sequencer, one complete
  stream, one request/observation namespace and one monotonic clock.
  Fold other clients' cursor, gesture and presentation facts into the
  coherent report, preserving invalidation, lifetime and one gesture
  identity. Independent counters collide; an incarnation reset clears
  state.
- Core command dedup is one high-watermark; retained operations use
  `op-<command_seq>`. The broker maps its client id to globally
  ordered commands. Native work is actuator only: route transient
  terminal results to the requester (issuing-reply-only results; a
  cache is insufficient) and never replay one-shot effects to
  subscribers.
- Separate presentation/query/command authorization from the exclusive
  report/actuator authority. A reconnect gets the current accepted
  presentation.
- Planner endpoints (`crates/omnitiler/src/planner_service.rs`,
  `crates/omnitiler/src/world_service.rs`) now implement the paired strict v2
  broker below. The stale alignment option and v1 discovery route are removed;
  there is no compatibility shim.

### Stage 2 boundary (complete offline)

- The paired implementation removes the stale alignment option and v1 discovery,
  and introduces sole-broker capture, sender-bound queue/take and presentation
  query routes. Capture binds generation, producer lifetime, source nonce and
  native invalidation watermark; core ordering and the monotonic clock remain
  service-owned. Queue and take also bind generation.
- Capture replies carry native work but no terminal command result; take serves
  only the authenticated issuing sender. Presentation queries contain neither
  one-shot work nor write-operation tokens. Lost queue/take replies retain their
  identity rather than minting duplicate commands.
- KWin JS cannot hear `WorldWake` through `callDBus`; `WaitWorldWake` now holds
  one asynchronous call completed by queued work. A broker cursor advances on
  enqueue/drain; only pending work wakes, so bursts drain and rejected captures
  cannot hot-loop. A 20 s server lease and verified 25 s host-limit watchdog
  recover silent errors/restarts without native events or idle captures. Owner
  auth, generation/producer/nonce fences and bounded dead-waiter cleanup apply.
  See [B1 evidence](../changes/kde-world-v2-port.md#b1-external-command-wake-delivery-offline-complete).
- Actual v2 reply fixtures now come from the Rust `world_wire_fixtures` example;
  retired `logical` workspace fields refuse. The entry-only TS engine harness
  does not prove broker authentication/replay; Rust flow/private-bus tests do.
- Logical targets resolve only through accepted output-specific native bindings.
  Unbound migration/creation targets hold, never native-id equality or ordinal
  guesses. Explicit `nativeCreations` now authorizes exactly-once native calls;
  `creationAttempts` returns same-turn causal ids or uncorrelated evidence, and
  only core binds. If immediate post-read fails, preserve incomplete execution
  evidence until a successful full read, then report `created:null` and the
  original write stamp. Never fabricate an empty after-order.
- Raw frame/member/view attempts now report dispatched setters with original
  broker-clock stamps, truthful immediate frame/prewrite reads and null DPI in
  KWin-logical coordinates. Host-owned logical member/view pre/post/base stay
  null; fresh native reads use `nativePre/nativePost/nativeBase`. Core alone
  resolves their identities. Bound valid acknowledgements
  retire only delivered matching rows; transport loss retains them, generation
  reset clears them. Stage 3 also actuates nonactivating maximize clear through
  `setMaximize(false, false)` and reports actual setter/pre/post evidence. A
  veto or already-clear observation never fabricates a dispatch attempt.
- Migration preserves core's stable logical id, binds fresh backing only through
  core's correlation, and never steals an already-bound empty desktop. Native-bound
  empties stay; ordinary trailing spares stay unbound until explicitly needed.
  Core desires under actuatable scope supply members (including zero), never
  requester-only terminal results. Untouched floating scope is actuatable only
  with an unheld output and no gesture, as in the Windows v2 reference.
- The five old oracles and restored both-view rewind pass. Released source
  backing adopts fresh at native order, even if never selected. The migration
  transfer remains in the first proven-rewind reply; the next observation
  resolves fresh adoption, retires the abandoned refill and settles with the
  destination identity/members retained and both user switches winning. No view
  is reapplied after the rewind. Only paired core refill desires may actuate;
  the legacy first-remaining-row fallback is deleted. See
  [B2](../changes/kde-world-v2-port.md#b2-source-view-rewind-during-migration).
  Omission is not settlement proof. Offline acceptance is not live acceptance.

## Delivered-baseline inventory (port or delete)

JS adapter (`kwin/src/`):

- Strict v2 wire and broker cutover are delivered offline. Former v1
  report/command shapes and fixtures have been replaced, not shimmed.
  Internal observation carriers still need the later policy-deletion audit.
- `world-observe.ts`: keep the capture skeleton (frame reads,
  inventory, closes, completeness) and re-target it to v2 facts,
  observed logical receipts and presentation ingress. Geometry stays
  in the coherent KWin adapter space (KWin logical pixels).
- `world-apply.ts`, `world-native.ts`: port to
  v2 desires and operation tokens (`frame_operation`,
  `clear_operation`, `op-<cmdSeq>` transfers). One-shot effects are
  spent before dispatch, never replayed.
- `world-entry.ts` (sequencer, invalidation, dispatch, busy fences):
  collapse into the single broker behind the one stream; delete the
  legacy plan route remnants and the v1 `DiscoverWorld`/`DescribeWorld`
  assumptions in step with `world_service.rs`.
- `world-drag.ts`, `world-command-context.ts`, `world-state.ts`,
  `world-shortcuts.ts`, `world-settings-live.ts`: delete adapter-side
  policy copies (admission, gesture resolution, refill, tiling toggle
  decisions). Shortcuts submit planner-resolved world commands only.
- Stage 4 deleted `born-maximized.ts`, `fixed-size-predicate.ts`,
  `migration-source-refill.ts` and `same-axis-move.ts`; configuration carriage
  moved into `world-settings-live.ts`, not policy. Unused trailing/displacement
  helpers in `workspace-native.ts` and the refill selector were deleted.
  `scope-class.ts` remains the fixture-only safety filter; local-fixed and
  gesture policy deletion remains incomplete. N6 deletes
  `world-born-maximized.ts` and its lifetime sets; core owns birth-clear under
  the neutral `birthClear` capability. Native maximize/fullscreen/sticky toggles
  remain unchanged.
  `world-float-nav.ts`
  is deleted: directional shortcuts submit core commands, workspace selections
  carry no implicit-output preselection, and keyboard sends carry the typed
  home-then-cached-live source. Shared owners include
  `workspace_scope.rs`, `window_policy.rs`, `output_selection.rs`, `admission.rs`.
  `tray-publisher.ts` keeps native placement/anchoring/styling and
  click-to-command; pager content comes from core.
- `provenance-entry.ts`, `native-id.ts`,
  `native-alias.ts`: keep only native identity/binding/recovery claims;
  logical homing moves to core receipts.

Native effect (`kwin/native-effect/`, `activewindowborder.*`,
`activeborderlogic.*`, `presentation_consumer.h`, `gesturefacts.*`):
the offline-accepted Stage 5 renderer consumes serialized core bounds and removes
group-highlight/visual-policy FFI derivation and legacy result forwarding.
Native styling (colors, anchoring, artwork) stays native. Every native
invalidation advance publishes its source counter and refreshes every surface;
only exact covering paint can reappear. Producing gesture identity isolates
queued previews across rapid serial changes, and focus/error hides preserve
ordering evidence. See the [accepted offline outcome](../changes/kde-world-v2-port.md#stage-5-renderer-causal-freshness-accepted-offline-2026-10-27).
The gesture cutover replaces the drag oracle with effect-owned identity and
delivered-Esc capture; terminal recovery remains unaccepted. The unused Rust
oracle FFI and its build/unsafe-inventory roots are removed. Pager/plasmoid work
is future and separate.

Stage 5 wire vocabulary is implemented but field naming remains tentative:
effect facts `{instance,invalidation}` become raw
`nativeInvalidWitness:{instance,counter}|null`, stripped before core decoding.
The read-only projection carries `coveredInstance`/`coveredInvalid`; the JS
subscriber and native endpoint both require exact live coverage. Preview
`producerInstance`/`producerSerial` must match the live native gesture
`{instance,serial}`. Unknown coverage and unproven preview producers cannot
draw. Endpoint loss only hides; explicit stream replacement alone resets paint
order. The recorded route executes the real handlers, facts endpoint, capture,
broker, subscriber and native endpoint; KWin signals and painting still require
physical checks.

Focused-fullscreen suspension is implemented offline as a neutral core opt-in.
The existing raw `fullScreen` capture becomes `WindowFact.fullscreen`; JS keeps
`paused=false` and derives no game or suspension policy. Core publishes optional
`suspendedOutput` and output-local hidden shapes. The broker omits preview paint
only when its resolution `domainOut` matches that core verdict, leaving take
results and gesture settlement intact. Native `windowFullScreenChanged` now
advances/publishes the same causal witness and refreshes all surfaces on both
entry and exit. Existing native fullscreen hide-only liveness vetoes remain.
Windows retains its adapter suspension; macOS native-fullscreen consumption is
coordination only. See the [contract and outcome](../changes/kde-world-v2-port.md#focused-fullscreen-presentation-suspension-2026-10-27).

## Host styling on KDE

- Two independent host styling colors, never ring-coupled (see the
  [generic rule](world-adapter-guide.md#8-border-underlay-drop-preview-and-pager)):
  the active-window ring keeps its existing border color/theme tokens
  (default: live system accent), and the pager focused-window
  highlight gets independent highlight color/theme tokens (default:
  live system accent), each explicitly overridable without touching
  the other. The focused miniature paints a full-strength accent
  border over a dimmer accent fill of the same color; unfocused
  miniatures are unchanged. The pager active workspace cell keeps the
  system accent with no theme switch. KDE has no pager renderer yet: its
  independent highlight settings and artwork are deferred with that surface,
  not implemented or coupled to the border by this port.

Tests: `kwin/tests/world-*.test.ts` and `test-fixtures/world-wire/` now
exercise strict v2. Canonical reply fixtures come from the Rust service;
request fixtures pin the JS producer, including populated raw attempts.

## KWin API facts the port relies on

- KWin `v6.7.4:src/window.h:281-289` exposes `managed` as `isClient`:
  KWin controls placement/other aspects, unlike override-redirect windows
  entirely handled by the application; `deleted` as `isDeleted` means already
  deleted, retained only for compositor animation. Both are constant Qt
  properties exposed through scripting's Window objects. Normalize deleted to
  explicit close and unmanaged to absent at the adapter boundary, never a
  candidate `WindowFact`; retain raw booleans for Rust normalization. This is
  host lifecycle, not N5 membership-policy classification. Scripting
  `windowList()` returns `workspace()->windows()` (`workspace_wrapper.cpp:505-509`),
  not the separate deleted collection (`workspace.h:205-213,354`); normal live
  closes use the existing event-gated close lane, not fabricated tombstones.
- Gesture source proof at `v6.7.4`: scripting's finish has no cancellation
  argument (`src/window.h:1512-1514`), and the effect merely re-emits it
  (`src/effect/effectwindow.cpp:83-91`). An effect input spy observes Esc before
  input filters (`src/keyboard_input.cpp:328-329`), but an earlier filter may
  consume that key; observing the key is not by itself causal cancellation.
  Xwayland's native cancel executes in RootInfo's earlier X11 filter
  (`src/events.cpp:614-621`, `src/netinfo.cpp:131-137`, `src/main.cpp:340-460`).
  A later effect filter plus deferred same-window matching cannot prove which
  terminal it cancels: multiple X events drain in one activation
  (`src/xwayland/xwayland.cpp:382-405`). A single native lifecycle/identity is
  tentative; the complete source is [parked](../changes/kde-world-v2-port.md#single-source-gesture-investigation-2026-10-26).
  Never infer Esc from geometry or claim unsupported completion as release.
- `workspace.cursorPos` (declared in `kwin/src/kwin-globals.d.ts`,
  backed by `src/scripting/workspace_wrapper.h` at the pin) is the
  cursor sample, and the script side can also call the script-visible
  `workspace.windowAt(pos, count)` hit test
  (`src/scripting/workspace_wrapper.h:483`,
  `src/scripting/workspace_wrapper.cpp:369` at the pin) without effect
  involvement or polling. The local `.d.ts` declares `cursorPos`
  and `windowAt`; never treat the declaration file as the API ceiling.
  Neither the JS nor the effect side has a warp
  API: cursor-warp is unsupported on KDE. Never claim an applied
  `cursor_warp`.
  Stage 6 decodes `windowAt`'s `QList<Window *>` with the existing bounded
  list decoder (default count one); only the normalized head is advisory.
  `cursorPosChanged` exists (`workspace_wrapper.h:276`, `.cpp:61`), but current
  KDE cursor-driven options are disabled: fresh sampling on relevant
  event/command/gesture captures is sufficient. Do not introduce polling or
  full captures on pure pointer motion, including while games run.
- Current frame reads use `frameGeometry` (`world-observe.ts::readRect`);
  writes use the guarded `frameGeometry` setter (`world-native.ts`).
  `readClientArea` calls `workspace.clientArea(MaximizeArea, output, desktop)`
  for the work area, not a window's client rectangle. Stage 3 captures
  read-only window `clientGeometry` for client-size hint conversion
  (`src/window.h` Q_PROPERTY at the pin).
- The declaration header cites 6.7.5 while these source checks pin 6.7.4.
  Comparing `src/window.h` and the workspace wrapper header/implementation
  between those tags confirms the relied-on scripting APIs are unchanged;
  this is not a reason to change the documented pin.
- Desktop select/transfer uses the declared wrappers
  (`currentDesktopForScreen`, `setCurrentDesktopForScreen`,
  `sendClientToScreen`, `createDesktop`/`removeDesktop`,
  `currentDesktopChanged`, `desktopsChanged`) through the
  `workspace-native.ts` per-output capability seam. KWin 6.7+ only
  APIs stay behind that seam, never called directly.
- Creation correlation at `v6.7.4`: `WorkspaceWrapper::createDesktop` is void but
  calls `VirtualDesktopManager::createVirtualDesktop` inline
  (`src/scripting/workspace_wrapper.cpp:295-298`). The manager inserts one fresh
  UUID desktop into `m_desktops` before returning
  (`src/virtualdesktops.cpp:448-482`); `workspace.desktops` reads that list directly
  (`workspace_wrapper.cpp:69-72`, `virtualdesktops.h:312-315`). Read full ids
  immediately before/after the call in the same JS turn; a unique fresh delta
  supplies `creationAttempts[].created`. Never use insertion position as identity.
  Creation silently does nothing at the desktop cap (25); a void return is not
  success proof. Signals are synchronous (`workspace_wrapper.cpp:42-47`): spend
  the operation before calling, tolerate reentrant invalidation, and report no
  correlated id if the actual delta is not a singleton. All outputs project the
  same global desktop list; only current cursors differ per output.
- `onAllDesktops` is an empty per-window desktop list (`src/window.h:2120-2123`),
  not current-desktop membership. Report `nativeWs: null` plus positive sticky
  proof, never substitute the current desktop. Core unknown-sticky home/slot
  retention remains a parked concern; this truthful encoding is not proof of
  complete sticky adoption.
- Geometry is one coherent KWin-logical space. Document and convert
  fractional KWin rounding (`Math.round`/`Math.trunc` in
  `world-observe.ts`, `world-drag.ts`) and keep it inside the adapter.
  Style fields ride the same coherent adapter units as fact frames
  (core presentation math is unit-agnostic); see the corrected
  `PresentationIngress` docs in
  `crates/tiler-core/src/world_report.rs`.
- Stage 5 render conversion: `world-observe.ts::toInt` and Rust raw
  `kde_raw.rs::math_round` agree on halves toward positive infinity
  (`-10.5 -> -10`); published integer rectangles pass through unchanged.
  Style values are converted once into the same logical units at ingress.
  `presentation_consumer.h::borderStrokeFromPublished` accepts only equal
  positive insets between core inner/outer. Pinned KWin `v6.7.4`
  `src/scene/borderoutline.h` inflates each side by that thickness and
  `outlinedborderitem.cpp` sets geometry from that inflation, reproducing
  core outer independently of later config-width changes. Underlay/preview
  use core outer/rect directly; `mapFromScene` only translates into the
  native item anchor. No additional DPI factor, outset or Windows frame
  compensation is applied. Freshness has separate accepted recorded-route
  evidence; independently observed native draw bounds remain outstanding.
- KWin size hints are client sizes: project decorated frames with
  `clientGeometry`, never with Windows invisible-border compensation.
- Fresh native readbacks (visibility, frame, foreground joined to the
  managed lifetime) prove arrival; setter returns and signal delivery
  alone do not. Async configure is a source-derived KDE/Wayland risk:
  first reproduce it natively and check whether KWin exposes pending/completion
  evidence before proposing a neutral settling representation. No new core
  signal is selected (see deferred work in
  [decisions](../decisions.md#port-deferred-investigations-2026-10-24)).

## Workspace modes on KDE (user-approved 2026-10-24)

- Shared (`shared`) is linked views with the user label "Workspaces
  span displays": all displays show and switch the same workspace.
  This is COSMIC `WorkspaceMode::Global`.
- `per-output-local` and `global-unique` are separate views, corresponding to
  COSMIC `WorkspaceMode::OutputBound`. The tentative 2026-10-28 vocabulary
  refinement labels them "Displays have separate workspaces - own set per display"
  and "Displays have separate workspaces - swap across displays", respectively.
- `equal_workspace_count` is a host limitation ("equal workspace
  count"), never called shared: same lengths but independent current
  views. On KDE the native global list means `equal_workspace_count`
  is true; expect empty padded workspaces (for example output 2 with
  ws4 occupied while 1-3 stay empty). Do not overspecify lifecycle
  beyond the existing rules.
- KDE default stays `per-output-local`. Both separate-view modes are
  fully supported only with the KWin per-output desktop setting
  enabled; warn when either per-monitor scope runs with that setting
  disabled, through actionable settings if feasible. `shared` needs no
  warning. The warning is required port work; direct settings action is
  conditional on feasibility.
- These workspace decisions are user-approved in
  [decisions](../decisions.md#workspace-vocabulary-2026-10-24) and
  supersede the stale tentative 2026-10-19 global-unique fallback. Core
  linked-vs-separate modes are valid on every platform; no new setting
  is added.

Stage 7 is delivered offline (2026-10-28): all three tokens, exact labels,
read-only separate-mode warning and a native **Open Virtual Desktops** action.
The fixed `kcm_kwin_virtualdesktops` page opens without automatically writing
`kwinrc [Windows] PerOutputVirtualDesktops`; the user owns any change. Native
all-mode and actual page-opening checks remain TBD; see the
[outcome](../changes/kde-world-v2-port.md#stage-7-workspace-modes-outcome-2026-10-28).

## Approved core changes this port must deliver

- Host-owned native adoption (user-approved 2026-10-24, implemented
  in core; native acceptance TBD): a fresh core generation adopts native
  desktops/Spaces in native order for each display with native window
  membership preserved and no workspace moves at launch; tiling still
  follows the existing startup arrangement inference
  (size/position). Subsequent outside moves are adopted as new
  membership; an outside-created native desktop becomes a workspace.
  The KDE port delivers and accepts against native facts; macOS
  reuses the same core change. `native[]` now bootstraps rings and publishes
  truthful per-output `native_binding`; the port must supply
  `WindowFact.native_workspace` (`nativeWs`) for native membership.
- Four occupied native desktops on a fresh core mean four logical
  workspaces with the same members and no workspace-move
  discriminator; the native result stays TBD until KDE-tested.
- `observed_workspace` remains a logical receipt, never a native-id alias.
  Native lists are complete only when both screen `complete` and report
  `inventory_complete` are true. Missing lists never prove deletion; duplicate
  output rows/within-output ids refuse, while global ids may repeat on outputs.
- Tentative count clarification: four native-bound occupied workspaces plus
  the existing ordinary trailing spare, not an exactly-four-total exception.
  [Decision and remaining neutrality concerns](../changes/kde-world-v2-port.md#decisions-and-core-neutrality-concerns)
  cover incompatible linked native ordering and empty-creation correlation.

## Build, check and gate commands

- Linux shell: `devenv shell` (system deps come from `devenv.nix`
  only). KWin script lifecycle and fixture-first rules:
  [live-kwin-testing](../live-kwin-testing.md).
- Planner/service: `cargo build --locked -p omnitiler`,
  `cargo test --locked -p omnitiler`, strict clippy and `cargo fmt
  --check` through the devenv shell.
- KWin script, from `kwin/`: `npm run typecheck` for the static
  declaration/bundle check, `npm run build` for the script bundle,
  and `npm test` for the full `kwin/tests/world-*.test.ts` suite
  (bundles, then runs the compiled `dist/tests` cases). Targeted v2
  legs include `kwin/tests/world-wire.test.ts`,
  `kwin/tests/world-apply-*.test.ts`,
  `kwin/tests/world-observe-caps.test.ts` and
  `kwin/tests/world-entry*.test.ts`.
- `just dev-on [--scope-class NAME]` / `just dev-off`
  (`just/linux.just`) stage and remove the dev-mode script; they are
  live session commands under the live-testing guide, not offline
  checks.
- Core-only gates the port must keep green (no native host needed):
  `cargo run --locked -p tiler-protocol --example world_acceptance --
  test-fixtures/world-actions.json` from the devenv shell, plus locked
  core/protocol builds, tests and strict all-target clippy.

## Material rough edges (no behavior change selected)

- The v2 world wire carries projected `WindowFact`s, not raw `SafetyFacts` or
  rich native classification/rule facts. The sole KDE capture wrapper now
  requires KWin-shaped raw candidate rows; Rust `kde_raw.rs` calls the existing
  shared fixed predicate and emits strict projected facts. There is no old
  projected-capture fallback. Native role/control normalization, richer app/rule
  capture remain unresolved in
  [Stage 4](../changes/kde-world-v2-port.md#stage-4-partial-outcome-and-deletion-boundary).
  Neutral `toggle-workspace-tiling` now resolves mode in core for shortcut and
  tray callers; explicit set remains available. The current
  [N6 outcome](../changes/kde-world-v2-port.md#n6-core-intents-outcome-and-blocker-2026-10-26)
  records passing toggle and automatic birth-clear evidence; remaining local-
  fixed focus/keep-above consumers stay unchanged. KDE sends `birthClear:true`;
  missing capability defaults false, preserving native birth ownership on
  Windows/macOS. No new controllability/liveness facts are selected in core.
  The gesture lifecycle API already exists and is next. No shared ingress
  redesign or broker logical-policy owner is introduced.

- B1 command wake and B2 released-source rewind pass offline.
  No passing transport or logical receipt is native settlement evidence.
- Fractional KWin rounding and native style conversion are adapter-local;
  [Stage 3 evidence](../changes/kde-world-v2-port.md#stage-3-outcome-2026-10-26)
  records the exact frame/client and style formulas and remaining bound gap.
- No JS/effect warp API exists; cursor-warp stays unsupported on KDE
  even when the core setting is on.
- The local `kwin-globals.d.ts` declares the pinned `windowAt` hit test;
  consumption remains Stage 6 work, not a claimed applied cursor route.
- The renderer cutover deletes native geometry/presentation policy and passes
  the complete recorded causal-freshness route offline. This does not establish
  physical signal delivery or painting; those checks remain outstanding.

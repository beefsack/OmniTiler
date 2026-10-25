# macOS world-contract-v2 port

## Goal and status

- Stage 0 reviewed; stage 1 complete offline; stage 2 and independent offline
  stage-3 work are partial, with app-hidden option B selected. The approved D2
  core correction, separate macOS automatic-float echo removal, born-state policy
  deletion, per-app native lifetime/flat-close path and tentative shared transient
  classifier integration are verified offline.
  Stage 0 evidence is from `a0f54105`
  (2026-10-24). The source-only fit review below is historical where superseded
  by the reviewed outcomes. No live tests were performed for this record.
- Replace the retired macOS contract, not wrap it. One `WorldEngine` owns policy;
  macOS captures native facts, actuates authorized desires and renders replies.
- Contract: [adapter guide](../porting/world-adapter-guide.md),
  [macOS notes](../porting/macos-port-notes.md),
  [decisions](../decisions.md#native-adoption-2026-10-24).
- Non-goals: compatibility/shims/dual routes, new workspace policy, a second
  native-adoption implementation, speculative asynchronous-settlement redesign,
  unparked passive titlebar drag, or resolution of parked product questions.

## Confirmed starting state

- `01a73798..a0f54105` contains eight commits, not documentation-only: it includes
  Windows pager/settings implementation, shared settings-label changes and core
  comments. No `crates/tiler-macos/src/` runtime source changed (only the macOS
  README). The port notes were added after their stated baseline; their inventory
  still applies. Their build section omitted native macOS contract-skew failures,
  corrected with this record.
- Native build fails with 31 errors: retired `DomainReport`, `OutputTopology`,
  `MinimizedEntry`, `WorkspaceBinding`, `AttemptFeedback`/`AttemptKind`; retired
  report fields; `WorkspaceDesire.logical_id`; changed select/send command
  shapes; a nonexistent builder input; and private engine-boundary access.
- The documented non-macOS failures are separate cfg leaks:
  `textedit_post.rs:1055` imports macOS-only `AX_ATTRIBUTE_UNSUPPORTED`;
  `select_owner.rs:241` calls `take_trailing_proof`, whose definition is gated
  by `cfg(any(target_os="macos", test))` (`workspace_select.rs:193-194`). They do
  not reproduce as errors on macOS. Cross-target compilation was blocked by the
  unavailable target standard library; the source mismatch was confirmed, not
  claimed as a successful cross-platform reproduction.

### Existing seams and deletion inventory

Paths in this subsection are under `crates/tiler-macos/src/`.

| Module / consumer | Current responsibility | Port or delete |
| --- | --- | --- |
| `world_builder.rs:6-50,60-63,332-360` | Pure retained-native-state report builder, still v1 domains/topology/bindings/minimized/feedback | Rewrite directly as v2 screens/windows/views/closed/native/raw attempts/presentation; delete retired shape and outdated header claims |
| `world_runtime.rs:3-49,185,229` | Engine-thread `WorldSession`, report construction, hidden-scope holds and old binding/session extraction | Keep one world owner; delete private `WorldEngine::engine()` inspection and adapter policy/session duplication |
| `world_dispatch.rs:3-17,135` | Per-job report/reply pairing, frame slots, focus outbox, select claims, stale-emission veto | Keep mechanical freshness and dispatch; port commands, authorities, presentation and raw attempt plumbing |
| `world_sequencer.rs:6-38` | Observation watermark, single flight, distinct request/command sequences, spend-before-dispatch effects, generation reset | Retain and test against v2 accepted/duplicate/consumed/stale cases; remove obsolete comments/allowances where no longer needed |
| `world_command_map.rs:3-14,35-48,96-101,158` | Armed select subset, offline send mapping, adapter output selection and resize-repeat cursor | Translate semantic commands only; core resolves targeting; delete local targeting/policy and obsolete command shapes |
| `drag_drop.rs:21-23,892-999` | Direct `Engine`/`CoreEvent`/`CoreReply`/`TiledPlan` preview/drop boundary | Replace active Meta-drag policy with world gesture ingress and correlated reply results; delete direct engine helpers, not re-export private APIs |
| `drag_fixture.rs:26-28,209,284-285,395,508,551`; `drag_session.rs:2173,2183` | Offline legacy drag engine harness and test-only imports | Port to world reports/replies and remove all retired-boundary imports, including tests |
| `runtime_core.rs:17`; `select_feedback.rs:75,116`; `workspace_send.rs:92`; `trailing_binding.rs:41,205,230-232`; `runtime_thread.rs:152,1279` | Old minimized/feedback/binding types and verdict staging | Delete adapter verdicts and trailing proof authority; replace with truthful v2 facts and raw attempts |
| `classifier.rs`, `workspace_select.rs`, `workspace_send*.rs`, `space_drag_transfer.rs`, `dock_swipe_select.rs`, `input_session.rs`, `runtime*.rs` | Native mechanics mixed with classification, selection/send/trailing/drag policy | Move policy to shared core calls/world commands; retain only AX/CG/Space observations, native binding, lifetime/coordinates, input capture and dispatch proofs |

- `tiler_core::boundary::CoreCommand` remains public world-operation vocabulary;
  its presence alone is not a retired-boundary violation. Old names in comments
  are not live consumers. The macOS-local `crate::engine::Engine` is a native
  probe-state thread, not the retired shared policy engine.
- Module headers describe older milestones, not guaranteed current activation:
  e.g. the command-map header calls sends offline-only while later native-send
  modules exist. Follow actual call sites during deletion, not header promises.

## Dependency: KDE native adoption

- Initial pull found no newer revision. At `a0f54105`, the approved host-owned
  adoption is not implemented: `crates/tiler-core/src/world_publish.rs:1008`
  publishes `native_binding: None`; capability/native observations exist but do
  not bootstrap native rings. See `docs/decisions.md:4584-4600`.
- KDE delivers fresh-generation native-order adoption with membership preserved,
  no workspace moves at launch, and later outside moves/creations. macOS consumes
  that delivery and its accepted tests; it implements no equivalent in the host
  or core. Recheck the actual source after updating, not only documentation.
- Update 2026-10-24: `c7f6af40` delivers the host-owned core adoption change;
  native acceptance is still separate. Only startup adoption and its live
  acceptance depend on that delivery. Stages 1, 2, 4 and offline stage-3
  translation/executors/readback/fake-native work proceed independently. No
  temporary adapter-minted logical workspace policy or fake native bindings.

## Reviewed outcomes (2026-10-24)

- Workspace authority: core owns adoption, existence/order/current/membership
  after outside changes, and native-to-logical bindings published through
  `WorkspaceDesire.native_binding`. macOS publishes native order/current,
  translates actual membership and views to logical receipts, and executes
  desires with raw attempts. No durable host binding table: only live reply
  lookups. Adapter-owned mapping was rejected as a second policy owner.
- App-hidden/Cmd+H: interim tentative A treats the window as gone, preserving
  legacy macOS behavior and the Windows precedent in the minimize/hidden
  reference outcomes. When this behavior is reached, inspect local pinned yabai,
  AeroSpace and Amethyst hide/unhide handling and core minimized behavior
  (including restore-on-focus); compare A, B (report minimized, possibly only
  documentation clarification), and C (neutral kept-but-host-hidden core state)
  before a final selection. No new core state now. Keep Windows-named
  `SafetyFacts`; renaming is cosmetic churn. This supersedes the source-only
  fit review's blanket prohibition on app-hidden omission.
- Async settlement: existing raw attempts (A), no core change. Stage 3 tests
  delayed arrival, intermediate Space reads, partial AX transition inventory
  (incomplete/hold, never closes), explicit refusal and a newer user switch. If
  A mis-settles, first test adapter-only B: post readback at a host completion
  notification with a justified genuine deadline. Only if B fails propose
  neutral pending C, aligned with KDE's async-configure investigation.
- Coordinates: macOS `FrameAttempt.dpi_before/after = None`; the logical-point
  space has no per-window basis. Approved documentation-only core/guide
  clarification landed separately as `b2d12a7c`; runtime semantics unchanged
  (`dpi_basis_changed` treats None as no change). Write whole points; round
  fixture-derived minima up and maxima down at the documented conversion.
  Stage 2 tests fractional AX readbacks and cross-display clamping, checking
  yabai/AeroSpace move sequencing. Only an actual clamp not fixable mechanically
  justifies proposing an opaque coordinate-basis token after KDE core delivery;
  Windows could send its DPI with no Windows behavior change.
- Creation correlation belongs to KDE's adoption design: no creation-attempt
  type exists and core does not construct `HoldReason::AmbiguousCreation`.
  Re-evaluate the delivered interface if it fits macOS poorly; no macOS-side
  correlation authority. Removal remains `existence=false` eligibility plus
  fresh occupant/pin checks.
- Fullscreen/system Spaces are not desktops: omit them from `native_order`.
  Report fullscreen windows with `fullscreen=true`, `observed_workspace=None`
  so core retains home; emit an empty logical view while a fullscreen Space
  is current. The former nonoptional `current_native` limitation belongs to the
  shared adoption design. Sticky requires positive all-Spaces proof. Stage 2
  checks whether All Desktops is per-display and whether core sticky scope fits.
  Fullscreen/sticky product questions stay parked TBD.
- The KDE adoption design received the ownership/creation/fullscreen issues,
  incomplete-snapshot adoption, in-flight-move correlation, separate-Spaces-off
  spanning domains, opaque/unstable native IDs and sticky scope. No separate
  macOS policy solution is implied by that handoff.

## Reviewed update (2026-10-25)

- App-hidden is option B: report Cmd+H windows as `WindowFact.minimized=true`
  through existing lifetime/explicit-close plumbing. Preserve the real native
  hidden/minimized cause in diagnostics; hide-then-terminate must emit a close.
  Add a direct-target focus test, not only the directional-source fence: no
  restore or focus of the hidden lifetime. This is a reversible adapter mapping.
- The proposed `WindowFact.minimized` documentation broadening is deferred
  pending a shared representation for KDE inactive-Activity windows. Do not
  change the shared field/comment ahead of that agreement.
- Updated tentative host-owned split supersedes the earlier logical-receipt
  translation proposal: publish opaque native membership as
  `WindowFact.native_workspace` (wire `nativeWs`), native order/current via
  `native[]`, and no logical `observed_workspace` or view receipts. Core alone
  derives logical membership/views. Reply `native_binding` resolves execution
  against live native-id-to-handle lookups; no host binding table. Logical
  receipts remain for tiler-owned domains.
- Tentative edge facts pending shared tests: native membership is `None` for
  fullscreen-Space windows, all-Spaces/sticky windows and unreadable membership.
  Sticky still needs positive native proof and a per-display verification plan.
- KDE stage 1b is not present at `0f3c92b5`. Creation attempts/correlation,
  authoritative native ordering and optional `current_native` are pending shared
  shapes. Only independent native select/send/remove/readback work proceeds;
  creation integration waits for the delivered contract and seven macOS
  discriminator tests. Existing async A/B/C evidence gates remain unchanged.
- Update: stage 1b subsequently landed in `4bd4aca9`, with native-only view
  settlement in `28c3cff2`. macOS consumes the delivered compatibility updates,
  including optional native current and creation row fields. This does not by
  itself wire native creation/select/send/remove execution or prove acceptance.
- Shared sticky semantics are unchanged; a coordinate-basis token would follow
  stage 1b only on reproduced evidence. The verified fixture reader hold-open
  fix is already on main; no macOS fixture change is required.

## Staged plan and gates

| Stage | Ports and deletions | Exit gate |
| --- | --- | --- |
| 0. State and fit review | This inventory, ranked contract review, dependency and gate baseline; no implementation | Findings and staged proposal available together; unsupported semantics remain TBD |
| 1. Atomic strict-v2 compile cutover | Rewrite builder/envelope and command shapes; port runtime/dispatch to public world snapshot/results, raw attempt types and native binding interface; port active Meta-drag oracle/harness to world gesture reports; delete all retired engine/event/reply/report/feedback/binding routes and repair or delete cfg-leak callers | Locked macOS build and full tests, strict all-target macOS clippy, workspace fmt; core gate set below; search finds no retired private-boundary consumers. No shim or parallel route, including fixtures/tests |
| 2. Policy and fact ownership | Replace classifier policy with native normalization plus shared in-process policy; delete local select/send/trailing/drag targeting/history/settlement policy; retain input eligibility/capture, lifetime and native proofs. Exercise classification/hidden/minimized/incomplete rosters and active Meta drag; passive titlebar source stays parked | Fake-native regressions for incomplete/closed/minimized, hidden-app ambiguity, exact cursor hit, command targeting, gesture lifecycle/preview/commit, stale/generation replies, no duplicate effects; stage-1 gates. Any unrepresentable safety semantics require a reviewed neutral core proposal, not a host policy workaround |
| 3. Native workspace binding and settlement | Consume KDE adoption; publish native order/current via `native[]` and per-window `native_workspace`, with logical receipts derived only in core. Port native select/send/remove executors to desires and raw pre/post attempts; creation integration waits for stage 1b. Delete trailing proof policy and host membership authority | KDE dependency accepted; four occupied native Spaces -> four logical workspaces, same members and zero launch workspace moves; outside move/create coverage. Fake async/refusal/unreadable/supersession/no-replay/removal-pins tests; stage-1 gates. Native hide/reveal checkpoints required before claiming workspace acceptance |
| 4. Effects and presentation | Replace local bounds/eligibility derivation with reply border/underlay/pager/preview; preserve native focus-arrival and last-moment vetoes. Wire independently overridable border and focused-miniature colours, default live accent, full-strength miniature border plus dimmer same-colour fill | Fake render/focus/stack/cursor tests, independent colour override and live theme-update tests, paused-game/unknown-focus hides, no unsolicited fullscreen/focus writes; stage-1 gates; fixture visual/native checkpoint where supported |
| 5. Native acceptance and closure | Complete fixture-first integration, physical input checks and native workspace lifecycle/multi-output coverage; remove remaining superseded policy caches/helpers/tests/docs rather than preserve a second route | All offline gates; signed bundle verification; native checkpoints below; narrow checker rejects missing/pending/fenced/skipped evidence before acceptance. Explicitly list unsupported host routes and deferred product behavior; no generic claim from the six-checkpoint core baseline |

### Gates at every implementation stage

- From the repository root, prefix Cargo/just commands with `mise exec --`.
- Core baseline: locked core/protocol build and tests, strict all-target clippy,
  workspace format check, and the ordered world-acceptance fixture (commands and
  stage-0 results below). Repeat after any relevant shared-source update.
- macOS: `cargo build --locked -p tiler-macos`, `cargo test --locked -p
  tiler-macos`, `cargo clippy --locked -p tiler-macos --all-targets -- -D warnings`;
  final platform recipes `just build`, `just test`, `just clippy`, `just fmt`.
  Cross-platform macOS stub compilation must be checked in the supported CI
  environments, not waived by native success. No toolchain installation assumed.
- Signed product bundle and fixture bundle build/verify are separate from Cargo
  compilation; deployment floor and persistent identity must be verified.
- Retired-boundary audit: search `crates/tiler-macos` for `CoreEvent`, `CoreReply`,
  `TiledPlan`, `tiler_core::engine`, and `.engine()`; inspect imports, qualified
  calls and tests rather than counting historical comments as consumers. Also
  reject remaining uses of `DomainReport`, `OutputTopology`, `MinimizedEntry`,
  `WorkspaceBinding`, `AttemptFeedback`, `AttemptKind` and `logical_id` as old
  shared report fields/types; native binding records may still have logical IDs.

### Native acceptance checkpoints

- Follow [live guide](../live-macos-testing.md) and
  [fixture reference](../../crates/tiler-live-fixture/README.md#macos-bundle-launch-and-scope).
  Use the signed regular-activation fixture bundle, verified persistent identity,
  LaunchServices launch and exact fixture-only bundle scope, never a directly
  launched executable or legacy helper as a substitute. Prove fixture identity,
  guarded input ownership and independent restoration before dependent effects.
- Basic route: bootstrap A/B, admit C, frame/coverage readback, same-app focus
  arrival, close/reflow; socket observations joined to fresh AX/CG identities.
- Mandatory native away/return: capture visible home; select away; capture the
  intermediate hidden state independently (visibility, frame, foreground and
  Space membership); return; capture native reveal and retained layout. Logical
  view echoes or command success alone do not satisfy either checkpoint.
- Narrow checking first rejects missing readbacks, pending operations,
  omissions/authority fences, coverage defects and skipped intermediate
  hide/reveal, then checks broader outcomes. No replacement shared connector is
  implied; its separation review remains deferred.
- Workspace legs: existing select/previous, send-follow/stay/inverse, sole-window
  focus, reused/new trailing target, two create/remove cycles, outside move/create,
  unknown-occupancy removal refusal, topology change and separate/spanning native
  modes. A native Space request is asynchronous shell work, never an arrival.
- Native Spaces in fullscreen/system states remain separately fenced; no
  fullscreen-Space scope or sticky-subset product decision is inferred. Mixed
  scale, multiple displays and supported release floor require their own proof.
- Physical chord delivery/readiness/smoothness remain physical checks; fixture
  socket focus/resize and synthetic diagnostics cannot establish them. Existing
  guarded controls are not yet fixture input routes. First accept that harness
  contract, or use the physical fixture procedure, never bypass its identity gate.
- Verify exact owned baseline restoration, stop/exit input and overlay release,
  and independent recovery before any failure/loss acceptance. No live game
  probes: game safety wins, including silent paused presentation and no added
  game-costing polling/hooks.

## Abstraction-fit review

- Citations below are line positions at `a0f54105`. Wire codec names are strict
  mirrors of core types, not a separate compatibility API. Ranking reflects
  migration impact; a host-mechanism gap is not automatically a core defect.

| Impact / interface cited | macOS fit and options | Recommendation / core impact |
| --- | --- | --- |
| Blocking: `WorldReport.native`, `host_owned_workspace_domains`, `WindowFact.observed_workspace`, `WorkspaceDesire.native_binding` (`crates/tiler-core/src/world_report.rs:314-331,646-691,881-895`; codec `crates/tiler-protocol/src/world_protocol.rs:622`) | A Space is a host-owned, asynchronously changing object, not a tiler-created logical virtual desktop. Current optional native rows do not establish the required initial binding. Options: shared host-owned adoption, or host-created logical mapping | Reuse KDE's approved core delivery only. Binding/observation stay mechanical; core owns logical membership. Existing approved core dependency, not a new macOS core implementation |
| High: projected `WindowFact` vs `SafetyFacts` (`world_report.rs:638-668`; `crates/tiler-core/src/window_policy.rs:38-56,127-166`; [guide section 2](../porting/world-adapter-guide.md#classification-ingress-and-remote-transport-rough-edge)) | Raw role/modal/app-hidden/activation-policy facts are absent from the world roster; shared safety vocabulary reflects Windows visible/cloaked/tool/owned-dialog/no-activate. macOS `AppFacts.hidden`/activation policy and sheet/modal rules (`crates/tiler-macos/src/classifier.rs:144-215`) do not all map losslessly. Options: call existing shared classifiers in-process after truthful normalization; if insufficient, a neutral shared safety/classification ingress or retained-hidden state | Prefer existing shared entry points for facts that fit. Do not relabel app-hidden as minimized or off-Space as destroyed, or keep native classification policy. Hide/unhide lifecycle needs the TBD discriminator below. A richer classification/hidden-state contract is a possible core change requiring user approval, not yet justified or selected; incomplete/unreadable holds are safe inability, not a permanent semantic substitute |
| High: attempts and asynchronous Space/frame settlement (`world_report.rs:383-448,450-535`; codec `world_protocol.rs:780-818`; [guide section 5](../porting/world-adapter-guide.md#5-raw-attempts-and-settlement)) | AX writes, queued SLS membership and Mission Control transitions do not settle synchronously. The fields already distinguish setter disposition from arrival, but immediate frame/readback and numeric DPI can look like synchronous Windows placement. Options: current raw attempts plus later observations; neutral pending/completion representation after a causal reproduced failure | Report actual call disposition (`Accepted` includes a successful dispatch, not completion), immediate pre/post and later freshly captured facts in order; unknown stays unknown. Never invent `Threw` for successful queuing. No core change now; pending/completion extension would need evidence and user approval, per existing deferred investigation |
| High: visibility/hide/cloak (`WindowFact.minimized`, `PresentationIngress.foreground_cloaked/hidden_workspace`, `SafetyFacts.visible/cloaked`: `world_report.rs:588-607,638-668`; codec `world_protocol.rs:1020-1034`) | DWM cloak has no literal AX equivalent. App hide, minimize, off-Space, occlusion and a missing AX row are different facts. Options: normalize only genuinely equivalent safety facts, or add explicit neutral availability/hidden-app state | Keep mechanisms native and distinguish the causes; `CG onscreen=false` alone proves none. Retain minimized facts explicitly; never silently omit retained app-hidden windows while claiming complete inventory (`world_report.rs:881-884`). A new hidden-state field/policy is a core proposal requiring approval only if existing shared gates cannot express the tested lifecycle |
| High: focus intent vs native focus arrival (`OneShotEffect::is_explicit_focus_for`, `focused_global`: `world_report.rs:218-241,956-966`) | App activation is not window focus; AX/SLPS/NSWorkspace reads are separate and can disagree. Options: existing effect correlation plus native proof, or a new core focus state machine | Keep the existing observed-focus authority and explicit/admission distinction; verify correlated effect dispatch and native arrival separately, using SLPS/NSWorkspace + exact per-app AX window/CG join. Do not globally suppress other effect kinds such as enabled hover focus. No new core option indicated; no automatic-focus product rule selected |
| Medium: `FrameAttempt.dpi_before/after`, `Rect`, style `*_px` (`world_report.rs:407-448,555-573,619-636`; codec `world_protocol.rs:814-815,1031-1033`) | A single integer DPI is Windows-shaped; macOS uses global CG/AX logical points and per-display backing scales, while fixture sizes/positions are physical pixels. Options: normalize units and document a real scale-to-DPI encoding, or a neutral coordinate-basis token | Use one top-left logical-point adapter space. Preserve `PointsRect::to_rect` finite/range checks and nearest whole-point rounding (ties away from zero: `crates/tiler-macos/src/topology.rs:13-21,64-84`); convert fixture pixels/client hints at the measured scale/chrome boundary with explicit directional rounding for minima. Do not pick 72/96 DPI or invent a readable DPI until justified; report unknown and assess its consequences. Neutral basis-token redesign requires user approval, not recommended without demonstrated failure |
| Medium: `WorkspaceMode`, `equal_workspace_count`, view/membership ownership (`world_report.rs:865-895`) | Native "Displays have separate Spaces" constrains feasible views; it is not itself a new core workspace policy. Options: consume existing configured modes with a truthful host-compatibility fence, or invent a platform mode | Use existing `per-output-local`/`global-unique` label "Displays have separate workspaces" and `shared` label "Workspaces span displays". Set equal counts only for a proven host requirement. No default/mode-switch product choice here; no core option needed |
| Medium: `WorkspaceDesire.existence` / `NativeObservation` (`world_report.rs:314-331,681-691`) | Create/destroy is shell-owned, asynchronous and can fail after a logical desire exists; removal can be blocked by foreign/sticky/system occupants. Options: guarded mechanical create/remove + observed native change, or a separate neutral lifecycle-attempt interface | Start with existing ambiguous-creation/hold vocabulary and truthful inability, exact native order/current rereads and fresh occupant/pin checks. Removal eligibility never orders blind deletion. If creation failure cannot be represented without guessing/retry, bring a concrete lifecycle-contract proposal for user approval; do not mint a macOS lifecycle policy |
| Medium: fullscreen/sticky facts (`WindowFact.fullscreen/sticky`: `world_report.rs:654-667`; current native topology exclusions `crates/tiler-macos/src/topology.rs:38-48`) | Native fullscreen introduces another Space, and multi-Space membership is not necessarily all-workspace sticky. One `observed_workspace` cannot describe arbitrary subsets. Options: current fullscreen/sticky facts plus narrow host fences; or a neutral multi-membership model | Preserve actual fullscreen flags and current safety fences; only report sticky with positive all-workspace proof, never any multi-Space count. Fullscreen-Space management and subset membership remain parked/TBD; no new core model selected |
| Medium: `ScreenReport.full_rect/work_area` (`world_report.rs:619-636`) | A single rectangle cannot represent every notch/strut exclusion, but native AppKit visible work area is already rectangular and includes menu/Dock constraints. Options: native FULL + visible work area, or a region-shaped core work area | Carry native FULL and visible rectangles separately, never infer missing geometry. Test menu/Dock/topology transitions; notch-straddling behavior remains TBD. No region API/core change without a demonstrated requirement |
| Medium: `WindowId`, explicit `closed`, opaque request identities (`world_report.rs:653-655,852-884`) | macOS PID/CGWindowID pairs may be reused and AX identities can disappear/reappear on hidden Spaces. Options: adapter lifetime registry/incarnation, or a new shared native-id type | Opaque IDs already fit: never reuse a live lifetime, distinguish actual close from temporary AX disappearance, bind setters to fresh native identity. Keep protocol native-id-free; no core change |
| Low: `PresentationIngress.pager_order` and one-shot stacking (`world_report.rs:579-586`; [guide section 4](../porting/world-adapter-guide.md#4-desired-world-state-what-the-reply-authorizes)) | CG order spans layers/apps, not one flat universally controllable z-order; focus/raise/stack are not interchangeable. Options: native-order projection and capability refusal, or platform-specific z-order fields | Filter one fresh CG stacking snapshot to exact lifetimes, preserve ordering; execute only supported authorized effects with native freshness proofs. Never use order as membership authority or claim an unsupported stack succeeded. No core change |
| Low: `GestureIngress` (`crates/tiler-core/src/world_gesture.rs:6-32`) | Lifecycle types fit the active consumed Meta-drag producer. Passive titlebar move cannot be inferred from AXMoved (`crates/tiler-macos/src/drag_native.rs:3-43`). Options: port existing producer, or invent a passive producer from frame motion | Port the active route to core lifecycle/preview/commit; leave passive titlebar drag parked. No `gesture=None` replacement for a functioning active producer, no copied host drop policy, no core change |
| Low: `cursor_hit`, `chord_held`, `move_window` (`world_report.rs:573-575,937-955`; codec `world_protocol.rs:1034-1035`) | Exact hit testing fits AX/CG; the "Win+Shift" comment is platform-specific, and macOS underlay chord remains a product question ([existing row](../spec/reference-outcomes.md#macos-group-underlay-discriminators)). Options: normalized capture booleans after the selected chord, or a new core key/modifier API | Keep native capture and Secure Input/lock inability; core sees semantic facts, never keycodes. Do not select a macOS underlay chord during this port or enable speculative hooks. No new core key API indicated |
| Low: host colours absent from core; style-only ingress (`world_report.rs:597-601`; [guide section 8](../porting/world-adapter-guide.md#8-border-underlay-drop-preview-and-pager)) | Native accent/theme observation belongs to AppKit rendering, not logical policy. Options: independent host tokens or new core colour fields | Keep two independent host colours: border tokens default live system accent; pager focused-window highlight separately defaults to system accent and is independently overridable, full-strength border + dimmer fill. No core colour API |

### Core proposals versus host work

- Already approved dependency: KDE's host-owned adoption. No new approval is
  inferred for additional fields or alternative adoption semantics.
- Possible new core proposals, not implementations: lossless shared safety /
  app-hidden lifecycle, a coordinate-basis replacement for numeric DPI, explicit
  pending-completion evidence, or lifecycle attempts. Each needs a concrete
  failing discriminator and user approval before core edits. None is accepted
  as necessary by this source-only review.
- Existing APIs plausibly fit focus, generic workspace modes, normal async
  receipts, native order projection, lifetimes, active gestures, input semantics,
  rectangular native work areas and independent host styling. Do not turn a
  Windows-flavoured comment or spelling into a speculative new abstraction.
- Behavioral ambiguities use the shortest TBD row in
  [reference outcomes](../spec/reference-outcomes.md#macos-classification-discriminators).
  This review adds only app hide/unhide; reuse existing fullscreen, sheet,
  accessory, nonstandard-role, native-workspace, underlay and drag rows. No parked
  result or foreign WM outcome is selected.

## Tentative decisions

| Decision | Alternatives | Reason |
| --- | --- | --- |
| Make the first implementation checkpoint an atomic compiling v2 boundary replacement, including the old drag oracle/harness | Re-export internals, shim v1, or leave a build-breaking intermediate route | One executable contract and meaningful macOS gates; no compatibility debt |
| Reuse sequencer and native thread/proof ownership, rewriting only their obsolete contract/policy surfaces | Replace concurrency infrastructure wholesale | Existing observation watermark, per-app AX isolation and spend-before-dispatch gates are native mechanics worth preserving |
| Normalize all report geometry to current CG/AX logical points and preserve nearest whole-point frame rounding; document fixture/hint conversion separately | Switch all AX/CG observations to physical pixels or mix spaces | Least native conversion churn and one coherent adapter space; no invented DPI |
| Reuse existing shared policy entry points before proposing richer ingress | Keep the native classifier as policy, or immediately widen core | Consumer-driven fit test; preserves policy ownership while exposing genuine unrepresentable facts |
| Treat missing native proof as inability/hold, not successful logical settlement | Success-by-dispatch or desired-state echo | Native arrival, visibility and focus must remain independently falsifiable |
| Interim app-hidden A treats the window as gone, pending the reviewed evidence comparison | B: report minimized; C: neutral kept-but-hidden core state | Preserve current legacy behavior without a new core state; not a final hidden-window product choice |
| Retain skipped outputs as incomplete screen rows, not absent topology | Omit outputs when native Space homing is unreadable | Avoid treating a transient observation gap as display removal/hotplug displacement |
| Use the current reply as a live binding/lifetime lookup; temporarily echo only previously asserted automatic-float facts | Read private core sessions or infer automatic provenance from indistinguishable float replies | Preserve the compile-cutover behavior without private API access; stage 2 must remove classification policy/cache ownership, not promote this assertion set to a new policy model |
| Gesture terminals do not use observation-only dispatch retries; a stale reply hides the preview until a fresh sample | Retry terminal ingress or show stale output | Stable core gesture IDs own terminal consumption; never replay a commit or paint stale shapes |
| Remove obsolete per-predicate Windows conformance pairs; keep real public classifier comparisons and fixture expectations | Restore removed Windows wrappers or compare core predicates with themselves | Shared predicate tests belong in core; self-comparison cannot establish platform conformance |
| Preserve the existing automatic-float echo until D2 is resolved in core | Remove the echo and accept retile on hint loss, or expand host policy | Keeps the selected product expectation and current regressions intact; does not claim completed policy ownership or add a workaround |
| Tentative shared classification A: one neutral transient fact yields tile, float member or leave unmanaged in process | Retain native transient float policy, or widen the wire roster | Small additive classifier; macOS normalizes sheets/dialogs/modals and projects the verdict into existing floating facts. Windows Dialog and OwnedDialog exclusions remain unchanged |

## Stage-1 outcome (2026-10-25)

- Strict-v2 macOS build is restored. Builder/runtime/dispatch use public world
  reports/replies and raw attempt types; old shared minimized/feedback/binding
  types, private session extraction and `trailing_binding` are deleted. Native
  creation/execution proof gates remain mechanical; no new host binding owner.
- Active Meta drag and its offline harness now share Started/Updated/Released/
  Cancelled ingress with press-minted identity and frozen start samples. Removed
  direct engine preview/drop helpers and the production operation-command drag
  route. Public `CoreCommand` remains only for other supported world operations.
  Passive titlebar drag remains parked.
- Fixed both cfg leaks: non-macOS FFI exports the actual AX unsupported constant;
  the cross-platform selection owner can call the portable proof-take method.
  Native success does not replace the still-needed cross-platform CI run.
- Review corrections accepted: eliminated vacuous core-vs-core conformance
  checks; moved empty-context refusal ahead of draining raw evidence/assertions,
  with a regression that fails under the old ordering; completed production
  gesture wiring instead of leaving a harness-only lifecycle route.
- Regression coverage includes native adoption-shaped reports, skipped/incomplete
  inventory holds, raw-attempt translation/generation fences, no-report evidence
  retention, current-press/hidden/unknown gesture vetoes, update preview targeting,
  release exactly once/replay dead, cancellation and stale/generation responses.
- Gates on the final cutover: `mise exec -- just build`, `just test` (1,269
  tests, 0 failed), `just clippy` (strict all-target), `just fmt`; locked
  core/protocol build and tests (1,236 tests, 0 failed), strict all-target
  core/protocol clippy and ordered acceptance 6/6 all PASS. Retired-boundary
  audit finds only historical comments, no consumers. No live tests, signed
  bundle acceptance or cross-target compilation claimed.
- Remaining later-stage work is explicit: richer native-to-logical observed
  receipts currently remain empty while core consumes native membership facts;
  ordinary frame-attempt/readback/clock plumbing, learning/backoff, presentation
  and policy ownership are not claimed complete by this compiling checkpoint.
- Stage-3 discriminator: a trailing send on the very first report refuses
  `send-target-unknown` because the lifecycle spare is minted after command
  staging. An observation-first sequence passes and matches production startup;
  no core change or host workaround selected. Test this boundary again with
  native lifecycle translation; do not broaden bootstrap acceptance implicitly.
- Stage-2 follow-up: prove lost terminal/timeout cannot leak an orphaned core
  gesture into the next gesture; native capture release is fail-closed today,
  but missing-terminal core cleanup has not been established by the current tests.

## Stage-2 option B implementation (2026-10-25)

- Landed as `24ff4eec`.
- `HiddenApp` now follows the retained minimized-fact path. Diagnostics retain
  distinct `classifier-omitted-hidden-app` and `classifier-omitted-minimized`
  tokens. The adapter mapping is reversible; no shared type/comment was changed.
- Repeated hidden/minimized snapshots preserve the previous proven home after
  live-member homing is pruned. Never-homed windows gain no invented home;
  unhide, actual close, app-generation replacement and domain retirement still
  clear the corresponding retained lifetime. Hide-then-terminate emits an
  explicit close through the retained home.
- Real world-engine tests prove frameless minimized holds and no hidden focus,
  exact close/no desired-row resurrection, repeated-hide retention, distinct
  diagnostic causes, steady-state direct-target focus refusal, and FIRST-hide
  same-report focus refusal under both complete and incomplete inventory.
  `FocusDirectional` is tested separately. The old test incorrectly named
  `WorldCommand::Focus` a directional source; that obsolete duplicate is removed.
- Host-owned reports keep `observed_workspace=None` and no logical views.
  Deleted native-to-logical raw-attempt translation; target IDs remain operation
  correlation only. Current attempt pre/post fields represent logical reads, so
  unsupported native pre/post remains `None`, not forged receipts. This limits
  refusal proof and must be assessed in the offline executor unit; no new native
  attempt schema is assumed to be part of stage 1b.
- Full macOS test recipe PASS: 1,279 tests, 0 failed. Strict all-target clippy
  and workspace fmt PASS. Core/protocol locked tests and strict clippy PASS;
  no shared behavior changed. No live acceptance claimed.
- Remaining fact gap: fullscreen-Space and multi-Space windows are still omitted
  by the old exact-Space partition, not explicitly reported with unknown native
  membership. This is not accepted as the tentative `None` edge implementation;
  the fact-ownership unit must keep those lifetimes truthful and fenced.

## Approved D2 core correction (2026-10-25)

- Landed as isolated correction `0b65cad5`.
- Known automatic floats now retain floating across fixed-hint loss with native
  `floating=false`; observed suppression or retained tile override still tiles.
  Explicit ToggleFloat remains a genuine tile win. Only the existing retained
  exception condition changed; no new type, transport or compatibility route.
- `automatic_hint_loss_with_native_unfloat_stays_floating_until_explicit_win`
  distinguishes the missing native-unfloat case, preserves the automatic marker
  without admission, then proves hintless suppress-to-tile and retained override
  across hint changes. Existing D2/D3/D5/D6 and ToggleFloat tests stay green.
- Locked core/protocol build and full tests PASS (1,287 tests, 0 failed), strict
  all-target clippy PASS, workspace fmt PASS, ordered acceptance PASS (6/6).
  Portable Windows tests PASS (724 tests); Windows-only `tiling_sys` tests need
  their native test environment. No Windows source changed.
- Windows `capture_floating` (`tiling_sys/world_capture.rs:667-701`) reports true
  for sticky, the adapter's retained `fixed_auto_seen` latch without a tile win,
  and its float/restart/born intent sets. It does not directly copy a core reply
  floating row. Thus its automatic lane already supplies `floating=true` while
  that latch remains; the new core regression covers the missing false-input lane.
- KDE test execution is blocked by absent project test dependencies; no install
  was performed. Fixed-float TS tests mostly mock core observations, but some
  engine-backed fixture tests do invoke Rust. Do not infer a passing KDE suite
  from that source inspection; native KDE/CI verification remains required.
- The correction is D2 conformance, not a new policy. macOS echo removal is a
  separate adapter change so its existing `cold_fixed_only_floats_automatic`
  gate proves the core alone preserves the contract.

### Parked post-port review

- Review whether `WindowFact.floating` should carry genuine host float signals
  only, rather than adapter echoes of core state. This review is after the port;
  no field semantics or intentional-float echo change is selected here.

## macOS automatic-float echo removal (2026-10-25)

- Removed `WorldSession.auto_asserted`, builder assertion parameters, first-sight
  fixed-hint assertions and retained automatic-origin reassertion. Native facts
  now carry `fixed_auto=false`; admission hints and the report predicate let core
  own the automatic marker. No compatibility path or replacement host cache.
- `cold_fixed_only_floats_automatic` passes with its original D2 expectation.
  A builder/world-engine seam separately proves plain-wire fixed admission,
  hint-loss retention and explicit suppress-to-tile without an adapter cache.
- Full locked macOS recipe tests PASS (1,283 tests); all-target build, strict
  all-target clippy and fmt PASS. Intentional placed-float echo and other existing
  lifetime/born handling are unchanged pending their own ownership review.
- The automatic-hint-loss blocker is closed. Shared classification and native
  lifetime/inventory redesign, local selection/send/trailing policy deletion and
  native executor integration remain unfinished; this checkpoint claims only
  the automatic-float policy-copy removal.

## Born-state ownership and remaining classification fit (2026-10-25)

- Removed adapter synthetic born-fullscreen floating and retained slot/member
  inference. Reports carry the native fullscreen flag; core alone keeps born
  arrivals slotless and handles their plain/fixed first exits. Removed the
  native-to-logical home lookup used solely by that policy copy. Intentional
  placed-float echo remains unchanged for the parked post-port review.
- Production classifier/fact-builder/world-engine tests preserve standard,
  transient, fullscreen and fixed-hint shapes, minimized/hidden omissions plus
  explicit kept facts, born-fullscreen slotless hold/first exit and ToggleFloat
  persistence. No synthetic automatic or fullscreen float flag is needed.
- Full macOS tests PASS (1,288 tests); strict all-target clippy and fmt PASS.
  This is policy-copy deletion, not a completed shared-classifier integration.
- Shared classifier fit remains open: `window_policy.rs:46-56,127-166` exposes
  safety booleans and eligibility refusals; native `classifier.rs:354-373`
  admits sheets/dialogs/modals as untouched float members. Treating a shared
  `Dialog` refusal as exclusion would change the selected native behavior, and
  inventing a shared float verdict would change the core interface. Native
  role/subrole/layer/app-policy normalization also has no lossless raw world
  ingress today. No speculative flag, precedence change or diagnostic rename
  was introduced. A neutral classification interface needs review before that
  remaining policy can leave the adapter; platform fact mapping alone is the
  alternative, not an implicitly approved core change.

## Stage-2 mechanical evidence and policy blocker (2026-10-25)

- Added fake-native tests through production `apply_scoped_frame` for a first
  size leg clamped to the old display, position move, then final size recovery.
  The size-position-size transaction reaches the whole-point target; no new
  coordinate-basis representation is indicated by this model.
- Fractional immediate AX readbacks remain raw actuator evidence; planning
  uses `PointsRect::to_rect` nearest whole points, ties away from zero. Tests
  cover positive/negative ties, valid fractional minima rounding upward via
  `NativeWindowFacts::size_hints`, and invalid/unknown hints remaining absent.
  No AX maximum is invented. Physical-fixture maxima conversion is not a new
  production hint source; no general maximum-conversion implementation is claimed.
- Added production gesture handoff cases for orphan cancellation, new-ID
  supersession after a missing terminal, and a late old release. Old IDs do not
  commit or corrupt the new preview; the newer ID commits once. Main-side
  cancellation releases capture; invalid/absent carrier proof stays hidden.
  These are fake-native lifecycle cases, not physical input or timeout timing
  acceptance.
- Policy blocker: removing the existing automatic-float echo makes
  `cold_fixed_only_floats_automatic` fail after fixed hints disappear: a previously
  automatic float is tiled again. `session/world.rs:1263-1265` states D2 is
  admission-only, but `:1354-1386` retiles a non-floating observation when
  `was_automatic && !fixed_now`. The selected D2 test and existing echo remain
  unchanged; the unaccepted removal was withdrawn, not blessed by repinning it.
- Historical proposal (now implemented above): retain known automatic floats
  across hint loss while preserving explicit tile/suppression wins. This would
  allow removal of the adapter assertion cache without changing D2. Re-scoping
  D2 instead is a product change. Any shared-core edit needs separate review.
- The attempted explicit unknown-native roster path was also withdrawn after
  review exposed repeat-hidden loss, unrelated-app clearing of a global
  inventory hold, and missing explicit closes for unhomed lifetimes at
  termination. No such implementation is committed. Before another cutover,
  lifetime/incompleteness must survive per-app repeated reads and unrelated
  updates, and closes must ride independently of native homing. Do not repeat
  a global-boolean/omission approach or choose an arbitrary output.
- Native classifier policy, local selection/send/trailing policy, fullscreen /
  multi-Space explicit unknown-membership facts and native executors are still
  unfinished. No core type/comment or sticky semantics were changed here.
- Current accepted checkpoint consumes stage 1b from `bddfef15`: macOS build,
  full tests (1,283 passed, 0 failed), strict all-target clippy PASS; locked
  core/protocol build, full tests (1,286 passed, 0 failed; 31 successful suites)
  and strict all-target clippy PASS; ordered acceptance PASS, 6/6. No
  signed-bundle/live acceptance is claimed.

### Sticky per-display verification plan

- Use signed fixture-only windows on two outputs; capture ordered desktop IDs,
  current desktop per output, exact caller/AX/CG lifetime, frame and visibility.
  Keep the separate-Spaces setting unchanged and record its observed mode.
- Assign one owned fixture window to All Desktops using the native control;
  switch existing desktops on each output independently and freshly read its
  membership, output affinity and visibility after each settled switch. Compare
  against the complete desktop order for each output, not a membership count.
- Restore that window's exact prior assignment/current desktops and verify the
  baseline. Stop on uncertain ownership, unsupported readback or unrelated drift.
  Scope remains TBD until this native evidence exists; no all-Spaces proof is
  inferred from multiple membership IDs, and no game probe is required.

## Per-app native lifetime and close authority (2026-10-25)

- Each app generation owns a persistent native roster plus its own incomplete
  flag/cause. Repeated missing/hidden reads retain identity; only that app's
  covering read clears its hold. Another app's read or a session watermark/gap
  reset cannot clear it. Homed input-member lists are derived projections, not
  lifetime owners or logical workspace policy.
- Reportable unknown membership uses `native_workspace=None`, no logical
  receipts and no inferred sticky flag. Output affinity requires uniquely
  grounded FULL-screen frame overlap; ties, off-screen or unreadable output
  retain identity under an app hold instead of choosing an arbitrary display.
  Excluded roster rows never reappear as stale live facts. Selected unmanaged
  exclusions do not stall the world merely because they have no managed frame;
  hidden/minimized lifetimes without representable facts stay held.
- Explicit closes are flat IDs, independent of native workspace homing. Removed
  partitioned close helpers/carriers. A pending outbox survives no-report,
  rejection, duplicate and stale replies; only IDs in an incorporated applied
  report drain. Relaunch fences old-generation pending IDs before a reused WID
  can be reported; unrelated app closes survive.
- Crucial read-side proof: an empty AX list is not a close oracle. An outside
  move to a hidden Space with stay can leave topology/current unchanged while
  `AXWindows` goes empty. The existing full CG window-server read now carries
  explicit completeness and exact bound-PID window IDs into the snapshot and
  republish equality. CG-alive or unreadable cases retain/hold; only fresh
  complete CG absence plus covering AX absence permits a missing lifetime to
  close. Both partial and empty AX prune paths use this evidence. No engine
  thread FFI, new polling cadence or fabricated error-to-empty conversion.
- Native order and FULL/work-area rows survive a native fullscreen current view
  with `current_native=None`; the actual context-derivation seam is tested.
- Regressions cover positive CG liveness after move-with-stay, genuine absence
  and close/reflow, partial AX plus a CG-live missing sibling, unreadable CG,
  repeated hidden reads interleaved with another app, selected unmanaged
  exclusions, grounded versus tied output, flat terminate while another app is
  incomplete, no-report/redelivery, and actual Launch/new-generation/WID reuse.
- Locked macOS build and full tests PASS (1,308 tests, 0 failed), strict
  all-target clippy and workspace fmt PASS. No live hide/reveal or signed-bundle
  acceptance claimed. The liveness oracle uses the existing all-window CG read;
  native fixture acceptance must still corroborate its across-Spaces behavior.
- The earlier withdrawn global-boolean/ephemeral-roster approach is superseded
  by this per-app path. Classification API fit and local select/send/trailing
  policy deletion/native executor integration remain open; no new shared core
  abstraction was introduced.

## Shared transient classification (2026-10-25)

- Tentative option A adds `MembershipVerdict` and `classify_membership` to
  `window_policy.rs`: existing safety gates leave unmanaged; otherwise the one
  neutral `transient` fact selects float membership versus ordinary tile
  eligibility. Fullscreen/maximized remain orthogonal; existing geometry/focus
  entry points and the wire contract are unchanged.
- macOS normalizes sheet role, dialog-family subrole and modal facts, preserving
  their diagnostic priority and existing native exclusions. Shared core now
  selects float membership; the existing builder projects it into
  `WindowFact.floating`. No float echo, minimized-comment or born-state change.
- Windows inspection confirms generic unowned `#32770` dialogs normalize to
  `SafetyFacts.dialog` and refuse `Dialog`; owned windows refuse `OwnedDialog`
  first. Its production classifier is unchanged. Both exclusions remain even
  with a hypothetical transient fact, preserving the intentional divergence.
- Coverage: all 512 safety combinations crossed with transient, all three
  macOS transient shapes through the production classifier/builder seam,
  native exclusion precedence, and Windows ordinary/dialog/owned combinations.
  Source/diff review found no blocking issue or wire change.
- Locked build PASS for core/protocol/macOS; full core/protocol tests PASS
  (1,288), macOS PASS (1,310), portable Windows PASS (725), all zero failures.
  Strict all-target clippy for all four crates, workspace fmt/whitespace and
  ordered core acceptance (6/6) PASS. Windows-native and KDE runtime suites are
  not claimed. No live or signed-bundle acceptance performed.

## Native executor interface checkpoint (2026-10-25)

- The remaining product select route still interprets ordinals/relative/previous
  and trailing locally in `SelectionState::execute`; send-follow updates its
  previous-history table. `runtime_thread.rs` also binds numbered claims before
  the core reply. These owners must be replaced with reply-driven native
  executors, not merely have their stale-target safety fences removed.
- Confirmed fit limitation: `MemberAttempt.pre/post` and
  `ViewAttempt.pre/post/baseline` carry logical workspaces only.
  `WorldSession::stage_view_attempt` and `stage_member_attempt` consequently
  discard native Space readbacks and publish unknown pre/post. Fresh `native[]`
  and per-window native facts can prove later arrival, but these attempt rows
  cannot carry native refusal preimages or a proven-then-abandoned view.
- The [shared native-preimage extension](../porting/world-adapter-guide.md#5-raw-attempts-and-settlement)
  is still draft; KDE's [B2 dormant-source discriminator](kde-world-v2-port.md#b2-source-view-rewind-during-migration-parked)
  additionally needs an identity/return-provenance decision. Neither is solved
  by inventing logical receipts or a host binding table. Reuse the
  [migration rewind row](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  and the native refusal/delayed-arrival row below the macOS workspace heading.
- Options: A, retain unknown attempt fields plus fresh native observations and
  mechanical arrival checks, leaving refusal/supersession acceptance open;
  B, consume the coordinated neutral native-attempt delivery before completing
  those executor discriminators (recommended). No additional shared change or
  adapter settlement workaround is selected here. Reply-driven targeting can
  be separated from this evidence limitation, but cannot close stage 3 alone.

## Stage-2 hide/unhide evidence (2026-10-25)

- Stage 1 landed as `d7a87d82`. Interim app-hidden A is unchanged; this checkpoint
  adds evidence tests only. Other stage-2 policy removal, fractional-readback /
  cross-display tests and orphaned-gesture terminal coverage are not complete.
- Reference sources were read in local clones at the pins in
  [prior art](../research/macos-port/prior-art.md); links below cite those
  same blobs, not the moving branch heads.

| Source | Hide/unhide behavior shown by source |
| --- | --- |
| yabai `dd845723416f5fe92af49fad5ebab00369e07edd` | [event_loop.c:432-540](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L432-L540): hide removes managed tree nodes and reflows; unhide re-adds eligible windows with chained insertion points. No saved hide slot in these handlers; not equivalent to core slot restoration |
| AeroSpace `74a1bf17e82d70e0a21945ba04bb4f590bf19f83` | [normalizeLayoutReason.swift:24-71](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/normalizeLayoutReason.swift#L24-L71): native app-hide is separate from minimize/fullscreen and moves windows to a workspace hidden-app container; unhide re-layouts prior tiled windows, not a saved tree-slot restore |
| Amethyst `5cfe2fe12b1a933139010da5760aaa7e44033e9b` | [WindowManager.swift:159-184](https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Managers/WindowManager.swift#L159-L184), [Windows.swift:44-55,170-187](https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Managers/Windows.swift#L170-L187): hide deactivates a PID; tracked windows remain but active layouts filter them. Unhide refreshes windows and reactivates the PID |

### Executed A/B discriminator

- Added five tests in existing `world_builder.rs` and `world_runtime_tests.rs`,
  with the hidden window and survivor in separate apps. Current world engines
  consume the real builder reports and native adoption facts, not private core
  sessions. Test names:
  `app_hide_omission_vs_explicit_minimized_report_shapes`,
  `hidden_omission_vs_flagged_minimized_geometry_parity`,
  `directional_focus_sourced_at_hidden_window_is_fenced`,
  `hidden_app_exclusion_creates_no_minimized_record_and_prunes_homing`, and
  `hidden_then_terminate_produces_no_explicit_close`.
- A (omission) and B (explicit minimized fact) produce matching hidden desired
  rows, survivor allocations and exact pre-hide return allocations in this
  sequence. Neither produces a focus effect for the hidden/returning window.
  This does not prove all A/B reply semantics equivalent.
- Correction: `WorldCommand::Focus` directly targets the named window, despite
  its direction field; `FocusDirectional` is the source/neighbor command. The
  historical test was misnamed. Both producer shapes refused `focus-unknown`
  with unchanged revision and no native unhide/restore effect in that sequence.
- Source explanation: `world_observe.rs:290-317` excludes minimized facts from
  ordinary engine windows; `session/world.rs:1278-1308` stores a D23 slot when a
  known tiled window goes missing; `session/world.rs:1869-1876` keeps restore
  from taking session focus. Complete absent facts and explicit minimized facts
  differ in membership bookkeeping (`world_members.rs:212-236`), so a general
  equivalence claim would be unsupported.
- Proven adapter gap: HiddenApp creates no minimized record and scoped-member
  refresh prunes its native homing. `terminate_close_map` then emits no explicit
  close for that hidden window. The minimized path retains enough identity/home
  to emit the close. A lingering core D23 slot is a source-derived risk, not an
  executed leak result; no lifecycle fix was made in this evidence checkpoint.
- Recommendation for the final product choice: B is the smallest explicit
  retained-lifetime representation using the current contract and existing
  minimized-close plumbing. It would truthfully document the core flag's broader
  host-hidden use instead of introducing C. A remains viable if close/lifetime
  proof is retained separately. C adds a new core state without a demonstrated
  need from this comparison. None is newly selected here; interim A remains.
- Verification: full macOS tests PASS (1,274 tests, 0 failed), strict all-target
  macOS clippy PASS, workspace fmt PASS and diff whitespace check PASS. No
  runtime source or core behavior changed in this evidence checkpoint.

### Coordinate and sticky source findings

- Both references write size-position-size to handle native screen-area
  constraints: [yabai window_manager.c:741-760](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L741-L760)
  and [AeroSpace MacApp.swift:411-419](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L411-L419).
  This supports the existing adapter sequence; it is not a reproduced clamp or
  evidence requiring a coordinate-basis token. Fractional/cross-display fake
  tests are still due.
- Core sticky handling is home-output scoped (`world_directional.rs:86-104`);
  yabai's `window.c:843-855` uses multi-Space membership without proving that
  macOS All Desktops is per-display. Source evidence does not establish the
  two-display native scope. Keep sticky=false without positive all-Spaces proof;
  do not widen core scope or infer native behavior. Native scope remains TBD.

## Stage-0 verification

All Cargo commands below were run with `mise exec --` at `a0f54105`.

| Command | Result |
| --- | --- |
| `cargo build --locked -p tiler-core -p tiler-protocol` | PASS |
| `cargo test --locked -p tiler-core -p tiler-protocol` | PASS: 1,220 tests, 0 failed; 28 successful result suites including doc tests |
| `cargo clippy --locked -p tiler-core -p tiler-protocol --all-targets -- -D warnings` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo run --locked -p tiler-protocol --example world_acceptance -- test-fixtures/world-actions.json` | PASS: `tiled-home-away-return`, 6/6 ordered checkpoints; core-only, not native hide/reveal |
| `cargo build --locked -p tiler-macos` | FAIL: 31 contract-skew/private-boundary errors described above |
| `cargo check -p tiler-macos --target x86_64-unknown-linux-gnu` | BLOCKED: unavailable target standard library; source-only confirmation of the two documented cfg leaks |
| macOS tests/clippy/platform recipes, signed build/verify and live acceptance | Not run in stage 0; native build already fails; no live outcome claimed |

- Native compiler diagnostic classes: `E0432`/`E0433` unresolved imports/paths,
  `E0422`/`E0425` absent types/constructors, `E0603` private legacy boundary,
  `E0624` private `WorldEngine::engine`, `E0609`/`E0560` retired fields,
  `E0308` select-output shape, and `E0559`/`E0026` send-output shape. Final output:
  `could not compile tiler-macos (lib) due to 31 previous errors`.

## Risks and open evidence

- KDE delivery may change exact native binding receipts; update/inspect it before
  depending on them. No macOS workaround for absent adoption.
- Projected safety facts may not preserve app-hidden/native-role semantics;
  indiscriminate omission or a guessed minimized flag can destroy slots or leak
  authority. The first discriminator must distinguish lifetime from visibility.
- Existing `maximized: false` and current-only membership assumptions are not
  proof of native capability; unknown maximization, fullscreen Spaces and sticky
  subsets need truthful fences, not fabricated values.
- AX partial/empty hidden reads, PID/window-id reuse, queued actuation, focus
  disagreement and a generation change can invalidate any cached authority.
- Native Space routes and supported-release fallback matrix are capability-gated
  product work. Previously accepted legacy behavior is evidence to preserve, not
  proof the new route works. Fixture input-harness limitations block automated
  claims until its own ownership contract is accepted.
- macOS uses absent DPI by reviewed decision; fractional readbacks and
  cross-display clamps still need the planned fake-native coverage.
- Remaining exact next step: align the native-attempt evidence limitation with
  the shared KDE B2 delivery/identity decision, then replace select/send/trailing
  owners with reply-driven executors and raw native readbacks. Shared transient
  classification is integrated offline; later policy/executor/presentation and
  native acceptance stages remain open. Do not invent shared shapes in the adapter.

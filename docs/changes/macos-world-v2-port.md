# macOS world-contract-v2 port

## Goal and status

- Stage 0 reviewed; stage 1 complete offline; stage 2 and independent offline
  stage-3 work are partial, with app-hidden option B selected. The approved D2
  core correction, separate macOS automatic-float echo removal, born-state policy
  deletion, per-app native lifetime/flat-close path and tentative shared transient
  classifier integration are verified offline.
  The reply-driven targeting/receipt cutover is accepted offline; selection
  setter truth, deferred creation receipts and arrival-bound selection focus
  have reviewed regressions. Presentation and native acceptance remain open.
  Stage 0 evidence is from `a0f54105`
  (2026-10-24). The source-only fit review below is historical where superseded
  by the reviewed outcomes. Narrow current-artifact live evidence is recorded
  below; full native acceptance is not complete.
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
- Update 2026-10-25: `c76ba5f1` delivers capability-gated
  `MemberAttempt.native_pre/native_post` and
  `ViewAttempt.native_pre/native_post/native_baseline` (`nativePre/nativePost/nativeBase`).
  Host-owned settlement uses native evidence only; core alone translates through
  its bindings. Non-null native evidence without the capability refuses.
  `9890c733` consumes these fields in KDE and closes its Stage 2 offline. The
  macOS compatibility edits already stage view pre/post and member post from raw
  Space reads; member pre and observation-time view baseline remain unknown until
  the native executor cutover captures them. Consume this delivery directly;
  native refusal/supersession proof is no longer blocked by absent shared fields.

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

## Decisions and review status

User review 2026-10-25 confirms the implementation decisions below, retires the
two superseded interim choices, and leaves classification and `None` membership
edge cases tentative. User review 2026-10-27 confirms the four 2026-10-27 rows
below (event-wake as interim with the approved direct-wake next step) and leaves
the older three-way classifier and `None`-membership rows tentative.
Acceptance of an approach does not claim native evidence.

| Status | Decision | Alternatives / reason |
| --- | --- | --- |
| Confirmed 2026-10-25 | Atomic compiling v2 boundary replacement, including drag oracle/harness | No shim, private re-export or parallel route |
| Confirmed 2026-10-25 | Reuse sequencer and native thread/proof ownership | Preserve mechanical watermark, AX isolation and spend-before-dispatch gates |
| Confirmed 2026-10-25 | CG/AX logical points with nearest whole-point rounding | One coordinate space; no invented DPI; fixture/hint conversion stays explicit |
| Confirmed 2026-10-25 | Existing shared policy entry points first | No copied native policy or speculative richer ingress |
| Confirmed 2026-10-25 | Missing native proof means inability/hold | Dispatch and desired-state echoes are not arrival proof |
| Retired, superseded | Interim app-hidden A omits the window | Selected option B reports minimized with lifetime/close retention |
| Confirmed 2026-10-25 | Skipped outputs remain incomplete screen rows | Transient unreadability is not display removal |
| Confirmed 2026-10-25 | Current-reply binding/lifetime lookup | No private inspection or durable adapter binding table; automatic echo is retired separately |
| Confirmed 2026-10-25 | No observation-only retry for gesture terminals | Stale reply hides preview; never replay a commit |
| Confirmed 2026-10-25 | Remove vacuous per-predicate conformance pairs | Keep real classifier comparisons and fixture expectations |
| Retired, superseded | Keep automatic-float echo until D2 | Core D2 correction and separate adapter echo removal have landed |
| Confirmed 2026-10-25 | Never guess output affinity | Require uniquely grounded FULL-screen overlap; ambiguous output holds |
| Confirmed 2026-10-25 | Missing AX with live/unreadable CG retains and holds | Only covering AX absence plus fresh complete CG absence proves close |
| Tentative | Three-way shared classifier A: tile, float member, unmanaged | One neutral transient fact; Windows Dialog/OwnedDialog exclusions unchanged |
| Tentative | Fullscreen, all-Spaces and unreadable membership use `None` | No guessed home or sticky proof; shared edge tests and native scope evidence remain pending |
| Confirmed 2026-10-26 | Post-arm cancellation/MC-exit timeout is `Threw`, not `Refused` | A native append already occurred; an unproven later selection must hold, not claim no dispatch |
| Confirmed 2026-10-26 | Topology drift after select dispatch retains the existing `Accepted`/unproven hold | Correct the stale refusal test, not production; no replay or history advance is inferred |
| Confirmed 2026-10-26 | Selection focus defers in the adapter until verified native arrival | Require correlated reply/transfer/binding and grounded focus priors; cancel on supersession or changed/unknown focus; no shared settlement-time change |
| Confirmed 2026-10-26 | Ambiguous trailing creation retains its genuine pre-order and original stamp | First later readable matching-display row completes once without a claimed identity; never echo the pre-order as an unreadable post-order |
| Confirmed 2026-10-26 | Extend the existing menu-bar indicator with a miniature band | Reuse its visibility and freshness/click gates instead of introducing another pager surface; miniature-only repaints retain the click generation |
| Confirmed 2026-10-26 | Independent focused-miniature colour/theme with full-strength border and dimmer same-RGB fill | Border and highlight each default to live accent and retain independent overrides; no shared colour fields |
| Confirmed 2026-10-26 | Normalize grounded hidden-workspace and zoom/full-display facts before core presentation | Reuse retained homing, fresh native currents and existing geometry predicates; unknown cloak and session state retain last-moment renderer vetoes |
| Confirmed 2026-10-26 | Retain Authentic Desktop-N holder ensure even with input off; no new gating | User decision: qualify tile help, save exact per-holder pre/post baselines on the next live leg, and never revert a pre-existing preimage |
| Confirmed 2026-10-27 | Matched numbered-send unbound backing executes through the existing creator; an explicit backing desire requires exactly one append, never a silently reused SID; bound/missing/mismatched stays pinned with no creation | Adapter contract completion, not a core policy or new eager-spare proposal; native macOS result TBD |
| Confirmed interim 2026-10-27 | Coalesced claim/effect events stage one early cadence body on the existing 50 ms pump; every dispatched terminal outcome requests one owner reread plus one early body even at unchanged revision | Current pump stays now; selected next work measures native before/after then replaces it with a direct event-driven main-thread completion wake; no measured native distribution claimed |
| Confirmed 2026-10-27 | Same-state held plan-truth retention (same current scopes, bindings, homing, and current workareas; fresh rows win; minimized absence stands; any change invalidates) plus a queued baseline-SID source fence before select/create auth spend (genuine pre/post evidence, `None` when unknown); presentation is already engine-stable, so no cache; own-append order growth at the same current stays valid | Adapter-only retention and fences; renderers still revalidate every shape against live native legs; native macOS before/after result TBD |
| Confirmed 2026-10-27 | Host-derived active-border centerline default (native base 16 on Tahoe 26+ / legacy 9 source fallback + `gap + width / 2`, capped at 64; explicit values verbatim, host-local, no core) | Standard-window fallback is approximate, not universal; fractional outsets keep pre-existing whole-point rounding; native evidence is the narrow fixture readback below, not product border pixels |

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
  was still draft at this checkpoint. Update from `9f0cfe22`: KDE's
  [B2 source-view discriminator](kde-world-v2-port.md#b2-source-view-rewind-during-migration-parked)
  now has a tentative released-source/fresh-adoption decision, also applicable
  to macOS; no dormant return binding or reverse provenance is intended.
  Implementation and contradictory retention-oracle alignment remain blocked.
  Neither is solved by inventing logical receipts or a host binding table. Reuse the
  [migration rewind row](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  and the native refusal/delayed-arrival row below the macOS workspace heading.
- Options: A, retain unknown attempt fields plus fresh native observations and
  mechanical arrival checks, leaving refusal/supersession acceptance open;
  B, consume the coordinated neutral native-attempt delivery before completing
  those executor discriminators (recommended). No additional shared change or
  adapter settlement workaround is selected here. Reply-driven targeting can
  be separated from this evidence limitation, but cannot close stage 3 alone.
- Superseding update: option B is selected and the shared dependency has landed
  in `c76ba5f1`, paired with KDE `9890c733` (see dependency status above). Capture
  native member pre/post and view pre/post/observation baseline directly, then
  prove async arrival, refusal and supersession through production executors.

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

## Reply-driven executor and presentation draft (2026-10-26)

- The current implementation draft removes local selection history/ordinal
  targeting and admission-time numbered send targets. Ordinary selections use
  core output resolution; sends use `HomeThenCachedLive`. Reply bindings, native
  handle lookup, observation-time view baselines and native member pre/post now
  ride the executor seams. No shared source changed.
- Positive previous-toggle tests now incorporate native arrival before flipping
  back and forth. Stale replies retain correlation without an actuatable plan;
  duplicate creation claims spend once. The diagnostic select batch uses the
  world engine rather than retaining its own history/targeting policy.
- Bound-target send execution separates actual setter disposition from arrival
  verdict: queued native success stays `Accepted` on delayed/unreadable arrival,
  pre-dispatch refusal is `Refused`, and actual partial/error dispatch is `Threw`.
  The owner captures native member/current reads; the follow-view operation id
  comes from a published transfer, not a constructed suffix. Executor-to-report
  joins cover readable refusal, delayed/unreadable arrival, replay and lifetime
  fences. This does not close the whole native settlement plan.
- Review rejects the selection/creation draft despite green tests:
  `SelectionBackend::select/create` still returns `Result<(), String>`, erasing
  successful dispatch versus pre-call refusal. The drain converts generic errors
  to `Threw`. Creation failure/unreadable paths also substitute the before-order
  (or empty vectors) for an unobserved after-order. Neither is valid raw evidence;
  the test asserting repeated-before is not an acceptance oracle.
- Required correction: carry typed dispatch truth from the actual native touch;
  retain an attempted creation with its real preimage and original stamp until
  the first readable full post-order. Never stage a guessed after-order, and
  never replay the create while awaiting that read. Join refusal, delayed arrival
  and supersession through the actual report/core path, not only a native latch.
- Trailing sends with an unbound target and an explicit creation desire remain
  blocked on that corrected creation seam. Their draft refusal is not accepted
  product behavior and is not the historical first-report `send-target-unknown`
  discriminator. Desires without bindings must be executable before cutover;
  bindingless rows without desires must still never create. Removal/pin and
  full native lifecycle acceptance remain open.
- The presentation draft consumes the reply's border geometry and carries
  configured whole-point gap/width through report and reply. Live accent remains
  native, with fresh focus/lifetime/session/Space vetoes. Review caught and
  corrected default-only style outsets, fullscreen-as-game pausing, and loss of
  borderless-full-display/zoom/exclusion fences. Non-default style has a real
  report -> core -> renderer test. Eligibility normalization remains partial;
  no Stage-4 completion is claimed.
- Abstraction-fit item: macOS has no grounded maximization bit in this path;
  the existing work-area zoom approximation and native exclusions still veto at
  render time, while report flags remain incomplete. Options: move grounded
  publisher facts into presentation ingress without changing core, or review a
  neutral unknown-state ingress if existing facts cannot express the hold.
  Prefer proving the first option before proposing a shared change. Reuse the
  [floating-zoom discriminator](../spec/reference-outcomes.md#macos-active-border-discriminators).
  No new core field or maximization product outcome is selected.
- No positively grounded game classification is available; ordinary native
  fullscreen must not masquerade as the game's `paused` lane. Native fullscreen
  and full-display safety vetoes remain; full game-lane acceptance is unproven.
- Shared-source update through `fc2a1dbe` adds remembered-workspace focus on
  actual view changes and restricts pager focus marks to the current workspace.
  The macOS focus dispatcher still admits explicit focus only for Focus tickets,
  discarding the new correlated select/switch effects. This is an uncovered
  integration gap, not a passing focus-restore result. Before merely broadening
  that filter, test asynchronous native selection: the requested window may
  still be off-Space when the one-shot effect is published. Options are a
  correlation-bound native pending effect released after proven arrival, or
  a neutral core settlement-time effect if the native approach cannot preserve
  cancellation/freshness. Prefer the native discriminator first; no shared
  change is selected. Never reapply after a newer switch or user focus change.
- Pager miniatures have no macOS painter; the existing menu-bar indicator is
  Space-level. Independent highlight overrides/full-strength border/dim same-RGB
  fill remain unfinished. No no-op public flags or unused painter scaffold is
  retained. UI options are an extension of the existing indicator versus a new
  pager carrier; prefer the existing indicator surface, subject to a product
  decision. Underlay trigger/carrier work remains parked; active drag preview
  continues consuming the core gesture result.
- Final offline draft gates after consuming shared source through `fc2a1dbe`:
  locked macOS/core/protocol build PASS; macOS full
  tests PASS (1,335: 1,187 library + 148 integration); core/protocol full tests
  PASS (1,314: 1,283 core + 31 protocol); strict all-target clippy for all three PASS; workspace fmt,
  whitespace and ordered acceptance 6/6 PASS. Earlier bounded gate invocations
  timed out between test binaries; sequential reruns with a larger bound passed.
  These gates do not waive the identified raw-evidence defects. No live tests,
  signed-bundle acceptance, cross-target or Windows gates claimed here.
- Exact next action: repair the native dispatch/creation evidence seam, then
  consume it for trailing sends and finish the executor/core discriminators
  before accepting the atomic targeting cutover. Prove the newly published
  selection-focus effect survives native arrival without fighting newer input.
  Keep pager/underlay/game-lane
  and sticky/native hide-reveal acceptance explicitly incomplete.

## Reviewed targeting and receipt cutover (2026-10-26)

- Supersedes the dispatch/creation rejection and selection-focus gap in the
  draft above. No core/protocol source changed. Native acceptance is separate.
- Select/create errors preserve typed native setter truth. Unreadable or
  unchanged post-orders retain a pending receipt with the real preimage and
  original attempt stamp; disagreeing readable rows report `Threw` with the
  genuine after-image. No guessed empty/before-echo post-order is emitted.
- Trailing sends consume the creation desire. Verified appends stage their
  receipt on ordinary and shutdown settlement; ambiguous creation is observed
  later through the real send-controller seam. Wrong-display reads retain it;
  first qualifying readable post-order completes once with `created: None`.
  Reuse never fabricates a creation. Native creation is never replayed.
- Selection `fx-<seq>` effects bind only to a real correlated View transfer,
  live reply Space binding and grounded focus/foreground priors. Matching
  verified native arrival releases once. Refusal, supersession, unknown or
  changed foreground, lifetime loss and full outbox drop the intent. The worker
  re-reads agreed native focus before writing: target already focused skips;
  unchanged prior permits; another same-app window or unknown evidence drops.
  Explicit focus retains its existing path. No main-thread AX read was added.
- Independent review found and closed a missing positive real-dispatch bind
  test and unknown-foreground permission. The positive test includes genuine
  away dispatch, raw ViewAttempt settlement and return binding. Refusal,
  delayed/unreadable arrival, latest-wins supersession, no replay and retained
  removal-pin suites pass. Mechanical formatting normalizes the earlier draft.
- Final gates: locked macOS build PASS; macOS tests PASS (1,353: 1,205 library
  plus 148 integration); strict all-target clippy PASS; workspace fmt and diff
  whitespace PASS. Unchanged shared baseline: core/protocol locked build/tests
  (1,314), strict clippy and ordered acceptance 6/6 PASS. Independent review
  repeated the 1,205 library tests, correction tests and integration spot checks.
- Unsupported: trailing-append remembered focus has no pre-append binding and
  drops safely; live post-switch focus-read availability and native arrival
  acceptance remain TBD. A final native read-to-write race is not claimed solved
  by offline tests. Shutdown abandons deferred focus without replay.
- Exact next action: finish the production pager carrier and presentation
  eligibility, then run signed-fixture sticky-scope and native hide/reveal legs.

## Reviewed production presentation (2026-10-26)

- Accepted offline: the existing menu-bar indicator consumes the last applied
  reply's pager content and native bindings. Miniatures are drawn by a native
  strip below the number buttons, without replacing their click targets. The
  existing indicator mode controls visibility. Miniature/accent-only repaints
  preserve click generation; topology and target freshness gates remain.
- `pager.rs` maps only current reply bindings, scales real source rectangles and
  produces drawable rows consumed by `IndicatorGuard::tick` and the native
  strip. Unmapped/unbound targets paint nothing. Focused miniatures require
  both the focused-workspace mark and exact foreground token. Their border is
  full strength, fill dimmer with the same RGB. Separate highlight colour/theme
  settings are consumed by the runtime; active border and highlight independently
  resolve the live accent. No disconnected painter or no-op flags remain.
- Presentation ingress now grounds hidden-workspace eligibility from retained
  home plus fresh native current and zoom/full-display eligibility from existing
  geometry predicates, respecting engine-placed tiling. Minimized/fullscreen
  facts remain distinct. Ordinary fullscreen is not a game classification.
  Renderer session/lifetime/focus/current-Space/exclusion vetoes remain additive.
  No new core facts or unknown-state semantics were introduced.
- Independent review verified actual tick/native drawing wiring, colour
  independence, applied-reply publication, native binding mapping, view lifecycle
  and click fencing. The first disconnected paint-model approach was rejected
  and replaced by the production carrier; no completion evidence uses it.
- Final cumulative gates: locked macOS build/tests PASS (1,375: 1,227 library
  plus 148 integration); strict all-target clippy PASS; locked core/protocol
  build/tests PASS (1,314), strict clippy PASS; workspace fmt, whitespace and
  ordered world acceptance 6/6 PASS. Platform recipes expand to these commands;
  `just fmt` also PASS. No core/protocol or shared-fixture diff.
- Remaining unsupported presentation: native miniature pixels, physical
  interaction and accent-change timing; native stacking order and retained
  float/tile miniature frames are not supplied (core fallback/live-only frames).
  Cloaked has no ingress roster fact; session state is main-thread confined.
  Renderer vetoes remain, not a claim that unknown states are grounded false.
  Full Stage-4/native acceptance is not claimed.

## Current-artifact native checkpoint (2026-10-26)

- Earlier app self-hide/show and runtime results from the older signed product
  bundle are historical, not evidence for this tree or native Space away/return.
- Rebuilt product and signed regular-activation fixture retain their persistent
  identities. Static product/helper/fixture verification passes with deployment
  floor 15.0. Verification logs were saved after the live leg: artifact digests
  match those observed for the rebuilt bundles, with unchanged artifacts and
  signing timestamps preceding the leg. This is continuity evidence, not a
  contemporaneously saved build log. Development-signature distribution-policy
  rejection is not misreported as a distribution acceptance.
- The current signed runtime ran bounded, fixture-only, input off, with an
  explicit pager-highlight override accepted. Saved native observations cover
  two fixture windows tiled on home, selection to an existing empty Space, and
  return. Away: native current changed, fixture AX inventory was empty while
  the app was not hidden, foreground was not the fixture, and fixture socket
  frames stayed unchanged. Return: native identity/frame/membership/foreground
  readback exactly matches the tiled home readback. Socket frames show no net
  hidden drift; they do not prove zero intervening writes.
- Saved evidence: `scope-tiled.json`, `scope-away.json`, `scope-back.json`,
  `fx-tiled.json`, `fx-away.json`, `fx-back.json`, `spaces-before.json`,
  `spaces-away.json`, `spaces-back.json`, `spaces-after.json`,
  `select-away.json`, `select-back.json`, `tile-outcome.json`, and
  `cg-before.json`/`cg-after.json`. Raw records remain outside the repository.
  The selector's stale `layout_after.current` is not used as arrival evidence;
  independent native current readbacks establish both transitions.
- Runtime reports clean shutdown, three topology updates, zero observation
  failures and zero forwarded input. Saved desktop topology/current restore
  exactly; common protected-window bounds do not move. Auxiliary foreign CG
  entries churn, so the full CG inventory is not claimed identical. Process
  absence/owned run-directory cleanup were observed but not saved independently.
- Miniature appearance is unproven: indicator update counts establish execution,
  not painted pixels or effective override colour. Accent-setting reads were not
  saved, so exact setting restoration is not certified by this checkpoint.
- Sticky per-display plan remains blocked: no second output or native GUI
  control was available. Keep sticky scope TBD and require positive proof; a
  single-output leg cannot discriminate per-display All Desktops semantics.
- Unexpected startup behavior: Authentic ensures Desktop-N shortcut holders even
  with input off. The log reports nine applied/resumed results using an existing
  preimage. Source skips writes when the current value already equals the applied
  value, so this count proves neither new writes nor zero writes. No per-holder
  pre/post baseline was saved. The existing preimage was not reverted. The tile
  help's blanket no-native-setting-writes claim was inconsistent with this path;
  it is corrected following the confirmed decision above.
- This is reviewed, scoped hide/reveal observation, not full native acceptance.
  Signed-fixture visual, physical-input, sticky and complete settings/lifecycle
  restoration gates remain open.

## Saved-baseline native checkpoint and state stream (2026-10-26)

- Supersedes the missing-baseline limits of the preceding checkpoint. Current
  product/fixture signed builds, digests, persistent identity, deployment floor,
  bundle-attributed preflight and fixture LaunchServices identity are saved
  contemporaneously. Fixture-only runtime, input off; no pre-existing preimage
  was reverted and no global accent preference was changed.
- Independent review verified exact read-only holder status before startup,
  after startup, away, return and teardown. All nine Desktop-N holders retain
  their original enabled state. Complete native binding snapshots match before
  and after. The existing preimage's saved SHA, mtime, size and permissions match
  before and after; the first post-file check was captured after teardown, and
  the supplemental leg also saves its matching post-file check in-leg. Startup
  resumption leaves the measured native shortcut state unchanged.
- Accent keys retain the same saved absence/error-status before and after.
  Owned process inventories are empty before launch and after teardown; fixture
  shutdown and run-directory removal are saved for the baseline leg. Native
  desktop order/current and Mission Control state restore. Common protected
  CG windows retain bounds/layer/visibility; only same-owner offscreen auxiliary
  IDs churn. No whole-desktop inventory identity claim is made.
- Home/away/return observations join exact fixture lifetimes and socket frames
  to native AX/CG, membership, current and foreground. Away hides fixture AX
  windows without hiding the app; independent native current names the other
  existing Space. Three timed hidden polls retain exact frames. Return native
  identity/frame/membership/foreground equals tiled home. Clean runtime shutdown
  is established by outcome payload and absent owned processes, not wrapper
  exit status.
- A supplemental leg uses the documented fixture `subscribe`/`send --follow`
  state stream, with driver receive timestamps, continuous through tiling,
  native away, return and shutdown EOF. Twelve State events include four
  geometry deltas during initial native tiling, establishing host-notification
  coverage for both windows without any socket `set`. Away/return produce only
  repeated geometry and focus handoffs: zero geometry deltas across the marked
  hidden/transition interval. Native current/AX/CG and socket observations
  corroborate the interval and exact returned geometry.
- This narrows the write evidence to no observed geometry change, not zero AX
  setter attempts. The tiler's initial layout does perform AX writes. No-op
  setters and compositor changes without delivered window notifications are not
  observable through the fixture state stream; polling alone also misses
  between-poll changes. Absolute zero-interim-write proof remains unsupported.
- Accepted evidence files include `holder-PRE.json`, `holder-STARTUP.json`,
  `holder-AWAY.json`, `holder-RETURN.json`, `holder-FINAL.json`,
  `bindings-PRE.json`/`bindings-POST.json`, `preimage-PRE-sha.txt`/
  `preimage-POST-sha.txt`, matching stat files, `accent-PRE.log`/
  `accent-POST.log`, `process-PRE.txt`/`process-POST.txt`, native Space/scope/CG
  and fixture frame readbacks, `tile-outcome.json`, signing/preflight logs,
  and supplemental `stream-events.log`, `stream-meta.txt`, `hide-interval.txt`.
  Private raw artifacts stay outside the repository and are not published.
- Noted variances: the indicator's destination occupancy scan changes although
  native CG shows no destination window and only shell chrome is enumerated;
  this is not owned residue or evidence of a product write. The supplement has
  a redundant acknowledgement-only subscription not used for geometry proof,
  sparser native polling and no separate run-directory absence file. The full
  baseline leg supplies lifecycle restoration evidence. No physical-input,
  pager-pixel, sticky, game or broad port acceptance follows from either leg.
- Help correction gates: locked macOS build and full tests PASS (1,376: 1,228
  library plus 148 integration), strict all-target clippy, package fmt and diff
  whitespace PASS. The live legs change no source; both native evidence sets
  received independent offline review.

### Remaining bounded decisions and evidence

| Item | Options | Recommendation / next evidence |
| --- | --- | --- |
| Authentic startup holder behavior with input off | Confirmed above: retain preset behavior, no gating | Help corrected and regression-tested; saved native per-holder/binding and preimage pre/post baselines match. A pre-existing preimage must not be reverted |
| Sticky per-display scope | Repeat the existing plan with two outputs and GUI control; or leave native scope unsupported | Keep sticky unproven and repeat the two-output plan when available; do not infer scope from membership count |
| Pager pixels and live theme appearance | User-assisted visual contrast; or a separately consented bounded capture | User-assisted visual check first; no global accent change or capture-grant reset during this checkpoint |
| Interim hidden setter attempts | Existing native/fixture geometry-event evidence; or add bounded adapter setter-attempt diagnostics | Accept the observed geometry/restoration checkpoint, not absolute zero-write proof; direct setter-attempt tracing is needed for the stronger claim |
| Trailing-append remembered focus | Keep fail-closed unsupported binding; or investigate receipt-bound binding, with shared settlement emission considered separately | Keep the safe drop for this cutover; prove any later adapter receipt-bound path before requesting a shared change |
| Presentation roster gaps | Retain renderer vetoes/core fallback/live-only frames; or plumb grounded native cloak/order/retained-frame evidence | Keep current safe boundaries; expand only with a failing discriminator and grounded native data |

## Current dogfood follow-up: workspace failure diagnosis and acceptance (2026-10-26)

- Intent: diagnose the current dogfood workspace failure, then prove the
  repair with fake regressions plus adapter coverage, latency before/after
  distributions, event-triggered topology and ambiguous-create readback with
  bounded observed-state recovery (no replay, no guessing), stable
  presentation and stale-topology command supersession.
- Method: reuse the shared platform-neutral stress definitions and native
  runner with configurable delays between actions (including none), then query
  real native state and compare against the expected final world (layout,
  membership, focus, shown workspace, pager). Dev legs run under `just dev`;
  the dev script prints log paths after SIGINT/stop for the post-leg evidence.
  Presentation acceptance uses the researched host corner-radius values.
- Acceptance: workspace actions converge to the expected final native state
  with no wedge or silent drift; unsupported native-fullscreen game workspace
  semantics stay investigation/options only, never acceptance criteria.
- Diagnosis status explicitly unresolved: current logs establish only later
  immediate `send-target-unknown` / scope-held / unchanged refusals rather
  than a queue wedge; the missing requested ordinal and per-scope context
  prevents causal attribution. No tentative implementation or product decision
  is made here.
- Source review confirms the accepted native creation seam is already implemented;
  missing-seam diagnosis is unsupported.

## Off-current native-fact recovery (2026-10-26)

- Established production-seam cause: AX partial/full-empty omissions hide an
  off-current member while positive CG still owns it; the native home is then
  lost/unknown and the off-current scope holds, so the later send is fenced
  `scope-held`. No core change.
- Fix (macOS only): the owning per-app worker attaches fresh SLS singleton
  membership plus CG liveness (layer/onscreen/integer bounds) for AX-absent
  CG-owner wids alongside the AX legs; the roster re-homes only a retained
  live lifetime onto a hidden (non-current) domain on an exact singleton plus
  live frame. Ambiguous (multi/unhomed/shared-SID), current-target,
  degenerate-frame, missing-entry and unreadable details drop the stale home
  and retain the hold until fresh AX; per-PID clearing uses the complete CG
  oracle only.
- Diagnostics: rejected sends log one bounded metadata-only `send refused`
  line (subject/index/follow/source, observation/request/command seq,
  held-scope detail); per-scope scope-held stderr spam is removed. No titles,
  content, or paths.
- Review corrections accepted: an incomplete-helper test asserting a false
  branch was replaced with the true branch; held-scope ring tokens kept.
  Causal repair only.
- Original-run note: the exact requested ordinal and per-scope context for the
  original dogfood run remain missing, so that run is not claimed proven; the
  `unknown`/`unchanged` refusal vocabulary is retained for those cases. This
  workspace part is complete OFFLINE; live native proof remains pending.
- Gates: locked macOS build PASS; macOS tests PASS (1,390: 1,242 library plus
  148 integration, 0 failed); strict all-target clippy PASS; workspace fmt and
  diff check PASS. No shared-source change, no live acceptance claimed.

## Numbered send backing desire via the existing creation executor (confirmed 2026-10-27)

- Accepted core behavior: a numbered (`follow 0..=9`) transfer whose current
  correlated reply carries an explicit creation desire still arrives with no
  native SID for the unbound follow target. Previously the macOS adapter kept
  that claim on the pinned numbered lane and refused (missing bindings never
  create); the desire-gated native creation seam existed but the numbered-send
  drain never entered it.
- Adapter completion (macOS only, no core change): a matched correlated
  unbound follow (`0..=9`, no `expected_sid`, all three desire legs present
  and naming the retained target logical/output) routes through the existing
  exact-append executor; bound targets stay pinned, and a missing/mismatched
  desire still refuses with no native creation. The original public digit is
  preserved for diagnostics (never forced to 0, never an invented desire).
- Explicit creation requires exactly one append even when the prior last
  proves empty: the core already resolved occupancy for a different unbound
  logical, so reusing the emptied native would retarget the wrong logical and
  stage no creation receipt. True no-desire and bound-target sends keep the
  usual reuse-or-append behavior unchanged.
- Third-spare note: the third logical spare exists without an eager physical
  desktop append by the existing contract; the plus indicator appears from
  retained occupancy. This change backs only explicitly desired sends, and is
  not a core policy change or a new eager-spare proposal.
- Offline regression (new): owner append-once genuine order/identity/stamp/
  member settlement plus a later send to the already-existing target; route
  and display-leg matrices; stale-focus, stale-topology, no-desire, and
  empty-last refusal cases. This part is complete OFFLINE; live native proof
  remains pending.

## Event-wake latency, held-geometry retention, queued-source fence (confirmed interim 2026-10-27)

- Cause: the 250 ms cadence owned claim drain and settled-completion
  reconcile, so both waited out the tick. Original dogfood timings: op-30
  reconcile wait 256 ms, input-to-reconcile 423 ms, native Dock 43 ms (raw
  logs private, not recorded here).
- Fix (macOS only, no core change, no new thread or timer): the engine
  claim store and both effect owners stage a coalesced wake flag, checked
  on the existing fixed 50 ms pump to run the cadence body early. This is
  a theoretical bound change of 250 to 50 ms per stage, NOT a measured
  native distribution; live/native before-after remains pending.
- Every dispatched terminal outcome (verified arrival, accepted-with-drift,
  unproven deadline, `Threw`, proven refusal) requests one whole-owner
  reread plus exactly one early body even at an unchanged revision; the
  terminal itself stands, nothing is replayed natively. Pending creation
  receipts complete exactly once on the first readable matching row with
  the original stamp preserved and no claimed identity. The existing
  250 ms routine and 1 s `SCOPED_BACKSTOP_INTERVAL` bound silence for
  lost/unsupported notifications.
- Same-state held plan-truth retention (adapter only): a same-view hold
  (same current scopes, workspace bindings, window homing, and current
  workareas) preserves the last applied authorized desired geometry
  instead of publishing empty plan truth, then refreshes once on
  observed recovery. Fresh rows always win; minimized absence stands; any
  view, binding, lifetime, or workarea change invalidates. Native frames
  were already stable across holds (holds forbid setters) and the engine
  presentation is already stable across holds (read-only over retained
  state), so neither takes a cache.
- Queued-source fence (adapter only): a tap claim whose observation-time
  baseline SID no longer names the live current refuses before any select
  effect or creation auth spend, with genuinely re-read pre/post evidence
  (`None` when unknown). Own-append order growth at the same current keeps
  the binding valid and still executes exactly once.
- Review corrections closed: reviewer-confirmed Worker additions (baseline
  fence on both select legs before auth spend; workareas in the retention
  key) plus two mechanical strict-clippy fixes with no behavior change
  (`filter_map`-to-`map`, test-helper type alias).
- Gates on this cut: locked macOS build PASS; locked tests PASS (47
  suites, 2,842 passed, 0 failed across core/protocol/macos; macOS-only
  re-run after the lint fixes: 14 suites, 1,417 passed, 0 failed); strict
  all-target clippy PASS; `fmt --all --check` PASS; no shared-source
  change. This part is complete OFFLINE; live native proof remains
  pending.

## Dev-script end log paths and parked radius research (2026-10-27)

- Dev script (`scripts/macos-dev.sh`): every terminal path now prints the
  launch log locations. `report_log_paths` emits `dev: end paths:` with the
  stdout/stderr log pointers; `die()` prints the error first, then the
  pointers (pre-guard failures have no logs and print the error alone); the
  pre-launch SIGINT/stop path reuses the same helper. Lock/run teardown is
  unchanged. Offline tests PASS (104 asserts, 0 failures in
  `scripts/macos-dev.test.sh`); `bash -n` clean on both scripts.
- Radius research parked with no implementation choice at that time: the existing default
  6 was unchanged and 9 was not blessed. Independent re-read of the JankyBorders
  sibling clone corrects the earlier quote: the fallback is
  `radius > 0 ? radius : 9`, not `radius > 0 ? 9 : 9`. Pins inspected
  (sibling clone, not this repository's history):
  `1dcf3477e1ed89eaf1a0c1ced3c6b15dcb2ff896` (per-window read) and
  `3a9d75cd1f7f36d2f3ef303e98f9eb781f5da423`. A 9 fallback does not establish
  a native 26 default; the per-window API is 26+ and private SLS reads are
  permitted with SIP on. No core/config edits at that time. Resolved
  tentatively by the host-derived default below; this paragraph is now
  historical.

## Host-derived corner default (confirmed 2026-10-27)

- Confirmed default (macOS only, no core change): unset
  `--active-border-radius` derives as native base + `gap + width / 2`
  (centerline outset, so the ring stays concentric with the window corner),
  capped at the configured radius maximum 64. The cap reflects the
  configured radius range; it never refuses the width/gap config.
  Explicit radii apply verbatim on every family; explicit 64 accepted and
  `>64` refused are unchanged. Resolution order is CLI over file over host
  default, after width/gap resolve; the OS read is the existing
  `macos_major` with the portable `None` stub.
- Bases: 16 on Tahoe release family 26+ (narrow fixture readback: one
  normal decorated resizable window, SLS read of 4 corners at 16 twice;
  borderless reads zero, recorded as UNKNOWN not zero); 9 on older/unknown
  as the source-history fallback from the sibling JankyBorders pins above
  (not a modern actual; no live pre-26 measurement in this tree). At the
  default width 3 gap 0 this gives 17.5 on 26+ and 10.5 older/unknown; the
  portable `None` route (10.5) matches `ActiveBorderStyle::default`, which
  the diagnostic `border_session` fixture style consumes explicitly and
  unchanged. Permitted width 32 gap 64 auto-derives 64 on 26+ and unknown.
- Approximate standard-window fallback, not universal; fractional outsets
  keep the pre-existing nearest-whole-point rounding at presentation, no
  architecture change. No product border-pixel claim and no pre-26 native
  actual measurement; raw probe artifacts are not retained (the measurement
  report notes the probe/run directory was removed). Own
  process/window/socket plus caffeinate cleanup and protected-baseline
  restore are confirmed for the readback leg.

## Dogfood review directions 2026-10-27

- User review 2026-10-27 (durable product decisions live in
  [decisions](../decisions.md#macos-dogfood-review-decisions-2026-10-27)):
  T1 numbered-send backing desire, interim T2 coalesced pump with the approved
  direct-wake latency unit as selected next work, T3 held geometry plus
  queued-source fence, and T4 host-derived corner default are confirmed;
  native acceptance for each remains pending.
- Lazy physical backing is kept (create on first switch/send, no eager
  Mission Control append); follow up by verifying the pager surfaces the core
  trailing workspace. Fullscreen/game direction (never force-exit; fullscreen
  windows stay logical members with bring-forward/reassociate semantics) is
  platform design first and unproven, not completed behavior; current send
  refusal and home-desktop limits stand as evidence. Every user command reaches
  core in order with stale-topology supersession per T3; no busy-drop. macOS
  stress work is dev-only against the shared spec and oracles, native side
  first, no shared-format changes here.

## Fixture command harness checkpoint (2026-10-27)

- Native latency measurement first needs a development-only signed-fixture
  command transport through the real product tap. `fixture-leg` owns the
  complete lifecycle; standalone `fixture-post`/`fixture-focus` now refuse.
  Authenticated socket readback, signing-requirement checks, native caller
  joins and balanced release remain separate from the parked stress scaffold.
- Revised contract: verify signing identity at session bind, then retain
  live process and server-window continuity. Complete socket/AX inventory
  proof before the selected-window check and final session/target commit;
  only native continuity checks run between down and up. Loss after down
  always attempts the cleared-modifier release and reports release failure
  explicitly. Signing deadlines kill and reap the owned child before return.
- Offline review accepts this design for fixture-scoped correctness and
  repeated calls without intentional pacing. Native calls still cost time;
  no atomic input/host observation or zero wall-clock gap is claimed.
  Delivery must be joined to product tap counters and native effects, not
  inferred from a posting receipt. Same-run window continuity retains the
  server's opaque handle as well as caller/run-tag/native identity checks.
- Gates on the revised source: full macOS library tests 1,313/0 plus all
  integration targets; focused harness review tests 30/0 and CLI fixture
  parsing 1/0; strict all-target clippy and format checks. Earlier ordering
  failures were corrected before live use. No baseline, direct-wake result
  or ingress-queue acceptance is claimed yet.

### Preliminary native harness leg and restoration gap (2026-10-27)

- Signed-fixture exploratory input observed directional focus, move and
  select-away effects through the product tap. No distributions or native
  latency acceptance are claimed. The guard itself cost 62-81 ms before
  down and 8-18 ms before up in these single-shot observations; posting
  costs are separate and are not command-in-to-complete latency.
- The leg omitted the required protected-window and per-shortcut-holder
  preimages and did not prove an independent return path before selecting
  away. It used pre-existing desktops rather than run-owned desktops.
  Consequently, exact restoration and protected-state noninterference are
  not established, and these observations do not close native acceptance.
- Fixture windows and all run-owned processes were closed; native desktop
  identities/count were unchanged, but the display remained on the secondary
  desktop rather than its initial desktop. No automatic restoration from
  older test evidence is inferred. Further native work requires current
  restoration plus a newly proven return path and complete baselines.
- The signing-requirement parser included trailing `Executable=` output;
  extraction now accepts only the actual designated-requirement line in
  either stream order. No path-bearing workaround is required.

### Enforced lifecycle and native authority scope (2026-10-27)

- The controller persists protected-window, holder, accent, process and
  native topology/current baselines before setup. Unreadable categories
  refuse; resolved-accent conversion failure is not recorded as absence.
  The Compatible product launch avoids shortcut-holder/preimage writes.
- Each Space-mutating leg exercises a newly created owned desktop away and
  back before admitting commands. Distinct retained owned desktops supply
  command destinations. A macOS diagnostic actuator gate checks the actual
  reply-bound SID, refuses unowned destinations and creation before dispatch,
  and leaves releases unconditional. This is native test authority, not
  copied logical targeting or a shared/core contract change.
- Product and fixture launches use LaunchServices with exact run identity.
  Teardown stops the primary product first, independently restores current,
  closes/shuts down owned fixtures, removes only registered pinned empty
  desktops, reaps owned processes, then compares fresh baselines. Failures
  collect residue and retain the receipt; no later capture error skips safe
  cleanup. Pending launches cannot silently report a clean leg.
- Latest implementation gates: locked macOS tests 1,372 library plus 148
  integration, strict all-target clippy and workspace format check pass.
  Independent offline review accepted the enforcement and ran focused guard,
  lifecycle, probe, input, send and CLI suites. First native proof remains
  pending; no new live leg, latency distribution, direct wake or ordered
  ingress acceptance is claimed by this checkpoint.
- First native proof attempt (post-only, same day): the controller refused
  before any fixture launch because both bundle paths joined the process
  working directory, which a LaunchServices launch does not set to the
  repository. Zero fixture/product/window/Space/input effects resulted; the
  persisted baseline receipt plus pending-launch residue were retained for
  inspection. The resolvers are now executable-anchored (own
  `.app/Contents/MacOS` root plus bundle sibling, explicit environment
  override unchanged, fail-closed otherwise) with injected-layout regression
  tests. Native proof itself remains pending with no posted chords.

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
- Targeting/receipt cutover is accepted offline above. Presentation and native
  acceptance remain open. Do not invent shared shapes in the adapter.

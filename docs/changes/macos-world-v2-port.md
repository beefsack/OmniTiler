# macOS world-contract-v2 port

## Goal and status

- Stage 0 reviewed; stage 1 in progress. Stage 0 evidence is from `a0f54105`
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

## Staged plan and gates

| Stage | Ports and deletions | Exit gate |
| --- | --- | --- |
| 0. State and fit review | This inventory, ranked contract review, dependency and gate baseline; no implementation | Findings and staged proposal available together; unsupported semantics remain TBD |
| 1. Atomic strict-v2 compile cutover | Rewrite builder/envelope and command shapes; port runtime/dispatch to public world snapshot/results, raw attempt types and native binding interface; port active Meta-drag oracle/harness to world gesture reports; delete all retired engine/event/reply/report/feedback/binding routes and repair or delete cfg-leak callers | Locked macOS build and full tests, strict all-target macOS clippy, workspace fmt; core gate set below; search finds no retired private-boundary consumers. No shim or parallel route, including fixtures/tests |
| 2. Policy and fact ownership | Replace classifier policy with native normalization plus shared in-process policy; delete local select/send/trailing/drag targeting/history/settlement policy; retain input eligibility/capture, lifetime and native proofs. Exercise classification/hidden/minimized/incomplete rosters and active Meta drag; passive titlebar source stays parked | Fake-native regressions for incomplete/closed/minimized, hidden-app ambiguity, exact cursor hit, command targeting, gesture lifecycle/preview/commit, stale/generation replies, no duplicate effects; stage-1 gates. Any unrepresentable safety semantics require a reviewed neutral core proposal, not a host policy workaround |
| 3. Native workspace binding and settlement | Consume KDE adoption; publish native order/current via `native[]`, window membership as `observed_workspace`, views as independently verified logical receipts. Port native select/send/create/remove executors to desires and raw pre/post attempts; delete trailing proof policy and host membership authority | KDE dependency accepted; four occupied native Spaces -> four logical workspaces, same members and zero launch workspace moves; outside move/create coverage. Fake async/refusal/unreadable/supersession/no-replay/removal-pins tests; stage-1 gates. Native hide/reveal checkpoints required before claiming workspace acceptance |
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
- Remaining exact next step: complete the atomic stage-1 cutover against the
  delivered shared contract; keep any new core proposal separate.

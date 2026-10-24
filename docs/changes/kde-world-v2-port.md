# KDE world-contract v2 port

## Goal and current outcome

- Make KDE capture/observe/actuate/render over the sole shared `WorldEngine`;
  Windows is the migrated reference. No v1 compatibility, policy copies or
  second owner. [Principles](../principles.md#layering) and the
  [adapter guide](../porting/world-adapter-guide.md) are authoritative.
- Stage 0 complete against `a0f54105`; stage 1 parked on the workspace-count
  ambiguity below. No runtime changes or native acceptance yet. This record
  remains active for the whole port; stages 2 onward have not started.
- Confirmed the `01a73798` inventory still applies: JS pins version 1
  (`world-seq.ts`), `world-report.ts` emits the old report vocabulary,
  `world_service.rs::parse_discovery_request` requires v1, and
  `WorldBackend::fresh` references the removed
  `aligned_per_output_workspaces` config field. The effect's `updateBorder`
  still derives eligibility and bounds. Core publishes no native bindings.
- Native API checks use the sibling KWin clone at the notes' `v6.7.4` pin,
  never fetched source. The declaration header separately cites 6.7.5;
  pinned comparison confirms identical relied-on scripting APIs between the
  tags. Keep the documented 6.7.4 pin; declarations are not the API ceiling.

## Stages, outcomes and evidence

| Stage | Outcome and deletion boundary | Required evidence |
| --- | --- | --- |
| 0. Inventory and plan | This plan, current-tree discrepancies, explicit parked contracts | Source inspection and unchanged core baseline; complete |
| 1. Host-owned adoption | Capability-gated core native order/bindings and native member intake; preserve startup inference and adopt later outside moves/creations; no KDE policy workaround | Four native memberships preserved on a fresh generation, no member moves, stable bindings, outside move/create regressions, incomplete evidence holds; parked on total workspace count |
| 2. Broker and strict v2 cutover | Port Rust service and JS wire together; sole identity/sequencer, stream, request/observation namespace and monotonic clock; replace v1 discovery, stale config, wire vectors and legacy routes, never shim them | Protocol/service/JS paired tests; invalidation during flight, duplicates, lost replies, reset, reconnect; authorization separated for query/presentation/commands versus report/actuator; native work actuator-only, terminal results issuing-reply-only, no subscriber effect replay |
| 3. Observe and actuate | v2 facts/receipts, presentation ingress and operation tokens; preserve native identity/recovery and last-moment vetoes; delete old feedback/verdict and native-derived homing routes | Fresh post-dispatch facts, truthful attempts, token spending, generation/watermark refusal, transfer/clear/float settlement and no fabricated arrival |
| 4. Shared policy only | Broker capture envelope carries raw native role/control/ownership/app/hint facts to existing Rust core entry points; delete TS admission, born-maximized, fixed-size, refill, scope, drag/gesture, same-axis move and tiling-toggle policy | Core-policy normalization and real capture-envelope tests; shortcut/gesture inputs never preselect verdicts; no shared-ingress redesign implied |
| 5. Pure presentation and coordinates | Effect renders core border/underlay/drop/pager bounds; delete effect geometry/eligibility derivation and obsolete policy FFI routes; keep native anchoring and immediate hide-only safety vetoes | Rendering/token/freshness/game-pause tests and host-matched native build; one KWin-logical coordinate space with fractional rounding and style-unit conversion proof; add window clientGeometry reads for hint conversion (not workspace clientArea), never Windows border compensation |
| 6. Host integration | Cursor from workspace.cursorPos plus workspace.windowAt, no polling; warp unsupported, never reported applied; independent active-border and pager-focused-window colors, default system accent, full-strength border/dimmer fill | Declaration/source checks and adapter tests; independent overrides; native styling remains host-owned |
| 7. Workspace modes and investigations | Keep per-output-local default, global-unique separate, shared linked; equal_workspace_count is a host limit; required separate-mode warning when KWin per-output desktops are off, settings action if feasible | All-mode offline/native cases; labels "Displays have separate workspaces" / "Workspaces span displays"; Activities/multi-desktop and configure investigations below, no speculative core changes |
| 8. Native acceptance and ownership audit | Fixture-first integrated journeys and deletion audit; no logical-receipt-only success claims | Independent native hide/reveal checkpoints, frame/visibility/foreground joins, both-output cases, exact restoration, no stale v1 or policy route |

## Stage 1 implementation boundary

- `WindowFact` currently carries only logical `observed_workspace`; a fresh
  generation has no logical ids for the adapter to read back. Physical output
  and frame cannot establish which native desktop contains a window.
- Necessary implementation direction within approved adoption: optional,
  capability-gated native member evidence with opaque host ids, distinct from
  logical receipts; retain core-owned `(output, native id)` bindings and publish
  `WorkspaceDesire.native_binding` only under the capability. No adapter-minted
  logical ids or native ids disguised as logical receipts.
- Extend validation, digest, conservative size accounting and strict v2 codec
  together. Native-free Windows reports must retain current behavior. Reuse the
  existing startup layout inference and transfer/gesture shielding; native facts
  are observations, never a second policy owner or settlement verdict.
- Tests must use native-shaped ids/order and nontrivial window geometry:
  fresh/repeated/reset generations, per-output and linked views, outside moves
  and desktop creation, unknown/partial evidence, pending transfers and stable
  binding publication. Four occupied native desktops remain the membership
  discriminator; do not silently redefine its total-count assertion.

## Tentative technical decisions

- Sequence the atomic service/JS v2 wire cutover before deleting capture-policy
  consumers, then renderer/host integration and native acceptance. Each stage
  removes its superseded route before acceptance; this is not dual-route support.
- Use a KDE broker capture envelope feeding existing shared Rust predicates,
  not a new shared classification ingress. This implements the guide's remote
  transport boundary without changing core policy or deciding parked questions.

## Parked product and core-neutrality concerns

### P1. Four-total versus existing trailing lifecycle

- Approved adoption says four occupied native desktops give four workspaces.
  `workspace.rs::plan_trailing` appends when the final workspace is occupied;
  `world_model.rs::converge_logical_post` runs that policy on the first accepted
  report. Thus unchanged lifecycle produces four native-bound occupied
  workspaces plus a fifth trailing spare. Exact four-total needs an exception.
- Options: (A) exactly four total under host-owned adoption, requiring an
  explicit lifecycle exception; (B) four native-bound occupied workspaces plus
  ordinary trailing spare, clarifying that four-to-four describes membership
  adoption, not total rows. Recommendation: B preserves existing lifecycle.
  Neither interpretation is selected here. Stage 1 is parked, not implemented.
- Neutrality: Windows currently uses core-created trailing workspaces; KDE
  global desktop lists and macOS per-display Spaces must both preserve native
  membership without implicit lifecycle exceptions. GNOME dynamic workspaces
  and ChromeOS desks likewise need capability-defined composition, not
  platform-name branches. Clarify once for the capability.
- Minimal [discriminator](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs)
  records the ambiguity; native outcome remains TBD.

### N1. Native inventory scope and completeness

- Current `NativeObservation` is `(output, native_order, current_native)` with
  no native-list completeness field. Validation permits absent native rows,
  duplicate output rows and repeated native ids. Harmless inert observations
  become load-bearing when they bootstrap rings or supply membership evidence.
- Options: define complete per-output projections using existing completeness
  flags and reject contradictory duplicates, or add explicit neutral native
  inventory/domain completeness. Recommendation: first prove whether existing
  screen/inventory completeness is sufficient on KDE and macOS; park any new
  shared field until that evidence exists. Missing rows are not removal proof.
- Windows' native-free default must be unchanged; KDE can repeat the same
  global desktop ids across outputs; macOS may expose different lists per
  display; GNOME may have a global dynamic list; ChromeOS may expose global
  desks. Never globally deduplicate an opaque id or assume identical lists
  merely because the Windows default needs neither. Shared-view conflicting
  lists need a defined hold, not silent loss of native members.

### Existing deferred product boundaries

- Activities/multi-desktop: existing tentative default retains inactive-activity
  windows unmanaged with slots held; all-desktop maps to sticky; subsets float
  without rehome. Options: retain this default or propose a native-evidence-backed
  alternative; recommendation: investigate fixture-first and preserve core.
- Async configure versus application clamp: options are existing convergence or
  a new neutral pending/completion representation. Recommendation: reproduce
  natively first, inspect pinned KWin completion evidence, then propose only if
  needed. Windows staged readbacks do not establish native completion; macOS,
  GNOME and ChromeOS would need equally truthful host evidence. No new signal.
- Preserve the rearchitecture's parked empty-workspace cross-output focus,
  edge-nearest versus MRU focus, float physical-output versus membership-home
  direction, explicit versus directional send, and cross-output drag preview
  versus release placement. Options remain current behavior or later separately
  approved policy; recommendation: no port-side change.

## Verification and live acceptance

- Linux commands run through `devenv shell --impure --`; dependencies stay in
  `devenv.nix`. No dependency changes for stages 0/1.
- Every core-changing commit: locked core/protocol build and all-target tests,
  strict all-target clippy (`-- -D warnings`), `cargo fmt --all -- --check`, and
  `cargo run --locked -p tiler-protocol --example world_acceptance --
  test-fixtures/world-actions.json`. Include Windows reference regressions
  when shared report construction/semantics change. Inspect `git diff --check`.
- KDE cutover: locked `omnitiler` build/tests/strict all-target clippy;
  `npm --prefix kwin run typecheck`, `build`, `test`; replace v1 vectors rather
  than keeping them green through shims. Effect stages use
  `just build-native-effect` (host-matched static build, no plugin load) plus
  applicable CMake tests. These are future gates, not current passing claims.
- Live work follows [live-kwin-testing](../live-kwin-testing.md) and the
  [fixture contract](../../crates/tiler-live-fixture/README.md): fixture-only
  `OmniTilerTestWindow` scope, fresh baselines, exact controller readiness and
  helper accounting, correlated reports/replies and exact teardown/restoration.
  The current readiness guide names v1 discovery; update it with the cutover.
- Join caller id, native lifetime, output, operation and provenance freshly.
  Test select-away AND intermediate native hide before select-back AND reveal;
  independently read visibility, frame and foreground. Reject missing reads,
  pending operations, holds/omissions, coverage defects or skipped checkpoints.
  Logical receipts, setter success and signal delivery alone never prove this.
- Cover native startup memberships, outside desktop moves/creation, all three
  workspace modes, both-output sends/migration/refill, geometry/constraints,
  presentation freshness and game pause. Physical shortcut/drag/game claims
  remain separate from guarded synthetic fixture checks. No shared connector
  redesign is part of this port.

## Accepted evidence (2026-10-24)

- Unchanged-source targeted tests through devenv: 6 lifecycle tests plus 7
  individual bootstrap/admission/capability/settlement/validation/digest tests,
  13 passed, zero failed. Core-only world acceptance: 6/6 checkpoints.
- These establish the existing append rule and inert native capability, not
  delivered adoption or native hide/reveal. No source or live state changed.
- Independent source review accepted the plan and parked count ambiguity;
  fresh lifecycle/report/bootstrap tests passed 12/12 and world acceptance
  passed 6/6 checkpoints. Pinned KWin inspection corrected a prior notes
  conflation: current `readClientArea` reads workspace work area, not window
  `clientGeometry`; that read-only property exists and must be added for the
  port's hint conversion. `windowAt` and `cursorPos` exist at the pin.
- Full unchanged core/protocol baseline through devenv: locked build and
  all-target tests passed (1,244 tests, zero failed/ignored, 28 suites), strict
  all-target clippy passed, workspace formatting and whitespace passed, and
  world acceptance passed 6/6 checkpoints. KDE and native gates were not run.
- Next: clarify P1's total-count interpretation; then implement the isolated
  adoption change and address N1 without changing native-free behavior.

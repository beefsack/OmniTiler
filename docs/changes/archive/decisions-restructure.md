# Decision register restructure

## Scope and acceptance

- Restructure the current decision register by topic without changing product,
  architecture, contract or governance choices. Preserve active rules and every
  tentative marker; retain unclear supersessions as conflicts for user review.
- Use the approved entry format, move evidence and status to change records,
  retain pending work, and repair repository-wide decision-anchor links.
- Acceptance: old-section mapping, evidence destinations, supersession audit,
  missing rationales and conflicts below; clean whitespace and ASCII-added-line
  checks; all decision-anchor references resolve.

## Approach

- Convert bounded topic batches, then reconcile duplicate topics and anchors.
- Review the result against the original register and linked records.
- Record the final mapping, evidence moves and verification here.

## Outcome (2026-11-01)

- The authoritative 6,204-line baseline is recovered as Git blob
  `842ec8c8f43c74f47f49546ffb48903804d4daf9`. It has 51 level-2 sections,
  not the previously reported 53. The 4,914-line partial is blob
  `f97e187465d315e881ca580dd6430ae14f9c4b44`. Both preservation inputs
  remain unchanged.
- Twenty topic files contain 272 decision entries: 261 active and 11 tentative.
  `docs/decisions.md` is now the index, with the one-owner rule, twenty topic
  scopes, six dated navigation lines, tentative navigation and link-only
  compatibility aliases. No policy body remains in the index.
- Earlier sampled and section-level audits missed active and tentative clauses.
  The replacement [source-clause disposition ledger](decisions-restructure-ledger.md)
  accounts for every baseline line, including headings and blank separators:
  94 ordered ranges, continuous coverage 1-6204, no gaps or overlaps.
  Dispositions: 90 moved, 2 evidence-moved, 1 removed-superseded, 1 index-line,
  0 removed-duplicate. Each moved range records its normative checks and owner.
- Restored omitted macOS foundation, observer/classification, focus/input and
  product clauses; Windows R4 retention, release-gate and multi-output delivery
  order; no-focus directional selection; shared core/adapter delivery order;
  Windows float-candidate ordering; and the superseded KDE-local selection and
  half-snap boundary. Earlier repairs also restored explicit empty `win`, Windows
  underlay sampling, KDE resize-press transport and tentative placement traces.
- Restored Rust routes A/F/C, including the admission sequence, decisions before
  lifecycles, KCM catalog/conflict logic, research refresh, measurement before
  replacement and route C's measured-latency prerequisite. R21 retains steps 0-2
  before 0.1 and step 3 after 0.1 or optionally before; effect reopening does not
  supersede A/F/C. The ledger also checks G4, Rust safety, licensing scope,
  R10-R21 and anchored border v4 against the baseline.
- Corrected approved transient/float clauses from tentative to active, retaining
  unresolved modal/parent outcomes. Removed duplicate package policy from the
  contract topic in favor of canonical world-rendering links; N5 explicit-tile
  precedence remains explicitly tentative, not an accepted package decision.
- There are 218 exact `Why: not recorded.` occurrences at 217 topic anchors,
  listed below. No rationale or rejected alternative was invented.
- Added one backlog continuation line under Push code down for routes A/F/C;
  existing pending native/physical work remains in place. No backlog content
  was removed.

## Conflict review

- Reviewed all twenty topic files for contradictory active rules. No unresolved
  active contradiction was established, so there are no `conflict, user review`
  entries to list. In particular, checked directional focus/maximize scopes,
  floating sends, G4 versus older host-synchronous wording, and R21 versus
  routes A/F/C. Explicit supersessions and host-specific exceptions remain
  scoped; tentative choices and missing physical proof were not resolved or
  promoted to approved policy.

## Old-section to topic-file mapping

Each row maps a baseline section to its current principal owner(s). This is a
navigation map; the [ledger](decisions-restructure-ledger.md) supplies
the subordinate-clause dispositions and verification.

| Baseline section | Topic file(s) under `docs/decisions/` |
|---|---|
| Latest-world rendering (2026-10-27) | `world-rendering.md` |
| Stress acceptance and dispatch (2026-10-27) | `world-rendering.md` |
| KDE port and race decisions (2026-10-26) | `world-rendering.md` |
| Accepted reviews of tentative decisions (2026-10-27) | `world-rendering.md`, `layout-and-admission.md` |
| Contract | `contract.md` |
| Focus | `focus.md` |
| Gaming | `gaming.md` |
| Interface review decisions (2026-10-30) | `windows.md` |
| Accepted reviews of tentative decisions (2026-10-29) | `focus.md`, `windows.md` |
| Windows decisions (2026-10-29) | `windows.md` |
| Windows decisions (2026-10-28) | `windows.md`, `focus.md` |
| Active-drag preview continuity (2026-10-27) | `world-rendering.md` |
| Cross-output gesture release placement (2026-10-27) | `world-rendering.md` |
| World command intent and birth ownership (2026-10-26) | `commands.md`, `contract.md` |
| KDE rapid directional input | `commands.md` |
| Development Entry Point | `development.md` |
| Scope and Platform Goals | `platforms.md`, `macos.md` |
| Windows Owner Threading | `windows.md` |
| Windows Multi-Output Foundation | `windows.md` |
| Project Name | `development.md` |
| Licensing | `development.md` |
| Development Environment | `development.md` |
| Settings, Tray and First-Run | `settings.md` |
| Workspaces | `workspaces.md`, `commands.md` |
| Move, Layout and Output Commands | `layout-and-admission.md`, `workspaces.md` |
| Fixed-Size Admission | `layout-and-admission.md` |
| Restart Persistence | `persistence.md` |
| Reference Matrix and Spec Authority | `reference-and-shortcuts.md` |
| Shortcuts, Conflicts and Presets | `reference-and-shortcuts.md` |
| Gaming Safety | `gaming.md`, `reference-and-shortcuts.md` |
| Placement, Minimums and Startup Adoption | `layout-and-admission.md` |
| Best-Effort Admission | `layout-and-admission.md` |
| Engine Architecture and Convergence | `engine.md`, `contract.md` |
| Visuals: Border, Underlay and Grouping | `visuals.md`, `contract.md` |
| Pointer, Drag and Drop | `visuals.md`, `macos.md` |
| KDE Support Floor | `kde.md` |
| Delivery and Installation | `kde.md` |
| Linux Planner Activation | `kde.md` |
| Live KWin/Plasma Boundary | `kde.md` |
| Observability | `observability.md` |
| Activation Requests | `activation-requests.md` |
| Window State: Float, Sticky, Maximize, Fullscreen | `window-state.md` |
| Engine Operations and Policy | `engine.md`, `window-state.md` |
| Workspace vocabulary 2026-10-24 | `workspaces.md` |
| macOS v2 implementation decisions 2026-10-25 | `macos.md` |
| macOS v2 targeting, receipt and presentation decisions 2026-10-26 | `macos.md` |
| macOS dogfood review decisions 2026-10-27 | `macos.md` |
| macOS follow-up review decisions 2026-10-28 | `macos.md` |
| Native adoption 2026-10-24 | `layout-and-admission.md` |
| Port deferred investigations 2026-10-24 | `kde.md` |
| Independent host highlight colors 2026-10-24 | `visuals.md` |

## Evidence moves and removals

- Implementation progress, test counts and bounded native/physical evidence
  moved to [the evidence archive](decisions-evidence-2026-10-31.md)
  or remain in the existing detailed records linked by each topic. The archive
  separates offline proof from native and physical acceptance.
- Superseded material excluded from current policy includes old
  visibility/protocol shapes, per-domain ownership and pending-pair routes,
  `ActiveGroup`, pre-R20 absolute border motion, KDE timed/Meta-only underlay
  triggers, Windows Win+drag, obsolete fixed-float send refusals, old aligned
  workspace mapping, dormant native return bindings, Carbon input, the macOS
  select-only all-sends-disabled stage, dynamic private radius, and the former
  post-0.1-only SkyLight underlay deferral. These are retained as scoped
  replacement/rejection context where needed. The ledger cites explicit source
  markings; its sole wholly removed range is baseline 839-848, whose 839-840
  marking supersedes the earlier Model A visibility allocation. No unmarked
  active clause was discarded as obsolete.
- Product evidence boundaries link to the existing live-testing guides.
- R13/R16/R17 are not decisions in the authoritative baseline. Their technical
  review-table outcomes were not invented as user-approved decisions.

## Backlog addition

- Added one continuation line under Push code down: route A bounded admission
  (born-maximized, then D06 output selection), then decisions before lifecycles;
  route F Settings KCM catalog/conflict logic in coordination with KDE catalog
  migration; route C native replacement requires measured latency need. It links
  to [the restored decision](../../decisions/engine.md#kwin-to-rust-migration-routes).

## Link work and verification

- Retargeted Markdown topic links across the existing documentation changes;
  unrelated edits were retained. Three non-Markdown references in `deny.toml`,
  `crates/tiler-kde-settings-exporter/src/kde_adapter.rs` and
  `crates/tiler-windows/src/tiling_sys/world_native.rs` are comment-only retargets.
  No executable behavior changed. The protected principles file was not edited;
  its legacy links resolve through link-only index aliases.
- The index was installed after complete ledger coverage, reported entry-format
  clearance, conflict review, backlog retention and zero unresolved current
  decision links. A stricter mandatory-field check subsequently found six
  missing Evidence fields; these were repaired and acceptance rerun, as recorded
  below. Final actual-corpus totals follow the complete record check.
- Ledger destinations and the proposed index were validated before cutover;
  all three raw non-Markdown decision references were resolved after cutover.
  Required fields have zero violations; evidence fields remain link-only.
- Unrelated broad-scan findings remain out of scope: five references to absent
  born-maximized source/test files. A sixth candidate is a GFM heading-parser
  false positive, not a broken decision link.
- `git diff --check` and added-line ASCII checks include tracked additions and
  untracked files. Read-only baseline/partial hashes remain unchanged.

### Final verification

- Actual repository: 1,522 tracked/untracked text files; 978 decision-related
  Markdown link occurrences, 390 distinct URL spellings, 361 anchored URL
  spellings and 234 distinct fragments. Unresolved decision links: 0.
  Raw non-Markdown decision references: 3; unresolved: 0. Counts distinguish
  URL spellings from normalized unique topic anchors.
- Ledger coverage command: `awk` range adjacency and disposition validation;
  output `rows=94 coverage=1-6204 errors=0`, with 90 moved, 2 evidence-moved,
  1 removed-superseded and 1 index-line. Every baseline line is covered once.
- Entry count: 272, comprising 261 active and 11 tentative; required-field
  violations: 0; exact missing-rationale count: 218. The rationale anchor list
  below matches the generated inventory without omissions.
- `git diff --check`: clean. Tracked added non-ASCII lines: 0; untracked
  non-ASCII lines: 0. Baseline and partial Git blob hashes still match the
  preservation inputs. The principles file remains unchanged by this work.
- Completion concerns the register restructure only; native/physical acceptance
  and unrelated missing-file references are not claimed fixed or verified.

## Method deviations and corrections

- Initial sampled preservation reports were invalidated by later omissions;
  complete source-range coverage and normative verification replaced them.
- External Python was used for complex Markdown parsing/retargeting, structural
  entry checks, rationale inventory/grouping, link validation and added-line
  ASCII checking. No scripts were added to the repository. CLI range and hash
  checks were used where sufficient.
- A review found the external validator reused an earlier line while checking
  proposal links. It now resets the parsed line on every iteration and was
  rerun against both the proposal and actual corpus. Initial range-validator
  invocation errors, a patch-context mismatch, an invalid blob-as-tree command
  and an ASCII-helper syntax error were corrected before successful reruns;
  none changed policy or served as acceptance evidence.
- A final mandatory-field check found six omitted Evidence fields despite an
  earlier zero-violation report. Added supporting record/spec links to project
  naming, reference tiers, settings correction, P2 settings scope, separate-view
  labels and target insertion; the strict five-field check now reports zero
  violations across all 272 entries. Consequently the index cutover preceded
  actual field completeness, rather than following it as intended; the final
  field, link, whitespace and ASCII checks were repeated after correction.

## Missing rationales

Each filename below is under `docs/decisions/`; fragments identify all 218
`Why: not recorded.` occurrences. The KDE release-gate anchor contains two
separate entries, hence 217 unique anchors. Other partial explanations are
retained verbatim rather than classified as wholly unrecorded.

### activation-requests.md
- `#hidden-window-activation-default`
- `#urgency-presentation-and-future-setting`

### commands.md
- `#workspace-tiling-intent`
- `#directional-focus-without-a-focused-window`
- `#kde-rapid-directional-input`

### contract.md
- `#core-birth-clear-ownership`

### development.md
- `#development-entry-point`
- `#tentative-macos-development-preview`
- `#development-environment-tools`
- `#reference-sources-and-design-preference`
- `#kde-development-scope-and-live-fixture`

### engine.md
- `#core-owned-world-and-platform-boundary`
- `#host-synchronous-adapter-exception-g4`
- `#portable-engine-and-host-seams`
- `#full-state-world-contract-and-convergence`
- `#world-transport-authority-and-boundedness`
- `#classification-and-convergence-ownership`
- `#kde-effect-rust-migration-sequence-r21`
- `#kde-sole-world-route`
- `#logical-workspace-authority-and-host-owned-observations`
- `#world-owner-adoption-and-cursor-first-placement`
- `#action-context-and-output-grounding`
- `#core-derived-native-settlement`
- `#gesture-lifecycle-and-native-transport`
- `#shared-presentation-and-cursor-authority`
- `#frameless-migration-placement`
- `#cosmic-v1-lifecycle-semantics`
- `#workspace-send-and-cross-output-r4-convergence`
- `#exhausted-cross-output-transfer-reuse`
- `#r4-own-mover-observation-tolerance`
- `#mvp-directional-path`
- `#policy-modes-and-future-tiling-profiles`
- `#cosmic-geometry-parity-boundary`

### focus.md
- `#workspace-active-window-memory`
- `#command-owned-linked-focus-recording`
- `#cross-output-directional-focus`

### gaming.md
- `#games-remain-workspace-members`
- `#xbox-mode-pause-and-release-gate`

### kde.md
- `#kde-support-floor`
- `#native-effect-packaging-and-version-pin`
- `#offline-packaging-and-release-delivery`
- `#nix-host-delivery`
- `#native-development-delivery`
- `#host-kwin-development-authority`
- `#planner-activation-and-loss`
- `#planner-request-binding-and-authorization`
- `#planner-and-kwin-restart-adoption`
- `#kde-fullscreen-presentation-and-host-integration`
- `#kde-gesture-transport`
- `#deferred-activities-support`
- `#deferred-asynchronous-configure-investigation`
- `#deferred-connector-and-cleanup-review`
- `#live-test-evidence-boundary`

### layout-and-admission.md
- `#placement-and-minimums`
- `#windows-declared-minimum-hints`
- `#windows-hint-diagnostics-and-retained-overlays`
- `#fixed-size-admission-predicate`
- `#fixed-size-admission-state-transitions`
- `#fixed-size-d2-drift-and-hint-loss-correction`
- `#fixed-size-d3-d7-user-tile-override`
- `#fixed-size-d7-durable-maximize-base`
- `#automatic-fixed-float-stacking-and-gaming-safety`
- `#core-owned-keep-above-desire`
- `#admission-activation-and-placement`
- `#reopened-window-admission`
- `#admission-focus-batch-invariance`
- `#output-targeting`
- `#move-and-resize-triage-defaults`
- `#same-axis-move-setting`
- `#parent-orientation-toggle`
- `#workspace-local-layout-selection`
- `#directional-moveoutput-transfer-and-selection`
- `#explicit-output-send-actions-and-chords`
- `#best-effort-admission-fallback`
- `#best-effort-d3-d4-no-space-result-and-focus`
- `#best-effort-d5-persistence-and-retry`
- `#best-effort-d6-command-and-transfer-coverage`
- `#best-effort-d7-capacity-eviction`
- `#best-effort-d8-startup-roster`
- `#best-effort-d9-overlap-projection`
- `#best-effort-d10-settings`
- `#best-effort-windows-capability-boundary`
- `#native-workspace-adoption-and-r19`
- `#native-creation-and-attempt-settlement`
- `#native-creation-correlation`
- `#native-attempt-settlement`
- `#native-migration-backing-and-removal`
- `#native-empty-workspace-removal-and-padding`
- `#native-occupancy-and-sticky-boundaries`
- `#native-hidden-representation-and-unknown-sticky-membership`
- `#shared-core-and-adapter-delivery-coordination`

### macos.md
- `#managed-native-spaces-input-and-transfers`
- `#managed-spaces-and-trailing-lifecycle`
- `#select-only-preview-and-physical-test-direction`
- `#fixture-and-scoped-input-defaults`
- `#phase-1-skeleton-and-observer`
- `#initial-macos-product-decisions`
- `#v2-implementation-cutover`
- `#complete-world-mapping`
- `#targeting-receipt-and-presentation`
- `#follow-up-review-and-investigation-priority`
- `#pager-trailing-spare-and-native-workspace-representation`
- `#fullscreen-games`
- `#trailing-creation-and-presentation-settlement`
- `#radius-and-native-space-creation`
- `#macos-ingress-stress-and-acceptance-limits`

### observability.md
- `#cross-component-diagnostics`
- `#output-send-and-windows-tracing`
- `#shortcut-diagnostics`

### persistence.md
- `#orderly-restart-adoption`
- `#intentional-float-restart-store`
- `#windows-restart-intent-markers`

### platforms.md
- `#platform-scope`
- `#windows-safety-and-sandbox`
- `#macos-phase-0-and-host-foundations`
- `#macos-01-release-scope`
- `#kde-01-release-gate-and-delivery`
- `#kde-01-release-gate-and-delivery`

### reference-and-shortcuts.md
- `#reference-outcome-authority`
- `#functional-specification-and-unresolved-work`
- `#shared-shortcut-defaults-and-conflict-model`
- `#windows-gaming-shortcut-containment`
- `#windows-gaming-probe-and-resolution-boundaries`
- `#kde-shortcut-staging-and-forcerevert`
- `#shortcut-mutation-reliability-and-required-chords`
- `#windows-key-input-and-snap`
- `#windows-first-run-rebinding-and-xbox-alternatives`
- `#windows-keyboard-resize-bindings`
- `#macos-preview-presets-and-native-shortcut-ownership`

### settings.md
- `#shared-settings-catalog-and-platform-ownership`
- `#settings-correction-and-proof-owner-boundary`
- `#kde-settings-package-ownership`
- `#p2-settings-follow-up-scope`
- `#windows-settings-surface-and-persistence`
- `#kde-unified-settings-and-host-conflicts`
- `#settings-result-dialogs`
- `#tray-ownership-and-status`
- `#windows-tray-pager-and-first-run`
- `#windows-tray-workspace-pager`
- `#windows-taskbar-pager-placement-and-geometry`
- `#windows-taskbar-pager-artwork-and-typography`
- `#windows-taskbar-pager-focus-color-and-focus-source`
- `#windows-taskbar-pager-suppression-and-safety`
- `#windows-taskbar-pager-bounded-work`
- `#macos-workspace-indicator`

### visuals.md
- `#shared-visual-safety`
- `#independent-host-highlight-colors`
- `#group-underlay-style-and-trigger`
- `#group-underlay-rendering-and-stacking`
- `#nested-drag-placement`
- `#native-color-refresh`
- `#tabbed-groups-and-post-0-1-gate`
- `#drag-restore-marker-retirement`
- `#drag-and-resize-completion-rules`

### window-state.md
- `#fullscreen-and-maximize-overlays`
- `#directional-maximize-and-fullscreen-operations`
- `#floating-membership-and-placement`
- `#sticky-floating`
- `#maximized-intentional-unfloat-b9`
- `#background-tiling-and-permissive-admission`
- `#minimize-restore-and-rehome`
- `#transient-dialogs-and-float-visibility`
- `#kde-floating-directional-focus-and-half-snap`
- `#windows-fullscreen-metadata-retirement`
- `#windows-float-and-sticky-persistence`

### windows.md
- `#switch-and-intent-decisions`
- `#windows-decision-set`
- `#windows-01-release-gate`
- `#accepted-protocol-and-phase-one-fences`
- `#pager-diagnostics-and-cli`
- `#renderer-and-completion`
- `#windows-idle-and-diagnostics`
- `#rust-unsafe-guardrail`
- `#multi-output-foundation`
- `#windows-multi-output-refinements`
- `#windows-multi-output-sequencing`

### workspaces.md
- `#kde-workspace-policy-authority`
- `#transfer-and-view-settlement`
- `#capacity-hold-and-best-effort-policy`
- `#workspace-scope-vocabulary`
- `#separate-view-labels`
- `#workspace-scope-persistence-and-hotplug`
- `#workspace-lifecycle-focus-and-history`
- `#workspace-target-insertion`
- `#windows-asynchronous-arrival-focus`
- `#windows-nonactivating-focus-application-and-rejection`
- `#kde-workspace-tiling-default-adoption`
- `#per-workspace-tiling-floating-and-bindings`
- `#workspace-selection-keys`
- `#workspace-send-follow-settlement`
- `#workspace-history-and-scoped-rings`
- `#workspace-history-shortcut-mappings`
- `#workspace-send-shortcut-mappings`
- `#workspace-migration-d1-bindings-and-scope`
- `#workspace-migration-d2-d3-target-selection`
- `#workspace-migration-d4-destination`
- `#workspace-migration-d5-source-refill`
- `#workspace-migration-d6-focus-settlement`
- `#workspace-migration-d7-floats-and-sticky-windows`
- `#workspace-migration-d8-overlays`
- `#workspace-migration-d9-history-and-automatic-return`
- `#kde-aligned-workspace-lifecycle`
- `#kde-aligned-output-disconnection`
- `#kde-aligned-output-reconnection`
- `#tentative-aligned-migration-d5-source-refill`
- `#tentative-aligned-empty-cell-and-spare-lifecycle`
- `#windows-owner-workspace-storage-and-recovery`
- `#windows-hidden-workspace-presentation`
- `#windows-workspace-focus-and-command-boundary`

### world-rendering.md
- `#stress-acceptance`
- `#kde-renderer-freshness`
- `#focused-fullscreen-presentation-suspension`
- `#membership-and-immediate-eligibility`
- `#float-toggle-intent-and-output-targeting`
- `#kde-pager`
- `#kde-lifecycle-and-accepted-vocabulary`
- `#other-accepted-kde-package-decisions`
- `#cross-output-gesture-release-placement`

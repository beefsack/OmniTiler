# Documentation hygiene (2026-11-01)

## Scope and outcome

- Refresh the [documentation index](../../README.md) to reflect the record model:
  vision, principles, behavior, normative contract, decisions, pending work,
  active/archive records, development setup and live-testing guides.
- Remove the blanket unported-platform claim rather than infer current native
  acceptance. The current wire version is 3, verified against
  `WORLD_CONTRACT_VERSION` in `crates/tiler-core/src/world_report.rs:41`.
- Retain the [historical decisions archive](../../decisions-archive.md) link;
  its introduction explicitly excludes current policy and implementation authority.
- Inventory: 60 top-level change records, not 61. Archive the three completed
  documentation packages below; conservatively retain the other 57. No backlog
  item names any of the three archived records as the home of pending work.
- Move the three previously untracked records without overwriting destinations.
  Preserve their bodies except ten outgoing relative-link retargets, which retain
  identical resolved destinations and fragments. No incoming references to the
  former paths existed, so no incoming links or code comments needed retargeting.
- No principles, decision, specification or contract meaning changed. No build
  or native/live acceptance is claimed.

## Archival triage

Archive requires explicit whole-task completion without remaining work, questions
or acceptance, and no pending-work home link from the backlog. A delivered subpart,
historical status or missing closure statement is insufficient. Evidence line
numbers refer to each linked record unless a backlog line is identified. The
three documentation closures do not close separately tracked product work.

| Record | Decision | Cited evidence |
| --- | --- | --- |
| [architecture-review-ar6-workspaces.md](../architecture-review-ar6-workspaces.md) | Keep | L30: "This alternative is not yet adopted." |
| [background-tiling.md](../background-tiling.md) | Keep | L7: acceptance remains the native residual. |
| [backlog-trim.md](backlog-trim.md) | Archive | L58: "Next action: none for this trim." |
| [contract-document.md](contract-document.md) | Archive | L140: "Documentation acceptance is satisfied"; L155-156 separates future contract migration. |
| [core-interface-review.md](../core-interface-review.md) | Keep | L9-13: evidence complete, user decisions pending; backlog L31-45 retains implementation work. |
| [custom-tile-runtime.md](../custom-tile-runtime.md) | Keep | L10-17: separate manual/live acceptance remains. |
| [decisions-restructure.md](decisions-restructure.md) | Archive | L197-198: completion concerns the register restructure, not unrelated native acceptance. |
| [dev-entry-point.md](../dev-entry-point.md) | Keep | L54: "Evidence and remaining checks"; backlog L110 retains entry-point acceptance. |
| [docs-followups-2026-11-01.md](docs-followups-2026-11-01.md) | Archive | Decisions 2, 2b, 3 and 4 were recorded; decisions 5 and 6 are now approved and recorded. Windows fullscreen baseline implementation and live acceptance remain queued. |
| [edge-drag-share-adjustment.md](../edge-drag-share-adjustment.md) | Keep | L84-85: later fixes have not run live; L100-101: gap loss not visually confirmed fixed. |
| [floor-ratio-feasibility.md](../floor-ratio-feasibility.md) | Keep | L17: "Next Action"; no explicit whole-record closure established. |
| [group-underlay-move-trigger.md](../group-underlay-move-trigger.md) | Keep | L13,21-22: A/B delivered, C remains parked; backlog L148 retains C. |
| [js-workload.md](../js-workload.md) | Keep | L18: "Next Action"; backlog L156 retains parked evidence work. |
| [kde-aligned-output-workspaces.md](../kde-aligned-output-workspaces.md) | Keep | L107: live two-output acceptance; no explicit whole-record closure established. |
| [kde-automatic-float-stacking.md](../kde-automatic-float-stacking.md) | Keep | L101-102: real-game and Steam acceptance remain pending. |
| [kde-dual-output-live.md](../kde-dual-output-live.md) | Keep | L3-8: incomplete port/native evidence; backlog L75 retains acceptance. |
| [kde-legacy-route-removal.md](../kde-legacy-route-removal.md) | Keep | L103: "Remaining acceptance"; backlog L116-117 retains cleanup. |
| [kde-live-acceptance.md](../kde-live-acceptance.md) | Keep | L19-21: remaining native/user checks; backlog L81 retains them. |
| [kde-open-animation-timing.md](../kde-open-animation-timing.md) | Keep | L11: "Keep this record active until" capture review. |
| [kde-physical-checklist.md](../kde-physical-checklist.md) | Keep | L4-5: "Status: UNRUN"; backlog L75,81 retains physical checks. |
| [kde-shortcut-conflicts.md](../kde-shortcut-conflicts.md) | Keep | L21: remains active for user-owned KDE acceptance. |
| [kde-triage-live.md](../kde-triage-live.md) | Keep | L5: pending single-output journeys. |
| [kde-world-v2-port.md](../kde-world-v2-port.md) | Keep | L24-25: "Full native acceptance remains TBD"; backlog L69 retains it. |
| [learned-size-limits.md](../learned-size-limits.md) | Keep | L3: parked pending multi-output evidence; backlog L90 retains work. |
| [macos-0.1-gate-proposal.md](../macos-0.1-gate-proposal.md) | Keep | L5,11: native/physical acceptance remains pending. |
| [macos-active-border.md](../macos-active-border.md) | Keep | L8-11: appearance/perceived tracking and broader acceptance remain separate; no whole-record closure established. |
| [macos-core-trailing-pager.md](../macos-core-trailing-pager.md) | Keep | Backlog L137 retains hidden-focus/pager work; record L114-115 retains active status and untested native pixels/feel. |
| [macos-dogfood-preview.md](../macos-dogfood-preview.md) | Keep | L81: remaining discriminator requires bounded physical HID pressure. |
| [macos-drag-drop-overlays.md](../macos-drag-drop-overlays.md) | Keep | L10-13: live drag and physical dogfood acceptance; no whole-record closure established. |
| [macos-group-underlay.md](../macos-group-underlay.md) | Keep | L43: "Production integration is blocked." |
| [macos-managed-workspaces.md](../macos-managed-workspaces.md) | Keep | L52: "Remaining live checks"; backlog L144 retains port acceptance. |
| [macos-phase-0.md](../macos-phase-0.md) | Keep | L5-6: ordinary-window transfer unresolved; MVP remains blocked. |
| [macos-phase-1-skeleton.md](../macos-phase-1-skeleton.md) | Keep | L9-10: live capability acceptance continues separately; L12-13 already links an archived delivery record. Keep the active pointer conservatively. |
| [macos-scoped-tile.md](../macos-scoped-tile.md) | Keep | L9-17: full input/delivery acceptance is not closed. |
| [macos-window-classification.md](../macos-window-classification.md) | Keep | L6-8: tentative selection pending user review; retention remains TBD. |
| [macos-workspace-indicator.md](../macos-workspace-indicator.md) | Keep | L11: acceptance includes live appearance; no explicit whole-record closure established. |
| [macos-world-v2-port.md](../macos-world-v2-port.md) | Keep | L12,16: presentation and native acceptance remain open; backlog L144 retains port work. |
| [native-effect-license-audit.md](native-effect-license-audit.md) | Keep | Audit outcome is recorded; the source-license decision is current at [Licensing](../../decisions/development.md#licensing). |
| [pending-live-checks.md](../pending-live-checks.md) | Keep | L19: native/live evidence is pending; backlog L186-265 links check groups. |
| [placement-aware-startup-adoption.md](../placement-aware-startup-adoption.md) | Keep | L3-6: historical implementation is not current-world acceptance evidence; no whole-record closure established. |
| [reliability-condition-investigation.md](../reliability-condition-investigation.md) | Keep | L19: "LIVE GATE PENDING." |
| [rust-first-edge-drag-route.md](../rust-first-edge-drag-route.md) | Keep | L64: "Open Questions For The User." |
| [shared-core-d23-rehome-and-13f-maximize-base.md](../shared-core-d23-rehome-and-13f-maximize-base.md) | Keep | L4: implementation and native acceptance remain pending; backlog L109 retains work. |
| [shared-settings-catalog-windows-tabs.md](../shared-settings-catalog-windows-tabs.md) | Keep | L4: physical acceptance pending; no whole-record closure established. |
| [shortcut-override.md](../shortcut-override.md) | Keep | L81-88: outstanding live checks and physical chords/restoration are unproven. |
| [shortcuts.md](../shortcuts.md) | Keep | L19: "Next Action"; backlog L160,168 retains layout/profile work. |
| [window-alignment-drag-investigation.md](../window-alignment-drag-investigation.md) | Keep | L3-8: on-device residual acceptance remains open. |
| [windows-0.1-gate-proposal.md](../windows-0.1-gate-proposal.md) | Keep | L4-5: native/physical acceptance remains pending. |
| [windows-all-member-workspace-sends.md](../windows-all-member-workspace-sends.md) | Keep | L65: "Physical acceptance remains pending; this record stays active." |
| [windows-best-effort-admission-live.md](../windows-best-effort-admission-live.md) | Keep | L3-8: follow-up partial; acceptance remains open. |
| [windows-cloak-spike.md](../windows-cloak-spike.md) | Keep | L3: native feasibility and recovery evidence pending. |
| [windows-dual-output-live.md](../windows-dual-output-live.md) | Keep | L16,19: remaining fixture/user acceptance; backlog L53 retains dual-output checks. |
| [windows-gaming-coexistence.md](../windows-gaming-coexistence.md) | Keep | L142-146: delivered implementation does not close physical acceptance; backlog L93,125 retains it. |
| [windows-launcher-geometry-fighting.md](../windows-launcher-geometry-fighting.md) | Keep | L4: four-launcher physical acceptance pending; backlog L94 retains work. |
| [windows-multi-output-foundation.md](../windows-multi-output-foundation.md) | Keep | L19: physical journeys remain pending; backlog L53,87-89 retains checks. |
| [windows-pager-workspace-flicker.md](../windows-pager-workspace-flicker.md) | Keep | L21,26: cause/remedy unresolved; cause not yet established. |
| [windows-physical-checklist.md](../windows-physical-checklist.md) | Keep | L6: physical matrix is unrun; backlog L50,89 retains checks. |
| [windows-single-output-dogfood.md](../windows-single-output-dogfood.md) | Keep | L688-690: keep active until physical outcomes are known; backlog L62 retains them. |
| [windows-taskbar-pager-icons.md](../windows-taskbar-pager-icons.md) | Keep | L59-60: wider DPI acceptance remains pending. |
| [windows-workspace-return-focus.md](../windows-workspace-return-focus.md) | Keep | L63: underlying refusal unexplained; backlog L28-29 retains staged-switch work. |

## Backlog decision links

The 2026-11-01 review initially added five one-liners under
[Open user decisions](../../backlog.md#open-user-decisions). Those choices are
now recorded or resolved; the backlog no longer lists the contract-authority
question as open. Decisions 5 and 6 are in the canonical
[contract decision](../../decisions/contract.md#contract-authority-and-v4-migration):

- GPL header relicensing and the native-effect source boundary: see the
  [license audit](native-effect-license-audit.md) and current
  [Licensing decision](../../decisions/development.md#licensing).
- Maximized-origin cross-output focus and fullscreen-state game protection:
  current decisions are indexed in [decisions.md](../../decisions.md).
- Windows app-owned fullscreen exit refusal remains, with adoption-baseline
  implementation and live acceptance queued; outcomes are recorded in the
  [follow-up record](docs-followups-2026-11-01.md#decision-outcomes-and-remaining-work)
  and [baseline plan](../windows-fullscreen-exit-baseline.md).
- Contract authority is resolved by the [coordinated v4 decision](../../decisions/contract.md#contract-authority-and-v4-migration);
  phase work is listed at the top of [Open work](../../backlog.md#open-work).

## Verification

- Final inventory uses tracked and untracked files: 633 Markdown and 896 other
  text sources, excluding two generated distribution artifacts. The repo-wide
  scoped audit checks 345 distinct link occurrences: seven incoming links to
  the index/moved records and 343 outgoing links from the index, moved records,
  backlog and this record, with overlaps counted once. Zero unresolved targets
  or fragments and zero new scoped regressions; no former-path references in
  Markdown or raw text/code. This is not a claim of global link cleanliness.
- All 896 non-Markdown text sources were searched for bare/relative target
  names as well as full paths: zero scoped references and zero unresolved.
- Reverse-retarget reconstruction matched the three original body hashes.
  All ten moved outgoing links retained identical resolved paths and fragments:
  decisions 4/4, contract 4/4, backlog 2/2.
- Whole-repository `git diff --check` and cached whitespace checks pass.
  Added-line ASCII scan: zero non-ASCII across 4,538 added tracked lines and
  all 32 untracked text files. The triage table accounts for every record once;
  all five new decision entries occur once and their links resolve.

## Pre-existing findings outside the moved-record/index scope

- Four retained fragment defects documented in the
  [backlog trim](backlog-trim.md#pre-existing-fragment-defects-retained):
  `native-spaces-routes.md#user-manual-transfer-check--2026-10-16`,
  `native-spaces-routes.md#live-verdict--2026-10-16`,
  `release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2`,
  `reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator`.
- Missing raw targets: `test-fixtures/output-topology-parity.json:3` names
  `docs/changes/shared-core-owned-output-topology.md`;
  `crates/tiler-core/src/observation_send_r4_tests.rs:4` names
  `docs/changes/observation-convergence-step-3.md`;
  `kwin/src/kwin-globals.d.ts:3` names
  `docs/changes/custom-tile-vertical-slice/research/type-provenance.md`.
- An unrelated README comment in
  `docs/changes/js-baseline-measurement/harness/run.sh:15` names absent
  `harness/README.md`; it does not refer to the documentation index.
- Additional broad-check candidates, not treated as authoritative full-repository
  results: `docs/spec/reference-outcomes/special-windows.md:778` links to absent
  `kwin/tests/world-born-maximized.test.ts`; research table text at
  `docs/research/integrated-tiling-workspace-value/plasma-krohnkite-baseline.md:514`
  and `docs/research/integrated-tiling-workspace-value/hands-on-validation.md:78`
  is parsed as unresolved reference labels `[+#desktop]` and `[+desktop id]`.
  These candidates and retained defects were not changed by this package.

## Method deviations and limits

- Temporary Python and Node helpers were used for Markdown parsing, GFM-style
  heading/HTML-anchor validation, body-hash reconstruction and link destination
  equivalence. No checker dependency or repository script was added.
- The initial broad Python scan misread inline code/math, heading slugs and
  source line fragments. Its unresolved count was discarded, not accepted as
  evidence. Corrected scoped checks, not broad-parser cleanliness, are the
  acceptance boundary.
- Final verification corrected an audit-output buffer limit and an omitted
  reverse-fragment mapping before accepting the complete results.
- The inventory differed from the expected count by one; all 60 discovered
  records are accounted for above. Uncertain records remain active.
- No content deviation: the package only changes the index, pending-decision
  reachability, record locations and necessary relative links.
- Documentation hygiene is complete. Next action: none for this package;
  the indexed product/licensing decisions and active acceptance work remain open.

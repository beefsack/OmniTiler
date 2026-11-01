# Documentation follow-ups (2026-11-01)

## Goal and scope

- Correct stale licensing names and gaming rationale; record the approved
  licensing decision without changing specified behavior.
- Audit KWin-free native-effect sources for KWin dependencies and adapted code;
  update only their SPDX license comments under the approved scope.
- Add missing minimal fullscreen action/focus reference sequences with sourced
  Tier 1 observations and unresolved OmniTiler outcomes marked TBD.
- Exclude implementation changes and principle edits. Decisions 5 and 6 were
  separately user-approved and recorded below; they authorize planning, not v4
  contract or fixture implementation.

## Acceptance and approach

- Preserve existing decision meaning and reference outcome/status cells except
  for the approved native-effect licensing boundary.
- Verify removed names against tracked files and the Cargo workspace.
- Compare reference sources locally at documented pins; cite determinate Tier 1
  outcomes and leave unsupported observations TBD.
- Review licensing corrections/audit, gaming rationale, then reference rows as
  bounded units. Record accepted evidence and unresolved product choices here.
- Verify touched-file links, ASCII added lines including new files, and
  `git diff --check` after integration.

## Licensing corrections and audit

- Added a supersession note to [licensing research section 8](../../research/licensing.md#8-approved-decision-2026-10-11),
  preserving the dated decision body. Tracked files and workspace manifests
  confirm that `oraclepress.h` and `tiler-kwin-effect-ffi` no longer exist.
- Removed the effect-staticlib references from the current
  [Licensing](../../decisions/development.md#licensing) and
  [Third-party notices](../../decisions/development.md#third-party-notices)
  entries. Removed the Cargo-staticlib claim from the native-effect CMake
  comment; no build statements changed.
- [Per-file audit](native-effect-license-audit.md): at audit start, 17 GPL-headed source files,
  of which only `activewindowborder.cpp/.h` directly include KWin headers.
  All 15 remaining files were compared with pinned KWin source. The only
  behavior match is `presentation_consumer.h:385-410`, which independently
  matches observed KWin outline inflation for interoperability and does not
  reuse KWin code. Bounded searches found no adapted code in it or the other
  14 files. User-approved outcome: all 15 are MIT.
- `activewindowborder.cpp/.h` remain GPL because they include KWin headers;
  plugin metadata and the combined effect binary remain GPL. Workspace
  membership was checked from tracked manifests; `cargo metadata` was not
  executed.

## Gaming rationale

- REQ-MAX-07 and REQ-MAX-07b now cite fullscreen-state inference and the
  uncertain-detection fallback rather than game identity or flawless gaming.
  The retained inference, reference tally, outcomes and statuses are unchanged.
- The Xbox-mode decision's assumption now cites the fullscreen-state rule and
  uncertainty fallback. Its signal requirement, mechanism, release gate and
  status are unchanged. Dated quotations in review/history records remain.
- Captionless inference is consistent with leaving uncertain fullscreen
  windows alone; it is not itself a principle conflict.

## Reference coverage

- Added [R-MAX-12](../../spec/reference-outcomes/maximize-fullscreen.md#r-max-12-explicit-actions-while-focused-fullscreen):
  separate fresh directional-move, workspace-switch/return and orientation-toggle
  legs while fullscreen. Tier 1 source branches distinguish refused moves,
  retained fullscreen state and layout behavior; unestablished settlement is TBD.
- Added [R-OUT-11](../../spec/reference-outcomes/multi-output.md#r-out-11-directional-focus-between-fullscreen-or-maximized-outputs):
  cross-output fullscreen and maximize focus legs with ordinary peers to
  distinguish overlay eligibility. sway's fullscreen destination-node branch
  is sourced; other unresolved target/state combinations remain TBD.
- Reused R-MAX-02 (directional focus), R-MAX-09 (send), R-MAX-10 (float),
  R-CLOSE-05 (close) and R-WS-09 (ordinary switch/return). The focused-fullscreen
  newcomer-focus question already has a fresh leg in
  [R-INS-06](../../spec/reference-outcomes/insertion.md#r-ins-06-open-over-an-overlay-maximized-plus-fresh-fullscreen-variant);
  linked it rather than duplicating it.
- Added OPEN requirements REQ-MAX-12 and REQ-FOC-07, with existing fixed subrules
  linked separately. Totals: 154 NORMATIVE, 29 OPEN, 183 requirements;
  162 reference scenarios. Each new scenario has 14 profile observations.
- Existing outcomes/statuses are unchanged. Fullscreen-origin destination
  eligibility remains unresolved; retained-allocation ranking does not newly
  grant eligibility. Maximized-origin directional focus retains its existing
  no-op, not a new cross-output exception.

## Decision outcomes and remaining work

- Decisions 2, 2b, 3 and 4 from this review are recorded in the topic decisions
  and functional specification. Decisions 5 and 6 are now approved and recorded:
  v3 remains authoritative until the exact coordinated core/all-adapter v4
  cutover, and migration follows the single phased plan. The guide's mismatch
  pointers preserve each current-source citation; its open-question section and
  backlog entry are removed. Windows app-owned fullscreen refusal remains;
  only the adoption-baseline implementation and its individually accepted
  live-check path are queued. No live checks were run.
- [Contract authority and migration decision](../../decisions/contract.md#contract-authority-and-v4-migration)
  is canonical; the [adapter guide rule](../../porting/world-contract.md#contract-authority-rule)
  and [phase-ordered backlog](../../backlog.md#open-work) link to it. The
  [Windows decision](../../decisions/windows.md#state-centric-v4-visibility)
  defers to that authority rather than duplicating it. The approved migration
  is a plan only: the normative v4 draft and wire fixtures have not been created.

## Verification and deviations

- Decision-record follow-up (2026-11-01): archived this record after recording
  decisions 5 and 6 and updating its cross-links. Targeted search found no stale
  source-path or removed-anchor references; `git diff --check` is clean and the
  added archive text is ASCII. A full repository link checker was not run.
- Source observations use the documented i3, sway, Hyprland, COSMIC and KWin
  pins, with file/line citation keys. No native/runtime results were inferred.
- Repaired two pre-existing dead source-link destinations (each used in two
  rows) to verified historical source permalinks; behavior and delivery claims
  were not rewritten. This mechanical repair was needed for touched-file link
  acceptance, outside the topical wording edits.
- Link verification uses a temporary Markdown parser; no repository checking
  tool or dependency was added. Checks: nine touched Markdown files have zero
  unresolved local file/heading/HTML-anchor links; added lines, including new
  files, are ASCII; `git diff --check` is clean. Historical source permalink
  paths were checked against repository objects, not HTTP availability.
- No remaining content deviation: the historical licensing decision body,
  principle text and existing normative behavior remain intact. The 15
  approved SPDX header changes and the GPL binary/metadata boundary are recorded
  above. Unresolved product choices above are not claimed complete.

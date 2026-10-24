# Documentation consolidation

Status: completed 2026-10-24. All four review areas are accepted; no runtime or
port implementation changed.

## Goal and scope

- Make current product and port status easy to find after the completed
  [core rearchitecture](core-world-model-rearchitecture.md#closing-outcome-2026-10-24).
- Archive completed/superseded backlog and decision history without losing
  intent; shorten active status narratives and link to evidence records.
- Review active change records and point-of-use docs for retired ownership,
  stale contracts, duplication and broken local links.
- Preserve priorities, parked questions, tentative decisions, specification
  rows and stable port-guide paths/decision anchors. `docs/principles.md` is
  unchanged; proposals, if any, are reported separately. No runtime or port work.

## Bounded units

1. Backlog: retain active work, promote delivered core/Windows status, preserve
   history in a linked archive and keep KDE/macOS ports and deferred reviews.
2. Decisions: retain current meaning and stable anchors, identify tentative
   entries, consolidate superseded history with traceable links.
3. Change records: archive complete/superseded work, retain accurate active
   scope and remove duplication with point-of-use docs.
4. Other docs: reconcile current architecture and navigation; mechanically
   check local paths and fragments across docs and READMEs.

## Acceptance and verification

- Each move preserves history and updates inbound and relative links.
- Active docs distinguish delivered shared-core/Windows v2 from unported KDE/
  macOS and their approved remaining core/native work.
- Deferred native acceptance/separation, cleanup/abstraction, fixture follow-ups
  and product questions remain visible, without new product choices.
- Inspect each area diff and verify whitespace and local links; record actual
  before/after backlog and decision sizes and any unresolved link exceptions.

## Accepted units

- Core close-out is archived with final fixture-only evidence; the core-only
  ordered baseline was rerun successfully at 6/6 checkpoints.
- Baseline sizes before consolidation: backlog 389,728 bytes; decisions
  331,400 bytes.
- Backlog unit accepted: active file 162,304 bytes; archived history 270,291
  bytes. Complete historical bodies retain delivered/superseded work; current
  entries link to records and reflect the completed core/Windows v2 boundary.
  Native connector/separation, cleanup/abstraction, fixture follow-ups, KDE/macOS
  ports and parked questions remain explicit active work. Preservation review
  found no missing unresolved requirement or tentative/parked item. Tail
  acceptance inventories remain detailed rather than discarding unique checks.
- Scoped Markdown path/fragment checking found only existing broken targets;
  these and archive-local fragment retargeting are carried into the final link
  cleanup unit. New port and core-closeout links pass; whitespace is clean.
- Decisions unit accepted: current file 332,893 bytes; superseded excerpts
  9,302 bytes in `docs/decisions-archive.md`. A short tentative-decision index
  improves navigation; required decision anchors remain stable. The current
  file retains durable hotplug, native-renderer and fencing/budget invariants
  rather than shrinking them into misleading historical summaries.
- Semantic review corrected overbroad draft supersession: core desires publish
  before native arrival, internal layout sessions remain, KDE's renderer bridge
  remains live pending its port, and active hotplug and no-replay rules remain
  current. Independent re-review accepted all corrections without a new product
  decision. Scoped link checks add no failures; the existing triage fragment is
  carried to final cleanup. Whitespace passes.
- Change-record unit accepted: archived the completed Windows tiling-system
  split, historical Windows phase-1 path, superseded Windows full-world
  adoption plan and historical cross-output directional delivery. Relative and
  inbound links were retargeted. Other active records retain concrete native,
  physical, investigation or port follow-ups; existing archived macOS milestone
  stubs now point to the current port contract.
- Active dual-output/checklist/backlog status now reflects delivered v2
  migration/cutover and targeting/settings. T2 refusal remains offline evidence,
  not an invented native/physical pass; the remaining manual matrix is explicit.
  Retired-route research/implementation notes are marked historical and point to
  current world-route contracts. No accepted product meaning changed.
- Full Markdown path/fragment checking found the same preexisting failures
  before and after these moves; no new broken target was introduced. Whitespace
  passes. Next: point-of-use docs/navigation and full link repair, including
  validating the checker's treatment of inline code and source line anchors.

## Closing outcome (2026-10-24)

Final sizes below use versioned UTF-8/LF bytes (earlier per-unit observations
above included working-copy line endings):

| File | Before | After | History archive |
| --- | ---: | ---: | ---: |
| `docs/backlog.md` | 389,728 | 160,330 | 266,975 |
| `docs/decisions.md` | 331,400 | 332,928 | 9,302 |

The backlog is 58.9% smaller. Decisions grew slightly because the tentative
index and explicit current/historical distinctions preserve active invariants;
reducing size was not allowed to change decision meaning.

- Added the [documentation index](../../README.md), separated active backlog
  work from archived delivery narratives, indexed tentative decisions and
  archived superseded contracts without changing their meaning. Four completed
  or superseded change records moved to this archive with inbound links updated.
  Detailed acceptance inventories and all unresolved product questions remain.
- Current-source banners distinguish delivered shared-core/Windows v2 from
  KDE/macOS legacy consumers, historical native evidence and intended port
  acceptance. Corrected stale macOS development-stub descriptions against its
  actual recipes; this does not claim the unported consumer currently builds.
- Fixed real local path/fragment failures across docs and recursive READMEs.
  Retired source/test evidence now points to locally verified historical Git
  blobs, while genuinely moved equivalents use current paths. One missing
  historical artifact directory is explicitly identified as unavailable rather
  than linked to a fabricated target. Specification changes are link-only;
  requirement and reference-outcome content is preserved.
- The final checker covers **564 Markdown files, zero broken local targets or
  fragments**. Inline-code tree literals, valid source line anchors and GitHub
  heading punctuation were validated rather than counted as broken links.
  Revised baseline: **81 real failures**, all repaired. Historical Git pins,
  paths and cited line ranges were verified locally. External URL availability
  was not tested. `git diff --check` passes.
- Independent final review accepted semantic fidelity, citation provenance,
  stable paths/anchors, specification preservation and the link checker, with
  no blocking finding. `docs/principles.md` and both external port prompts are
  unchanged; there are no principles proposals or unresolved user decisions
  introduced by this pass. No source code, dependency or platform implementation
  was changed.
- Native connector/separation review, code cleanup/abstraction review, fixture
  follow-ups, port implementation, tentative choices and parked questions remain
  active in the backlog. The core close-out's native/physical limitations still
  apply; documentation cleanup grants no new acceptance claim.

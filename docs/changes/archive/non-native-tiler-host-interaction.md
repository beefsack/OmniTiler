# Non-native tiler host-interaction research

- Goal: deliver the P1 host-interaction research with candidate lessons for user selection.
- Scope: KDE/KWin, GNOME, Windows and macOS host-integrated tilers; six requested interaction/reception dimensions, pinned read-only local source and dated web evidence. Documentation only; no live testing, dependency installs, implementation items or approved-rule changes.
- Acceptance: concise report (target under 500 lines), per-tiler tables, source provenance and reception evidence limitations; cross-host synthesis; per-host evidence/current approach/gap/option comparisons; decision tensions explicit; delivered backlog link; independent citation sample; whitespace/link checks; intended-files-only publication.
- Approach: gather and write four bounded host report sections sequentially; integrate synthesis and project comparisons; independently spot-check citations and claims before publication.
- Units: KDE -> GNOME -> Windows -> macOS gathering; integration; independent review; final verification/publication.
- Baseline: clean main at `065ca54`; three user stashes preserved. Existing catalogue, platform feasibility and visual/gaming research reused. Research selects no product behavior.
- Verification: inspect actual diff and claim provenance, check report length/local links and `git diff --check`; scope is the report, backlog and this archived note.
- Accepted evidence: pinned per-tiler tables and dated illustrative issue/community samples for four hosts. MosaicWM is GNOME, not KWin; Bismuth/bug.n are historical; gTile/FancyZones/Custom Tiles are contrasts. Additional Whim/Seelen and Rift/GlazeWM macOS mechanisms included where useful. No numerical reliability ranking or lower-level-latency claim is supported.
- Integration: compared the findings with current Engine, workspace, shortcut, rendering, gaming and live-boundary decisions. Candidate lessons retain user choice; no rule changes or implementation items. Backlog marked delivered.
- Independent review: checked mechanisms and tracker citations across all four hosts, local links and decision anchors. No blocking findings; GNOME hash provenance and KDE creation-vs-closure date labels corrected. Reddit re-fetching was blocked; report explicitly retains that limitation. Published-source livesplit-hotkey 0.8.1 substrate also read to pin the dependency claim.
- Outcome: [report](../../research/non-native-tiler-host-interaction.md), under 500 lines, with eight cross-cutting findings, host reception summaries, evidence/current approach/gap/option lesson table and six decision tensions. Documentation only; no live testing, new dependencies or approved-decision edits.
- Verification: intended docs-only diff inspected; report 394 lines; local-link and decision-anchor spot-checks passed; ASCII/placeholder scan clean; `git diff --cached --check` passed across all three intended files. Published as single-line commit `docs: research non-native tiler host interaction` to origin/main.
- Process: no semantic failed implementation approach or unresolved blocker. Three existing stashes untouched.
- Next action: user selects candidate lessons from the report; no implementation work is queued by this delivery.

## Backlog detail moved 2026-11-01

### Original backlog lines 1580-1596

````markdown
- P0 | Windows release gate | Now part of 0.1 (user 2026-10-17; was its own
  later release, user 2026-10-10); exact Windows 0.1 gate pending definition
  under the 0.1 release item. Current contents: managed
  per-monitor workspaces (multi-output), gaming coexistence (Win+G/F11
  containment review, Xbox mode detection, M16 event-driven owner measurement),
  combined workspace-presentation research (hidden-workspace Alt+Tab/taskbar
  semantics plus taskbar indicator parity 10), restart persistence (item 8,
  on-window markers selected 2026-10-10) and package/update channel decision (short research refresh
  during Windows release planning; most obvious and unsurprising for users).
  Gaming and Windows presentation research run after multi-output,
  before release; presentation research has produced options plus a
  recommendation for the user. The parked selection decision remains open
  and is distinct from release completion. Non-native lessons and evidence stay linked:
  [event/pump boundary](research/non-native-tiler-host-interaction.md#layer-layout-authority-events-and-reconciliation-dim-1),
  [workspaces/Alt+Tab](research/non-native-tiler-host-interaction.md#workspaces-monitors-overview-and-alttab-dim-4),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).
````

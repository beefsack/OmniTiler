# Release 0.1 Triage Decisions And Reference Tiers

Status: complete - triage review recorded 2026-10-12; implementation
and native acceptance remain in the [backlog](../../backlog.md).

## Goal And Scope

- Record the user's 2026-10-12 reference tiers and D01, D03, D09, D10,
  D12, D13, D16, D22, D23, D25, D26, D27 and D28 selections in decisions, triage,
  spec, outcome cells and backlog.
- Record the user's 2026-10-12 approval of Proposal B and N1-N4, completing
  triage review with the multi-output acceptance prerequisite.
- Preserve historical proposals and untested outcomes; update the Windows
  handoff and existing shortcut-dialog parity item.
- Documentation only; D01's KDE fullscreen focus fence remains code work,
  D16 needs shared-core/KDE float transfer after native roundtrip verification,
  D23 needs shared-core/KDE minimize storage/restore and null-focus handling,
  D27 requires a native KWin trace before implementation assessment, and D28
  needs KDE fullscreen carry plus must-pass native acceptance including a game.

## Acceptance And Approach

- Promote seventeen selected requirement rows to NORMATIVE with decision links.
- All 28 triage units and Proposal B / N1-N4 classification user-approved.
  Verify original 53 M39/K12/W2 plus four M additions = 57 M43/K12/W2;
  D28 remains a check-1 sub-leg, counted once. Multi-output must-pass checks
  require a two-output setup (VM or hardware) before 0.1 ships.
  P2 settings stay limited to approved D09, D10 and D27.
- Reconcile current summaries and indexes, verify changed local links and
  anchors, and run `git diff --check`.

## Material Decisions

- [Reference tiers](../../decisions.md#reference-tiers) replace the earlier default
  rule; prior decisions stand unless revisited.
- [Admission](../../decisions.md#placement-minimums-and-startup-adoption) preserves
  native activation ownership, including explicit switching and host denial.
- [Move defaults](../../decisions.md#move-layout-and-output-commands) retain only
  the two-option D09/D10 post-0.1 settings.
- [D27 activation](../../decisions.md#activation-requests), approved option (a):
  Mark urgent only by default, no workspace/focus change; Hyprland/i3/sway
  Tier-1 agreement reverses tentative Switch and focus. Gaming guard is
  urgency-only in both modes; explicit Alt+Tab/taskbar selection is separate,
  and INS-07 no-steal remains scoped to inactive-workspace tiling admission.
- Post-0.1 `Activation request` setting: `Mark urgent only` (default; tooltip
  Hyprland, i3, sway) / `Switch and focus` (tooltip COSMIC, xmonad). COSMIC
  Tier 1 plus xmonad Tier 2 qualifies the alternative; functional names/options,
  WM names tooltips only. P2 retains D09/D10/D27.
- D27 KDE trace-first condition: native Wayland xdg-activation and Xwayland
  under default focus-stealing prevention. Attention-only is verification-only;
  native workspace/focus switching makes an urgency-only override conflict
  with D01, requiring a return to the user before code. Otherwise request/urgency
  handling needs code per the gap row; native outcomes stay TBD.
- [D16 floating transfer](../../decisions.md#workspaces), approved option (a):
  retain float and size on arrival, destination tiles untouched, for follow/stay
  and numbered/relative sends. Hyprland/i3/sway Tier 1 plus xmonad/qtile/awesome
  Tier 2 retain float against COSMIC fresh re-tile. No setting: the alternative
  has no second Tier 1/2 reference.
- D16 needs code: shared core `NotTiled` and KDE non-tiled send refusals remain
  gaps. First verify untested native WS1 -> WS2 -> WS1 roundtrip, then shared
  core first (serves Windows), then KDE adapter; Windows adapter work goes to
  item 22. Reconcile earlier intentional-float ineligibility as a D16 gap,
  retaining the separate floating-workspace boundary journey and sticky refusal.
- [D22 float-out reflow](../../decisions.md#window-state-float-sticky-maximize-fullscreen),
  approved option (a): surviving siblings keep relative proportions (50/25/25,
  float middle 25 -> approximately 67/33). No setting. COSMIC (observed, not
  source-proven), i3 and sway (Tier 1) agree; no evidenced alternative;
  Hyprland/Tier 2 untraced. Thin basis disclosed: i3/sway one family plus
  separate COSMIC observation.
- D22 is verification-only: shared removal retains survivor share ratios per
  the revision-bound gap assessment. Native R-FLT-03 reflow live check remains
  pending; selection establishes no native evidence.
- [D23 minimize/restore](../../decisions.md#window-state-float-sticky-maximize-fullscreen),
  approved option (a): minimize releases allocation, siblings reflow, MRU focus,
  stored old slot; restore to stored slot with no OmniTiler focus write (host
  handles restore focus); sole minimize retains workspace, focus none, stored
  slot. No setting. COSMIC Tier 1 source-proven default; qtile/awesome Tier 2
  vacate, awesome restores old slot; Hyprland/i3/sway no counterpart or
  untraced non-voting; no evidenced alternative to allocation release.
- D23 needs shared-core-first storage/restore and sole-minimize null-focus
  handling, then KDE wiring. Windows follows later via item 22: retained
  allocation is divergence to change, not an approved exception. Native
  outcomes remain TBD; earlier restore "no focus steal" wording is superseded
  by no OmniTiler focus write with host focus left native.
- [D25 owner restart](../../decisions.md#restart-persistence), approved option
  (a): keep host workspace set; re-observe membership, no moves; restore focus
  only where provable; rebuild layout by re-adoption, no saved-layout restore;
  intentional floats stay floating via existing restart store. No setting.
  Tier basis: i3 Tier 1 plus xmonad Tier 2 membership/focus, set kept 5/8,
  COSMIC no counterpart, no evidenced alternative. Verification-only.
- [D26 end-session restore](../../decisions.md#restart-persistence), approved
  option (b): restore persisted owned gaps/settings; re-observe host workspace
  set/membership/maximize; no invented layout restore. Listed 0.1 known
  limitation: intentional-float memory does not survive logout/login under
  unchanged REQ-RST-01d lifetime; reopened apps have new identities. Cross-login
  memory revisited after 0.1 with shared restart store. No code; verification-only.
- Native dogfood/live checks will assess D25/D26, per User 2026-10-12;
  selection is not native acceptance.
- [D28 fullscreen send](../../decisions.md#window-state-float-sticky-maximize-fullscreen),
  selected option (b) for 0.1, rejecting the recommended hold: an explicit
  send of a fullscreen window, e.g. a game, is a user action and should be
  honoured. KDE carries fullscreen on numbered/relative follow/stay sends,
  matching G-D2 maximized carry and Windows item 9. No setting. Original
  triage tier tally: i3/sway Tier 1 plus qtile/awesome Tier 2 carry, bspwm
  context, COSMIC/Hyprland untraced in that tally, no evidenced alternative.
  Detailed source evidence is retained separately from the approval tally.
- D28 needs KDE code: existing fullscreen-send refusal becomes a gap, removed
  as a listed 0.1 known issue. Explicit commands honoured; no unsolicited
  fullscreen writes; preserve fullscreen flag with no tiling geometry writes
  beyond native workspace movement. Native fullscreen send including a real
  game is must-pass; stop and return to user if native observation shows KWin
  cannot carry without dropping/corrupting fullscreen.
- [Proposal B](../../research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2)
  and N1-N4 approved exactly User 2026-10-12: original 53 checks M39/K12/W2;
  K = 5, 24, 26, 29, 35, 36, 37, 38, 40, 41, 52, 53; W = 3, 4; all others M.
  Four new must-pass N1-N4 give 57 total = M43/K12/W2. D28 native fullscreen
  send with a real game stays within original check 1, counted once.
- Multi-output must-pass checks require a two-output setup (VM or hardware)
  and must pass before 0.1 ships. Known-issue-allowed checks require a listed
  issue and no crash or silent tiling stop; Windows-release checks are outside
  KDE 0.1. Classification approval is separate from native/live evidence.

## Evidence And Outcome

- Initial 2026-10-12 record promoted eight rows (129 NORMATIVE / 9 PROVISIONAL /
  28 OPEN). D27 approval promoted REQ-ACT-01 (130 NORMATIVE / 8 PROVISIONAL /
  28 OPEN). D16 approval promoted REQ-WS-05 (131 NORMATIVE / 7 PROVISIONAL /
  28 OPEN). D22 approval promoted REQ-FLT-03 (132 NORMATIVE / 6 PROVISIONAL /
  28 OPEN). D23 approval promoted REQ-MNZ-01/02/03 (135 NORMATIVE /
  3 PROVISIONAL / 28 OPEN). D25/D26 promoted REQ-RST-01c/REQ-RST-02
  (137 NORMATIVE / 1 PROVISIONAL / 28 OPEN). D28 promotes stable REQ-MAX-09b
  from refusal hold to KDE carry, yielding verified current totals of
  138 NORMATIVE / 0 PROVISIONAL / 28 OPEN (166 rows).
- All 28 triage units and Proposal B / N1-N4 classification approved; triage
  review complete. Historical options/tallies and unsupported reference
  outcomes are retained; approvals and their rationale are recorded.
- Windows item 22 updated for approved defaults/settings and native foreground
  considerations; existing shortcut-dialog parity item updated to P2, with
  KDE user live review linked. No duplicate item added.
- D27 updated in decisions, triage, spec/counts/index, Ours target cells,
  backlog gap/settings and Windows item 22; no product code or live tests.
- D16 updated in decisions, triage, spec/counts/index, R-WS-05 Ours targets,
  backlog approved gap/implementation queue and Windows item 22. Existing
  R-WS-05 supplies the roundtrip discriminator; no product code or live tests.
- D22 updated in decisions, triage, spec/counts/index, R-FLT-03 Ours targets,
  backlog approved verification row/queue and Windows item 22. Existing
  R-FLT-03 supplies the ratio-vs-equalize discriminator; no product code or live tests.
- D23 updated in decisions, triage, spec/counts/index, R-MNZ-01..03 Ours
  targets, backlog approved gap/implementation queue and Windows item 22.
  Existing minimize/restore/sole-window scenarios discriminate the selected
  behavior; no product code or live tests.
- D25/D26 updated in decisions, triage, spec/counts/index, restart Ours targets,
  backlog approved verification rows/queue, Windows items 8/22 and known
  limitations. Existing R-RST-01/02 and the new-login discriminator are reused;
  no product code or live tests. Membership/set/focus native acceptance remains
  pending, distinct from existing float/sticky identity evidence.
- D28 updated in decisions, triage, spec/counts/index, R-MAX-09 Ours target,
  backlog gap/implementation queue, Windows handoff and Pending live checks.
  Refusal removed from listed known issues. Proposal B check 1 now includes
  the approved must-pass fullscreen/game sub-leg; original 53-check tally
  and then-proposed M39/K12/W2 counts unchanged. No product code or live tests.
- Proposal B approval recorded in triage, release gate, Pending live checks,
  decisions and spec summary. Exact K/W IDs, four new M checks, M43/K12/W2
  total and D28 single counting verified; generic two-output prerequisite
  recorded. This change record is complete and archived; remaining delivery
  and native/live checks stay in the backlog.
- Requirement counts, changed local link targets/anchors, ASCII additions,
  requirement table structure, unchanged foreign reference outcomes and
  `git diff --check` passed. Native outcomes remain TBD; D01's fullscreen focus
  fence remains implementation work.

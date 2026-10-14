# Windows taskbar workspace indicator research

## Goal and acceptance

- Produce concise Windows workspace-indicator options and one recommendation
  for user choice; the indicator is a dogfooding blocker pulled forward from
  Windows workspace presentation research.
- Cover workspace numbers, active/occupied/urgent state, multi-output behavior,
  interaction, gaming, APIs/dependencies, DPI/theme, Windows 10/11, estimated
  implementation size and risks. Incorporate D27 5a: hidden-window urgency marks
  its workspace urgent and clears on visit.
- Cite repository/prior-art source and official API evidence; reject infeasible
  surfaces with evidence. Deliver the research link in the presentation backlog
  item with recommendation pending user choice.
- Research only; no production changes or live desktop tests.

## Approach

1. Inspect current Windows tray/workspace seams and sibling Windows WM/bar
   implementations through bounded source reads.
2. Verify Windows shell surfaces against official documentation and write the
   options/recommendation research note.
3. Review evidence and coverage, update the backlog, run `git diff --check`,
   reconcile upstream documentation and deliver the documentation change.

## Verification and outcome

- Source review established the existing stable-GUID tray, shell-restart
  recovery and change-gated sync; workspace urgency acquisition remains absent.
- Reviewed komorebi-bar's per-monitor clickable workspace strip and the
  GlazeWM/Zebar/YASB presentation ecosystem. Official shell documentation
  supports tray, taskbar overlay/thumbnail and AppBar options; deskband and
  Widgets do not provide the supported modern taskbar surface required here.
- [Research](../../research/windows-port/taskbar-workspace-indicator.md) covers
  five options, rough implementation estimates and their risks. Recommended:
  one numbered tray icon plus per-output popup, pending user choice. D27 5a is
  a fixed input, not an open mark/clear decision.
- Backlog links updated under presentation research and parity item 10;
  hidden-window Alt+Tab research remains a separate design question.
- Documentation-only review; `git diff --check` passed. No production changes
  or live desktop tests. No performance/fullscreen acceptance claimed.
- Next action: user chooses surface, output display and click behavior;
  implementation must separately resolve urgency ingestion and verify game,
  DPI/theme, shell recovery and overflow behavior.

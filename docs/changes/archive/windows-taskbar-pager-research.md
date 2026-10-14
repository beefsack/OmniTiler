# Windows taskbar pager research

## Goal and scope

- Establish whether public Windows APIs can host a Plasma-style pager inside
  each monitor's taskbar; compare supported alternatives and unsupported prior art.
- Research and documentation only. No production changes or live desktop tests.
- Preserve the per-output pager target; recommendations remain pending user choice.

## Acceptance and approach

- Add a concise, sourced follow-up to the existing taskbar indicator research,
  covering hosting, rendering, fullscreen/game safety, complexity and staged delivery.
- Distinguish documented contracts, observed source behavior and unverified claims.
- Update the existing backlog reference to the follow-up and pending choice.
- Bounded units: hosting/prior-art evidence, then rendering and recommendation
  synthesis; review their evidence and integrate the documentation.
- Verify the final documentation diff and `git diff --check`.

## Outcome

- Added [pager-style follow-up](../../research/windows-port/taskbar-workspace-indicator.md#pager-style-taskbar-follow-up)
  with Microsoft API/specification evidence and source-based prior-art comparison.
- Windows 11 has no supported arbitrary in-taskbar host. Windows 10 legacy
  DeskBands are technically available but Microsoft discourages new development.
- Recommended tray+popup for dogfooding and an owned adjacent per-monitor AppBar
  for persistent pager visuals; literal embedding requires unsupported shell coupling.
- Covered scaled window geometry, cached icon acquisition, drawing choices,
  per-output fullscreen hiding and lifecycle obligations. Game performance and
  hide/reservation coexistence remain unvalidated; no product choice was selected.
- Backlog points to the recommendation pending user choice.
- Documentation review and `git diff --check` passed. No live desktop tests.

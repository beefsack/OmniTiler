# Learned size limits - reconciliation phase 2 (parked 2026-09-28)

Status: parked by the user (2026-09-28, option A) pending multi-output test-system evidence. The
unaccepted candidate is preserved on branch `wip/learned-size-limits` (commit
"WIP: learned size limits (reconciliation phase 2, parked)"); its full
change note is there.

## Candidate summary

- Rust: optional per-request `learned_max_sizes` caps, projected separately
  from native hints; feasible caps give spare extent to siblings, native
  maximums unchanged. About +257 production lines.
- KWin: per-window cap learning/expiry from completed writes. About +517 net
  production lines; interim three-strike acceptance kept as fallback.
- Not accepted: infeasible or ignored caps can be re-promoted and resent (no
  reply feedback), learned/expired logs are uncorrelated, and cap state can
  survive a changed allocation. Exceeds the complexity rule for its value.

## Why parked

The recorded unexplained shortfalls are from the multi-output test system, not the
single-output test system (test-system trace `omnitiler-dev.uE1S5n.log` shows only
1-2 px work-area settling). DP-6 writes of `8,52,2032,1092` were observed at
`1920x1036`, and Ghostty-class requests of height 1092 held 1036. 1920x1036
equals HDMI-A-2's work area (`2048,116,1920,1036`), which suggests the window
is being constrained to the other output (our output assignment or bounds, or
KWin's per-output constraint) rather than a client-chosen limit. Unproven; the
`p13` 36 px shortfall is not explained by this.

## Next action

On the multi-output test system, capture a `just dev trace` with one tall tiled window on DP-6 and
confirm its output, bounds and KWin constraints at the shortfall. If it is an
output mismatch, fix that root cause and re-evaluate whether phase 2 is
needed. Resume from the branch only if a genuine client-held limit remains.

## Backlog detail moved 2026-11-01

### Original backlog lines 1038-1060

````markdown
- P1 | Shared: tolerate host-settled geometry | User 2026-10-19: hosts often
  settle a window at a frame slightly different from the requested one (DPI
  virtualization rounding, character-cell size increments in terminals,
  minimum/maximum hints, self-resizing clients). Find one general,
  platform-neutral solution so the tiler accepts small discrepancies without a
  back-and-forth fight with the host, not a per-platform exception.
  Research/design first: (1) inventory current core/KDE/Windows/macOS behavior
  when the observed frame differs from the request (re-issued writes, existing
  tolerances, readback mismatch logging); (2) check how Tier 1 input WMs
  (sway, i3, Hyprland, COSMIC; local pinned sources) treat increments, hints
  and host rounding for tiled windows; (3) propose a rule, draft: the
  host-settled frame is accepted for that allocation and the same window is
  not re-written for the same allocation; small discrepancies leave
  neighbors alone; larger ones (increments, minimums) reflow neighbors at most
  once; logs report host-settled outcomes truthfully. Weigh against the parked
  [learned size limits](changes/learned-size-limits.md) candidate (rejected
  for complexity). Any core change follows
  [Layering](principles.md#layering) and needs user approval before
  implementation. Evidence cases: Windows DPI-unaware one-pixel offsets at
  125% scale (`9fa32420`,
  [record](changes/windows-best-effort-admission-live.md)), terminal
  increments, decision on interactive-resize noise (rounding, size
  increments, self-resizing clients).
````

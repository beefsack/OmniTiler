# Commands and gestures

- R14 window-state commands live only in [contract.md](contract.md#window-state-commands-r14).
- R18 scripting surface lives only in [contract.md](contract.md#scripting-command-surface-r18).

### Workspace tiling intent
- Decision: `ToggleWorkspaceTiling { output, workspace }` flips retained core mode at execution; `SetWorkspaceTiling` is for genuine explicit choices. KDE shortcut/tray and Windows toggle callers use neutral intent; repeated queued toggles compose. Wire is `toggle-workspace-tiling` with `out/ws`, never `tiled`.
- Why: not recorded.
- Assumes / revisit if: explicit set remains distinct from toggle.
- Status: active, 2026-10-26
- Evidence: [KDE N6 outcome](../changes/kde-world-v2-port.md#n6-core-intents-outcome-and-blocker-2026-10-26).

### Directional focus without a focused window
- Decision: when directional focus has no focused window, encode the subject as an explicit empty `win` for both in-process and JSON callers. Missing, null or invalid ids, and empty ids on other commands, retain strict validation.
- Why: not recorded.
- Assumes / revisit if: no-focus behavior is described in [workspace lifecycle](workspaces.md#workspace-lifecycle-focus-and-history).
- Status: active, 2026-10-27
- Evidence: [directional focus without a window](../changes/archive/directional-focus-without-window.md).

### KDE rapid input
- Decision: every KDE user command press queues FIFO in press order, with no busy refusal. A window-targeting command resolves its subject from settled native focus at dispatch, unless it is an explicit press-time-subject route. Native observation invalidation fences stale setters; a sent command never replays; generation changes discard old contexts. Internal drag-verdict and unfloat continuations keep their own path.
- Why: i3, sway, Hyprland and COSMIC dispatch every matched press sequentially; one queue removes the directional-only branch and the busy refusal.
- Assumes / revisit if: physical-key and perceived-latency acceptance remain pending. Production currently queues only directional focus/move; extending the queue is pending work.
- Status: active, 2026-11-08 (user decision; supersedes the 2026-10-19 directional-only tentative entry)
- Evidence: [change record](../changes/archive/kde-rapid-input-geometry.md).

- Cross-output gesture release placement lives only in [world-rendering.md](world-rendering.md#cross-output-gesture-release-placement).

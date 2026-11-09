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
- Decision: when directional focus has no focused window, encode the subject as an explicit empty `win` for both in-process and JSON callers. Missing, null or invalid ids, and empty ids on other commands, retain strict validation. The explicit current-focus object has the separate execution-time meaning in [KDE rapid input](#kde-rapid-input), never the empty marker's no-focus meaning.
- Why: not recorded.
- Assumes / revisit if: no-focus behavior is described in [workspace lifecycle](workspaces.md#workspace-lifecycle-focus-and-history).
- Status: active, 2026-10-27
- Evidence: [directional focus without a window](../changes/archive/directional-focus-without-window.md).

### KDE rapid input
- Decision: every KDE user command press queues FIFO in press order, with no busy refusal. Queueing parks the command and wakes the existing capture route; one fresh capture incorporates observations once, then core drains the whole parked batch in order without waiting for native application or another capture per action, and returns one latest publication. Commands arriving during a capture park for the next batch. Window-targeting commands use `win:{"currentFocus":true}` to resolve core's current focus at execution, after the preceding action; explicit `win` identities retain by-id and press-time-subject meaning, and empty directional-focus `win` retains its genuinely no-focus meaning. Native focus/open/close observations incorporate at the capture's place in the ordering. Implicit maximize uses core-resolved focus rather than requiring settled native focus; explicit maximize retains its existing focus gate. The adapter applies only the latest publication: at most one pending publication, an in-flight write completes, then it jumps to the newest; superseded unissued targets and one-shot focus/raise/stack effects drop. Native refusal is learned from later observation; a sent command never replays; generation changes discard old contexts. Internal drag-verdict and unfloat continuations keep their own path.
- Why: i3, sway, Hyprland and COSMIC dispatch every matched press sequentially; one queue removes the directional-only branch and the busy refusal. Applying every action in core while the platform applies only the latest world keeps latency low under bursts; it matches the contract's latest-wins apply rule.
- Assumes / revisit if: incoming close and attempt evidence survives skipped native publications. Close currently reports native lifecycle state, not an outgoing close command; any future non-idempotent command needs retained work that supersession cannot drop. Physical-key and perceived-latency acceptance remain pending.
- Status: active; capture-boundary implicit-subject focus mismatch is corrected and all seven scoped gates pass. A post-correction injected burst, mapped once from observed fixture order, settles all 60 commands READY with zero refusals and final native/core agreement. Earlier 51 refusal causes remain individually unavailable; correct semantic refusals are allowed and queued refusals now retain per-command internal reasons. Four presentation-hide diagnostics remain unclassified. Independent latest-only native history, physical-key and perceived-latency acceptance remain open, 2026-11-08 (user decision; explicit execution-time subjects and single-capture batch execution supersede the 2026-10-19 directional-only tentative entry) User 2026-11-09 dogfood: navigation and window moves felt near-instant, the most responsive to date (informal perceived-latency evidence; formal acceptance still open).
- Evidence: [current change](../changes/kde-rapid-input-latest-world.md), [earlier outcome](../changes/archive/kde-rapid-input-geometry.md).

- Cross-output gesture release placement lives only in [world-rendering.md](world-rendering.md#cross-output-gesture-release-placement).

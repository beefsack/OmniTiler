# D23 minimized rehome: source-slot lifetime (Tier 1 research)

Objective: B tiled on WS1, minimized, membership rehomed to WS2/output while
minimized, restore on destination, then return B to source. Does the source
slot survive? OPEN parked user decision; no code. Commit `3d55cba0` pins below
are COSMIC; Hyprland/i3/sway pins were directly verified against pinned
upstream sources (blob links below), consistent with the existing
minimize-matrix citations.

Pinned source blobs: COSMIC
[`3d55cba0`](https://github.com/pop-os/cosmic-comp/tree/3d55cba06c9cf6f27609cdefb520f7857dba20af) -
[workspace store](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs#L104-L137),
[unmap](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs#L659-L669),
[surface route](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs#L732-L752),
[move](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/mod.rs#L3333-L3360),
[dest push](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/mod.rs#L3383-L3418),
[restore](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/mod.rs#L4439-L4453),
[remap](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/layout/tiling/mod.rs#L458-L545),
[migration](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs#L598-L603).
Hyprland
[`19fb395d`](https://github.com/hyprwm/Hyprland/tree/19fb395d45314960e6f79f17994a84094f1cd4f6) -
[state handler](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/view/window/Window.cpp#L839-L846),
[X11 echo](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/view/window/X11Backend.cpp#L381-L383),
[Wayland no-op](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/view/window/WaylandBackend.cpp#L366),
[dispatcher inventory](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/config/shared/actions/ConfigActions.cpp).
i3
[`903bcd51`](https://github.com/i3/i3/tree/903bcd518df32b0e055b17f5da3f988a0187fd3d) -
[iconic reject](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/handlers.c#L847-L857),
[scratchpad](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/commands.c#L1921-L1951).
sway
[`1652c54b`](https://github.com/swaywm/sway/tree/1652c54b73f67df17b7b4ab0b0f7048204aa8104) -
[minimize echo](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/xwayland.c#L622-L634),
[activation clear](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/xwayland.c#L287-L288),
[visibility](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/view.c#L1149-L1180),
[command table](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/commands.c#L114-L143).

## COSMIC (Tier 1, native minimize exists): source forgets, destination fresh-admits

- Slot storage is per-workspace: `Workspace.minimized_windows`
  (`src/shell/workspace.rs:104,108,124-137 @3d55cba0`).
- Moving a minimized window removes the source entry: `unmap_element`
  takes the matching `MinimizedWindow` out of source `minimized_windows`,
  clears the minimized flag and returns the stored restore data
  (`src/shell/workspace.rs:659-669 @3d55cba0`; `unmap_surface` routes through
  it, `:732-752`).
- `move_window` then explicitly drops the tiling tree anchor
  (`state.state.take()`, `src/shell/mod.rs:3357-3360 @3d55cba0`) and, because
  `is_minimized` was captured before unmap (`:3333`), pushes the entry onto
  the DESTINATION workspace's `minimized_windows` (`:3383-3418`) and returns
  early (never maps tiled on arrival). Cross-output moves take the same path
  (`:3350-3355`).
- Destination restore searches all workspaces for the entry
  (`unminimize_request`, `src/shell/mod.rs:4439-4447 @3d55cba0`) and remaps on
  the holding workspace. With `state=None`, `remap` skips both the
  parent/index (`src/shell/layout/tiling/mod.rs:458-503`) and sibling-regroup
  (`:505-541`) branches and falls to fresh admission (`:544-545`).
- Return leg (source-state level): the destination restore consumed the entry
  (`swap_remove`); nothing remains on source at the source-state level, so
  sending B back is inferred as an ordinary mapped move plus fresh admission.
  Exact return rendered shape and user-facing shortcut reachability for moving
  while minimized remain TBD; the inference is ordinary fresh-return, not a
  proven rendered frame.

Limits: this is the `move_window` surface path, which explicitly handles the
minimized case; the exact user-facing shortcut dispatch that reaches it while
minimized, plus animation/focus frames, is untraced. Whole-workspace
`migrate_workspace`/`set_output` keeps minimized entries with the workspace
(`workspace.rs:598-603`; `mod.rs:1052-1060`) - that is workspace relocation,
not window rehoming, and does not change the per-window finding. Sticky-layer
minimized entries (`mod.rs:379,4419-4437`) are out of this tiled scenario.

## Hyprland / i3 / sway (Tier 1): scenario inapplicable natively, non-voting

- Hyprland `S(S-hyp-mininv)`: the sole `stateRequest` listener handles
  fullscreen/maximize only (`Window.cpp:839-846`); backends echo/no-op
  (`X11Backend.cpp:381-383`, `WaylandBackend.cpp:366`); no minimize verb in
  the dispatcher inventory (`ConfigActions.cpp:200-1824` at pin).
  `@19fb395d45314960e6f79f17994a84094f1cd4f6`.
- i3 `S(S-i3-mininv)`: `WM_CHANGE_STATE` iconic requests are rejected and
  reverted (`handlers.c:847-857`); scratchpad is a separate mechanism
  (`commands.c:1921-1951`). `@903bcd518df32b0e055b17f5da3f988a0187fd3d`.
- sway `S(S-sway-mininv)`: the sole minimize listener echoes the xwayland flag
  when unfocused only (`xwayland.c:622-634`), activation clears it
  (`:287-288`), visibility/arrange never consult it (`view.c:1149-1180`); no
  minimize verb (`commands.c:114-143`), scratchpad separate.
  `@1652c54b73f67df17b7b4ab0b0f7048204aa8104`.
- Consequence: B never enters a minimized slot, so "rehome while minimized"
  cannot occur natively - any move is an ordinary tiled move, and there is no
  restore/source slot to keep or forget. Scratchpad/hidden analogues are not
  minimize votes per the matrix header and are not counted here.

## Options (OPEN; none selected)

- (A) Keep source-only remembered context: destination restore still
  fresh-admits per the selected fallback; only a later source return may
  best-effort rejoin the surviving original neighborhood (side/group/index
  where possible, comparable old share), never guaranteed exact. This implies
  no cross-domain restore and claims no reference behavior for the
  hypothetical OmniTiler option. Consequence: retains stale source context
  after B demonstrably belongs elsewhere; conflicts with the proven COSMIC
  source-state removal above (source entry removed, anchor cleared).
- (B) Forget source slot on rehome: source-state entry removed on send and
  the destination holds a degraded (anchorless) entry; destination restore
  fresh-admits; return is inferred ordinary fresh-return (exact rendered
  shape TBD, see COSMIC limits). Consistent with the already selected
  "destination fresh-admission fallback" and "sent elsewhere admits fresh"
  rules at the source-state level.
- (C) Conditional: explicit minimized-send cleans up the source slot, while
  confirmed passive membership drift retains it. Complexity cost: two
  lifetime rules plus a confirmed-drift detector; native-route dependency:
  explicit send is the only COSMIC-proven path, so passive-drift COSMIC
  applicability stays unsupported/TBD. No Tier 1 source evidence distinguishes
  the passive leg.

## Recommendation (tier rule; OPEN, not selected)

COSMIC is the sole voting Tier 1 WM here (Hyprland/i3/sway have evidenced no
counterpart = non-voting, not disagreement); no Tier 2 contradicts the
forget behavior at the source-state level. The tier-rule lean is option (B) -
re-homing a minimized window forgets the source slot at the source-state
level (degraded entry transferred to destination); destination restore
fresh-admits; source return is inferred ordinary fresh-return with exact
rendered shape TBD. This is a lean only; the decision stays OPEN for the
user. No setting (D23 has none selected). The existing matrix row
(`reference-outcomes.md:169-177`, source row "Source slot after membership
rehoming while minimized") already selects destination fresh admission; the
source half remains TBD pending user approval.

## Discriminating sequence (canonical matrix row)

See [D23 best-effort restore and close discriminators](../spec/reference-outcomes.md#d23-best-effort-restore-and-close-discriminators),
variant "Source slot after membership rehoming while minimized". Ours
outcome/decision TBD pending user approval.

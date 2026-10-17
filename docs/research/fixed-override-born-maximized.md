# Fixed-override born-maximized: Tier 1 intersection (13f follow-up)

User decision 2026-10-17: option A (preserve the durable tiled base beneath
native maximize under either born-maximized setting, including hint loss).
Approved shared policy, not implemented. All targets including KDE are
affected by the future change; the Windows interim stays untouched until
shared delivery.

Scope: adopted/born-maximized durable fixed-origin tile override, both
`core.born_maximized` values including hint loss. Generic non-fixed maximize
routing is an unchanged control. No live testing was run. No shared/KDE/
Windows behavior is changed by this file; the Windows interim stays untouched.

Normative anchors (unchanged): D3 explicit tile wins for the same live client
(hide/show/domain/re-adoption); D7 durable fixed-origin tile override survives
owner restart; D4 fixed born-maximized uses a floating base. Tentative
Windows-only route: hydrated fixed-origin override keeps a reserved tiled slot
underneath native maximize under both born modes incl hint loss; generic
non-fixed maximize routing unchanged; R-SPC-13 native discriminator stays TBD.

## Verified direct-upstream evidence per Tier 1 WM

- COSMIC @3d55cba06c9cf6f27609cdefb520f7857dba20af (re-verified against pinned
  local copies; line numbers match the in-repo legend):
  - Fixed predicate is whole-size equality
    ([layout/mod.rs:17-55](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/layout/mod.rs)).
  - Map path: non-dialog tiles via `tiling_layer.map`; the unmaximize loop at
    `mod.rs:3001-3009` targets OTHER existing maxima, not the newcomer; a
    pending maximize then overlays via `maximize_request` at `:3020-3022`
    ([mod.rs:2997-3022](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/mod.rs)).
  - `maximize_request` records original geometry+layer and overlays the work
    area; `unmaximize_request` restores that layer
    ([mod.rs:4461-4544](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/mod.rs),
    [workspace.rs:1002-1043](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs)).
    Ordinary tiled newcomer therefore keeps a retained Tiling slot beneath
    maximize; a fixed newcomer admits Floating, so maximize retains a Floating
    base with no tiled slot.
  - Explicit floating toggle maps directly without hint reclassification
    ([workspace.rs:1491-1519](https://github.com/pop-os/cosmic-comp/blob/3d55cba06c9cf6f27609cdefb520f7857dba20af/src/shell/workspace.rs));
    workspace enable retiles ordinary floats without a hint check
    (`:1440-1454`). Live-toggle analogue only, not a restart-hydrated
    override under maximize.
  - No owner-restart-with-live-clients counterpart: `persist` carries pinned
    workspaces only (`mod.rs:853-864,1511-1524`), `to_pinned` carries output
    match/tiling flag/id/name only (no window/float state), and `Terminate`
    stops to the login manager with no re-exec verb.
- Hyprland @19fb395d45314960e6f79f17994a84094f1cd4f6 (verified via direct
  HTTP fetch of pinned raw files):
  - Maximize is a client fullscreen mode, not generic fullscreen: unmapped
    maximize stores a pending `FSMODE_MAXIMIZED` request; mapped maximize
    routes to `setFullscreenMode(...,FSMODE_MAXIMIZED)`; maximize-off clears
    to `FSMODE_NONE` only when the client state is `FSMODE_MAXIMIZED`
    ([Window.cpp:883-897](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/view/window/Window.cpp)).
    Map applies the pending/requested client mode and replaces the existing
    workspace FS (`:1527-1562` region).
  - `FSMODE_FULLSCREEN` covers the monitor box; `FSMODE_MAXIMIZED` covers the
    work area; entering a mode from floating remembers the float size; exit
    (`FSMODE_NONE`) leaves placement to layout recalc
    ([FullscreenHandler.cpp](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/managers/fullscreen/handler/FullscreenHandler.cpp):
    `setTargetSizeAndPosition`, `setTargetFullscreenModeInternal`).
  - Fixed hints: Wayland either-axis with minima >1
    ([Window.cpp:1026-1038](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/view/window/Window.cpp));
    X11 both-axes positive equality
    ([X11Backend.cpp](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/desktop/view/window/X11Backend.cpp)
    `traitsFrom` `FIXED_SIZE`). `suggestsFloat` runs at initial map only.
  - Explicit float toggle has no refusal branch and clears/re-applies
    fullscreen around the toggle
    ([LayoutManager.cpp:32-53](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/layout/LayoutManager.cpp)
    `changeFloatingMode`;
    [ConfigActions.cpp](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/config/shared/actions/ConfigActions.cpp)
    `floatWindow`).
  - No restart counterpart: `reloadConfig` re-applies config on the live tree
    only; `exit` stops the compositor
    ([ConfigActions.cpp:1223-1239](https://github.com/hyprwm/Hyprland/blob/19fb395d45314960e6f79f17994a84094f1cd4f6/src/config/shared/actions/ConfigActions.cpp)).
    Pending-request store/consume plus the maximize-echo guard live in
    `WindowFullscreenPolicy.cpp`.
- i3 @903bcd518df32b0e055b17f5da3f988a0187fd3d (verified via direct HTTP
  fetch of pinned raw files):
  - Fixed admission floats: min==max with both maxima >0, alongside
    dialog/utility/toolbar/splash/modal/transient branches
    ([manage.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/manage.c)
    `want_floating`). `manage_existing_windows` re-runs ordinary manage over
    mapped windows on every start; layout matches only with a restart file
    (`manage.c:282-283`, `restart_mode` flag in `load_layout.c`).
  - Orderly restart preserves tiling via a whole-tree JSON dump, not an
    independent marker: `cmd_restart` carries the IPC fd and calls
    `i3_restart`
    ([commands.c:1698-1724](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/commands.c));
    `i3_restart` stores the layout (`dump_node`) and re-execs with
    `--restart`
    ([util.c](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/util.c)
    `store_restart_layout`/`i3_restart`); `tree_restore` returns false on
    missing/slurp/append failure
    ([tree.c:66-123](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/tree.c))
    and the caller falls back to `tree_init`
    ([main.c:923-936](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/src/main.c));
    swallow criteria are class/instance/role/title/machine/id/dock/
    insert_where/restart_mode (no versioned membership/partial semantics, no
    tile-override store). Fresh start recomputes admission (pre-stop explicit
    tile lost).
  - Explicit `floating_disable` inserts after the tiling-focused descendant;
    later hint updates never re-admit (`floating.c` `toggle_floating_mode`,
    `floating_disable`; `handlers.c` hint path is float-only clamp).
  - No maximize verb: `commands.spec` INITIAL lists `fullscreen` but no
    maximize; maximize is derived from layout only
    ([parser-specs/commands.spec](https://github.com/i3/i3/blob/903bcd518df32b0e055b17f5da3f988a0187fd3d/parser-specs/commands.spec)).
    `reload` is config-only (`cmd_reload`).
- sway @1652c54b73f67df17b7b4ab0b0f7048204aa8104 (verified via direct HTTP
  fetch of pinned raw files):
  - Fixed floats: xdg either-axis min==max with both minima nonzero
    ([xdg_shell.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/xdg_shell.c)
    `wants_floating`); xwayland either-axis positive equality plus
    dialog/utility/toolbar/splash/modal
    ([xwayland.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/xwayland.c)
    `wants_floating`). Admission evaluates `wants_floating` at map
    ([view.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/view.c)
    `view_map`); the runtime hint path handles urgency/size only.
  - Explicit toggle is the `container_set_floating` path (float
    detach+recenter; unfloat re-admits after focus-inactive tiling with
    fractions reset; no old-slot restore)
    ([container.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/tree/container.c)
    `container_set_floating`).
  - No maximize verb: maximize request only schedules a configure, no mode
    change
    ([xdg_shell.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/desktop/xdg_shell.c)
    `handle_request_maximize`); runtime command inventory carries `exit` and
    `reload` with no restart verb
    ([commands.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/commands.c)
    `command_handlers`); `reload` is in-place config reload, not a process
    restart
    ([reload.c](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/commands/reload.c)
    `cmd_reload`/`do_reload`).

Unavailable exact outcomes: none of the four studied Tier 1 WMs evidences
the exact case (durable
fixed-origin marker + owner restart with live clients + maximize under both
born modes). i3/sway have no maximize verb at all; COSMIC/Hyprland have no
owner-restart-with-live-clients journey; i3's restart memory is a whole-tree
dump with placeholders, not an independent fixed-origin marker, and carries
no maximize dimension. Missing counterparts are recorded as TBD, not votes
for or against any candidate.

## Shared candidate options for ALL targets (incl KDE)

- A. Durable explicit tile wins under maximize with a reserved base on every
  target. A settled/hydrated explicit tile over known fixed origin keeps its
  reserved tiled slot beneath native maximize regardless of the born_max
  setting. The born_max setting controls only ordinary (non-override)
  admissions' overlay/clear. Automatic fixed (D4) keeps its floating base
  and re-floats on unmaximize. Hydration works without hints (identity +
  marker, never hint inference).
  Consequences: manual intent survives restart/hint loss/maximize uniformly;
  config stays coherent (born_max remains meaningful for the ordinary path);
  hydrated overrides are exempt from Tile-on-launch; KDE applicability and
  implementation gaps require audit before any shared implementation, no KDE
  change; Windows interim already implements this and stays untouched.
- B. Born-max setting controls overlay/clear for hydrated overrides too
  (tile-on-launch clears a hydrated override's native maximize at adoption
  but keeps its durable TILED base and marker).
  Consequences: D7 tile intent is respected (base and marker survive
  restart); native maximize is lost at adoption under Tile-on-launch, so
  overlay continuity depends on the setting value; born_max gains a second
  role (ordinary admissions plus hydrated overrides). B does not violate
  D7; it is a precedence choice between setting uniformity and overlay
  preservation, not a discard of persisted intent.
- C. Fixed-auto D4 wins even over a hydrated override (re-float it when
  maximized). Consequences: re-floating discards the durable tile base and
  marker, contradicting D3/D7; destroys manual intent on exactly the journey
  D7 was selected for; rejected.

## Recommendation (design inference, NOT Tier 1 consensus)

Recommend candidate A as the shared candidate, selected User 2026-10-17
(option A; approved shared policy, not implemented; Windows interim
untouched; KDE behavior unchanged until a later user-approved change). Basis: native overlay preservation and intent
continuity, supported by actual Tier 1 analogues - COSMIC's
original-layer mechanism retaining a Tiling slot beneath maximize for
ordinary tiled windows; Hyprland's refusal-free explicit float toggle and
pending-maximize-at-map path; i3's orderly-restart tile retention via
placeholders plus `floating_disable` surviving later observation; sway's
explicit `container_set_floating` path surviving observation (map-only
admission). None of these is the exact durable case. D3/D7 do not logically
require A, and B does not violate D7: B is a live alternative that keeps
durable tile intent while sacrificing overlay continuity under
Tile-on-launch. The recommendation prefers A for overlay preservation, and
the choice stays OPEN for the user.

## Discriminating sequence (canonical matrix row)

See [R-SPC-13](../spec/reference-outcomes/special-windows.md#r-spc-13-adopt-an-already-mapped-fixed-client-on-owner-startup),
Windows marker discriminator: explicitly tile fixed E with the owner running,
remove maximum hints, natively maximize E and N, restart the owner, then
disable/enable tiling; repeat under both `core.born_maximized` values with
hint loss and the N generic non-fixed control. Reference outcomes and Ours
observed outcomes TBD.

## Verification limits

- COSMIC checked against pinned local copies; Hyprland/i3/sway checked by
  direct fetch of pinned raw files at the commits above. No live testing;
  all native Ours legs remain TBD.

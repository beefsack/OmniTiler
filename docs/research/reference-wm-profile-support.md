# Hyprland, bspwm, And PaperWM Profile Support

Status: research complete on 2026-09-17. This is decision support only. It
does not select a profile, host adapter, IPC topology, compositor plugin,
GNOME extension, packaging format, panel, settings toolkit, or runtime work.
No live compositor, DBus, session, configuration, or manual testing occurred.

## Scope And Baseline

"Support" has two materially different meanings. They must not be collapsed:

| Meaning | What it would provide | What this research does not establish |
| --- | --- | --- |
| Portable behavior profile | A versioned Rust policy and its matching shortcut catalog, projected by a selected host adapter. | Native compositor parity, an adapter, or a selected host. |
| Native integration | Coexistence with, or implementation inside, the named WM/compositor using its public control surface. | That public observation/control is structural-layout authority or safe to co-own. |

The current direction remains `cosmic_v1`: the core owns deterministic ordered
N-ary split trees, shares, policy, and logical intent; adapters own native
identity, work area, pixel projection, focus, shortcuts, native workspaces,
actuation, and verification. See [the shared-core boundary](../changes/archive/shared-rust-core-architecture.md)
and [current decisions](../decisions/layout-and-admission.md#directional-moveoutput-transfer-and-selection). A future
profile is required to include both algorithm and matching shortcuts, but no
Hyprland or native backend is selected.

The pins below are release baselines, retrieved 2026-09-17:

| System | Pin and provenance | Important default distinction |
| --- | --- | --- |
| Hyprland | [`v0.56.2`](https://github.com/hyprwm/Hyprland/tree/v0.56.2), commit [`efb50993780079460b0cbed1363e2166a2de1d9f`](https://github.com/hyprwm/Hyprland/commit/efb50993780079460b0cbed1363e2166a2de1d9f), released 2026-08-05. | The first-run generated configuration embeds the tagged [`example/hyprland.lua`](https://github.com/hyprwm/Hyprland/blob/v0.56.2/example/hyprland.lua). It is an editable example, not an enforced global keymap. The unversioned wiki is Latest git and is not evidence of this release's defaults. |
| bspwm | [`0.9.12`](https://github.com/baskerville/bspwm/tree/0.9.12), commit [`c5cf7d3943f9a34a5cb2bab36bf473fd77e7d4f6`](https://github.com/baskerville/bspwm/commit/c5cf7d3943f9a34a5cb2bab36bf473fd77e7d4f6), released 2025-10-08. | bspwm has no keyboard-binding subsystem; its tagged [`examples/sxhkdrc`](https://github.com/baskerville/bspwm/blob/0.9.12/examples/sxhkdrc) requires `sxhkd` (or an equivalent `bspc` invoker), and is neither a bspwm nor distro default. bspwm does have configurable native pointer modifier/actions. |
| PaperWM | [`v50.0.1`](https://github.com/paperwm/PaperWM/tree/v50.0.1), commit [`af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b`](https://github.com/paperwm/PaperWM/commit/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b), released 2026-04-21. Tagged [`metadata.json`](https://github.com/paperwm/PaperWM/blob/v50.0.1/metadata.json) declares Shell 45-50. | Its GSettings schema supplies extension defaults. PaperWM may blank conflicting GNOME bindings while enabled and restore saved values on disable; actual conflicts remain host, distro, extension, and user-config dependent. It is a GNOME Shell extension, not a standalone compositor. |

Availability of a source or protocol does not grant permission to copy code or
configuration. This record makes no licensing-reuse claim.

## Semantics

### Hyprland

Hyprland is a Wayland compositor with compositor-owned layouts. The tagged
example chooses `dwindle`; it also configures `dwindle.preserve_split=true`,
`master.new_status="master"`, and `scrolling.fullscreen_on_one_column=true`.
Those are example choices, not universal defaults. The current official layout
documentation lists Dwindle, Master, Scrolling, Monocle, and custom layouts;
the table only states behavior verified from the tagged source/example or the
official layout documentation snapshot retrieved on the date above.

| Concern | Source-evidenced behavior and consequence |
| --- | --- |
| Structural model and opening | Dwindle is a BSPWM-like binary tree. Its ordinary split orientation follows the parent geometry unless `preserve_split`; `preselect` can direct the next tiled insertion. Master has master area(s) and a stack, with configurable orientation and new-window status. Scrolling is a horizontally growing tape of columns, and Monocle gives each tiled window the work area. These are incompatible topologies, not variants of one ordered N-ary split tree. |
| Focus and directional movement | Directional focus is `focus({ direction=... })`. `window.move({ direction=... })` and `window.swap({ direction=... })` are separate actions; the documented generic move is not COSMIC-style perpendicular tree restructuring. Cross-monitor fallback and exact target selection are compositor configuration/layout policy. |
| Groups versus splits | Hyprland groups are tabbed containers occupying one window's layout space, with `group.toggle/next/prev/active/lock` and `move into_group/out_of_group`. They are distinct from Dwindle binary splits, Master areas, and Scrolling columns. A portable split-only engine must refuse a requested tabbed-group operation unless a selected adapter declares that group capability. |
| Resize, cycling, and placement | Mouse drag floats then retiles on drop; `window.resize()` handles interactive resize. Dwindle has `splitratio`, `togglesplit`, `swapsplit`, `rotatesplit`; Master has `mfact`, master/stack swapping and orientation messages; Scrolling has column resize, fit, promote, swap-column, expel/consume messages. There is no verified global equalize command. Client minimum sizes/aspect constraints can still cause compositor geometry divergence. |
| Close and collapse | `window.close()` asks the client to close. Dwindle removes a window from its live binary tree; its ordinary splits are not permanent by default. No retained empty-region/receptacle model is evidenced. |
| Exceptions | Float is a per-window state outside tiling. `fullscreen` has `fullscreen` and `maximized` modes, with a `layout_aware` option; the two should not be represented as one operation. `pin` is documented for floating windows and shows them on all workspaces. |
| Workspaces and outputs | Workspaces are compositor-owned and can be bound to monitors by rules, kept empty with `persistent`, moved between monitors, or made special. They are not evidence for the KWin `per-output-local`, `global-unique`, or `shared` logical-workspace modes. |
| Pointer and keyboard | The example supplies Super+LMB drag and Super+RMB resize. It also supplies Super+wheel workspace traversal. Keyboard commands are all configuration-defined. |

Primary behavior sources: tagged [example](https://github.com/hyprwm/Hyprland/blob/v0.56.2/example/hyprland.lua), current official [dispatchers](https://wiki.hypr.land/Configuring/Basics/Dispatchers/), [Dwindle](https://wiki.hypr.land/Configuring/Layouts/Dwindle-Layout/), [Master](https://wiki.hypr.land/Configuring/Layouts/Master-Layout/), [Scrolling](https://wiki.hypr.land/Configuring/Layouts/Scrolling-Layout/), and [Monocle](https://wiki.hypr.land/Configuring/Layouts/Monocle-Layout/) pages. The wiki is a development snapshot retrieved 2026-09-17, not a release-pinned default configuration.

### bspwm

bspwm is an X11 window manager. It represents windows as leaves in a full
binary tree, responds to X events and its socket, and deliberately delegates
key and pointer translation to another process. Its two desktop layouts are
`tiled` and `monocle`; monocle is not a tab/group type.

| Concern | Source-evidenced behavior and consequence |
| --- | --- |
| Structural model and opening | A desktop owns a binary partition tree. The first tiled window occupies its tiling rectangle. Automatic insertion uses `longest_side`, `alternate`, or `spiral` and `initial_polarity`; manual preselection (`node -p DIR`, optional `-o RATIO`) chooses the next split. Empty receptacles can be inserted and later filled, unlike Hyprland's ordinary live Dwindle tree. |
| Focus and directional movement | `node -f DIR` selects a spatial directional node, subject to bspwm selector/focus settings. `node -s NODE` swaps nodes; `node -n NODE` reparents/sends onto a node; `node -d` and `-m` transfer to a desktop or monitor. `node -v` is free displacement for floating windows, not tiled-tree movement. |
| Groups and columns | bspwm has no tabbed/stacked group model: one window occupies each leaf. It has binary split operations, preselection, receptacles, and `monocle`, but these must not be advertised as group support. |
| Resize, cycling, and placement | `node -r` adjusts a split ratio; `-z` resizes a chosen edge/corner; `-y`, `-R`, and `-F` change split type, rotate, and flip; `-E` equalizes to default ratios, `-B` balances by area, and `-C` circulates a subtree. Pointer modifier plus button 1 moves, button 2 resizes nearest side, and button 3 nearest corner, configurable by pointer actions. |
| Close and collapse | `node -c` closes and `-k` kills. Removing a window leaf promotes/collapses its sibling subtree; a receptacle is the explicit retained-empty alternative. |
| Exceptions | States are tiled, pseudo-tiled, floating, and fullscreen. Floating remains a leaf but consumes no tiling space; fullscreen fills the monitor. Flags include hidden, sticky, private, locked, marked, and urgent. Sticky is monitor-desktop scoped, not Hyprland-style all-workspace pinning. There is no maximize state. |
| Workspaces and outputs | Monitors each show one focused desktop from a fixed, user-managed desktop list. `desktop` and `monitor` commands focus, transfer, swap, reorder, add, or remove their respective objects. RandR/Xinerama output changes and EWMH/ICCCM behavior remain X11-specific. |
| Pointer and keyboard | bspwm itself has no key or pointer binding file. Built-in pointer actions and `click_to_focus` are WM settings; the upstream keyboard catalog below exists only if the example starts `sxhkd`. |

Primary source: tagged [`bspwm(1)` source](https://github.com/baskerville/bspwm/blob/0.9.12/doc/bspwm.1.asciidoc), especially Description, Insertion modes, Pointer Bindings, Domains, Settings, Events, and Selectors; tagged [README](https://github.com/baskerville/bspwm/blob/0.9.12/README.md).

### PaperWM

PaperWM is an in-process GNOME Shell extension. Its `Space` model is an array
of columns, each an array of windows. It lays out a horizontally scrollable
strip, clips it to the monitor viewport, and scrolls the clone container to
make the selected window visible. This is neither an on-screen binary split
tree nor ordinary stacked tabs. A portable profile would require a new
column/viewport projection model rather than silently mapping it to
`cosmic_v1` N-ary splits.

| Concern | Source-evidenced behavior and consequence |
| --- | --- |
| Structural model and opening | The default open position is right of the active window. Settings can cycle right, left, start, end, down, and up; the tagged defaults enable right/left/down in that cycle. A column stacks windows vertically; columns form an unbounded horizontal strip. |
| Focus and movement | Directional and linear focus ensure the selected window is visible by scrolling the viewport. Left/right swaps whole columns; up/down swaps rows within the selected window's column. The exact tagged control flow and examples are below. |
| Column operations | `slurp-in` consumes a window into the active column. `barf-out` expels the bottom window, while `barf-out-active` expels the active window, to its own column. No generic tab group, merge-all, or split-tree reparenting API is established. |
| Resize, cycling, and placement | Width and height can increment/decrement or cycle through configured steps. `toggle-maximize-width` is horizontal maximize within the tiling geometry. Mouse edge previews can activate concealed windows; drag supports column layout and edge drift. |
| Close and collapse | `close-window` requests client deletion. The extension's window-removal handler removes the row; if that was the last row, it removes the column. This is automatic live-column collapse, not a separately bindable operation or retained empty column. |
| Exceptions | Scratch windows are detached floating, above, and sticky via extension internals. `paper-toggle-fullscreen` is separate from width maximize. No PaperWM schema action for general native sticky or full maximize was found. |
| Workspaces and outputs | PaperWM maintains one scrollable tiling per workspace. The workspace stack is shared across monitors, while additional monitors make another workspace visible. It overrides GNOME dynamic-workspace handling to retain enough workspaces for visible monitors. This does not equal KWin's three logical workspace modes. |
| Pointer, gestures, and keyboard | Top-bar/position-bar scroll changes visible windows; edge previews are clickable; touchpad swipes move the viewport or workspace stack (Wayland-only). Extension keybindings are GSettings defaults, not compositor-global immutable defaults. |

Primary source: tagged [`tiling.js`](https://github.com/paperwm/PaperWM/blob/v50.0.1/tiling.js), [`keybindings.js`](https://github.com/paperwm/PaperWM/blob/v50.0.1/keybindings.js), [`settings.js`](https://github.com/paperwm/PaperWM/blob/v50.0.1/settings.js), [`patches.js`](https://github.com/paperwm/PaperWM/blob/v50.0.1/patches.js), [`topbar.js`](https://github.com/paperwm/PaperWM/blob/v50.0.1/topbar.js), and [README](https://github.com/paperwm/PaperWM/blob/v50.0.1/README.md).

#### PaperWM `v50.0.1` Movement And Collapse Trace

The tagged key handlers call `Space.swap` for every directional move
([`keybindings.js:230-237`](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/keybindings.js#L230-L237)).
`Space.swap` finds `[column, row]`, changes column for left/right or row for
up/down, rejects an out-of-range target, swaps the row, swaps the column, then
lays out and ensures the selected window is visible
([`tiling.js:1063-1094`](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/tiling.js#L1063-L1094)).
The two swaps make the actual semantics precise:

| Before and command | After | Source result |
| --- | --- | --- |
| `[[A] [B*] [C]]`, move right | `[[A] [C] [B*]]` | Left/right leaves `row` unchanged, making the row swap a no-op and swapping whole columns. |
| `[[A] [B* B2] [C]]`, move right | `[[A] [C] [B* B2]]` | A stacked column moves intact; its row order is retained. |
| `[[A] [B* B2]]`, move down | `[[A] [B2 B*]]` | Up/down leaves `column` unchanged, making the column swap a no-op and swapping rows in that column. |
| `[[A*]]`, move up or down | unchanged | The origin column has no target row, so `Space.swap` returns. Left from the first column and right from the last also return. |

`slurp-in` calls `Tiling.slurp(mw)`, while `barf-out` calls
`Tiling.barf(mw)` and `barf-out-active` calls `Tiling.barf(mw, mw)`
([`keybindings.js:322-332`](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/keybindings.js#L322-L332)).
Their keybinding path uses the current opening-position setting and the default
bottom insertion, not a generic tree reparent operation:

| Before and command | After | Source result |
| --- | --- | --- |
| `[[A*] [B]]`, slurp with right/default opening | `[[A* B]]` | `slurp` takes row 0 from the neighboring column, appends it to the active column, and removes the now-empty source column. |
| `[[A] [B*]]`, slurp with left/start opening | `[[B* A]]` | The left neighbor is appended beneath the active member. |
| `[[A B* C]]`, barf out | `[[A B*] [C]]` | Without `expelWindow`, `barf` removes the bottom row and creates a new adjacent column. |
| `[[A B* C]]`, barf out active | `[[A C] [B*]]` | With `expelWindow`, it removes the active row and creates a new adjacent column. |

These transformations are implemented by tagged [`Tiling.slurp`
lines 5228-5310](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/tiling.js#L5228-L5310).
The tagged [`Tiling.barf` lines 5317-5359](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/tiling.js#L5317-L5359)
implements the expel cases. Each is a no-op respectively with fewer than two columns or fewer than two rows
in the active column. `close-window` calls `Meta.Window.delete`; the tagged
removal path removes the row and splices the column when it becomes empty
([`keybindings.js:318-320`](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/keybindings.js#L318-L320),
[`Space.removeWindow:981-1030`](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/tiling.js#L981-L1030),
[`remove_handler:3950-3967`](https://github.com/paperwm/PaperWM/blob/af20e70e5d3f9c83a7e1bd548fbcb9377a4b698b/tiling.js#L3950-L3967)).
For example, closing `B2*` in `[[A] [B1 B2*] [C]]` yields `[[A] [B1] [C]]`;
closing sole `B*` in `[[A] [B*]]` yields `[[A]]`.

## Default Shortcut Catalogs

These are reference baselines for profile design, not a proposed project
binding catalog. `UNBOUND` means the stated tagged baseline has no binding for
that operation; it does not mean the operation is unavailable. This project's
current US-only and Plasma conflict decisions remain unchanged.

### Hyprland `v0.56.2` Generated Example

Source and provenance for this entire table: tagged
[`example/hyprland.lua`](https://github.com/hyprwm/Hyprland/blob/v0.56.2/example/hyprland.lua),
retrieved 2026-09-17. `mainMod` is `SUPER`, expanded below as `Super`.
The example configures `kb_layout="us"`; that is an example setting, not a
portable layout solution.

| Action family | Tagged example binding | Meaning / absence |
| --- | --- | --- |
| Terminal, launcher, close | Super+Q; Super+R; Super+C | `kitty`; `hyprlauncher`; close active window. Super+E opens example `dolphin`; Super+M invokes shutdown fallback. |
| Focus and move/swap | Super+Left/Right/Up/Down | Directional focus only. Directional move and swap are **UNBOUND**. |
| Float, fullscreen, maximize | Super+V | Toggle float. Fullscreen and maximize are **UNBOUND**. |
| Layout and groups | Super+P; Super+J | Pseudotile; Dwindle-only `togglesplit`. Master/Scrolling/Monocle operations and all group actions are **UNBOUND**. |
| Resize and cycle | Super+LMB drag; Super+RMB resize | Pointer drag/resize. Keyboard resize, cycling, ratios, and balance are **UNBOUND**. |
| Workspaces | Super+1..9/0; Super+Shift+1..9/0 | Focus workspace 1..10 (`0` is 10); move active window to it. Super+wheel up/down selects `e+1`/`e-1`. |
| Special/output | Super+S; Super+Shift+S | Toggle `special:magic`; send active window there. Output focus/send and workspace-to-output operations are **UNBOUND**. |

### bspwm `0.9.12` Upstream `sxhkdrc`

Source and provenance for this entire table: tagged
[`examples/sxhkdrc`](https://github.com/baskerville/bspwm/blob/0.9.12/examples/sxhkdrc),
with the tagged [`examples/bspwmrc`](https://github.com/baskerville/bspwm/blob/0.9.12/examples/bspwmrc)
starting `sxhkd` if absent. Retrieved 2026-09-17. Brace expansion is expanded
below. These bindings do not exist unless `sxhkd` or an equivalent is running.

| Action family | Upstream example binding | Meaning / absence |
| --- | --- | --- |
| Terminal, launcher, close | Super+Return; Super+Space; Super+W; Super+Shift+W | Example `urxvt`; `dmenu_run`; close; kill. |
| Focus and swap | Super+H/J/K/L; Super+Shift+H/J/K/L | Focus west/south/north/east; swap west/south/north/east. Super+C/Shift+C cycles local visible windows; Super+P/B/Comma/Period follows parent/brother/first/second. |
| States | Super+T; Super+Shift+T; Super+S; Super+F | Tiled; pseudo-tiled; floating; fullscreen. Super+M toggles desktop tiled/monocle. Maximize is unavailable. |
| Flags and placement | Super+Ctrl+M/X/Y/Z; Super+Y | Marked/locked/sticky/private; send newest marked node to newest preselected node. |
| Preselect | Super+Ctrl+H/J/K/L; Super+Ctrl+1..9; Super+Ctrl+Space | Preselect west/south/north/east; ratio .1-.9; cancel. |
| Resize and floating displacement | Super+Alt+H/J/K/L; Super+Alt+Shift+H/J/K/L; Super+Arrows | Expand edge; contract edge; move floating window. Rotate, flip, equalize, balance, and circulate are **UNBOUND**. |
| Workspaces and outputs | Super+1..9/0; Super+Shift+1..9/0; Super+[ / ]; Super+Tab | Focus/send desktop I..X; previous/next local desktop; last desktop. Monitor focus/send/swap bindings are **UNBOUND**. |
| Mouse | WM `pointer_modifier` plus button 1/2/3 | Move; nearest-side resize; nearest-corner resize. These are bspwm settings, not `sxhkdrc` bindings. |

### PaperWM `v50.0.1` Schema Defaults

Source and provenance for all three tables: tagged
[`org.gnome.shell.extensions.paperwm.gschema.xml`](https://github.com/paperwm/PaperWM/blob/v50.0.1/schemas/org.gnome.shell.extensions.paperwm.gschema.xml),
registered by tagged [`keybindings.js`](https://github.com/paperwm/PaperWM/blob/v50.0.1/keybindings.js),
retrieved 2026-09-17. `[]` schema values are shown as `UNBOUND`.

| Action family | Default bindings | Meaning / absence |
| --- | --- | --- |
| Open, close, focus | Super+Return or Super+N; Super+BackSpace; Super+Period/Comma; Super+arrows; Super+Home/End | New same-application window; close; next/previous; directional; first/last. Loop/global/numbered-position focus is **UNBOUND**. |
| Move and columns | Super+Ctrl+Period or Super+Shift+Period or Super+Ctrl+Right; corresponding Comma/Left; Super+Ctrl+Up/Down; Super+I; Super+O; Super+Shift+O; Super+T | Move whole column right/left; swap active row up/down; slurp a neighbor; barf bottom row; barf active row; take-and-drop navigation. |
| Resize and view | Super+Plus/Minus; Super+Shift+Plus/Minus; Super+R; Super+Alt+R; Super+Shift+R; Super+Alt+Shift+R; Super+[ / ] | Width increment/decrement; height increment/decrement; cycle width/height forward/back; drift viewport. |
| Float, maximize, fullscreen | Super+F; Super+Shift+F; Super+Escape / Shift+Escape / Ctrl+Escape | Width maximize; fullscreen; show scratch / scratch layer / attach scratch. General float and sticky bindings are **UNBOUND**. |
| Workspace | Super+PageDown/PageUp; Super+Ctrl+PageDown/PageUp; Super+Above_Tab; Super+Shift+Above_Tab | Switch or send within current-monitor workspace sequence; previous workspace and reverse. All-monitors sequence bindings are **UNBOUND**. |
| Output and workspace-output | Super+Shift+arrows; Super+Ctrl+Shift+arrows; Ctrl+Alt+Shift+arrows; Super+Alt+arrows | Focus monitor; send active window; move workspace; swap workspace with directional monitor. |
| Modes and panel | Super+Shift+C; Super+Shift+W; Super+Ctrl+B | Cycle focus mode; cycle opening/drop position; toggle top and position bars. Direct opening-position selectors and individual bar toggles are **UNBOUND**. |

PaperWM compares its bindings with Mutter, GNOME Desktop, Shell, media-key, and
known extension schemas. On conflicts it records the original values in
`restore-keybinds`, changes conflicting GNOME entries to empty arrays while
enabled, and restores them on disable. Its runtime also changes
`attach-modal-dialogs`, `workspaces-only-on-primary`, and `edge-tiling` while
enabled. These are source-evidenced coexistence effects, not a recommendation
to override any project or GNOME binding. `Above_Tab` means the physical key
above Tab (grave on US QWERTY); PaperWM documents period/comma as US-marked
`>`/`<` keys and provides no complete non-US binding policy.

## Native Integration And Panel Boundaries

| Host | Public state/control surface | Structural-authority limit | Panel handoff |
| --- | --- | --- | --- |
| Hyprland | [`hyprctl`](https://wiki.hypr.land/Configuring/Using-hyprctl/) queries plus [IPC](https://wiki.hypr.land/IPC/) request and event sockets expose clients, monitors, workspaces, dispatchers, and events. Dispatchers can focus/move windows and workspaces or send layout messages. | Socket commands are per-operation control with asynchronous re-observation, not an atomic multi-window layout transaction or a portable tree API. An external Wayland client has no generic geometry authority. A native layout plugin is C++ ABI/hash coupled; its documented API/hash checks and `hyprpm` pins must match the selected release. | Waybar's Hyprland workspaces module consumes IPC state and sends workspace dispatches on click. It is not workspace authority; config-provider/Lua protocol compatibility must be pinned. Hyprland ships no bar. |
| bspwm | [`bspc`](https://github.com/baskerville/bspwm/blob/0.9.12/doc/bspwm.1.asciidoc) `query -T`, `query -N/-D/-M`, and `subscribe` observe tree/state/events; node/desktop/monitor commands control them. | The WM owns the binary tree, preselection, ratios, focus history, and pointer policy. There is no plugin ABI and no documented atomic batch. A separate Rust tree plus `bspc` writers would be competing authorities. X11-only scope is explicit. | Polybar's bspwm module and the upstream lemonbar example consume `bspc subscribe report`; click/scroll actions send `bspc desktop -f`. Such a bar is a second command writer, not an authority. |
| GNOME/PaperWM | Only an in-process Shell extension can use `Meta.Window`, `Meta.Workspace`, `Main.wm.addKeybinding`, and Shell actors. External Wayland clients do not receive this window-management authority. | PaperWM already controls geometry, GNOME workspace policy, key conflicts, gestures, overview patches, and top-bar actors. Co-enabling another tiler is two layout authorities. PaperWM identifies Tiling Assistant and several workspace/panel/gesture extensions as conflicting classes. | PaperWM replaces Activities with a workspace indicator/name and adds focus/open-position controls and a position bar. GNOME's native workspace UI remains the alternative. Treat either as a native status surface, not custom workspace authority. |

The general Wayland constraint is source-backed: `ext-foreign-toplevel-list-v1`
is list-only, while taskbar-oriented foreign-toplevel management is not a
layout-tree protocol. Hyprland-specific sockets do not make that API portable,
and Mutter requires extension context. See the GNOME/Wayland evidence in
[cross-platform feasibility](cross-platform-support/feasibility.md#gnome).

## Support Paths

Source-backed limitations are above. The following is a proposal for later
approval, not a current decision.

| Path | Feasible scope | Boundary and cost |
| --- | --- | --- |
| A. Portable profiles first | Add sealed `hyprland_v1`, `bspwm_v1`, or `paperwm_v1` policy IDs with their matching shortcut catalogs and capability requirements. Replay versioned offline observations through the transport-free contract. | Hyprland needs separate layout policies rather than one generic profile. bspwm needs binary-tree/receptacle semantics. PaperWM needs columns plus viewport state. Unsupported tabs/groups/gestures fail closed rather than being redefined as splits. No native backend follows from this path. |
| B. Hyprland IPC adapter | A selected, version-pinned normal-window subset could observe via JSON/events and issue dispatcher commands, with post-observation verification. | Hyprland remains layout authority. Exact structural insertion, atomic multi-window edit, renderer behavior, gestures, workspace ownership, client-hint handling, and hotplug guarantees are unavailable from this surface. |
| C. Hyprland layout plugin | A selected plugin could become the layout authority rather than fight it. | C++ in-process plugin, exact Hyprland/dependency hash and API compatibility, per-release build/test/packaging cost. Only consider after an approved journey proves IPC insufficient. |
| D. bspwm socket adapter | A selected X11-only adapter could replay `query -T` and `subscribe` fixtures and drive bounded `bspc` operations. | There is no native plugin path. The binary tree and external shortcut/panel writers remain bspwm-owned unless a single-authority handoff is designed and verified. |
| E. PaperWM behavior profile | Offline policy vectors can represent PaperWM columns, opening, slurp/barf, focus visibility, and shortcut semantics. | This requires a model/projection decision distinct from `cosmic_v1`; it supplies no GNOME integration. |
| F. GNOME extension alternative | A thin, pinned project extension could implement a selected PaperWM-like profile with `Meta.*` authority. | It must be an alternative to PaperWM, not a co-installed tiler. It needs per-Shell-major compatibility, enable/disable restoration, extension review/delivery, and GNOME-native shortcut conflict handling. |

Neither native-only first ports nor adapter-owned managed logical workspaces are
silently selected here. As established in [cross-platform feasibility](cross-platform-support/feasibility.md#workspace-and-overview-alternatives), native workspaces are a reduced but useful scope; a managed logical model with offscreen placement is a credible separate choice for portable numbered switch/send/per-output mapping, with explicit shell UX and recovery tradeoffs. Capability limits must refuse unsupported requested behavior, not redefine it.

### Proposed Evidence Order

1. Finish the current all-settings-live launch blocker before expanding profile
   settings. Define profile settings semantically, including algorithm, matching
   shortcut catalog, host capability, conflict state, and live-apply result;
   keep KCM, native Qt, and web frontends as unselected presentation choices.
2. Select one profile semantics target and write pure, versioned golden vectors.
   Include initial insertion, focus, directional move/swap/reparent, group or
   column refusal, resize/cycle, close/collapse, exception states, workspace,
   output, and client minimum-size/aspect drift outcomes.
3. Add only offline fixtures: Hyprland `clients/workspaces/monitors/binds` plus
   event transcripts; bspwm `query -T`, names/config, and `subscribe` events;
   PaperWM schema and source-derived normalized column/viewport traces. Redact
   native handles and replay through the existing contract/reconcile harness.
4. If separately approved, run a bounded capability prototype for one selected
   host/version: verify observation identity, one normal-window geometry/focus
   operation, shortcut collision behavior, workspace state/control, hotplug,
   and final re-observation. Do not enable plugins, extensions, or a panel in
   that prerequisite.
5. Choose one layout authority before implementation. Add native renderer,
   gesture, plugin, panel, or managed-workspace work only after the selected
   normal-window path proves its contract.

### Decisions Still Required

- Which meaning of support is wanted per system: portable profile, native
  integration, or portable profile followed by a bounded native prototype?
- Which exact profile and version are first, and which Hyprland layout is in
  scope? "Hyprland" alone is not one layout policy.
- Is PaperWM an emulated profile on another host, or a GNOME-native alternative?
  Co-installation with PaperWM needs an explicit single-authority design.
- Are non-identical host workspace modes and shortcut chords acceptable, or is a
  managed logical-workspace prototype required for a particular journey?
- Which group types are supported? Hyprland tab groups and PaperWM columns are
  not current split-tree groups.
- What keyboard-layout/conflict policy applies beyond the current US baseline?
- May a panel only display state, or may it issue native workspace commands?
  The recommended next panel research should keep it non-authoritative and
  decide click ownership/reversibility separately.

## Source Index

- Hyprland `v0.56.2`: [release](https://github.com/hyprwm/Hyprland/releases/tag/v0.56.2), [generated example](https://github.com/hyprwm/Hyprland/blob/v0.56.2/example/hyprland.lua), [IPC](https://wiki.hypr.land/IPC/), [plugin API guidance](https://wiki.hypr.land/Plugins/Development/Getting-Started/), and [plugin guidelines](https://wiki.hypr.land/Plugins/Development/Plugin-Guidelines/). Wiki pages are dated development documentation, retrieved 2026-09-17.
- bspwm `0.9.12`: [release](https://github.com/baskerville/bspwm/releases/tag/0.9.12), [README](https://github.com/baskerville/bspwm/blob/0.9.12/README.md), [manpage source](https://github.com/baskerville/bspwm/blob/0.9.12/doc/bspwm.1.asciidoc), [example `sxhkdrc`](https://github.com/baskerville/bspwm/blob/0.9.12/examples/sxhkdrc), [example panel](https://github.com/baskerville/bspwm/blob/0.9.12/examples/panel/panel), and [Polybar bspwm module](https://github.com/polybar/polybar/wiki/Module:-bspwm). The first four are primary upstream sources; Polybar is ecosystem evidence only.
- PaperWM `v50.0.1`: [release](https://github.com/paperwm/PaperWM/releases/tag/v50.0.1), [metadata](https://github.com/paperwm/PaperWM/blob/v50.0.1/metadata.json), [schema](https://github.com/paperwm/PaperWM/blob/v50.0.1/schemas/org.gnome.shell.extensions.paperwm.gschema.xml), [tiling](https://github.com/paperwm/PaperWM/blob/v50.0.1/tiling.js), [keybindings](https://github.com/paperwm/PaperWM/blob/v50.0.1/keybindings.js), [settings](https://github.com/paperwm/PaperWM/blob/v50.0.1/settings.js), [patches](https://github.com/paperwm/PaperWM/blob/v50.0.1/patches.js), [topbar](https://github.com/paperwm/PaperWM/blob/v50.0.1/topbar.js), and [README](https://github.com/paperwm/PaperWM/blob/v50.0.1/README.md).
- GNOME and cross-host limits: [current feasibility record](cross-platform-support/feasibility.md#gnome) and its pinned GNOME/Mutter/Wayland source list. It distinguishes public extension authority from external Wayland capabilities.

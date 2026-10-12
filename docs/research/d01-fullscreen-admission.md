# D01 / INS-06: admission and explicit focus over fullscreen

- Status: research recommendation, pending user review, 2026-10-12. No policy
  approval or product implementation. Source analysis only; no runtime tests.
- Finding: "keep the game shown and focused" is not synonymous with native
  gaming compatibility. KWin 6.6.3 normally activates new Wayland windows at
  shipped Low focus-stealing prevention, and Alt+Tab brings the chosen window
  above an unrelated fullscreen game without clearing its fullscreen flag.
- Recommendation: structurally admit C, retain B's overlay state/slot and
  geometry protections, preserve the host's admission activation decision,
  and honor explicit user focus switches. Do not force either C or B into
  focus on automatic admission. This revises the proposed fullscreen focus
  rule, not an approved decision.

## Fixtures and tally

- Ordinary resizable top-level C, unrelated to B, no special rules; same shown
  workspace/output. Start with B fullscreen and focused. Maximize is a fresh
  separate leg, where supported. Dialogs/transients are separate cases.
- Explicit-selection leg: select existing A by target/cycle/taskbar-equivalent
  action. Directional navigation can be fenced even when target selection is
  allowed; the two actions must not be conflated.
- Pins below were checked. Eight non-scrolling profiles vote. niri is useful
  context but does not vote; PaperWM, karousel and paneru do not vote either.
- Source evidence describes compositor policy, not measured game behavior or
  animation/pixel settlement. "Retained below" is not a fullscreen-state exit.

| WM / pin | New C over fullscreen, default | Focus | B fullscreen exit? | Options / qualifications | Source |
|---|---|---|---|---|---|
| COSMIC `3d55cba0` | C tiled and rendered above B | C | No; B retained below | Same-output/workspace map focus; no FSP check on this branch | [C](#cosmic) |
| Hyprland/Dwindle `19fb395d` | B cleared; C visible tiled | C | Yes | `misc:on_focus_under_fullscreen=2`; 0 ignore, 1 take over. Floating windows use a separate above-fullscreen path | [H](#hyprland) |
| bspwm `e11eff4` | B restored to last state; C visible tiled | C | Yes | Focus neutralizes an occluding fullscreen window; no equivalent policy option in this path | [B](#bspwm) |
| i3 `903bcd51` | C admitted behind B | B | No | `popup_during_fullscreen=smart` default affects transients, not ordinary C | [I](#i3) |
| sway `1652c54b` | C admitted behind B, hidden by fullscreen visibility gate | B | No | `popup_during_fullscreen=smart` default affects transients, not ordinary C | [S](#sway) |
| xmonad/Tall + EWMH `284dd52` / contrib `5097a45` | C tiled behind B's fullscreen float | C | No | Profile uses contrib `ewmhFullscreen` hooks; not a core fullscreen state | [X](#xmonad) |
| awesome/tile `0a5e50cf` | Shipped rule raises C above B | C | No; B loses fullscreen stacking layer | `focus` rule plus `raise=true`; focus-only without raise is different | [A](#awesome) |
| qtile/Columns `83c697a5` | C tiles; final cover backend-dependent, unverified | C | No | `auto_fullscreen=true`, `floats_kept_above=true`; X11 demotion does not prove C above; Wayland separate layer | [Q](#qtile) |
| niri `ed22699d` (context, no vote) | C opens another column; view stays on B | B | No | Default Smart activation suppresses C under active pending fullscreen | [N](#niri) |

- Focus tally: C 6/8, B 2/8. Fullscreen state: exit 2/8, retain 6/8.
- Visibility tally: C visible 4/8 (COSMIC, Hyprland, bspwm, awesome), behind
  3/8 (i3, sway, xmonad), unsettled/backend-qualified 1/8 (qtile). Neither
  focus nor visibility supports a blanket majority claim for "stay behind".
- Maximize tally, only four applicable voting profiles: C focused 4/4;
  B unmaximized 2/4 (COSMIC, Hyprland), retained 2/4 (awesome, qtile).
  bspwm/i3/sway and this xmonad profile have no maximize counterpart.

| WM | New C while B maximized | Explicit selection of existing A while B fullscreen |
|---|---|---|
| COSMIC | Unmaximizes B, tiles/focuses C | A focused/rendered above; B retained fullscreen below |
| Hyprland | Default mode 2 clears maximize, tiles/focuses C | Tiled target: default exits B; 0 redirects to B, 1 transfers fullscreen. Directional `movefocus` fenced by default `binds:movefocus_cycles_fullscreen=false`; direct/cycle selection differs |
| bspwm | No counterpart | A focus clears occluding B to last state |
| i3 | No counterpart | Criteria focus / pager activation exits B and focuses A; directional/relative navigation can stay fenced |
| sway | No counterpart | Explicit target `focus` disables covering fullscreen and focuses A; directional focus goes to outputs instead |
| xmonad profile | No counterpart | A gets focus but remains below retained B fullscreen float |
| awesome | B retained at normal layer; C focused/raised above | Shipped `focus.byidx` raises A, so B retained below. Pure focus-only does not demote B |
| qtile | B removed from tiled layout into maximized float; C tiles/focuses; cover unverified | A gets group focus; B retained. X11 demotes B's layer but may leave it atop that layer; Wayland cover unverified |
| niri (no vote) | B's maximized column retained; C activates and view fits C | A becomes visible by viewport movement; B retains fullscreen offscreen, no exit |

## Reference source evidence

### COSMIC

- [Map policy, `src/shell/mod.rs:2997-3034`](https://github.com/pop-os/cosmic-comp/blob/3d55cba0/src/shell/mod.rs#L2997-L3034): normal admission unmaximizes existing maximized tiles, maps C, and returns same-workspace newcomer focus; TODO explicitly notes missing FSP on this branch.
- [Focus application, `src/wayland/handlers/compositor.rs:405-416`](https://github.com/pop-os/cosmic-comp/blob/3d55cba0/src/wayland/handlers/compositor.rs#L405-L416) and [focus stack updates, `src/shell/focus/mod.rs:142-224`](https://github.com/pop-os/cosmic-comp/blob/3d55cba0/src/shell/focus/mod.rs#L142-L224).
- [Render, `src/shell/workspace.rs:1646-1849`](https://github.com/pop-os/cosmic-comp/blob/3d55cba0/src/shell/workspace.rs#L1646-L1849): focused fullscreen emits first; when C/A is focused, normal layers emit before cached fullscreen elements. [Caller, `src/backend/render/mod.rs:718-1082`](https://github.com/pop-os/cosmic-comp/blob/3d55cba0/src/backend/render/mod.rs#L718-L1082) and [Smithay front-to-back convention, rev `e3d461a`, damage renderer](https://github.com/Smithay/smithay/blob/e3d461a/src/backend/renderer/damage/mod.rs#L323) establish that B is **below**, not above, C/A. Retaining a fullscreen target does not mean retaining its visual cover.

### Hyprland

- [Defaults, `src/config/values/ConfigValues.cpp:587-590,623`](https://github.com/hyprwm/Hyprland/blob/19fb395d/src/config/values/ConfigValues.cpp#L587-L623): `misc:on_focus_under_fullscreen=2`, `misc:exit_window_retains_fullscreen=0`, `binds:movefocus_cycles_fullscreen=false`.
- [Map, `src/desktop/view/window/Window.cpp:1498-1588`](https://github.com/hyprwm/Hyprland/blob/19fb395d/src/desktop/view/window/Window.cpp#L1498-L1588): modes ignore/take-over/exit; ordinary tiled C differs from floating C. [Explicit focus, `src/desktop/state/FocusState.cpp:34-87`](https://github.com/hyprwm/Hyprland/blob/19fb395d/src/desktop/state/FocusState.cpp#L34-L87); [directional fence, `WindowQuery.cpp:111-125`](https://github.com/hyprwm/Hyprland/blob/19fb395d/src/desktop/state/WindowQuery.cpp#L111-L125).
- `misc:new_window_takes_over_fullscreen` is absent from the pinned option inventory; do not present that historical name as an option/alias at this pin. Layout-managed fullscreen variants are outside this default Dwindle fixture.

### bspwm

- [Manage focus, `src/window.c:214-220`](https://github.com/baskerville/bspwm/blob/e11eff4/src/window.c#L214-L220), [focus neutralization, `src/tree.c:600-602`](https://github.com/baskerville/bspwm/blob/e11eff4/src/tree.c#L600-L602), [restore occluding fullscreen, `tree.c:1989-2004`](https://github.com/baskerville/bspwm/blob/e11eff4/src/tree.c#L1989-L2004), [fullscreen stack level, `src/stack.c:123-133`](https://github.com/baskerville/bspwm/blob/e11eff4/src/stack.c#L123-L133).
- `ignore_ewmh_fullscreen` concerns accepting fullscreen requests, not this unconditional focus-neutralization path. State inventory has tiled/pseudo-tiled/floating/fullscreen, not maximize.

### i3

- [Map fullscreen branch, `src/manage.c:423-459`](https://github.com/i3/i3/blob/903bcd51/src/manage.c#L423-L459); [transient popup exceptions, `manage.c:492-526`](https://github.com/i3/i3/blob/903bcd51/src/manage.c#L492-L526). Default `PDF_SMART=0` in [configuration enum](https://github.com/i3/i3/blob/903bcd51/include/configuration.h#L257-L271). `leave_fullscreen`, `smart`, `all`, `ignore` apply to popup behavior, not ordinary tiled C.
- [Explicit criteria selection, `src/commands.c:1468-1477`](https://github.com/i3/i3/blob/903bcd51/src/commands.c#L1468-L1477) calls [unblock, `src/con.c:302-311`](https://github.com/i3/i3/blob/903bcd51/src/con.c#L302-L311), which disables covering fullscreen. [Pager source, `src/handlers.c:779-809`](https://github.com/i3/i3/blob/903bcd51/src/handlers.c#L779-L809) also unblocks; app requests are separately controlled by `focus_on_window_activation` (default smart).
- [Directional focus permission, `src/con.c:2327-2359`](https://github.com/i3/i3/blob/903bcd51/src/con.c#L2327-L2359) is not an explicit-target prohibition. Maximize is derived metadata, not a maximize verb/state.

### sway

- [Map focus gate, `sway/tree/view.c:696-709`](https://github.com/swaywm/sway/blob/1652c54b/sway/tree/view.c#L696-L709); [transient-only leave, `view.c:918-926`](https://github.com/swaywm/sway/blob/1652c54b/sway/tree/view.c#L918-L926); [visibility, `view.c:1187-1194`](https://github.com/swaywm/sway/blob/1652c54b/sway/tree/view.c#L1187-L1194). Defaults [config.c:257-258](https://github.com/swaywm/sway/blob/1652c54b/sway/config.c#L257-L258): activation urgent, popup smart. `popup_during_fullscreen` values smart/ignore/leave_fullscreen do not let ordinary C steal focus on map.
- [Explicit target focus, `sway/commands/focus.c:405-412`](https://github.com/swaywm/sway/blob/1652c54b/sway/commands/focus.c#L405-L412) disables the obstruction first; [directional fullscreen branch, `focus.c:138-156`](https://github.com/swaywm/sway/blob/1652c54b/sway/commands/focus.c#L138-L156) differs. Raw seat focus remains fenced by [seat.c:1148-1156](https://github.com/swaywm/sway/blob/1652c54b/sway/input/seat.c#L1148-L1156).

### xmonad

- [Manage, `src/XMonad/Operations.hs:107-124`](https://github.com/xmonad/xmonad/blob/284dd52/src/XMonad/Operations.hs#L107-L124) inserts C with focus; [arrange/restack, `Operations.hs:183-204`](https://github.com/xmonad/xmonad/blob/284dd52/src/XMonad/Operations.hs#L183-L204) puts floats before tiles; [explicit focus, `Operations.hs:397-409`](https://github.com/xmonad/xmonad/blob/284dd52/src/XMonad/Operations.hs#L397-L409) retains the floating map. [Xlib restack convention](https://www.x.org/releases/current/doc/libX11/libX11/libX11.html#Changing_Window_Stacking_Order): first listed window stays top, subsequent windows stack underneath.
- [Contrib EWMH hooks, `XMonad/Hooks/EwmhDesktops.hs:296-309,682-709`](https://github.com/xmonad/xmonad-contrib/blob/5097a45/XMonad/Hooks/EwmhDesktops.hs#L296-L309): `doFullFloat`/`doSink`; no admission/focus sink. This is the documented profile, not a vote for every configurable xmonad layout/hook.

### awesome

- [Shipped global rule, `awesomerc.lua:470-478`](https://github.com/awesomeWM/awesome/blob/0a5e50cf/awesomerc.lua#L470-L478), [rule activation, `lib/ruled/client.lua:662-665`](https://github.com/awesomeWM/awesome/blob/0a5e50cf/lib/ruled/client.lua#L662-L665), [activation raises, `lib/awful/permissions/init.lua:197-208`](https://github.com/awesomeWM/awesome/blob/0a5e50cf/lib/awful/permissions/init.lua#L197-L208).
- [Fullscreen layer, `stack.c:129-159`](https://github.com/awesomeWM/awesome/blob/0a5e50cf/stack.c#L129-L159) depends on **most recently raised**, not focus alone. [Cycle selection, `lib/awful/client/focus.lua:67-72`](https://github.com/awesomeWM/awesome/blob/0a5e50cf/lib/awful/client/focus.lua#L67-L72) explicitly raises. B's fullscreen boolean persists; maximized has no special stacking layer. Earlier matrix "C underneath B" was incorrect for shipped admission rules.

### qtile

- [Group admission, `libqtile/group.py:226-244`](https://github.com/qtile/qtile/blob/83c697a5/libqtile/group.py#L226-L244) focuses stealable C; [mark-floating, `group.py:304-320`](https://github.com/qtile/qtile/blob/83c697a5/libqtile/group.py#L304-L320) removes maximized B from Columns but keeps fullscreen B's slot. Defaults [default_config.py:180-198](https://github.com/qtile/qtile/blob/83c697a5/libqtile/resources/default_config.py#L180-L198).
- X11 [focus, `backend/x11/window.py:1295-1332`](https://github.com/qtile/qtile/blob/83c697a5/libqtile/backend/x11/window.py#L1295-L1332), [check-stacking, `core.py:944-952`](https://github.com/qtile/qtile/blob/83c697a5/libqtile/backend/x11/core.py#L944-L952), [layer/restack, `window.py:910-1180`](https://github.com/qtile/qtile/blob/83c697a5/libqtile/backend/x11/window.py#L910-L1180): B loses focused-fullscreen layer, but demotion places it at the top of the shared layer (or keeps its order). Thus demotion alone does **not** establish C above; behind is the source-inferred outcome, final stacking unverified.
- Wayland [state layers, `backend/wayland/window.py:634-641`](https://github.com/qtile/qtile/blob/83c697a5/libqtile/backend/wayland/window.py#L634-L641) use LAYER_MAX/LAYER_FULLSCREEN; focus path does not establish cross-layer paint order. Backend-specific render/settlement remains TBD.

### niri

- [Smart gate, `src/layout/workspace.rs:651-656`](https://github.com/YaLTeR/niri/blob/ed22699d/src/layout/workspace.rs#L651-L656): active pending fullscreen suppresses Smart activation; maximize does not. [Column focus, `workspace.rs:950-1059`](https://github.com/YaLTeR/niri/blob/ed22699d/src/layout/workspace.rs#L950-L1059) changes active column without exiting B; [fullscreen state, `workspace.rs:1288-1351`](https://github.com/YaLTeR/niri/blob/ed22699d/src/layout/workspace.rs#L1288-L1351). Explicit selection moves the viewport rather than removing B's fullscreen state.

## Native KWin baseline

- Version: **v6.6.3**, the permitted 6.6.x fallback. Requested `8438567a`
  could not be retrieved; no 6.7.4 behavior is inferred from master.
- Default FSP is **Low (1)** in [src/kwin.kcfg:109-113](https://github.com/KDE/kwin/blob/v6.6.3/src/kwin.kcfg#L109-L113). The stale source comment calling Normal the default is not configuration evidence.
- **Wayland ordinary new windows:** [src/workspace.cpp:918-943](https://github.com/KDE/kwin/blob/v6.6.3/src/workspace.cpp#L918-L943) explicitly allows new input-taking, non-popup/non-desktop windows at FSP <= Low, including while B is fullscreen. No fullscreen-special deny exists here. At Medium+ activation depends on [token/transient policy, src/activation.cpp:572-626](https://github.com/KDE/kwin/blob/v6.6.3/src/activation.cpp#L572-L626); denied windows restack under the active window, with attention requested when a denied activation token was supplied. Rules/minimized/no-input/special windows are separate qualifiers.
- **X11/Xwayland new windows:** [src/x11window.cpp:848-902](https://github.com/KDE/kwin/blob/v6.6.3/src/x11window.cpp#L848-L902) calls `allowWindowActivation(userTime(), false)`. [Policy, x11window.cpp:4348-4430](https://github.com/KDE/kwin/blob/v6.6.3/src/x11window.cpp#L4348-L4430) allows fresh timestamps/same-app cases; explicit user-time 0 denies; Low allows unknown timestamps when active protection is below High. [Timestamp provenance, x11window.cpp:4191-4279](https://github.com/KDE/kwin/blob/v6.6.3/src/x11window.cpp#L4191-L4279) includes startup/creation time and same-app existing-window checks. A stale/zero-time background spawn can stay behind and demand attention; a user launch can activate. "Fullscreen means deny" is false.
- **Stacking:** [Window::belongsToLayer, src/window.cpp:553-599](https://github.com/KDE/kwin/blob/v6.6.3/src/window.cpp#L553-L599) gives active fullscreen ActiveLayer; ordinary/maximized windows otherwise use NormalLayer unless other flags apply. [isActiveFullScreen, window.cpp:2324-2335](https://github.com/KDE/kwin/blob/v6.6.3/src/window.cpp#L2324-L2335) keeps B high if active, if active window is on another output, or if active is its transient. Accepted unrelated C on the **same** output takes focus/raises, B ceases active-fullscreen stacking eligibility, and C can appear above while B's fullscreen flag remains. [setActive layer update, window.cpp:488-512](https://github.com/KDE/kwin/blob/v6.6.3/src/window.cpp#L488-L512); [stack layer rebuild, src/layers.cpp:578-591](https://github.com/KDE/kwin/blob/v6.6.3/src/layers.cpp#L578-L591). X11 groups/transients impose additional constraints; exact game frames/input settlement untested.
- **Alt+Tab/taskbar:** [TabBox::accept, src/tabbox/tabbox.cpp:979-987](https://github.com/KDE/kwin/blob/v6.6.3/src/tabbox/tabbox.cpp#L979-L987) calls `activateWindow(c)` (not force=true). [activateWindow, src/activation.cpp:286-350](https://github.com/KDE/kwin/blob/v6.6.3/src/activation.cpp#L286-L350) raises/requests focus without an FSP check or fullscreen clear. [Pager FromTool, src/netinfo.cpp:155-179](https://github.com/KDE/kwin/blob/v6.6.3/src/netinfo.cpp#L155-L179) uses forced activation; FromApplication is FSP-gated. Steam can therefore come in front of an unrelated fullscreen game by explicit user selection; keeping the game focused would defeat that native action.
- **Tiler focus setter:** [src/scripting/workspace_wrapper.cpp:232-234](https://github.com/KDE/kwin/blob/v6.6.3/src/scripting/workspace_wrapper.cpp#L232-L234) routes `activeWindow` assignment to `activateWindow`, without re-running map FSP. A script write is not "let FSP decide". Native activation precedes `windowAdded` on the normal [Wayland path](https://github.com/KDE/kwin/blob/v6.6.3/src/workspace.cpp#L935-L943); admission should preserve that result rather than replay a stale focus intent.

## Windows 11 baseline (Microsoft documentation)

- [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow) restricts programmatic foreground changes: foreground lock/menu conditions plus qualifying input, launch ownership or timeout. Even qualifying requests can be denied; denied apps can flash their taskbar button. New window creation alone does not guarantee foreground rights.
- [LockSetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-locksetforegroundwindow) is released by Alt input or a system-caused foreground change. Explicit user switching and unsolicited application activation are distinct; a tiler should not fight the user's switch.
- [DXGI SetFullscreenState](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiswapchain-setfullscreenstate) documents exclusive-mode unavailability when unfocused/occluded or another app is fullscreen. [DXGI flip-model guidance](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/for-best-performance--use-dxgi-flip-model) describes faster Alt+Tab for windowed/borderless flip presentation. These do not promise arbitrary new windows will draw above an exclusive game, or one universal minimize/exit outcome across games and fullscreen optimizations. Windows-native runtime parity remains unverified; do not encode KWin layering in the shared core.

## Current OmniTiler KDE behavior

- Read at `01be284`: ordinary automatic admission uses retained convergence,
  not an unconditional "focus newcomer" operation. [Normal admissions,
  world.rs:1032-1102](../../crates/tiler-core/src/session/world.rs#L1032-L1102)
  tile C; [focus rule, world.rs:1103-1128](../../crates/tiler-core/src/session/world.rs#L1103-L1128)
  selects observed tiled focus if valid, otherwise retained focus/MRU. Thus
  observed B normally yields B, observed C yields C. Fullscreen/maximized B
  retains its tile slot.
- [Geometry guards, plan-adapter.ts:9775-9791](../../kwin/src/plan-adapter.ts#L9775-L9791)
  skip B's fullscreen/maximized geometry. [Focus application:9947-10007](../../kwin/src/plan-adapter.ts#L9947-L10007)
  skips background/toggle-float and reconcile-over-floating/sticky, but has
  no fullscreen-specific fence. It only calls `setActive` if live active
  identity differs from the reply target. [Entry setter:4938-4941](../../kwin/src/plan-adapter-entry.ts#L4938-L4941)
  assigns `workspace.activeWindow`.
- Stable cases normally cause **no focus write**: native kept B -> plan B;
  native activated C -> plan C. A stale/divergent reply or focus change during
  application can still cause a write to B or C without a fullscreen guard.
  Therefore "always steals focus to C today" is incorrect; absence of a fence
  establishes an interference path, not its frequency or a measured outcome.
- [Backlog D01](../backlog.md) calls admission needs-code for the prior
  blanket-fullscreen proposal; [REQ-INS-06](../spec/functional-spec.md#insertion)
  remains PROVISIONAL. That implementation assessment is relative to the old
  tentative target, not evidence that a blanket B-focus policy is desirable.
  Native visual/input settlement and joint CLOSE-05 cleanup remain TBD.

## Principle, options and recommendation

- [Gaming Compatibility](../principles.md#gaming-compatibility): games must run
  exactly as without the tiler, including no unwanted focus changes. Native
  host activation and explicit user selection belong to that baseline.
  "No unwanted focus changes" does not mean "never let another window focus".

| Option (unapproved) | Consequences |
|---|---|
| 1. Native activation ownership (recommended) | Admit C structurally, preserve B's overlay state/slot and geometry; observe/adopt host focus without an automatic admission focus override. Host-approved C can appear above; host-denied C stays behind. Explicit user selection remains effective. Best match for native gaming and host FSP configuration |
| 2. Blanket fullscreen protection / tile behind | Force B to remain focused/visible regardless of native decision. Predictable interruption shield, like i3/sway ordinary admission, but changes KWin Low behavior and can fight native activation. Explicit selection must still escape; merely skipping a tiler write cannot undo host activation |
| 3. Always show/focus C | Matches six profiles' newcomer focus but can override native denial, interrupt games, and focus invisible C in retained-cover policies. Clearing B fullscreen additionally changes game state and is not necessary for KWin Alt+Tab |
| 4. Queue C until overlay clears | Avoids admission effects but delays normal app use, especially under maximize; does not stop a host-created native window from already activating. Adds lifecycle complexity without establishing native parity |

- Recommend option 1 for KDE, with maximize and fullscreen separate in evidence
  but the same native activation ownership principle. Keep explicit selection
  separate from unsolicited activation (D27's tentative guard is not an
  Alt+Tab policy). No approval, setting or shared-core rule is inferred.
- Verification needed before implementation claims: compare tiler-off/on on
  the same host configuration for native Wayland open at Low/Medium, Xwayland
  fresh/stale activation, Alt+Tab/taskbar selection to Steam-like ordinary A,
  separate maximized B, and overlay-close cleanup. Record focus, visible
  pixels/input recipient and B's fullscreen flag independently. Those native
  outcomes remain unverified; research supplies the discriminating sequences.

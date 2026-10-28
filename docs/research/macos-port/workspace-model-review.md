# macOS workspace model review

- Date: 2026-10-28. Source-only synthesis; no new live evidence.
- Recommendation: retain native Spaces as default; investigate virtual workspaces
  as a future opt-in mode, not an immediate replacement. Recommendations below
  do not change existing decisions.
- Standing context: tier-2 SIP-on, no Dock injection; 2026-10-15 parking unsuitable
  as initial substitute; 2026-10-28 pager direction is a separate selectable
  unbound core-designated spare with lazy backing ([decisions][decisions]).
- Priorities: authenticity, consistency, reliability first; jank/performance
  second; configurability/visual UX third; game safety absolute
  ([VISION][vision], [principles][principles]).
- Related evidence: [prior art][prior-art], [port plan][macos-plan],
  [native routes][native-routes]. Upstream documentation claims are not runtime
  acceptance; negative findings are limited to the inspected sources.

## 1. Pins and prior art

Sources were inspected from the existing local clones at these revisions.

| Source | Pin |
|---|---|
| AeroSpace | `74a1bf17e82d70e0a21945ba04bb4f590bf19f83` |
| rift | `8114ccc534b5c9a7e6a77ce27568d8739be9216f` |
| komorebi-for-mac | `bdf10452f10ebf15ec1978cb05d0aac79ef5611f` |
| FlashSpace | `968015308d8d284a98831f90f365538a63456ee9` |
| yabai | `dd845723416f5fe92af49fad5ebab00369e07edd` |
| Amethyst | `5cfe2fe12b1a933139010da5760aaa7e44033e9b` |
| glide | `ef3b30f88445fe4b517eb4c60023011f6d3725dd` |
| paneru | `8a213313538a352537b26e25131cfe75f1e00724` |
| Hammerspoon | `23e387e2805a9890066366e0ac96c71b27f0cfd5` |
| PaperWM.spoon | `2a561eb63df4fd72fc2556e411bd4e1c88f57f48` |
| Defi | `effabd625a26dc8593d4420032cf9dffb8d9ffbe` |

Silica's move path is covered by the existing pinned [native-routes survey][native-routes]; no separate new Silica finding is claimed here.

| Project | Workspace model | Empty / trailing behavior | Indicator / pager | Mechanism and limits |
|---|---|---|---|---|
| AeroSpace | Per-window name-created virtual workspaces ([Workspace.swift][aero-ws]) | Stub reuse; GC keeps persistent, nonempty, visible, or focused rows; no automatic trailing invariant established | Tray names per monitor, focused star, fullscreen brackets ([tray][aero-tray]); i3 styles hide empty non-visible rows ([label][aero-label]) | AX corner parking, proportional floating-position restore, Zoom pixel exception; tiled restore via layout ([MacWindow.swift][aero-park]). Termination recenters, not exact durable restore ([termination][aero-util]). Guide documents Cmd-Tab restore, 1px slivers, monitor leaks and Mission Control thumbnail issues ([guide][aero-guide]). No-SIP requirement is an upstream claim ([README][aero-readme]). |
| rift | Virtual workspace vector per native Space; membership in WindowStore ([model][rift-vw]) | Configured count 1-128; no trailing designation established | Menu-bar index/name/layout/active/count/mini rectangles ([menu][rift-menu]); `show_empty_workspaces` setting ([config][rift-toml]) | All inactive-workspace windows use corner parking ([engine][rift-engine]), 1px reveal and overlap-minimizing corner choice ([placement][rift-hidden]); distinct from scrolling-edge parking. Private AX identity plus SLS/SLPS bindings ([bindings][rift-sky]). No-SIP, own Mission Control and fullscreen multi-display motivation are upstream claims ([README][rift-readme]). |
| komorebi-for-mac | Configured per-monitor workspace ring; per-window placement ([monitor][komo-mon]) | `CycleEmptyWorkspace` focuses an existing empty row, not an automatic spare ([CLI][komo-cli]); no trailing invariant established | Workspace widget can hide empty workspaces; no trailing designation established ([widget][komo-widget]) | Corner parking with saved rect ([window][komo-win]); clean-stop restore ([main][komo-main]). Fullscreen and app hiding break state per upstream [known issues][komo-issues]. Launch-Space scope, separate-Spaces OFF for multi-monitor ([installation][komo-install]); private CGS reads/SLS batching ([bindings][komo-sky]); SIP requirement unknown. |
| FlashSpace | App-assignment UUID workspaces, user-created persistent rows ([model][flash-ws], [CLI][flash-cli]) | No automatic trailing invariant established | Menu-bar title template ([title][flash-title]); optional Screen Recording previews ([settings][flash-prev]) | App assignments cannot split same-app windows. Normal switching uses app hide, raise and activate ([manager][flash-wm]); one native Space/display, separate-Spaces ON ([README][flash-readme]). Termination restores PiP/corner map only ([delegate][flash-app], [PiP][flash-pip]); general hidden-app restoration not established. Private-API/SIP requirement not established. |
| yabai | Native Spaces keyed by SID ([space][yabai-space]) | Create/destroy via scripting-addition guards ([manager][yabai-sm]); no automatic trailing invariant established | No built-in pager established; JSON status for external consumers ([query][yabai-win]) | AX minimize differs from app-hidden. Native moves/swipes have non-SA routes ([native-routes][native-routes]); Dock-injected scripting addition needs reduced SIP. |
| Amethyst | Native Space id/type/UUID ([model][am-space]); per-UUID layouts, current-user-Space gating ([screen][am-screen]) | Indexed moves; no create/trailing lifecycle established | Status menu/layout HUD, not a workspace pager ([delegate][am-app]) | AX plus synthetic drag via Silica ([native-routes][native-routes]); reference 50/400 ms holds and 1 s switch-back delay are not OS minima. |
| glide | Native Space plus per-space viewport ([layout][glide-layout]); native fullscreen excluded ([screen][glide-screen], [server][glide-ws]) | Empty-root reset is layout cleanup, not a trailing workspace rule | Optional native Space-number menubar ([status][glide-status]); group bars are tab/stack UI, not workspace paging | Off-viewport frames park at screen edges ([viewport][glide-vp]); this is strip scrolling, not replacement of native workspaces. |
| paneru | Native Space x virtual rows ([workspace][paneru-ws]) | Optional reaping of non-active empty rows above index zero; default OFF ([config][paneru-config]); no guaranteed trailing spare | Stable empty numeric slots in query/subscription output for SketchyBar ([format][paneru-query]); menubar/OSD. Fullscreen index-zero marker is not a spare marker | 10px strip sliver ([layout][paneru-layout]); restores origin/focus and preserves arriving Cmd-Tab focus ([workspace][paneru-ws]). Fullscreen strip avoids relocation and suppresses overlays. No-SIP claim with layering limits ([configuration][paneru-conf]); not game acceptance. |
| Hammerspoon | Native Spaces API | No built-in trailing invariant established | Lua queries for custom bars; no built-in pager established | Experimental private-plus-AX routes; visible Mission Control add/goto/remove ([spaces.lua][hs-spaces]). |
| PaperWM.spoon | Native per-Space scrolling strip | No trailing invariant established | External-bar offsets documented, no own pager established ([README][paper-readme]) | Native index switch/send and user-Space filtering ([space][paper-space], [tiling][paper-tiling]); viewport-edge slivers described in earlier prior art are not inactive-workspace parking. |
| Defi | Globally unique dynamic per-monitor workspaces, documented design ([ADR][defi-adr]) | Exact one trailing empty/monitor; populated trailing becomes ordinary; empty inactive ordinary pruned; named rows persist | Runtime trailing UI marker unknown | ADR specifies public AX vertical transitions, budgeted animation or immediate placement. It explicitly accepts ordinary keys continuing to reach the previously focused parked app on an empty workspace until new native focus intent. This is a risk, not OmniTiler acceptance. |

## 2. Virtual mode: mechanics, side effects and safety

Virtual ordinary-workspace switches avoid native Space transitions and native
create/remove churn. The potential benefit is smaller than comparison against
slow Mission Control switching alone: [existing native evidence][native-routes]
already includes tier-2 single-display Dock-swipe selection and bridged-send
improvements. No source-only speed or reliability comparison establishes a winner.

Private API use does not itself imply reduced SIP: distinguish SIP-on upstream
claims from yabai's Dock-injected SA. Authority to order arbitrary foreign
windows out remains unproven in the [inspected routes][macos-plan-s5]; it is not
an established sliver-free alternative.

| Risk | Evidence / unknown | Recommended mitigation or acceptance gate |
|---|---|---|
| Slivers and display leaks | AeroSpace documents 1px lines and needs a free bottom corner per monitor; rift chooses a lower-overlap corner; paneru retains 10px | Assess visible cost and rescue tradeoff with owned windows, including arrangements without a free corner. Do not promise invisible parking. |
| Mission Control / Expose / Stage Manager | AeroSpace documents tiny MC thumbnails and an app-grouping workaround; Stage Manager and App Expose behavior are not established here | Separate shell-acceptance cases; do not change settings merely to conceal defects. |
| Multi-display / native fullscreen | AeroSpace recommends separate-Spaces OFF but documents a black second monitor during native fullscreen; ON has reported focus/performance issues. FlashSpace instead requires ON | Validate both modes, hotplug and scale changes; retain native fullscreen safety rather than adopting a reference's preferred setting blindly. |
| Dock / Cmd-Tab / empty views | AeroSpace documents Cmd-Tab workspace return; paneru preserves arriving focus; Defi admits keys can still reach a parked app | Adapter reports activation provenance; core decides switch/pull behavior. Explicitly discriminate empty-view key delivery and direct Dock activation. |
| Quit / crash / update restore | AeroSpace recenters or offers manual sliver rescue; komorebi has clean-stop restore; FlashSpace's termination restore is PiP/corner-only. rift layout persistence is not proof of stranded-window recovery | Durable preimage before parking, identity-safe independent restore and visible manual recovery. A saved layout is not crash recovery ([recovery plan][macos-plan-recovery]). |
| Sleep/wake, display changes, consent loss | No OmniTiler virtual-mode acceptance; Defi's affinity/reconnection rule is documented design | Reobserve identities/topology, invalidate stale writes and recover narrowly; test wake, re-plug, resolution/scale and TCC loss. |
| Games / fullscreen | No surveyed source proves game-safe runtime behavior. Current native-fullscreen bring-forward is unproven ([decision][fullscreen-decision]) | No unasked game/fullscreen moves, resize, focus, overlays or input interference; games remain members for explicit workspace actions. Never force-exit fullscreen. Virtual mode still needs the native-fullscreen logical-membership bridge. |

App hiding is app-granular; minimizing changes Dock state. Neither is a faithful
drop-in for per-window workspace isolation. Cmd+H's current mapping to explicit
minimized facts must not become a fake parking observation ([decisions][decisions]).

## 3. Existing core fit and macOS adapter work

- **Yes, the core already supports the logical/hidden model.** Windows demonstrates
  workspaces backed by adapter hide/reveal, not one native desktop per logical row.
  Core publishes full [workspace rings][core-pub], per-output
  [`logical_view`][core-view], retained window homing and
  [trailing lifecycle][core-ws]. The default protocol contract is tiler-owned;
  native-Space mode opts into host-owned IDs. Reuse policy, not another tiler's
  workspace store.
- **Windows mechanics are not portable proof.** Its [receipts][win-receipts]
  prove hidden membership with fresh identity/claim association plus continued
  invisibility; missing observations alone prove nothing. Native visibility and
  hide-ledger state are separate ([diff][win-diff]); unreadable geometry holds,
  not fabricated geometry ([capture][win-capture]). Its automated hide/reveal and
  watcher recovery evidence does not establish macOS behavior, nor Windows
  physical/multi-display/game acceptance ([record][windows-managed]).
- **macOS needs a second actuator and recovery path:** park/unpark; durable
  geometry/display/session/lifetime preimages; retained AX identities beyond
  onscreen CG inventory; causal own-write versus external-frame classification;
  focus/view/member readbacks; native-fullscreen bridging; and native/virtual
  coexistence and mode-switch recovery. Exact coexistence semantics are unselected.
  Windows continued-invisibility proofs cannot simply accept visible parked
  slivers. Do not fake pre-park rects, minimized state, or closure; whether a small
  generic core contract extension is needed remains an investigation, not a claim
  that the mode is adapter-only. macOS watchdog/identity recovery remains unknown
  ([plan][macos-plan-recovery]).
- **The trailing designation remains necessary.** Current
  [`WorkspaceDesire`][core-desire] lacks it. The selected pager projection is not
  its implementation. Native mode would show an explicitly designated *unbound*
  spare separately; a bound trailing row remains a native row. In a virtual
  tiler-owned contract every `native_binding` is `None`, so absence cannot identify
  the spare. Recommend the small optional explicit core designation in either mode;
  virtual display needs no native backing. No schema/capability shape is selected.
- **Configurability is not automatic semantic fallback.** Host Integration's
  undocumented-route/documented-fallback rule does not authorize silently changing
  native Spaces into parking. That user-visible model change needs explicit choice.

## 4. Recommendation, effort and native backlog

| Item | Recommendation / effort and value |
|---|---|
| Default and option | Keep native default. Investigate a future opt-in virtual mode, add only after recovery/shell/fullscreen safety gates. Potentially high authenticity and fast-switch value, but no quantified performance advantage or native-integration equivalence established. Reliability and absolute game safety outweigh a cheap settings flag. |
| Effort | **M** for an owned-window prototype reusing core policy; **L** for production because geometry recovery, shell/focus behavior, native fullscreen and multi-display remain substantial. **S** for the explicit trailing marker alone, including publication/protocol/consumer tests. Relative estimates, not elapsed-time promises. |
| Trailing pager marker | Keep the proposed optional core designation under native-only, virtual-only or dual-mode options. Hiding mechanism does not transfer workspace/pager policy to macOS. Lazy backing remains native-mode-only mechanics. |
| Newly shelve native backlog? | **None** under the recommended optional mode: native remains supported/default. Preserve existing [2026-10-28 shelving][roi-decision]. |
| Unnecessary *within virtual ordinary-workspace mode* | Native MC create/remove and destructive-empty proof/lazy-backing machinery; ordinary Dock-swipe selection; bridged/titlebar native window sends and Desktop-N chord plumbing. They remain necessary on the native path. |
| Still needed under either option | Native-fullscreen logical membership/bring-forward/send-reassociation without forced exit; best-effort admission; recovery, FFI review, overlays, drag/drop and reliability/game-safety work. Virtual parking does not retire these. |

## 5. Explicit unknowns

- Stage Manager, App Expose and direct Dock activation; empty-view key delivery.
- Durable macOS lifetime identity, crash restore/watchdog and consent-loss recovery.
- Native-fullscreen bring-forward and cross-display/mode-switch interactions.
- macOS 15 runtime acceptance and game-safe runtime behavior.
- Defi runtime trailing UI marker. Its ADR is design evidence, not tested behavior.

[decisions]: ../../decisions.md
[principles]: ../../principles.md#priorities
[macos-plan]: plan.md
[macos-plan-s5]: plan.md#5-managed-per-monitor-workspaces-and-hiding-without-dock-injection
[macos-plan-recovery]: plan.md#10-crash-recovery-snapshots-vs-stranded-window-restore
[native-routes]: native-spaces-routes.md
[prior-art]: prior-art.md
[vision]: ../../../VISION.md
[fullscreen-decision]: ../../decisions.md#macos-dogfood-review-decisions-2026-10-27
[roi-decision]: ../../decisions.md#roi-review-and-receipt-retirement
[core-desire]: ../../../crates/tiler-core/src/world_report.rs#L318-L337
[core-view]: ../../../crates/tiler-core/src/world_report.rs#L339-L351
[core-ws]: ../../../crates/tiler-core/src/workspace.rs#L20-L71
[core-pub]: ../../../crates/tiler-core/src/world_publish.rs#L1046-L1159
[win-receipts]: ../../../crates/tiler-windows/src/tiling_sys/world_native_receipts.rs#L22-L86
[win-diff]: ../../../crates/tiler-windows/src/tiling_sys/world_diff.rs
[win-capture]: ../../../crates/tiler-windows/src/tiling_sys/world_capture.rs#L949-L979
[windows-managed]: ../../changes/archive/windows-managed-workspaces.md
[aero-ws]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/Workspace.swift
[aero-tray]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/ui/TrayMenuModel.swift#L26-L70
[aero-label]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/ui/MenuBarLabel.swift#L41-L70
[aero-park]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacWindow.swift#L123-L183
[aero-guide]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/docs/guide.adoc#L438-L526
[aero-util]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/util/appBundleUtil.swift
[aero-readme]: https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/README.md
[rift-vw]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/model/virtual_workspace.rs
[rift-engine]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/layout_engine/engine.rs#L2605-L2620
[rift-hidden]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/model/hidden_window_placement.rs#L19-L74
[rift-menu]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/actor/menu_bar.rs
[rift-toml]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/rift.default.toml
[rift-readme]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/README.md
[rift-sky]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/skylight.rs#L325-L381
[komo-mon]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/monitor.rs
[komo-win]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L506-L577
[komo-main]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/main.rs
[komo-install]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/docs/installation.md
[komo-issues]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/docs/known-issues.md
[komo-widget]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi-bar/src/widgets/komorebi.rs#L55-L84
[komo-cli]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/docs/cli/cycle-empty-workspace.md
[komo-sky]: https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/skylight.rs
[flash-ws]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/Workspaces/Models/Workspace.swift
[flash-cli]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/CLI/Commands/WorkspaceCommands.swift
[flash-wm]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/Workspaces/WorkspaceManager.swift
[flash-title]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/MenuBar/MenuBarTitle.swift
[flash-app]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/App/AppDelegate.swift#L47-L49
[flash-pip]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/PictureInPicture/PictureInPictureManager.swift
[flash-readme]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/README.md
[flash-prev]: https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/Settings/SpaceControl/SpaceControlSettingsView.swift
[yabai-space]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space.c
[yabai-sm]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c
[yabai-win]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c
[am-space]: https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Model/Space.swift
[am-screen]: https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Managers/ScreenManager.swift
[am-app]: https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/AppDelegate.swift
[glide-layout]: https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/layout.rs
[glide-screen]: https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/screen.rs
[glide-ws]: https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/window_server.rs
[glide-vp]: https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/model/scroll_viewport.rs
[glide-status]: https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/status.rs
[paneru-ws]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/ecs/workspace.rs
[paneru-config]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/config.rs
[paneru-layout]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/ecs/layout.rs#L38
[paneru-query]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/QUERY_AND_SUBSCRIBE_FORMAT.md
[paneru-conf]: https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/CONFIGURATION.md
[hs-spaces]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua
[paper-space]: https://github.com/mogenson/PaperWM.spoon/blob/2a561eb63df4fd72fc2556e411bd4e1c88f57f48/space.lua
[paper-tiling]: https://github.com/mogenson/PaperWM.spoon/blob/2a561eb63df4fd72fc2556e411bd4e1c88f57f48/tiling.lua
[paper-readme]: https://github.com/mogenson/PaperWM.spoon/blob/2a561eb63df4fd72fc2556e411bd4e1c88f57f48/README.md
[defi-adr]: https://github.com/qeude/Defi/blob/effabd625a26dc8593d4420032cf9dffb8d9ffbe/docs/adr/0004-adopt-dynamic-workspaces.md

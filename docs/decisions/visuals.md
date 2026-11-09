# Visuals and pointer interaction

### Shared visual safety
- Decision: no border or group visual shows while a window is maximized or fullscreen. Hide native active border for applet popup; restore for ordinary focus. Do not infer fullscreen from output-sized/borderless geometry where native flag exists.
- Why: not recorded.
- Assumes / revisit if: native committed state, not requested maximize state, controls suppression; no polling, timers or geometry heuristics.
- Status: active, 2026-09-21
- Evidence: [KDE support and delivery](kde.md#kde-fullscreen-presentation-and-host-integration).

### Anchored active border v4 (R20)
Border motion policy is owned by [Anchored border (R20)](contract.md#anchored-border-r20). Group underlay geometry remains owned by the group-underlay decisions below.

### Border style defaults and theme mapping
- Decision: active border is an MVP requirement. KDE's OpenGL native C++ effect remains disabled by default; its native QWidget KCM hot-applies/repaints. `UseThemeColor` defaults true and uses active Selection background from `KColorScheme` with configured-color fallback; false uses configured color unconditionally. Active border outline radius defaults to 6 on all platforms; revisit per host if dogfood does not suit. Windows uses documented `UISettings.GetColorValue(Accent)`, replacing `DwmGetColorizationColor`; retain availability/positive-alpha gate and configured `#2a82da` fallback. Border theme/accent and configured color are selectable; pager uses available accent independently or existing blue fallback. Border width/gap/radius style remains host-scaled; outer theme accents do not mutate saved settings.
- Why: radius 6 looked right in Windows dogfood; Windows colorization color is not a guarantee of Personalization accent parity.
- Assumes / revisit if: Windows initial accent parity physically accepted on one output; startup/idle/fallback fault paths remain pending. Black/white/grey wallpaper checks remain before 0.1. macOS uses native accent and separate host mechanics in its own entry.
- Status: active, 2026-10-11
- Evidence: [radius default](../changes/archive/windows-border-radius-default.md), [underlay options](../changes/archive/kde-dogfood-underlay-options.md).

### Independent host highlight colors
- Decision: active-window border and pager active-window highlight have independent color overrides, both defaulting to live system accent. Windows stores `core.pager_highlight.color` (`#rrggbb`, fallback `#2a82da`) and `core.pager_highlight.use_theme` (default `true`) as host styling, not shared-core behavior; Settings uses existing color/theme validation, persistence and live apply. Focused miniature uses resolved highlight full-strength on border and dimmer same-RGB fill; unfocused miniature gray and active workspace cell accent fill are unchanged. Border overrides never affect pager. Each port offers independent styles; core presentation semantics unchanged.
- Why: not recorded.
- Assumes / revisit if: no new shared color fields.
- Status: active, 2026-10-24
- Evidence: [macOS production presentation](../changes/macos-world-v2-port.md#reviewed-production-presentation-2026-10-26), [migration evidence](../changes/archive/decisions-evidence-2026-10-31.md#development-and-macos-evidence).

### Group underlay style and trigger
- Decision: group underlay default is `#662a82da` (40% accent blue, Qt `#AARRGGBB`) on KDE/Windows, preserving explicit saved colors without migration. Enabled filled premultiplied-alpha click-through nonactivating surface; extension -1 follows border width; explicit extension 0..32 logical px. KDE/Windows trigger is Meta/Win+Shift held (both required, either order, extras allowed) or matching focused native title-bar interactive move; modifier alone and resize alone do not trigger. End/cancel/removal clears move arm, not held chord. Windows samples chord levels at the existing 100 ms pump and wakes on transitions; a bounded `WM_NCHITTEST` at move START arms only a known title-bar move, not timeout, unknown/custom-client hits or keyboard-only moves. No new polling, timed flash or input hook. Core derives focused leaf's immediate parent group and projected members; render union of source rectangles expanded by gap/border/extension even as live dragged frame moves. Hide unavailable/root-leaf/floating/minimized/maximized/fullscreen subjects.
- Why: not recorded.
- Assumes / revisit if: no foreign-window attributes changed; physical feel, sustained chord, fullscreen/custom-frame/topmost, other output/DPI and nested-layout checks remain follow-up. Native group-underlay surfaces require below-member proof.
- Status: active, 2026-10-13
- Evidence: [group underlay trigger](../changes/group-underlay-move-trigger.md), [Windows group geometry](../changes/archive/windows-group-underlay-geometry.md).

### Group-underlay rendering and stacking
- Decision: underlay is filled surface below lowest visible group member and active-border surface. Windows uses owned carrier, rechecks geometry/visibility/stacking at refresh even if cached input unchanged, repairs above-anchor displacement. KDE active border is target child at Z=-1; underlay image below lowest painted member at Z=-2, reanchors on stacking/membership/visibility changes. Group shape uses minimum-hint projector; Windows carries retained advisory hints only, never native/client rectangles. Child inherits slide translation; accepted overlap may paint over edge of nongroup window below anchor. No custom scene/render mechanism selected. Retire old `ActiveGroup` query and use uniform wire presentation on all platforms.
- Why: not recorded.
- Assumes / revisit if: overlap and physical stacking/pixels remain explicit acceptance; no absolute group-underlay geometry is changed by R20 anchored border.
- Status: active, 2026-10-24
- Evidence: [KDE port notes](../porting/kde-port-notes.md), [Windows group underlay](../changes/archive/windows-group-underlay.md).

### macOS border and underlay
- Decision: interim active border uses public AppKit borderless/nonactivating/click-through panel on main thread, Managed collection behavior, relative above-target ordering; no SkyLight renderer. Reuse core visual visibility/inner geometry/theme gates: eligible active tiled/floating windows, width 3, gap 0, radius 6, fallback `#2a82da`, theme enabled. Native accent `NSColor.controlAccentColor` in sRGB; geometry AppKit points/backing scale. No blanket move/animation suppression; fresh owner/current Space/scope/identity/freshness/lock facts all agree before show. Underlay stays out of product until SkyLight-created-window discriminator proves below every member, then native snapping/Mission Control; failed AppKit-relative and SkyLight-on-AppKit ordering code removed. Later evaluation direct ordering had success without global placement change; transaction ordering errored; neither proved below-every-member, so no product integration. Do not infer SkyLight-created renderer failure. Active border stays above target; below-target/band parity remains follow-up.
- Why: public cross-process ordering passed owned fixture; smaller public API/lifetime surface sufficient for continued border evaluation. Underlay below-every-member is required safety/parity invariant.
- Assumes / revisit if: owned-fixture switching/empty Space/fullscreen exit and synthetic border-region receipt passed; physical appearance, foreign stacking, successful float/zoom suppression, multiple displays and actual games remain follow-ups. SkyLight underlay evaluation is 0.1 scope per later gate; not automatically deferred after a failed attempt.
- Status: active, 2026-10-17
- Evidence: [macOS active border](../changes/macos-active-border.md), [group underlay](../changes/macos-group-underlay.md), [macOS 0.1 gate](../changes/macos-0.1-gate-proposal.md).

### Pointer and drag behavior
- Decision: plain click focuses at press; hover alone does not change focus. Host-native pointer move/resize keeps floats free-framed; project resize refuses floats. Follow host-native drag presentation where available: KDE Meta+drag keeps KWin pointer-following frame, Windows title-bar drag stays native, macOS same. Meta client drag focuses dragged window at press on KDE/Linux; Windows historical Win+drag was dropped. Escape cancels titlebar drag; KDE Meta-drag Escape remains unavailable due desktop lock-screen chord collision. Gap-drag only when gaps >0; edge-drag covers gap 0. Tiled drops use shared core insertion/drop resolver; center/stack drop refused while stacks unselected. Native cross-output drop joins destination output's tiling at release point, not snap-back; no atomic native geometry claim.
- Why: native title-bar drag is platform convention outside Linux; host conventions and core slot resolver preserve expected interaction.
- Assumes / revisit if: KDE modifier drag Esc collision has no planned fix; physical title-bar/cross-output acceptance remains platform-specific. Grouping means nested split-tree placement, not tabs/stacks/shared tiles/compositor groups.
- Status: active, 2026-10-27
- Evidence: [cross-output drag preview](../changes/archive/cross-output-drag-preview.md), [macOS drag/drop](macos.md#drag-and-drop).

### Nested drag placement
- Decision: portable drag placement source-classifies COSMIC group edges (32 px normally, 80 px only for exact prior portable `(group, edge)` hover), group interiors and window zones. Same-axis group edges use first/last insertion; perpendicular edges wrap group; group interiors insert after source predecessor. Window left/right make horizontal before/after, top/bottom vertical before/after. Center middle-third is stack drop refused closed. Drag preview freezes source token/domain/revision at START; cancellation/invalidation/self/center/outside/finish/teardown clear preview. Cross-output destination is output under pointer at Finish; verify membership via existing observation, no new arrival timer/retry.
- Why: not recorded.
- Assumes / revisit if: no tabs/stacks/compositor groups or stock-KWin parity claim; physical resize/float exclusion checks remain user checks.
- Status: active, 2026-09-29
- Evidence: [cross-output drag preview](../changes/archive/cross-output-drag-preview.md), [KDE dual output](../changes/kde-dual-output-live.md).

### Windows active-border surface and theme delivery
- Decision: Windows border is default-on owned per-pixel-alpha layered click-through/nonactivating tool surface; never mutate foreign-window attributes. Use shared visual policy, width 3/gap 0, target-DPI rounding; eligible active windows independent of tiled/floating membership, fenced by scope/identity. Place immediately below fresh target in its topmost/normal band; reconcile visibility/geometry/z-order even when cached drawing inputs match. DWM shadows may tint composed ring; DIB RGB is not final screen RGB.
- Why: `DWMWA_BORDER_COLOR` controls color only and visible-frame thickness is Get-only.
- Assumes / revisit if: physical display/input, other DPI/output and topmost/style variants remain follow-up. KDE live theme accent uses `KColorScheme`; Windows uses documented `UISettings.GetColorValue(Accent)` with availability/positive-alpha gate and configured `#2a82da` fallback. Pager accent remains independent.
- Status: active, 2026-10-19
- Evidence: [Windows border](../changes/archive/windows-active-border.md), [dogfood](../changes/windows-single-output-dogfood.md).

### Native color refresh
- Decision: Windows shared broadcast versions trigger one owner-thread cached accent resolve for both surfaces on colorization/settings change, including hidden reshow and renderer-only idle notifications through existing owner wake. Failed query uses narrow fallback until next notification; no polling/timer. Keep one thread-confined WinRT apartment; renderer paints posted accent snapshot and never queries WinRT. Drawing remains change-gated, and normal logs retain source changes even when RGB matches.
- Why: not recorded.
- Assumes / revisit if: initialization cleanup is same-thread for `S_OK` and `S_FALSE`; failures own no cleanup and remain cached until shutdown. Idle notification/fallback fault-path acceptance remains pending.
- Status: active, 2026-10-19
- Evidence: [Windows accent reliability](../changes/windows-single-output-dogfood.md).

<a id="tabbed-groups-and-post-0-1-gate"></a>
### Tabbed groups and post-0.1 gate
- Decision: grouping means nested split-tree structure/placement; tabs, stacked/shared groups and compositor grouping are excluded. Tabbed stacks are first post-0.1 item; until then center/stack drops refuse. Full tab core/protocol remains post-0.1 with coordinated v4 visibility-ready suppression authority/correlation independent of workspace ownership, own capability flag, core-composed workspace vs inactive-tab suppression, and truthful hidden observations. Tab close provisionally selects positional-next, clamped at end; confirm Tier 1 when tab work starts.
- Why: not recorded.
- Assumes / revisit if: no tab/stack carrier, controls or bindings are selected yet. Compatible later protocol bump expected around 0.2; no zero-later-bump requirement.
- Status: active, 2026-10-29
- Evidence: [R-GRP-03](../changes/archive/reference-comparison-implementation.md), [R21](engine.md#kde-effect-rust-migration-sequence-r21).

### Drag-restore marker retirement
- Decision: on topology-change signals, validated complete desktop-ID/output-name list proves a marker's workspace/output removal only when absent; settle `outcome=unavailable plan=none` and drop marker. Failed or malformed list proves nothing. No timer, count cap, inferred departure or fabricated plan.
- Why: not recorded.
- Assumes / revisit if: topology completeness is required for retirement; failures retain marker.
- Status: active, 2026-09-26
- Evidence: [mid-drag destination recovery](../changes/archive/mid-drag-destination-recovery.md).

### Drag and resize completion rules
- Decision: native same-output workspace send during Meta-drag supersedes Started workspace target; ignore stale drop and let complete observations reflow/admit mover. Refused-drop restore binds to fresh observed destination. Interactive resize classifies grabbed edge from pointer/start frame, preferring matching native press for KWin-thirds; retile only final grabbed edges, both axes for corner, ignore incidental rounding/client changes. Cancel, no usable edge, no grabbed-edge movement or lost identity means no resize plan. Rejection gets one bounded drag-correlated reconcile, no retry/loop. User 2026-11-09: KDE K38 retains committed resize/8px-gap/no-extra-reconcile physical proof, using v4 command/reply/native convergence instead of the old one-planned-applied-result oracle.
- Why: not recorded.
- Assumes / revisit if: modifier-resize press is passive; no grab/consume. Usable press selects KWin 6.7.5 thirds; otherwise fallback uses Started pointer, exact thirds/interior rule and nearest-edge/corner rule. No reliable KWin grabbed-edge signal; physical resize and float exclusion checks remain user checks.
- Status: active, 2026-11-09
- Evidence: [mid-drag recovery](../changes/archive/mid-drag-destination-recovery.md), [cross-output drag preview](../changes/archive/cross-output-drag-preview.md), [approved KDE acceptance](../changes/kde-v4-acceptance-checklist.md#approved-acceptance-decisions).

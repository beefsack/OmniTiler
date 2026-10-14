# macOS window-management prior art

Status: researched 2026-10-03, source inspection only. Upstream behavior is
not a macOS guarantee or our acceptance evidence. Unknown means not established
from the fetched sources. Recommendations live in the [tentative plan](plan.md).

Selected direction (user 2026-10-03,
[decisions](../../decisions.md#scope-and-platform-goals)): tier 2, public
plus private APIs with SIP left enabled; no Dock injection, no reduced SIP.
Managed per-display workspaces mandatory for feature-complete; underlay
experiment with outline fallback; same KDE actions/behavior; modifier mapping
open; per-binding shortcut conflict list decided 2026-10-03; gaming needs no
overlays/moves/input interference.

## Control API, workspace model, SIP

| Project | Window-control API (fetched) | Workspace model | SIP |
| --- | --- | --- | --- |
| AeroSpace | Public AX; exactly one private fn `_AXUIElementGetWindow` (README + private.h) | Virtual sets via offscreen parking; ignores native Spaces | None required |
| yabai | AX base + Dock scripting-addition injection for elevated ops (README + SIP wiki) | Native Spaces manipulation | Partial disable for scripting addition; optional only |
| Amethyst | AX via Silica; Spaces IDs are private CGS types via Silica (Space.swift) | Native Spaces (throw to space N) | None documented in fetched files |
| Rectangle | AX snap/move/resize current Space | No cross-Space moves stated (README) | None in fetched README lines |
| Rectangle Pro | Unknown (fetched page lists features, no API detail) | App layouts, display triggers (feature page) | Unknown from fetched page |
| Hammerspoon | AX geometry plus private `_AXUIElementGetWindow` identity (HSuicore.m), `CGSSetDebugOptions` shadows and SLS Spaces helpers | Native Spaces via `hs.spaces` | No SIP change documented; private helpers version-coupled |
| PaperWM.spoon | Hammerspoon AX windows and private identity/Spaces helpers | Per-Space tiling strip; native Spaces kept | No SIP reduction documented |
| Loop | AX (AccessibilityManager) + event-tap monitors + private SkyLight files | No virtual workspaces in fetched README (halves/quarters/stash) | None documented in fetched files |
| FlashSpace | App show/hide, not per-window AX moves (README design notes) | App-assignment virtual sets, one macOS space per display | None in fetched files |
| Phoenix | AX (PHAXUIElement/PHWindow); Space `moveWindows` deprecated on newer macOS (commit #354) | Native Spaces | None in fetched files |
| Glide | AX per-app threads + `CGWindowListCopyWindowInfo` + private SkyLight + event tap (ARCHITECTURE.md) | Integrates with native Spaces | None claimed; private SkyLight is version-coupled |
| komorebi-for-mac | Tiling extension to standard desktop (README); internals Unknown | Tiling over standard desktop; details Unknown | Unknown from fetched page |
| Swindler | Public AX Swift wrapper (README 0% Spaces) | No workspace authority (Spaces 0%) | None (pure AX library) |
| Paneru | AX + SkyLight per README; offscreen slivers | Per-monitor strip + experimental virtual rows; keeps native Spaces | None claimed in fetched README scope |
| JankyBorders | Private SkyLight (`SLSReleaseWindow` in border.c; broad SLS imports) | Follows target Space ID; no workspace management | No SIP change; private API is version-coupled |
| skhd | Hotkey daemon only, no window authority | N/A | None in fetched README scope |

## Permissions, keyboard, multi-display, jank

| Project | Permissions (fetched) | Keyboard mechanism (fetched source) | Multi-display | Animation/jank notes (fetched) |
| --- | --- | --- | --- | --- |
| AeroSpace | Accessibility | Carbon via soffes/HotKey 0.2.1 SPM dep (Package.swift + HotKey README) | Shared workspace pool; guide prefers separate Spaces OFF and a free bottom corner on each display | Parking leaves 1px slivers; tiny Mission Control previews; guide reports focus/performance issues with separate Spaces ON |
| yabai | Accessibility; Screen Recording iff animations (README table) | External (skhd or other; README) | Requires separate Spaces ON (README, all macOS lines) | Scripting addition enables animation control; animations need Screen Recording |
| Amethyst | Accessibility | KeyboardShortcuts 2.2.4 + MASShortcut via HotKeyRegistrar migration; Carbon-family dependencies | Focus/throw across screens | Historical Stage Manager classification gap (closed, unmerged PR #1331); current behavior untested |
| Rectangle | Accessibility | MASShortcut fork rxhanson/MASShortcut + Sparkle via SPM (README) | Per-display tiling rows/columns | Animated-snapping option; app minimum-size fights (README) |
| Rectangle Pro | Unknown from fetched guide | Unknown (closed source) | Layouts triggered on display connect/disconnect; supports Intel/arm64 | Edge stashes slide out under cursor; no measured jank data |
| Hammerspoon | Accessibility (AX); Input Monitoring for eventtap use | Carbon `RegisterEventHotKey` in libhotkey.m (sig 'HMSP', `kEventHotKeyExclusive`); repeat via NSTimer | `hs.screen` APIs | Three-step size-position-size frame set in libwindow.m |
| PaperWM.spoon | Inherits Hammerspoon AX | Hammerspoon hotkeys (`bindHotkeys`, modal) | Vertical arrangement advised | Parked windows sit in screen-edge margin (still visible/clickable) |
| Loop | Accessibility (`AXIsProcessTrustedWithOptions`, AXPermissionsChanged monitor); entitlements file empty | Event-tap monitors (`BaseEventTapMonitor`: CFMachPort, `CGEvent.tapEnable`, restart cascade guard) + private SkyLight | Move across screens | Preview window before commit; custom frames |
| FlashSpace | AX wrappers (Accessibility folder); entitlements show no sandbox/special entries | KeyboardShortcuts dep (wojciech-kulik fork, project.yml); capture substrate inside dep not opened here | Per-display assignment (static/dynamic); requires separate Spaces ON | No animations by design (hide/show is instant); PiP corner-hide experimental |
| Phoenix | Accessibility (installation guide) | Carbon `RegisterEventHotKey` in PHKeyHandler.m; repeat via NSTimer; mouse-only global monitor | `Screen` API; Space has `screens()` | `Space.moveWindows` deprecated/broken on 13.6+/14.5+/15.0+; no measured jank data |
| Glide | Accessibility | `WmController` owns hotkey registration (ARCHITECTURE.md); exact capture API not named there = Unknown | Requires separate Spaces ON for multi-monitor | Per-app threads + transaction IDs vs AX delay; CGEvent tap for mouse only |
| komorebi-for-mac | Unknown | Unknown (`komorebic` CLI + UDS subscribe documented; key-capture substrate not stated) | Unknown detail | Unknown |
| Swindler | AX (sample requests trust) | N/A (library) | Screen API 90%, Spaces 0% | Cached reads, async writes vs AX IPC delays (design notes) |
| Paneru | Accessibility | TOML/Lua bindings documented; underlying capture API not in fetched README = Unknown | Per-monitor strip; vertical arrangement advised | Never-resize-open-windows principle; sliver workaround |
| JankyBorders | No AX permission dependency claimed; other TCC needs not established | N/A (border renderer) | Per-display borders | Private target-relative ordering; no measured jank data in fetched sources |
| skhd | Accessibility (`AXIsProcessTrustedWithOptions` in skhd.c); aborts when Secure Keyboard Entry set (`CGSIsSecureEventInputSet`) | CGEventTap (`CGEventTapCreate` `kCGSessionEventTap`/`kCGHeadInsertEventTap`, mask KeyDown+NX_SYSDEFINED, tap-disable restart); FSEventStream config hotloader (hotload.c); Carbon only tracks front-app switch (carbon.c) | N/A | Modal/passthrough system; blacklist/media-key support |

## License (raw text opened) and maintenance (dated commit evidence)

| Project | License (raw file fetched) | Latest commit on fetched branch + notes |
| --- | --- | --- |
| AeroSpace | MIT (LICENSE.txt, 2023 Nikita Bobko); explicitly not notarized (README) | 2026-10-01 main (atom). Pre-1.0 beta |
| yabai | MIT (LICENSE.txt, 2019 Asmund Vikane) | 2026-06-14 master (atom). SIP wiki edited Apr 2026 |
| Amethyst | MIT (LICENSE.md, 2015 Ian Ynda-Hummel) | 2026-08-19 development (atom). Release 0.24.3 Apr 2026 |
| Rectangle | MIT (LICENSE, 2019-2026 Ryan Hanson; Spectacle-based) | 2026-10-02 main (atom). macOS version lines in README |
| Rectangle Pro | Proprietary, explicitly closed source (getting-started guide), paid trial | Download 3.92 visible on feature page; no source activity available |
| Hammerspoon | MIT (LICENSE, 2014-2025 contributors) | 2026-07-08 master (atom). Release 1.1.1 Feb 2026 |
| PaperWM.spoon | MIT (LICENSE, 2021 Michael Mogenson) | 2026-09-24 main (atom) |
| Loop | GPL-3.0 (LICENSE, 2026 Kai Azim) | 2026-09-30 develop (atom). macOS 13+ |
| FlashSpace | GPL-3.0 (LICENSE) | 2026-09-17 main (atom). MARKETING_VERSION 4.18.79; macOS 14.0+ |
| Phoenix | MIT (LICENSE.md; app-icon assets carved out; bundles Sparkle/lodash notes) | 2025-08-31 master (atom). v4.0.1; macOS 10.14+ |
| Glide | Apache-2.0 OR MIT (both license files fetched) | tmandry/glide main 2026-09-30; glide-wm/glide main 2026-10-02 (atom). Both visible; canonical home unresolved |
| komorebi-for-mac | Komorebi 2.0.0 (PolyForm Strict fork; personal-use permitted purpose; commercial use needs paid Individual license; public releases + private KomoCorp nightly repos) | 2026-05-05 master (atom). Corrects any non-macOS assumption: this repo is the macOS tiler |
| Swindler | MIT (LICENSE, 2017 Tyler Mandry) | 2022-09-06 main (atom). Alpha; Spaces 0% |
| Paneru | MIT (LICENSE.txt, 2025 Karinushka) | 2026-10-02 main (atom) |
| JankyBorders | GPL-3.0 (LICENSE) | 2026-05-14 main (atom). macOS 14.0+ |
| skhd | MIT (LICENSE.txt, 2017 Asmund Vikane); repo now under asmvik (koekeishiya redirect) | 2025-12-09 master rename commit (atom); last functional tag v0.3.9 May 2023; README maintenance-mode banner |

## Per-project source notes (fetched specifics)

- AeroSpace: Package.swift pins `soffes/HotKey` exact 0.2.1; AppBundle links HotKey product.
  HotKey README: "wraps the Carbon APIs for dealing with global hot keys".
  private.h declares `AXError _AXUIElementGetWindow(AXUIElementRef, uint32_t *)`
  (the single private fn; README says everything else is public AX). README:
  not notarized by author choice, a distribution fact separate from API surface.
- yabai README requirements table: Intel Big Sur 11+ / ARM Monterey 12+ through
  Tahoe 26; Accessibility mandatory + restart; Screen Recording iff animations;
  separate Spaces must be ON. SIP wiki (edited Apr 2026): injection into
  Dock.app owns window-server connection; SIP-gated list = space/layer/sticky/
  shadow/transparency/animation/scratchpad/PiP; per-arch csrutil variants.
  Keyboard: external skhd (README points at asmvik/skhd).
- Amethyst: HotKeyRegistrar.swift migrates MASShortcut defaults to
  KeyboardShortcuts (`KeyboardShortcuts.Name`, `setShortcut`, `onKeyUp`) with
  `override` path breaking MASShortcut bindings. Package.resolved pins
  KeyboardShortcuts 2.2.4, MASShortcut (shpakovski master), Silica (ianyh
  master), Sparkle 2.9.5. HotKeyManager.swift maps NSEvent modifiers to Carbon
  `shiftKey/cmdKey/optionKey/controlKey`. Space.swift: `CGSSpaceID /
  CGSSpaceType` from Silica. Silica ships private CGS headers (CGSSpace.h,
  CGSWindow.h, CGSHotKeys.h) plus a CGSInternal submodule.
- Rectangle README: uses rxhanson/MASShortcut fork (upstream archived) + Sparkle
  via SPM for shortcut recording and updates; Pro is a separate proprietary
  product built on top. Explicit no-cross-Space-moves statement in README.
- Hammerspoon libhotkey.m: `RegisterEventHotKey(keycode, mods, {sig 'HMSP',
  monotonicID}, GetEventDispatcherTarget(), kEventHotKeyExclusive, ...)` with
  `InstallEventHandler` for pressed/released; key repeat synthesized with
  NSTimer (`keyRepeatDelay`/`keyRepeatInterval`). spaces/private.h declares SLS*
  externs; libspaces.m calls `SLSMainConnectionID`,
  `SLSCopyManagedDisplaySpaces`, `SLSMoveWindowsToManagedSpace`,
  `SLSCopySpacesForWindows`, `SLSGetActiveSpace`, plus Sonoma-14.5+ path via
  `SLSSpaceSetCompatID`/`SLSSetWindowListWorkspace`. HSuicore.m calls private
  `_AXUIElementGetWindow` during window construction and snapshots; geometry
  uses AX. libwindow.m adds private `CGSSetDebugOptions` for shadows. These
  identity/Spaces helpers must not be copied into a public-only adapter.
- Loop: `BaseEventTapMonitor` manages CFMachPort taps with `CGEvent.tapEnable`,
  run-loop sources, and restart-cascade guard; Event Monitoring folder holds
  Active/Passive/Local monitors on an EventTapThread. AccessibilityManager
  tracks `AXIsProcessTrustedWithOptions` + AXPermissionsChanged notifications.
  Loop.entitlements is empty (AX is runtime consent, not an entitlement).
  `Loop/Private APIs` holds SkyLightBridgedSPI/SymbolLoader/ToolBelt,
  SLSWindowTags, PrivateApis.swift: private SkyLight without SIP change,
  version-coupled like other SLS consumers.
- FlashSpace: project.yml deps include `wojciech-kulik/KeyboardShortcuts`
  (branch main) + Sparkle + TOMLKit/Yams; MARKETING_VERSION 4.18.79, macOS 14+.
  Accessibility folder holds AXUIElement/NSRunningApplication wrappers.
  Entitlements disable sandbox only. README design notes: no per-window or
  layout management by design (UNIX philosophy); PiP workaround parks windows
  in a screen corner; tips point at SKHD + CLI + Raycast extension.
- Phoenix: PHKeyHandler.m registers Carbon hotkeys (sig 'FNIX',
  `kEventHotKeyExclusive`) with first-come exclusivity handling and NSTimer
  repeat; PHGlobalEventMonitor.m uses `addGlobalMonitorForEventsMatchingMask`
  for mouse events only. Commit 2025-08-31 deprecates `Space#moveWindows` on
  13.6+/14.5+/15.0+. LICENSE.md is MIT with an app-icon carve-out.
- Glide ARCHITECTURE.md: sys layer wraps AXObserver/AXUIElement,
  CGWindowListCopyWindowInfo, private SkyLight, event tap, CFRunLoop executor;
  WmController owns hotkey registration (capture API unnamed: Unknown);
  CGEvent tap covers mouse (focus-follows-mouse, warp). Dual Apache-2.0/MIT.
- komorebi-for-mac (branch master): macOS tiler in Rust with `komorebic` CLI,
  UDS event subscriptions, client crate; public releases repo plus private
  KomoCorp nightly repo for sponsors. LICENSE.md is Komorebi 2.0.0 text
  (permitted-purpose + personal-use clauses); README adds the commercial-use
  paid-license path. Key-capture substrate: Unknown from fetched page.
- Swindler: MIT library, cached AX model; Spaces 0% per README scope; latest
  commit 2022-09-06 (stale alpha). No hotkey role (N/A).
- Related library AXSwift (MIT): thin Swift wrapper of the C AX client API,
  explicit errors, no cached state or workspace policy. Swindler builds on it;
  neither is a Rust adapter or a substitute for identity/recovery proof.
- Paneru: MIT; per-monitor strip, sliver workaround, native-Space compatible;
  key bindings in TOML/Lua, capture substrate Unknown from fetched README.
- JankyBorders: GPL-3.0; border.c calls `SLSReleaseWindow` and other SLS fns:
  private SkyLight ordering without SIP change, version-coupled.
- skhd: event_tap.c creates `CGEventTapCreate(kCGSessionEventTap,
  kCGHeadInsertEventTap, ...)`; skhd.c taps KeyDown + NX_SYSDEFINED with
  tap-disabled restart, requires AX (`AXIsProcessTrustedWithOptions`), aborts
  when Secure Keyboard Entry is set (`CGSIsSecureEventInputSet`), refuses root.
  hotload.c watches config via FSEventStream. carbon.c only tracks front-app
  switch. MIT; maintenance-mode banner; owner renamed koekeishiya to asmvik.

## Cross-cutting contrast (decision input, not a decision)

- Not all less-jank routes need SIP changes. yabai Dock injection needs partial
  SIP disable, but SkyLight/SLS consumers (Hammerspoon spaces helpers,
  JankyBorders, Loop private APIs, Glide detection, AeroSpace's single helper)
  install and run under stock SIP; they are version-coupled instead. Do not
  portray private-API use as SIP-gated, and do not portray public AX as the
  only stock-SIP path.
- Public AX lowers the system/security burden (no injection, no private
  linkage) but is not an installability guarantee: Accessibility consent is
  still required and MDM can deny it. Notarization is a distribution
  property, not App Store approval and not a statement about API surface
  (AeroSpace ships unnotarized by author choice while using mostly public
  API plus one private helper).
- Lower-level routes can avoid some animations (hide/show without moves,
  ordering without polling) but that does not guarantee less jank. Private
  routes add version coupling; AX routes add client latency and non-atomic
  resizing; parking adds visible slivers/shell effects. Input taps have a
  separate timeout/Secure Input risk. These are tradeoffs, not a ranking.
- Keyboard: discrete Carbon-style registration (Hammerspoon, Phoenix,
  AeroSpace via HotKey lib, Amethyst/Rectangle/FlashSpace via
  MASShortcut/KeyboardShortcuts deps) cannot intercept or suppress system
  chords the way an active session event tap can. Verify actual system-chord
  ownership and tap-mode TCC requirements per OS; project AX permission lists
  are not proof of every input mode. See the [setup runbook](../../macos-dev-environment.md).
- Corporate/distribution: Apple's PPPC payload can allow or deny Accessibility
  and input access by signed identity; stock SIP alone does not establish MDM
  installability. App Store rules require sandboxing (2.4.5) and public APIs
  (2.5.1), with alternate-desktop review exposure (2.5.8). Developer ID signing
  and notarization are a separate direct-distribution path, not Store approval.

## Source gaps (do not fill by inference)

- Exact key-capture substrate still Unknown after README-level reads:
  Rectangle Pro, komorebi-for-mac, Paneru, Glide (WmController unnamed).
  FlashSpace names its KeyboardShortcuts dep; the capture API inside that fork
  was not opened here.
- Rectangle Pro and komorebi-for-mac window-control internals: not stated on
  fetched pages. Closed-source stays Unknown (Rectangle Pro).
- Glide canonical home: resolved 2026-10-14 to `tmandry/glide`
  (`glide-wm/glide` redirects there via API; snapshot `ef3b30f8` below).
  Earlier dual listing above is a historical fetch record.
- License texts opened for all rows except Rectangle Pro (proprietary page,
  no license text) and dep libs (soffes/HotKey README states MIT; full dep
  license texts not opened).
- Dated evidence is latest-commit visibility on fetched feeds, not an
  activity verdict. Swindler (2022-09-06) is the only clearly stale tree.

## Source checkout snapshot (2026-10-14)

Full default-branch checkouts taken 2026-10-14 for source availability;
commit dates are committer dates. These snapshots do not replace source
evidence cited at historical pins (the plan.md blob links and the spec
oracle pins in `docs/spec/reference-outcomes.md` stay authoritative for
their claims). `koekeishiya/*` URLs in the Sources list below are
historical fetch records; the canonical owner is now `asmvik` (both
`koekeishiya/yabai` and `koekeishiya/skhd` redirect there via API,
verified 2026-10-14; the `asmvik/yabai` README/wiki links above were
already canonical). `MrKai77/Loop` differs from canonical `mrkai77/Loop`
by case only.

| Checkout | Upstream URL | Branch | HEAD SHA | Commit date |
| --- | --- | --- | --- | --- |
| cosmic-comp | https://github.com/pop-os/cosmic-comp | master | 4d14528f9108cbc0df5404b3b4b8fa6db75ddb41 | 2026-10-08 |
| Hyprland | https://github.com/hyprwm/Hyprland | main | 1bd31aaa579a40680ce876e1547e371b60c45522 | 2026-10-13 |
| i3 | https://github.com/i3/i3 | next | be04c853ee7fa01a82a0c81f8217565395e2627c | 2026-10-12 |
| sway | https://github.com/swaywm/sway | master | 1652c54b73f67df17b7b4ab0b0f7048204aa8104 | 2026-09-21 |
| yabai | https://github.com/asmvik/yabai | master | dd845723416f5fe92af49fad5ebab00369e07edd | 2026-06-14 |
| skhd | https://github.com/asmvik/skhd | master | a7105d5b3db6b25d01c0ded79a25536ab284b34f | 2025-12-10 |
| AeroSpace | https://github.com/nikitabobko/AeroSpace | main | 74a1bf17e82d70e0a21945ba04bb4f590bf19f83 | 2026-10-01 |
| Amethyst | https://github.com/ianyh/Amethyst | development | 5cfe2fe12b1a933139010da5760aaa7e44033e9b | 2026-10-13 |
| Silica | https://github.com/ianyh/Silica | master | ebead82a28486e7719b7ed69633ecfed8a4fe2d2 | 2026-09-29 |
| hammerspoon | https://github.com/Hammerspoon/hammerspoon | master | 23e387e2805a9890066366e0ac96c71b27f0cfd5 | 2026-07-08 |
| phoenix | https://github.com/kasper/phoenix | master | 916f8f19740530cd7cd63448ec0b6fa62925cebf | 2025-08-31 |
| Loop | https://github.com/MrKai77/Loop | develop | 573ace3071641bbe002527ccfc0811842300ccba | 2026-10-13 |
| FlashSpace | https://github.com/wojciech-kulik/FlashSpace | main | 968015308d8d284a98831f90f365538a63456ee9 | 2026-09-17 |
| komorebi-for-mac | https://github.com/LGUG2Z/komorebi-for-mac | master | bdf10452f10ebf15ec1978cb05d0aac79ef5611f | 2026-05-04 |
| glide | https://github.com/tmandry/glide | main | ef3b30f88445fe4b517eb4c60023011f6d3725dd | 2026-10-12 |
| Swindler | https://github.com/tmandry/Swindler | main | bf2c42f1db8bb1aa6c0b634d9fca3ab4bd37931d | 2022-09-05 |
| paneru | https://github.com/karinushka/paneru | main | 8a213313538a352537b26e25131cfe75f1e00724 | 2026-10-07 |
| JankyBorders | https://github.com/FelixKratz/JankyBorders | main | a7297ca7d1933f3a30b12e8f10750e8d84eeee1e | 2026-05-14 |
| PaperWM.spoon | https://github.com/mogenson/PaperWM.spoon | main | 2a561eb63df4fd72fc2556e411bd4e1c88f57f48 | 2026-10-13 |
| Rectangle | https://github.com/rxhanson/Rectangle | main | 1d46069c27b3b0564bd7cc1e6d97bf6d53093079 | 2026-10-13 |
| HotKey | https://github.com/soffes/HotKey | main | a3cf605d7a96f6ff50e04fcb6dea6e2613cfcbe4 | 2024-12-28 |
| AXSwift | https://github.com/tmandry/AXSwift | main | e18a18453d135ad45809a384ee5139e05ea52def | 2021-11-14 |
| rift | https://github.com/acsandmann/rift | main | 8114ccc534b5c9a7e6a77ce27568d8739be9216f | 2026-10-13 |
| Defi | https://github.com/qeude/Defi | main | effabd625a26dc8593d4420032cf9dffb8d9ffbe | 2026-10-13 |
| glazewm | https://github.com/glzr-io/glazewm | main | 5709ad0a3c7c386bbc3e38166a865ffc12937515 | 2026-04-09 |

All 25 checkouts are full (non-shallow) clones. `rift`
(acsandmann/rift, ~2.5k stars, pushed 2026-10-14) and `Defi` (qeude/Defi,
MIT, pushed 2026-10-13, Niri-inspired scrollable tiler) are GitHub-search
additions. `glazewm` HEAD matches the plan-cited pin `5709ad0a`.
Swindler's repo-level push stamp (2023-12-31) comes from side branch
`restoration`; default-branch `main` HEAD is still 2022-09-05. The spec
`PaperWM` profile pin `8bf6dd26` is absent from `PaperWM.spoon` (that pin
belongs to the GNOME Shell PaperWM tree, not the macOS spoon).

## Sources (only URLs fetched in this unit)

- https://api.github.com/repos/Hammerspoon/hammerspoon/git/trees/master?recursive=1
- https://api.github.com/repos/koekeishiya/skhd/git/trees/master?recursive=1
- https://api.github.com/repos/kasper/phoenix/git/trees/master?recursive=1
- https://api.github.com/repos/nikitabobko/AeroSpace/git/trees/main?recursive=1
- https://api.github.com/repos/ianyh/Amethyst/git/trees/development?recursive=1
- https://raw.githubusercontent.com/Hammerspoon/hammerspoon/master/extensions/hotkey/libhotkey.m
- https://raw.githubusercontent.com/Hammerspoon/hammerspoon/master/extensions/spaces/private.h
- https://raw.githubusercontent.com/Hammerspoon/hammerspoon/master/extensions/spaces/libspaces.m
- https://raw.githubusercontent.com/Hammerspoon/hammerspoon/master/extensions/window/libwindow.m
- https://raw.githubusercontent.com/kasper/phoenix/master/Phoenix/PHKeyHandler.m
- https://raw.githubusercontent.com/kasper/phoenix/master/Phoenix/PHGlobalEventMonitor.m
- https://raw.githubusercontent.com/koekeishiya/skhd/master/src/carbon.c
- https://raw.githubusercontent.com/koekeishiya/skhd/master/src/event_tap.c
- https://raw.githubusercontent.com/koekeishiya/skhd/master/src/hotload.c
- https://raw.githubusercontent.com/koekeishiya/skhd/master/src/skhd.c
- https://raw.githubusercontent.com/ianyh/Amethyst/development/Amethyst/Events/HotKeyManager.swift
- https://raw.githubusercontent.com/ianyh/Amethyst/development/Amethyst/Managers/HotKeyRegistrar.swift
- https://raw.githubusercontent.com/ianyh/Amethyst/development/Amethyst/Model/Space.swift
- https://raw.githubusercontent.com/ianyh/Amethyst/development/Amethyst.xcworkspace/xcshareddata/swiftpm/Package.resolved
- https://raw.githubusercontent.com/nikitabobko/AeroSpace/main/Package.swift
- https://raw.githubusercontent.com/nikitabobko/AeroSpace/main/Sources/PrivateApi/include/private.h
- https://raw.githubusercontent.com/nikitabobko/AeroSpace/main/Sources/PrivateApi/include/private.m
- https://raw.githubusercontent.com/nikitabobko/AeroSpace/main/README.md
- https://raw.githubusercontent.com/MrKai77/Loop/develop/Loop/Utilities/Event%20Monitoring/BaseEventTapMonitor.swift
- https://raw.githubusercontent.com/MrKai77/Loop/develop/Loop/Utilities/AccessibilityManager.swift
- https://raw.githubusercontent.com/MrKai77/Loop/develop/Loop/Loop.entitlements
- https://raw.githubusercontent.com/wojciech-kulik/FlashSpace/main/project.yml
- https://raw.githubusercontent.com/wojciech-kulik/FlashSpace/main/FlashSpace/FlashSpace.entitlements
- https://raw.githubusercontent.com/tmandry/glide/main/ARCHITECTURE.md
- https://raw.githubusercontent.com/tmandry/glide/main/LICENSE-MIT
- https://raw.githubusercontent.com/tmandry/glide/main/LICENSE-APACHE
- https://raw.githubusercontent.com/FelixKratz/JankyBorders/main/src/border.c
- https://raw.githubusercontent.com/rxhanson/Rectangle/master/README.md
- https://raw.githubusercontent.com/asmvik/yabai/master/README.md
- https://github.com/asmvik/yabai/wiki/Disabling-System-Integrity-Protection
- https://raw.githubusercontent.com/Hammerspoon/hammerspoon/master/LICENSE
- https://raw.githubusercontent.com/nikitabobko/AeroSpace/main/LICENSE.txt
- https://raw.githubusercontent.com/kasper/phoenix/master/LICENSE.md
- https://raw.githubusercontent.com/koekeishiya/skhd/master/LICENSE.txt
- https://raw.githubusercontent.com/asmvik/yabai/master/LICENSE.txt
- https://raw.githubusercontent.com/ianyh/Amethyst/development/LICENSE.md
- https://raw.githubusercontent.com/rxhanson/Rectangle/master/LICENSE
- https://raw.githubusercontent.com/MrKai77/Loop/develop/LICENSE
- https://raw.githubusercontent.com/wojciech-kulik/FlashSpace/main/LICENSE
- https://raw.githubusercontent.com/FelixKratz/JankyBorders/main/LICENSE
- https://raw.githubusercontent.com/tmandry/Swindler/main/LICENSE
- https://raw.githubusercontent.com/karinushka/paneru/main/LICENSE.txt
- https://raw.githubusercontent.com/mogenson/PaperWM.spoon/main/LICENSE
- https://raw.githubusercontent.com/LGUG2Z/komorebi-for-mac/master/LICENSE.md
- https://github.com/soffes/HotKey
- https://github.com/ianyh/Silica
- https://github.com/ianyh/Silica/tree/master/Silica/Sources
- https://github.com/ianyh/Silica/tree/master/Silica/include
- https://github.com/MrKai77/Loop
- https://github.com/MrKai77/Loop/tree/develop/Loop
- https://github.com/MrKai77/Loop/tree/develop/Loop/Utilities
- https://github.com/MrKai77/Loop/tree/develop/Loop/Private%20APIs
- https://github.com/MrKai77/Loop/tree/develop/Loop/Utilities/Event%20Monitoring
- https://github.com/wojciech-kulik/FlashSpace
- https://github.com/wojciech-kulik/FlashSpace/tree/main/FlashSpace
- https://github.com/wojciech-kulik/FlashSpace/tree/main/FlashSpace/Accessibility
- https://github.com/LGUG2Z/komorebi-for-mac
- https://rectangleapp.com/pro
- https://github.com/nikitabobko/AeroSpace/commits/main
- https://github.com/nikitabobko/AeroSpace/commits/main.atom
- https://github.com/Hammerspoon/hammerspoon/commits/master.atom
- https://github.com/kasper/phoenix/commits/master.atom
- https://github.com/asmvik/skhd/commits/master.atom
- https://github.com/ianyh/Amethyst/commits/development.atom
- https://github.com/mrkai77/Loop/commits/develop.atom
- https://github.com/wojciech-kulik/FlashSpace/commits/main.atom
- https://github.com/rxhanson/Rectangle/commits/main.atom
- https://github.com/asmvik/yabai/commits/master.atom
- https://github.com/tmandry/glide/commits/main.atom
- https://github.com/glide-wm/glide/commits/main.atom
- https://github.com/LGUG2Z/komorebi-for-mac/commits/master.atom
- https://github.com/LGUG2Z/komorebi/commits/master.atom
- https://github.com/tmandry/Swindler/commits/main.atom
- https://github.com/karinushka/paneru/commits/main.atom
- https://github.com/FelixKratz/JankyBorders/commits/main.atom
- https://github.com/mogenson/PaperWM.spoon/commits/main.atom
- https://nikitabobko.github.io/AeroSpace/guide
- https://raw.githubusercontent.com/Hammerspoon/hammerspoon/master/Hammerspoon/HSuicore.m
- https://github.com/mogenson/PaperWM.spoon
- https://github.com/tmandry/Swindler
- https://github.com/karinushka/paneru
- https://raw.githubusercontent.com/ianyh/Amethyst/development/README.md
- https://github.com/ianyh/Amethyst/issues/1258
- https://github.com/ianyh/Amethyst/pull/1331
- https://rectangleapp.com/pro/docs/getting-started
- https://raw.githubusercontent.com/tmandry/AXSwift/main/README.md
- https://raw.githubusercontent.com/tmandry/AXSwift/main/LICENSE
- https://support.apple.com/guide/deployment/privacy-preferences-policy-control-payload-settings-dep38df53c2a/web
- https://developer.apple.com/app-store/review/guidelines/
- https://developer.apple.com/documentation/coregraphics/cgevent/tapcreate(tap:place:options:eventsofinterest:callback:userinfo:).md

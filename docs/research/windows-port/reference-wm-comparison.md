# Windows reference WM comparison (GlazeWM, komorebi, Seelen, Whim vs ours)

Research only. No product decisions, no binding/catalog changes, no parked-policy
changes. Source facts cite inspected source at pinned SHAs; runtime behavior is
untested unless noted. Inference and documented-but-unverified claims are labeled.
Per area: verified facts, then ours, then gap/runtime-unknown, then recommendation.

**Shortcut finding: no working official desktop-API Win+G/Win+F11 suppression
technique was established. Containment stays PARKED.** E8 masks Start-menu
activation; it is not proof of suppressing Xbox/Game Bar (section 4).

## Pins

| Repo | HEAD | Origin |
| ---- | ---- | ------ |
| ours (omnitiler) | `a20e8e8efded9b84892778e9c292b5b09cad68b8` | local |
| GlazeWM | `5709ad0a3c7c386bbc3e38166a865ffc12937515` | https://github.com/glzr-io/glazewm |
| komorebi | `e0709f02bfae4e503bf4640f58ee75ecbbfdbb97` | https://github.com/LGUG2Z/komorebi |
| Seelen-UI | `56c1d75dae814bd5c3d03d5eea3d6f0428b02db1` | https://github.com/eythaann/Seelen-UI.git |
| Whim | `1e86b579206373a8e9939a5343b67696419ed284` | https://github.com/dalyIsaac/Whim.git |
| win-hotkeys git (Seelen dep) | `9123179fab1aa932a3eccc7c19848f3435989154` | https://github.com/Seelen-Inc/windows-keyboard-hook |
| whkd (remote, not cloned) | `4f0aaa131f5bd709c35b4869d224ff265be166e8` | https://github.com/LGUG2Z/whkd |
| Zebar (remote, not cloned) | `5f90d341867122175cb4a57af594a44752885056` | https://github.com/glzr-io/zebar |

Immutable pattern: `https://github.com/<org>/<repo>/blob/<pin>/<path>#Lx-Ly`.
Ours citations are local links labeled `file:lines @ a20e8e8`.

## 1. Hiding

- Facts (komorebi): `HidingBehaviour` Hide / Minimize / Cloak (default).
  Upstream labels Hide EOL, reports Electron issues, and calls Cloak
  "undocumented SetCloak"; these are upstream claims, not comparative tests:
  [core/mod.rs#721-727](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/core/mod.rs#L721-L727).
- Hide/restore branch (`SetCloak(hwnd,1,2)` vs `(hwnd,1,0)`):
  [window.rs#696-709](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L696-L709),
  [window.rs#731-736](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L731-L736).
  Cloak is private `IApplicationView::SetCloak` via COM, not `DWMWA_CLOAK`:
  [com/mod.rs#67-100](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/com/mod.rs#L67-L100);
  upstream's code comment reports `DWMWA_CLOAK` write failing `E_ACCESSDENIED` on foreign HWNDs:
  [window.rs#279-283](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L279-L283).
- Facts (Glaze): `HideMethod` Hide / Cloak (default) / PlaceInCorner:
  [parsed_config.rs#143-150](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-common/src/parsed_config.rs#L143-L150),
  default Cloak:
  [parsed_config.rs#110-120](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-common/src/parsed_config.rs#L110-L120);
  sync branches cloak vs show/hide:
  [platform_sync.rs#460-467](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/general/platform_sync.rs#L460-L467);
  cloak is `IApplicationView::set_cloak(1, 2/0)`:
  [native_window.rs#466-491](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L466-L491).
  PlaceInCorner parks at a monitor corner with a 1px sliver:
  [platform_sync.rs#351-385](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/general/platform_sync.rs#L351-L385).
- Facts (Seelen/Whim): Seelen workspace hide is minimize-based:
  [virtual_desktops/mod.rs#516-526](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/virtual_desktops/mod.rs#L516-L526),
  call at:
  [virtual_desktops/mod.rs#385-391](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/virtual_desktops/mod.rs#L385-L391);
  interactable gate excludes cloaked except own transient cloaks:
  [application/windows.rs#212-221](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/modules/apps/application/windows.rs#L212-L221).
  Whim hide is `SW_HIDE`:
  [NativeManager.cs#51-55](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim/Native/NativeManager.cs#L51-L55).
- Ours: `SW_HIDE` via `ShowWindowAsync` + pumped visibility readback;
  no cloak, no virtual-desktop API:
  [product_hide.rs:618-647 @ a20e8e8](../../../crates/tiler-windows/src/product_hide.rs).
- Native desktops are distinct: these cloak/minimize/hide paths emulate workspaces;
  komorebi additionally gates events against its associated native desktop
  [process_event.rs#200-256](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/process_event.rs#L200-L256), not a COM desktop-switching replacement.
- Gap/runtime unknown: cloak keeps `IsWindowVisible` true while hiding composition; SW_HIDE removes the taskbar entry; minimize keeps it. Chromium staleness and 24H2+ cloak untested.
- Recommendation: keep ours under the official-API constraint. Private `IApplicationView::SetCloak` is undocumented/private API, not a public `DWMWA_CLOAK` substitute; cloak + `DeleteTab` only conditionally if taskbar churn is measured (low priority).

## 2. Admission (Steam / Firefox / elevated / UWP)

- Facts (komorebi): ignore/manage overrides + style/cloak gates; layered-whitelist calls out Steam:
  [window.rs#1237-1244](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L1237-L1244);
  defaults include `steam.exe`, Firefox name-change-on-launch and multi-window
  `ApplicationFrameHost.exe`: [lib.rs#83-134](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/lib.rs#L83-L134).
  Firefox launch tolerance skips minimized windows:
  [window_manager_event.rs#171-211](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window_manager_event.rs#L171-L211).
- Facts (Seelen, strongest citable): UWP frame-creator resolution:
  [window/mod.rs#298-329](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/windows_api/window/mod.rs#L298-L329);
  excludes owned/child, tool/noactivate, TabProxy, deleted tabs, frozen:
  [application/windows.rs#212-281](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/modules/apps/application/windows.rs#L212-L281);
  unelevated-unmanageable ops forward to an elevated service:
  [window/mod.rs#351-418](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/windows_api/window/mod.rs#L351-L418).
- Facts (Whim/Glaze): Whim ignores `steamwebhelper.exe` unless title `Steam`:
  [DefaultFilteredWindowsKomorebi.g.cs#226-227](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim/Filter/DefaultFilteredWindowsKomorebi.g.cs#L226-L227),
  plus `MozillaTaskbarPreviewClass`:
  [DefaultFilteredWindowsKomorebi.g.cs#156-161](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim/Filter/DefaultFilteredWindowsKomorebi.g.cs#L156-L161).
  Glaze gates on style/visibility with a Flow.Launcher carve-out:
  [manage_window.rs#91-151](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/window/manage_window.rs#L91-L151),
  default ignores for Search/Shell/StartMenu hosts:
  [user_config.rs#107-194](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/user_config.rs#L107-L194);
  optional uiAccess manifest: [build.rs#7-53](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/build.rs#L7-L53).
- Ours: shell-class and `#32770` refusal plus medium-integrity,
  same-SID/session, scope gates:
  [product_hide.rs:203-307 @ a20e8e8](../../../crates/tiler-windows/src/product_hide.rs).
- Gap/runtime unknown: Steam/Firefox/UWP foreground behavior under our gates is untested; we refuse elevated where Seelen forwards via service.
- Recommendation: keep ours; log focused Steam/Firefox/UWP observations during normal use (S); layered-whitelist / frame-creator ideas only as future filter inputs, not now.

## 3. Minimum size

- Facts (komorebi): user-configured eligibility floor, not per-app declared
  minima - `minimum_window_width/height` config:
  [static_config.rs#494-499](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/static_config.rs#L494-L499),
  backing `MINIMUM_WIDTH`:
  [window.rs#67](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L67),
  checked in `should_manage` with debug flags:
  [window.rs#995-1018](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L995-L1018).
- Facts (others, targeted search only): Glaze nearest equivalent is a relative
  `MIN_TILING_SIZE = 0.01` fraction:
  [tiling_size_getters.rs#8-12](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/traits/tiling_size_getters.rs#L8-L12),
  plus float-if-not-resizable [manage_window.rs#301-306](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/window/manage_window.rs#L301-L306); no `MINMAXINFO` plumbing found.
  Seelen/Whim: no per-app minima enforcement found (targeted search,
  not universal proof).
- Ours: fresh `WM_GETMINMAXINFO` track validation and outer-to-visible mapping
  via frame insets; invalid maps to unknown, never zero:
  [tiling.rs:81-116 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/tiling.rs).
  Queries budget 10ms each/40ms per operation:
  [minimum-size record:53-57 @ a20e8e8](../../changes/archive/windows-minimum-size-hints.md).
- Gap/runtime unknown: relative clamping vs declared-minima behavior untested side by side.
- Recommendation: keep ours; cite as our lead. No adoption.

## 4. Input, Win key, Game Bar / Xbox (parked)

- Facts (Glaze/Whim LL hooks): Glaze installs + consumes on match,
  keypress-only:
  [keyboard_hook.rs#116-124](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/keyboard_hook.rs#L116-L124),
  [keyboard_hook.rs#150-195](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/keyboard_hook.rs#L150-L195);
  Glaze engine parses Win/LWin/RWin from user binding strings but the sample
  default avoids Win:
  [key_code.rs#176-179](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/models/key_code.rs#L176-L179).
  Whim installs + consumes matched keydown-only:
  [KeybindHook.cs#26-35](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim/Keybind/KeybindHook.cs#L26-L35),
  [KeybindHook.cs#57-84](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim/Keybind/KeybindHook.cs#L57-L84).
- Facts (win-hotkeys crate, both consumers): Seelen pins git 0.5.1:
  [Cargo.lock#7543-7545](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/Cargo.lock#L7543-L7545);
  the crate is LL-hook (`SetWindowsHookExW(WH_KEYBOARD_LL)` install,
  `LRESULT(1)` on Block, E8 silent-key masking):
  [hook.rs#52-85](https://github.com/Seelen-Inc/windows-keyboard-hook/blob/9123179fab1aa932a3eccc7c19848f3435989154/src/hook.rs#L52-L85),
  [hook.rs#87-150](https://github.com/Seelen-Inc/windows-keyboard-hook/blob/9123179fab1aa932a3eccc7c19848f3435989154/src/hook.rs#L87-L150),
  [hook.rs#152-185](https://github.com/Seelen-Inc/windows-keyboard-hook/blob/9123179fab1aa932a3eccc7c19848f3435989154/src/hook.rs#L152-L185).
  Seelen calls `start_keyboard_capturing` + `register_hotkey` (crate method,
  not Win32 `RegisterHotKey`):
  [hotkeys.rs#11-20](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/service/hotkeys.rs#L11-L20),
  [hotkeys.rs#64-70](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/service/hotkeys.rs#L64-L70).
  whkd uses registry `win-hotkeys 0.5.1`:
  [Cargo.toml#20-33](https://github.com/LGUG2Z/whkd/blob/4f0aaa131f5bd709c35b4869d224ff265be166e8/Cargo.toml#L20-L33),
  dispatching via `HkmData::register` to `komorebic`:
  [main.rs#39-75](https://github.com/LGUG2Z/whkd/blob/4f0aaa131f5bd709c35b4869d224ff265be166e8/src/main.rs#L39-L75).
  Registry 0.5.1 also installs `WH_KEYBOARD_LL`, waits up to 250ms for an action,
  and uses E8 on Replace: [hook.rs:21-25,115-145,196-238](https://docs.rs/crate/win-hotkeys/0.5.1/source/src/hook.rs#115).
  This differs from Seelen's pinned git implementation; neither is Win32 `RegisterHotKey`.
  Komorebi core has no input path by design (external whkd/AHK):
  [design.md#14-22](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/docs/design.md#L14-L22).
- Facts (MS): LL nonzero blocks target-window delivery
  ([LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc));
  `MOD_WIN` is a reservation, not impossibility proof
  ([RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey));
  Keyboard Filter supports custom blocked chords on Enterprise/Education/IoT,
  not the selected Windows 11 Pro; enabling it is a machine feature requiring restart
  ([overview](https://learn.microsoft.com/en-us/windows/configuration/keyboard-filter/),
  [custom keys](https://learn.microsoft.com/en-us/windows/configuration/keyboard-filter/wekf-customkey)).
- Ours: LL install ([snapkey.rs:2876-2906 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/snapkey.rs)), minimal callback ([snapkey.rs:2303-2327 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/snapkey.rs)), E8 mask pair + `VK_MASK` ([snapkey.rs:2778-2806 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/snapkey.rs), [snapkey.rs:65-96 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/snapkey.rs)). Observed leak: consumed marked Win+F11 still raised the Xbox prompt ([decisions.md:177-184 @ a20e8e8](../../../docs/decisions.md), [backlog.md:103-126 @ a20e8e8](../../../docs/backlog.md)).
- Gap/runtime unknown: no working official-API Win+G/Win+F11 suppression is established in any ref (absence-of-technique, not proof none exists). All OS-setting probes stay parked pending the user, not a top next action.
- Recommendation: keep parked; document the leak. Neither LL consumption nor
  E8 masking supplies new suppression evidence; bindings/settings remain user choices.

## 5. Recovery

- Facts (komorebi): snapshot + restore-all on Stop/Ctrl-C:
  [window_manager.rs#1360-1393](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window_manager.rs#L1360-L1393),
  [main.rs#349-379](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/main.rs#L349-L379);
  panic hook logs only, no restore-on-panic:
  [main.rs#106-129](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/main.rs#L106-L129).
- Facts (Glaze/Seelen): Glaze `wm-watcher` best-effort shows tracked windows
  and restores taskbar/border/transparency on unexpected IPC close:
  [wm-watcher/main.rs#26-53](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-watcher/src/main.rs#L26-L53).
  Seelen isolates per-command COM panics via `catch_unwind`:
  [com.rs#184-197](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/windows_api/com.rs#L184-L197).
- Ours: exact-owner stop file + 5s wait ([lifecycle.rs:99-111 @ a20e8e8](../../../crates/tiler-windows/src/lifecycle.rs)), same-exe watcher with readiness handshake ([product_hide.rs:36-63 @ a20e8e8](../../../crates/tiler-windows/src/product_hide.rs)), reveal on success and error paths with ledger retained on Uncertain ([lifecycle.rs:688-737 @ a20e8e8](../../../crates/tiler-windows/src/lifecycle.rs)).
- Gap/runtime unknown: reference forced-crash behavior untested; ours already has
  accepted automatic watcher-reveal evidence, not newly retested here
  [managed-workspace record:60-85,98-123 @ a20e8e8](../../changes/archive/windows-managed-workspaces.md).
- Recommendation: keep ours (strongest identity fencing). Note Glaze border/transparency residue coverage for our overlay story (S).

## 6. Monitors / mixed DPI

- Facts: Glaze enumerates per-display DPI [display.rs#67-89](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/display.rs#L67-L89).
  Komorebi uses PMv2 and monitor DPI [windows_api.rs#1145-1148](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/windows_api.rs#L1145-L1148), [windows_api.rs#1246-1262](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/windows_api.rs#L1246-L1262);
  its frame-bounds helper contains a DPI TODO, not proof of incorrect scaling
  [windows_api.rs#765-777](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/windows_api.rs#L765-L777).
- Glaze adds a pending-DPI double-move:
  [platform_sync.rs#450-456](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/general/platform_sync.rs#L450-L456).
- komorebi-bar reserves space via `MonitorWorkAreaOffset`:
  [bar.rs#477-493](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi-bar/src/bar.rs#L477-L493),
  [main.rs#234-305](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi-bar/src/main.rs#L234-L305).
- Ours: PMv2 + all-monitor enumeration already exist, device-keyed domains with
  whole-workspace displacement/return [tiling_sys.rs:172-267,9949-10167 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/tiling_sys.rs).
  Per-target DPI border scaling [active_border.rs:129-163 @ a20e8e8](../../../crates/tiler-windows/src/active_border.rs).
- Gap/runtime unknown: our physical acceptance is single-monitor; parity 9 and
  mixed-DPI acceptance belong to the other Windows test system. Legacy `display-0`
  constants do not describe the managed-workspace runtime.
- Recommendation: investigate fresh DPI/frame readback after cross-output moves;
  Glaze's double-move is a candidate only if measurements justify it (L).

## 7. Fullscreen games / anti-cheat

- Facts (Seelen): borderless-cover detection (no THICKFRAME + inner rect
  covers monitor, 1px Chromium tolerance):
  [mod.rs#300-315](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/windows_api/mod.rs#L300-L315);
  gaming template unmanages Steam auxiliary windows (title other than `Steam`)
  and paths containing `steamapps`: [gaming.yml#1-20](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/static/apps_templates/gaming.yml#L1-L20).
- Facts (others): Glaze treats fullscreen first-class (`should_fullscreen`:
  [window_getters.rs#102-124](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/traits/window_getters.rs#L102-L124),
  `MarkFullscreenWindow`:
  [native_window.rs#493-509](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L493-L509)).
  Komorebi: no fullscreen detection helper found (targeted search). Whim: only a `Playnite.FullscreenApp.exe` default-ignore. Anti-cheat: no evidence anywhere; all geometry uses public APIs.
- Ours: fullscreen refused admission, managed-path hide only, loop suspends on fullscreen foreground ([product_hide.rs:284-293,1069-1080 @ a20e8e8](../../../crates/tiler-windows/src/product_hide.rs), [lifecycle.rs:654-688 @ a20e8e8](../../../crates/tiler-windows/src/lifecycle.rs)).
- Gap/runtime unknown: game-overlay/topmost runtime untested; no anti-cheat
  compatibility claims. Seelen's separate tray/taskband hook DLL is shell
  integration, not a technique to adopt for gaming [lib.rs#154-196](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/hook_dll/src/lib.rs#L154-L196).
- Recommendation: keep ours (suspend-on-fullscreen closest); borrow Seelen borderless-cover wording only when the gaming story is scoped (S).

## 8. Focus (foreground lock)

- Facts (komorebi): empty mouse `SendInput` to self, then
  `SetWindowPos TOP/SHOWWINDOW/ASYNC` + `SetForegroundWindow`:
  [windows_api.rs#698-723](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/windows_api.rs#L698-L723).
  Startup also attempts to zero session `ForegroundLockTimeout`
  [main.rs#254](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/main.rs#L254),
  [windows_api.rs#1161-1197](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/windows_api.rs#L1161-L1197).
- Facts (Glaze/Seelen): tagged self input (`FOREGROUND_INPUT_IDENTIFIER=6379`)
  then `SetForegroundWindow`:
  [native_window.rs#284-307](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L284-L307).
  Seelen documents silent TOPMOST-pending failure; transient `AttachThreadInput`
  borrow around `SetWindowPos`, still `NOACTIVATE`:
  [mod.rs#424-459](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/windows_api/mod.rs#L424-L459).
- Ours: E8 `SendInput` last-input prime with no Alt ([tiling_sys.rs:4355-4404 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/tiling_sys.rs)); one bounded attach + single `SetForegroundWindow` with immediate detach, exact readback, pumped settle ([tiling_sys.rs:4532-4620 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/tiling_sys.rs)).
- Gap/runtime unknown: relative miss rates untested.
- Recommendation: keep ours; measure any focus misses before changing the ladder.
  Do not adopt komorebi's session-wide timeout mutation.

## 9. Animations / latency

- Facts (komorebi): animations default OFF with engine cancel/wait; ghost-move
  cloaks the source (Chromium skips pre-paint), animates a live DWM-thumbnail
  ghost, then uncloaks:
  [window.rs#251-318](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L251-L318),
  fade via `DwmUpdateThumbnailProperties`:
  [window.rs#382-385](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/window.rs#L382-L385);
  `position_window` uses `NO_ACTIVATE|NO_SEND_CHANGING|NO_COPY_BITS|FRAME_CHANGED`
  + optional async [windows_api.rs#482-539](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/windows_api.rs#L482-L539).
  Defaults: [animation/mod.rs#59-83](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/animation/mod.rs#L59-L83).
- Facts (Glaze/Seelen): Glaze reads/toggles `SPI_GET/SETANIMATION` (tray only)
  and throttles mouse Move:
  [dispatcher.rs#208-241](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/dispatcher.rs#L208-L241),
  [mouse_listener.rs#224-270](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/mouse_listener.rs#L224-L270).
  Seelen disables per-window transitions:
  [mod.rs#828-832](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/background/windows_api/mod.rs#L828-L832),
  with a ShowWindow-vs-Async deadlock note.
- Ours: no animation API; Snap prevention via `SPI_GET/SETWINARRANGING` ledger; hook callback bounded; 8 dispatches/tick ([snapkey.rs:65-96 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/snapkey.rs), [snapkey.rs:2303-2327 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/snapkey.rs)).
- Gap/runtime unknown: no frame-time numbers on any side; no performance superiority claims.
- Recommendation: keep immediate moves; investigate per-window transitions only
  for measured residue, with recovery preimages (S).

## 10. Borders

- Facts (komorebi): full D2D custom vs thin accent border managers, following
  LOCATIONCHANGE + animation rects; accent set/remove on retile/restore:
  [border_manager/mod.rs#228-300](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/border_manager/mod.rs#L228-L300).
  Tracked D2D rendering: [border.rs#114-194](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/border_manager/border.rs#L114-L194).
- Facts (Glaze/Whim): Glaze has no overlay; per-window `DWMWA_BORDER_COLOR`
  with 50ms delayed re-apply for self-painting windows:
  [native_window.rs#617-638](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L617-L638).
  Delayed reapply: [platform_sync.rs#558-580](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/commands/general/platform_sync.rs#L558-L580).
  Whim: no active-border color plugin found at pin (targeted search; its
  `DwmSetWindowAttribute` use is corner preference only:
  [NativeManager.cs#171-189](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim/Native/NativeManager.cs#L171-L189))
  - not a blanket claim.
- Ours: owned overlay with KDE-parity geometry, per-target DPI, theme gate, fullscreen/maximized/cloaked suppression ([active_border.rs:129-206,290-347 @ a20e8e8](../../../crates/tiler-windows/src/active_border.rs)).
- Gap/runtime unknown: self-painting window residue untested.
- Recommendation: keep ours (parity geometry); Glaze's delayed attribute reapply
  matters only to a DWM-color path, not our independently rendered overlay.

## 11. Window drag/drop and drop previews (parity 7/8)

- Facts (komorebi): observes user drags (MoveResizeStart/End:
  [process_event.rs#647-660](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/process_event.rs#L647-L660); cross-monitor `transfer_window`, resize-vs-move by edge delta:
  [process_event.rs#780-820](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi/src/process_event.rs#L780-L820)).
- Facts (Whim preview overlay): `LayoutPreviewPlugin` computes
  `MoveWindowToPoint` + `DoLayout` at the cursor:
  [LayoutPreviewPlugin.cs#31-39](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim.LayoutPreview/LayoutPreviewPlugin.cs#L31-L39),
  [LayoutPreviewPlugin.cs#50-101](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim.LayoutPreview/LayoutPreviewPlugin.cs#L50-L101);
  transparent borderless overlay under the cursor, `SWP_NOACTIVATE|SWP_SHOWWINDOW`:
  [LayoutPreviewWindow.xaml.cs#29-38](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim.LayoutPreview/LayoutPreviewWindow.xaml.cs#L29-L38),
  [LayoutPreviewWindow.xaml.cs#47-93](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim.LayoutPreview/LayoutPreviewWindow.xaml.cs#L47-L93),
  [LayoutPreviewWindow.xaml.cs#132-144](https://github.com/dalyIsaac/Whim/blob/1e86b579206373a8e9939a5343b67696419ed284/src/Whim.LayoutPreview/LayoutPreviewWindow.xaml.cs#L132-L144).
- Glaze likewise observes user drags and drops temporary floating windows back
  into tiling [handle_window_moved_or_resized_end.rs#24-28,101-142](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm/src/events/handle_window_moved_or_resized_end.rs#L101-L142).
- Tile-drop overlay is distinct from komorebi's DWM-thumbnail move animation
  (see 9) - do not conflate. KDE drag is the behavior oracle; Whim is
  observe-don't-drive, not a Meta+drag producer.
- Ours: parity 7/8 not implemented; only hit-code precursors exist:
  [group_underlay.rs:101-145 @ a20e8e8](../../../crates/tiler-windows/src/group_underlay.rs).
- Gap/runtime unknown: drop-target geometry and fresh-admission filtering during drag untested.
- Recommendation: prototype geometry + fresh admission first (M), then preview overlay shaped on Whim (L). No implementation here.

## 12. Taskbar / workspace indicators (parity 10)

- Facts (komorebi-bar): per-monitor workspace strip from socket `MonitorInfo`
  (`focused_workspace_idx`, `should_show`/hide-empty); click focuses via
  `FocusMonitorWorkspaceNumber`:
  [komorebi.rs#192-223](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi-bar/src/widgets/komorebi.rs#L192-L223),
  helper: [komorebi.rs#790-806](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi-bar/src/widgets/komorebi.rs#L790-L806),
  hide-empty rule: [komorebi.rs#844-876](https://github.com/LGUG2Z/komorebi/blob/e0709f02bfae4e503bf4640f58ee75ecbbfdbb97/komorebi-bar/src/widgets/komorebi.rs#L844-L876);
  space reserved via the workarea-offset protocol (see 6).
- Facts (Seelen grid widget): per-monitor rows/columns with `active`/`viewing`
  classes, names, click/Enter to switch via `SwitchWorkspace`, destroy button:
  [Workspace.svelte#34-47](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/ui/svelte/workspaces-viewer/app/Workspace.svelte#L34-L47),
  [Workspace.svelte#72-110](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/ui/svelte/workspaces-viewer/app/Workspace.svelte#L72-L110),
  driven by `active_workspace`:
  [Monitor.svelte#24-36](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/ui/svelte/workspaces-viewer/app/Monitor.svelte#L24-L36), [Monitor.svelte#82-95](https://github.com/eythaann/Seelen-UI/blob/56c1d75dae814bd5c3d03d5eea3d6f0428b02db1/src/ui/svelte/workspaces-viewer/app/Monitor.svelte#L82-L95).
- Facts (Zebar): bundled Glaze starter renders named workspace buttons with
  separate `focused`/`displayed` classes; click issues `focus --workspace`.
  The komorebi starter renders names and focused styling without a click handler:
  [with-glazewm.html#126-142](https://github.com/glzr-io/zebar/blob/5f90d341867122175cb4a57af594a44752885056/resources/starter/with-glazewm.html#L126-L142),
  [with-komorebi.html#127-138](https://github.com/glzr-io/zebar/blob/5f90d341867122175cb4a57af594a44752885056/resources/starter/with-komorebi.html#L127-L138).
  These are standalone HTML/React bars, not native taskbar items.
  Glaze pairs cloak with `DeleteTab`/`AddTab`:
  [native_window.rs#511-529](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/windows/native_window.rs#L511-L529).
  Whim bar is a WinUI/XAML plugin from template config (existence only).
- Ours: no presenter; `workspace_event` vocabulary unconsumed;
  item 10 mechanism unselected:
  [workspace.rs:600-620 @ a20e8e8](https://github.com/beefsack/OmniTiler/blob/a20e8e8/crates/tiler-windows/src/workspace.rs).
- Gap/runtime unknown: strip vs grid vs BYO-UI fit for KDE parity untested.
- Recommendation: design + prototype from komorebi-bar strip +
  workarea-offset and Seelen grid click/names when scoping 10 (M).
  No mechanism choice here.

## Follow-ups (bounded; S <= day, M few days, L multi-day; scoped estimates, not guarantees)

1. (M) Parity 7: native Meta+drag producer + current Engine drop geometry and fresh
   identity/admission gates. Accept: KDE-reference move/drop and cancellation rows.
2. (L) Parity 8: nonactivating drop-preview overlay based on the same projected
   target; Whim supplies lifecycle prior art. Accept: preview equals committed
   drop; cleanup on cancellation, focus loss, output change and stop.
3. (L) Parity 9: complete/verify existing multi-output routing, disconnect/return,
   cross-monitor movement and mixed-DPI frame readback on the other Windows test system.
   Accept: reproducible topology/DPI matrix; double-move only if needed.
4. (M) Parity 10: agree strip/grid/taskbar presentation, then prototype named
   workspace indicators with separate focused/displayed state. Accept: click
   routing and work-area coexistence; presentation remains a user decision.
5. (S) Focused admission observations: Steam/Firefox/UWP foreground rows during normal use. Accept: observation rows, no code.
6. (S, PARKED, user-authorized only) Shortcut settings probe: user-applied Xbox-mode-off / Game Bar toggles recheck. Accept: pass/fail log or park retained.
7. (M, conditional low priority) Hiding investigation only for measured SW_HIDE
   failures. Private cloak requires a separate authority/architecture decision;
   acceptance includes crash recovery and taskbar restoration, not appearance alone.

## Caveats

- Read-only clones; no live input, no registry/policy writes, no forced-crash
  probes. Cloak flags, Chromium staleness, elevated round-trips, 24H2+ drift
  unknown until live-tested.
- Absence claims are targeted-search results at pinned heads, not proof
  of nonexistence.
- Related: [prior-art catalogue](../prior-art.md), [Windows plan](plan.md),
  [reference outcomes](../../spec/reference-outcomes.md). Source/UX ideas here
  are recommendations, not approved product changes.

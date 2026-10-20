# CLI-controlled live test windows

- **Existing tools?** None of the surveyed tools covers the complete requirement set.
  Reuse windowing libraries and existing ownership checks, not general app automation.
- **Feasible?** Yes: a Rust control harness with a portable window backend and small
  toolkit/platform adapters. Literal arbitrary geometry and every fullscreen mode
  are not portable, especially on Wayland; return explicit capability limits.
- **Recommendation:** winit + softbuffer + CPU-rendered text, one shared interface,
  many windows per process by default, optional separate process/toolkit groups.
  These are research recommendations, not implementation or backlog decisions.

## Existing project assets

- [Windows CLI](../../crates/tiler-windows/src/bin/tiler-test-window.rs) already
  provides `run`, `inspect`, `show`, `minimize`, `restore`, `move`, and `close`.
  `move --to X,Y,W,H` resizes too. The default is a fixed-style 640x480 Win32
  window with class `OmniTilerOwnedTest`, not a state-rendering fixture.
- [Ownership checks](../../crates/tiler-windows/src/test_window.rs) bind peer
  executable, user/session, class, process identity/liveness, and lifetime tag.
  Show/minimize/restore/move require the tag, avoid activation, and check readback;
  inspect still verifies ownership but accepts no supplied tag; close's tag is optional.
  Preserve these semantics, not a new arbitrary-HWND command surface.
- The CLI also has experimental `cloak-preflight`, `cloak`, `uncloak`, and
  `cloak-observe`. Its [cloak spike](../../crates/tiler-windows/src/cloak.rs)
  uses private `IApplicationView::SetCloak`, fenced to Windows x64 build 26300;
  this is not a portable/public DWM implementation or a general ABI guarantee.
- The Windows helper was [accepted on 2026-10-14](../decisions.md) as the standard
  disposable helper. Keep it compatible until replacement parity is established;
  [live Windows testing](../live-windows-testing.md) remains the point of use.
- [macOS probe](../../crates/tiler-macos/src/bin/probe_target.rs) creates a separate
  AppKit process with PID/bundle/nonce handshake and nonce-bearing title. Existing
  fixtures include fixed size, hung/slow response, deferred creation/self-close,
  transient panel/sheet, Space-join, and origin. These are spawn-time/file-triggered
  fixtures, not a general runtime query/set server. Preserve their identity model.
- [KDE proof](integrated-plasma-structural-feasibility/proof-protocol.md) uses
  `xterm -class OmniTilerTestWindow` and `konsole --desktopfile omnitiler-test`,
  admitted by exact `resourceClass`; these really are disposable sentinel windows.
- Missing shared capabilities: dynamic hints/restyling/modes, rendered state and
  description, caller-addressed multi-window sessions, and batch/event protocol.
  This informs the existing [P1 shared-baselines discussion](../backlog.md), not
  a plan or change to that item.

## Prior art: useful pieces, not an off-the-shelf replacement

| Surveyed source | Useful capability | Gap for this request |
|---|---|---|
| [Hyprland child-window](https://github.com/hyprwm/Hyprland/blob/19fb395d/hyprtester/clients/child-window.cpp) | Small real xdg-shell test client | No general runtime remote-control interface found in this fixture |
| [niri visual tests](https://github.com/YaLTeR/niri/blob/ed22699d/niri-visual-tests/README.md) | Real layout/rendering with mock windows | Not live application clients |
| [swaymsg](https://github.com/swaywm/sway/blob/1652c54b/swaymsg/main.c), [komorebic](https://github.com/LGUG2Z/komorebi/blob/e0709f02/komorebic/src/main.rs), [GlazeWM IPC](https://github.com/glzr-io/glazewm/blob/5709ad0a/packages/wm-common/src/ipc.rs), [yabai](https://github.com/koekeishiya/yabai/blob/dd845723/src/message.c), [AeroSpace](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17/Sources/Common/model/clientServer.swift) | CLI/IPC and identity conventions | Control the tiler, not an owned, dynamically restyled fixture |
| [xdotool](https://www.semicomplete.com/projects/xdotool/), [wmctrl](https://manpages.debian.org/testing/wmctrl/wmctrl.1.en.html) | Move/resize/state changes on existing X11 windows | X11-only, no fixture renderer, no project ownership boundary |
| [Weston clients](https://manpages.debian.org/testing/weston/weston.1.en.html), [wlroots tinywl](https://github.com/swaywm/wlroots/tree/master/tinywl) | Protocol demos; tinywl is an example compositor, not a client | Not a cross-platform remote fixture service |
| [GTK inspector/test hooks](https://docs.gtk.org/gtk4/running.html), [Qt examples](https://doc.qt.io/qt-6/qtexamples.html), [winit examples][winit] | Real toolkit clients and window API demonstrations | Not the requested unified CLI/state/batch helper |
| [Appium](https://appium.io/docs/en/latest/), [WinAppDriver](https://github.com/microsoft/WinAppDriver), [AT-SPI](https://github.com/GNOME/at-spi2-core) | Outside-in accessibility/UI automation | Control/observe apps rather than supply the required owned fixture |

- Source searches also covered [cosmic-comp](https://github.com/pop-os/cosmic-comp/tree/3d55cba0)
  and [Whim](https://github.com/dalyIsaac/Whim/tree/1e86b579);
  no equivalent surfaced in the scoped search. Completeness is **UNVERIFIED**.
  Smithay-specific helpers and current wlroots upstream clients remain
  **UNVERIFIED**. This is a surveyed-candidates verdict, not proof none exists.

## Rust library comparison

- All rows target Windows/macOS/Linux, subject to backend support. GTK is most
  useful here as a Linux toolkit witness, not a Win32/AppKit chrome substitute.
- `B/E` below means borderless/display-mode-exclusive fullscreen, not proof of
  game-renderer exclusive presentation. All portable APIs retain host limitations.
- Raw handles are escape hatches, not permission to call platform APIs safely on
  any thread or change toolkit-managed state without reconciling its bookkeeping.

| Candidate | Chrome/backend | Hints and runtime state | Handles / multiple windows | Dependencies and fit |
|---|---|---|---|---|
| **winit 0.30.13** [API][winit] | Win32/AppKit frames; X11 and Wayland; negotiated SSD or sctk-adwaita CSD fallback, not actual GTK/Qt | Min/max, resizable, decorations, move/resize, max/min, visibility, B/E; per-method platform exceptions | raw-window-handle; repeated create in one event loop | Apache-2.0; safe common API, no Qt/GTK dependency. Recommended core |
| **tao** [API][tao] | Win32/AppKit; real GTK3 `ApplicationWindow` on Linux | winit-shaped hints/state plus closable/minimizable/maximizable; Linux fullscreen limited to current monitor | Raw handles and GTK window access; multi-window | Apache-2.0; GTK 0.18 Rust bindings mean native GTK3 dependencies, not GTK 0.18 system libraries. Useful GTK3 alternative, heavier core |
| **SDL3**, sdl3 0.20.0 [API][sdl] | Native platform window frames; not real Qt/GTK clients | Min/max, border toggle, max/min/restore, position/size, hide/show; fullscreen bool plus display-mode selection. Runtime resizable setter **UNVERIFIED** in binding | HasWindowHandle/HasDisplayHandle; multiple windows; Window is !Send/!Sync | MIT binding; SDL3 system or source build. Plausible game-oriented alternative, unnecessary C dependency for basic fixtures |
| **gtk4-rs** [API][gtk] | Actual GTK4, including GTK CSD/headerbar cases | Decorated/resizable, default size, max/min/fullscreen; minimum via widget sizing. No general max-geometry setter in surveyed GtkWindowExt; B/E split not exposed there | Multiple GtkWindows; GDK-specific raw-handle recipe **UNVERIFIED** | GTK4/GLib development libraries; safe Rust binding, separate GLib loop. Best explicit GTK4 witness |
| **cxx-qt** [project][cxxqt] **or qmetaobject** [API][qmeta] | Real Qt through C++/QML, not alternative window styles supplied by winit | Qt window flags, constraints, and states need QML/C++ wrapper exposure; exact Rust coverage **UNVERIFIED** | Multiple Qt windows; raw/native handle bridge wrapper-defined, **UNVERIFIED** | Qt development libraries and C++ build toolchain; cxx-qt is a QObject bridge, not a complete Rust QWidget API. Prefer a tiny Qt fixture over a large generic wrapper |
| **Slint 1.18.1** [Window][slint], [Qt backend][slintqt] | Qt or winit backend; Qt really supplies windowing/style, no GTK backend | Size/position, max/min/fullscreen; layout constraints and `no-frame` in DSL. Exact dynamic constraint/resizable recipe **UNVERIFIED**; no portable B/E split in Window API | raw-window-handle feature; multi-window | Qt backend requires Qt and uses software rendering; winit backend avoids Qt. GPLv3, royalty-free desktop, or commercial licensing; proprietary desktop does not inherently require paid seats [terms][slintlicense] |
| **iced** [API][iced] | winit-based app framework, custom content, not genuine GTK/Qt | Size constraints, resizable/decorations, max/min, move/resize; one fullscreen mode in surveyed API, not B/E | Multi-window actions; exact raw-handle access **UNVERIFIED** | Rust rendering/framework stack, more than text needs. Stable-version API pin still needed; dev docs must not silently become a contract |
| **fltk-rs 1.5.23** [API][fltk] | FLTK content with platform frames; not GTK/Qt | size_range min/max, border toggle, fullscreen, iconize; exact maximize/runtime-resizable recipe **UNVERIFIED** | Multiple windows; native/raw-window-handle access | MIT binding; bundled/system FLTK C++ build and Linux display/graphics dependencies. Valid lightweight toolkit, no fidelity advantage here |
| **Native windows-rs / objc2**, X11/Wayland clients | Exact host class/role and style | Most precise host cases, still cannot bypass protocol restrictions | Direct HWND/NSWindow/X11 IDs/surfaces; native loops support multiple windows | [windows-rs][winrust], [objc2 AppKit][appkit]; most maintenance and reviewed unsafe boundary. Use for gaps, not duplicate the whole portable harness |

- **Toolkit-agnostic Rust plus all native toolkits?** No surveyed drop-in library
  selects authentic Win32/AppKit/GTK/Qt with the entire low-level control matrix.
  winit is OS/window-system agnostic, not a GTK/Qt selector; tao adds GTK3,
  Slint offers Qt/winit but not GTK. Share the wire protocol, not a forced UI loop.
- Separate small GTK/Qt backend executables are simpler than loading both loops
  and native dependencies into one binary. Optional builds preserve portability.
  Actual decoration mode is negotiated: GNOME commonly needs CSD; KDE can supply
  SSD. A Qt client using KDE SSD and a GTK headerbar client test different paths.
- [Raw AppKit handle][appkithandle] contains an **NSView**, not NSWindow. Reach its
  NSWindow on the main thread with correct ownership/lifetime checks. HWND, X11
  window IDs and Wayland wl_surface handles likewise do not erase host restrictions.

## Platform-specific cases and boundaries

| Platform | Useful additional cases | Common library versus host work |
|---|---|---|
| Windows | Hide/show, cloak/uncloak, owned dialogs, tool/noactivate styles, topmost, taskbar presence | winit exposes visibility, topmost, creation-time owner and some taskbar/style options [extensions][wwin]. Arbitrary WS_EX changes, tool/noactivate cases, DWM cloak, and taskbar-tab operations need host APIs. [ShowWindow][show] activation differs by flag; preserve non-activation where required |
| macOS | Native fullscreen Space, simple fullscreen without new Space, plain borderless window, NSPanel/sheet, collection behaviors, app hide | winit exposes simple-fullscreen, shadow, and tabbing [extensions][wmac]. NSPanel identity, detailed collectionBehavior and app-hide need AppKit. App hide affects all windows in the app, unlike per-window order-out [AppKit][appkit] |
| X11 | WM_NORMAL_HINTS min/max/fixed, EWMH states/type, transient-for dialog, override-redirect unmanaged fixture | winit exposes basic hints/state plus creation-time WM_CLASS, window type and override-redirect [extensions][wx11]. Detailed hints/transient relationships need X11 work. ICCCM size hints and WM_CHANGE_STATE minimize differ from EWMH state requests; clients must not set HIDDEN directly [ICCCM][icccm], [EWMH][ewmh] |
| Wayland | xdg-toplevel constraints/maximize/fullscreen/minimize, app_id, decoration negotiation; optional layer-shell fixture | Core protocol has no absolute/global positioning, generic unminimize request, or minimized-state feedback. winit exposes local size/scale and common requests; position/visibility APIs are unsupported and exclusive fullscreen no-ops [API][winit]. Layer-shell is a separate role, not an xdg-toplevel toggle [protocol][layer] |

- [DWMWA_CLOAK][dwm] sets app cloaking; DWMWA_CLOAKED reads app/shell/inherited
  cloak flags. Hiding and cloaking are distinct. Neither alone proves Steam's
  exact soft-close lifecycle; compare events, visibility and taskbar behavior.
- Preserve **bordered windowed**, **undecorated windowed**, **borderless
  fullscreen**, and **display-mode exclusive** as distinct requested modes.
  On macOS winit borderless fullscreen normally creates a native fullscreen Space;
  simple fullscreen does not. A software buffer plus display-mode change does
  not certify a game's DXGI swapchain/exclusive presentation semantics.
- Wayland [xdg-shell][xdg] size limits are hints/configure negotiation; local
  size/scale are observable, global position is not. Interactive move/resize
  requires a valid seat/input serial; winit has drag APIs, but background JSON
  commands cannot manufacture a valid user action. Title/border drags should use
  toolkit-standard behavior, not custom input hooks.
- [xdg-decoration][deco] negotiates CSD/SSD; it does not guarantee the requested
  mode on every compositor. Likewise min/max and fixed-size requests may be
  clamped/ignored. `resizable=false` generally controls user resizing, not all
  programmatic size changes; report actual results, not the requested value alone.

## Recommended interface and process model

- A **shared interface is useful**: stable caller IDs, batch orchestration and
  one vocabulary. It need not imply one PID per window or identical toolkit behavior.
- Default: one serving process/event loop owns many windows. The same CLI routes
  IDs to windows; use explicit process groups or one child per window when PID,
  hung-app behavior, or toolkit identity is part of the case. Add a thin broker
  for those groups, not a mandatory separate broker for the common single-process case.
- Multiple windows in one process exercise same-app behavior; separate children
  exercise cross-app behavior. Wayland app_id and X11 WM_CLASS can differ per
  window (winit supplies them at creation); a new process is not required just
  for those tags. macOS distinct bundle identities additionally need bundle
  packaging, not merely different PIDs.
- Preserve Windows sentinel/token and macOS nonce binding; Linux class/app_id/title
  tags retain the sentinel convention. Keep caller ID, PID, backend and native
  identity in snapshots. Never accept arbitrary foreign HWND/PID targets.
- Transport: Unix-domain socket on Linux/macOS; Windows named pipe with an owner
  security descriptor. Owner-restricted endpoint plus per-run nonce; no public
  TCP listener needed. Windows AF_UNIX is an alternative, not inherently unsafe,
  but offers no established advantage here over native pipes.
- JSON lines, version and request ID; `spawn`, `set`, `get`, `list`, `close`,
  `batch`, `subscribe`. Spawn takes caller IDs/options; set/get/close accept ID
  lists. Close destroys only the selected owned windows; publish closure events.
  List enumerates owned IDs; subscriptions publish state/creation/closure/errors.
- Per-window results, ordered batch dispatch, **not an atomic host transaction**.
  Distinguish accepted request, observed state, pending settlement, unsupported,
  unknown, and error; a successful setter is not proof the WM accepted it.
- Recommend client-area physical pixels for size/hints; report scale and logical
  size too. Position is an explicitly named outer/global coordinate where supported;
  outer/client rectangles must not share an ambiguous `rect` field.
- IO hands commands to the UI/main thread and receives events back; redraw only
  on state, description, resize/expose, or host notification. No state polling loop
  where an event exists. Render all exposed variables, including unknown values,
  backend/PID/ID, requested/observed hints/mode/visibility, scale and errors.
- Description is independently settable, not overloaded into the identity title.
  [softbuffer][softbuffer] supplies pixels, not text; [fontdue][fontdue] supplies CPU font
  rasterization/layout. A bundled font avoids platform font differences. egui is
  an immediate-mode convenience alternative with extra rendering dependencies;
  it neither provides native toolkit chrome nor is necessary for a text panel.

```json
{"v":1,"request_id":"r1","op":"spawn","windows":[{"id":"a","size_px":[640,480],"description":"resizable"},{"id":"b","resizable":false}]}
{"v":1,"request_id":"r2","op":"set","ids":["a","b"],"patch":{"description":"two-window case","decorations":false}}
{"v":1,"request_id":"r3","op":"batch","items":[{"op":"spawn","windows":[{"id":"c"}]},{"op":"set","ids":["a"],"patch":{"mode":"borderless_fullscreen"}},{"op":"close","ids":["b","c"]}]}
```

## Scope choices, risks, and discussion order

- Proposed home: a development-only workspace member `crates/tiler-test-window`
  containing executable/control types, portable backend and small host modules;
  not tiler-core or the production tiler protocol. Split toolkit executables only
  as needed. Keep existing binaries/contracts while reusing safe parsing and
  ownership concepts; do not blindly transplant their single-window loops.
  The current [Windows FFI loop](../../crates/tiler-windows/src/ffi/test_window.rs)
  quits on window destruction, and current validation refuses owned/parented windows.
  Multi-window and intentional-owned-dialog fixtures require explicit adaptation.
- Suggested scope progression, not an implementation plan: common window/state
  protocol and text first; raw host cases second; authentic GTK/Qt fixtures next.
  All builds remain Rust-led; Qt may require a small QML/C++ boundary.
- Safe library APIs cover most work. Native mutations and pointer/lifetime
  conversions need minimal reviewed unsafe modules, not merely unsafe handle casts.
  Qt/GTK/SDL/FLTK add system/build weight; none has explicit development-library
  declarations in the current [devenv.nix](../../devenv.nix). winit/softbuffer also
  need a Linux X11/Wayland/xkbcommon dependency audit for selected Cargo features.
  Record required Linux libraries in `devenv.nix`; Windows/macOS prerequisites
  follow their dev docs and `mise.toml`.
  No dependency files need changing for this feasibility finding.
- Main risks: Wayland capability gaps; toolkit/compositor hint and decoration
  differences; app-vs-window identity; raw calls desynchronizing library state;
  real exclusive rendering versus mode changes; preserving existing safety checks.
  Optional binding details marked **UNVERIFIED** require API checks before selection.
- Future discriminating observations: set min=max then request a conflicting size;
  toggle decorations on one of two windows; hide then reshow the same identity;
  compare same-PID and separate-PID windows. Host-specific outcomes remain TBD for
  later testing and [reference outcomes](../spec/reference-outcomes.md), not guessed here.

| Ordered user decision | Recommendation |
|---|---|
| 1. Initial scope versus complete toolkit suite immediately | Start with common geometry/state; retain GTK/Qt as explicit follow-on fidelity coverage |
| 2. Shared interface and process granularity | Shared IDs/batches; many windows per process by default, optional explicit process groups |
| 3. Generic Rust library versus genuine GTK/Qt clients | winit core plus small separate GTK4 and Qt fixtures; do not simulate toolkit chrome |
| 4. Meaning of exclusive fullscreen | Cover display-mode/fullscreen-state behavior first; treat true game-renderer parity as separate scope |
| 5. Existing Windows/macOS helper compatibility | Extend/reuse ownership semantics and preserve binaries until parity, not immediate replacement |
| 6. Local transport | Unix sockets plus Windows named pipes; JSON lines and event subscriptions |

[winit]: https://docs.rs/winit/0.30.13/winit/window/struct.Window.html
[wwin]: https://docs.rs/winit/0.30.13/winit/platform/windows/trait.WindowAttributesExtWindows.html
[wmac]: https://docs.rs/winit/0.30.13/winit/platform/macos/trait.WindowExtMacOS.html
[wx11]: https://docs.rs/winit/0.30.13/winit/platform/x11/trait.WindowAttributesExtX11.html
[tao]: https://docs.rs/tao/0.37.1/tao/window/struct.Window.html
[sdl]: https://docs.rs/sdl3/0.20.0/sdl3/video/struct.Window.html
[gtk]: https://gtk-rs.org/gtk4-rs/git/docs/gtk4/prelude/trait.GtkWindowExt.html
[cxxqt]: https://github.com/KDAB/cxx-qt
[qmeta]: https://docs.rs/qmetaobject/latest/qmetaobject/
[slint]: https://docs.rs/slint/1.18.1/slint/struct.Window.html
[slintqt]: https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_qt/
[slintlicense]: https://slint.dev/pricing
[iced]: https://docs.rs/iced/latest/iced/window/index.html
[fltk]: https://docs.rs/fltk/1.5.23/fltk/prelude/trait.WindowExt.html
[winrust]: https://learn.microsoft.com/en-us/windows/dev-environment/rust/rust-for-windows
[appkit]: https://docs.rs/objc2-app-kit/latest/objc2_app_kit/struct.NSWindow.html
[appkithandle]: https://docs.rs/raw-window-handle/latest/raw_window_handle/struct.AppKitWindowHandle.html
[show]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow
[dwm]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
[icccm]: https://www.x.org/releases/X11R7.6/doc/xorg-docs/specs/ICCCM/icccm.html
[ewmh]: https://specifications.freedesktop.org/wm-spec/wm-spec-latest.html
[xdg]: https://wayland.app/protocols/xdg-shell
[deco]: https://wayland.app/protocols/xdg-decoration-unstable-v1
[layer]: https://wayland.app/protocols/wlr-layer-shell-unstable-v1
[fontdue]: https://docs.rs/fontdue/latest/fontdue/
[softbuffer]: https://docs.rs/softbuffer/latest/softbuffer/

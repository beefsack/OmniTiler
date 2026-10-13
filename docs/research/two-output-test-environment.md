# Two-Output Test Environment (Research)

Status: research only. No runs were performed on any route, so every
runtime verdict below is RUNTIME-PENDING until exercised. Route selection
and its dependencies are still open.
Inventory: [Proposal B](release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2) ([Pending live checks](../backlog.md#pending-live-checks)).
Legend: SOURCE = pinned-source fact with a link in sec 4-6 or 8;
RUNTIME-PENDING = read from source, not yet exercised.

## 1. Recommendation

- Full Plasma VM, 2 connected virtio heads normally; 3 heads for check #2.
- Local GTK UI; no VNC/network listeners.
- Guest dependencies declared in the guest profile through the existing
  NixOS script/effect/settings module plus planner/tray integration (sec 8).
- Relative cost: the VM costs more disk/build/time than nested or host
  virtual-monitor routes, but it is the only listed route with full-session
  fidelity (sec 3).

## 2. Routes (nested Wayland and --virtual/headless kept separate)

| Route | Relative cost | Verdict |
| --- | --- | --- |
| Nested Wayland (`kwin_wayland`, WaylandBackend outputs) | Cheapest; shares host GPU/RAM, no guest disk | Suitable for source-level layout/geometry acceptance; full static use is useful but cannot finish the full gate (no session services, input/scale fidelity gaps, `--no-global-shortcuts`) |
| `--virtual` + headless Plasma | Low-medium; desktop plus remote-view/input processes | Needs session services plus remote view/input; `--virtual` alone is not a session (sec 4) |
| Full Plasma VM (GTK) | Higher; guest RAM, OS/Plasma store closure and sparse disk | Recommended (sec 1); isolated guest configuration and outputs, host GPU render access for virgl |
| Host krfb virtual monitor | Lower; stream/viewer processes, no guest image | Needs host-topology exception; not recommended under current preservation boundary; no host listener command (sec 6) |
| DRM dummy plug | Medium, hardware-gated | Needs a physical/hardware path; not portable |
| RDP into session | Medium | Needs stream + view + input plumbing; not turnkey |
| Weston nested | Low | Does not run the KWin effect/adapter under test |
| KWin test harness | Low | No full Plasma session |

## 3. Route coverage matrix

| Need | Nested Wayland | --virtual + headless Plasma | Full VM | Host krfb |
| --- | --- | --- | --- | --- |
| Static 2 heads; 3 for #2 | SOURCE: static geometry at startup | SOURCE-capable once session plumbing lands (RUNTIME-PENDING) | SOURCE: boots 2 normally, 3 for #2 (RUNTIME-PENDING) | SOURCE-capable geometry; barred by boundary (sec 2) |
| Hotplug (#51; #14 legs) | Not established: screencast path has a registration gap (source inference, sec 4) | SOURCE: registered screencast create/remove path; helper, PipeWire and identity verification RUNTIME-PENDING | RUNTIME-PENDING: GTK close is not unplug; no QMP disconnect-head; guest-krfb synthetic path unproven for cable/EDID | SOURCE: stream-lifetime create/remove; return affinity RUNTIME-PENDING; needs boundary exception |
| Mode/scale mixed + dynamic change (#34/N4) | SOURCE: parent resize changes mode; child scale follows parent, not kscreen | SOURCE: mode/custom-mode/scale changes; private kscreen plumbing RUNTIME-PENDING | RUNTIME-PENDING: guest kscreen changes mode + scale dynamically; virgl not native-GPU fidelity | SOURCE: DRM virtual output supports mode/custom-mode/scale; needs boundary exception |
| Arrangement (position/transform) | SOURCE: position/transform honored | RUNTIME-PENDING | RUNTIME-PENDING guest kscreen arrangement | Boundary-barred |
| Full stack + physical input/shortcuts (#46) | Cannot finish: `--no-global-shortcuts`, no session services | RUNTIME-PENDING only with services + view/input | RUNTIME-PENDING capable: real shortcuts + session | Boundary-barred |
| Sleep/hardware/game (#30/N3, #1/D28 legs) | Fidelity gap: no host sleep, no game stack | Fidelity gap | Fidelity gap (not impossible): the host can suspend with the VM alive, but that skips the host KWin/real-DRM/game stack; a game runs in the guest yet virgl/timing cannot prove native cost/non-interference; per-gate adequacy is a user call (sec 7) | Boundary-barred |

## 4. KWin 6.7.4 pinned findings

Pin `8438567a`, full `8438567a741826da8b7536a8b10eb3af8fc8820d`.
Base `KW`: `https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d`.

| Claim | Pinned file + symbols |
| --- | --- |
| Startup outputs | [src/main_wayland.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/main_wayland.cpp): `outputCount` covers nested + virtual; one size + scale applied to all startup outputs |
| Nested output changes | [src/backends/wayland/wayland_output.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/wayland/wayland_output.cpp): `WaylandOutput::applyChanges` honors enable/transform/position, ignores scale, `desiredMode` does not update `currentMode`; `applyConfigure` updates `currentMode` + scale from the parent fractional-scale path; resize drives modes |
| Output-window close | [src/backends/wayland/wayland_output.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/wayland/wayland_output.cpp): `closeRequested` connects to application quit, rather than removing one output |
| Virtual outputs, no local UI | [src/backends/virtual/virtual_output.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/virtual/virtual_output.cpp): `VirtualOutput::applyChanges` honors mode/custom modes/scale/position/enabled; no visible local UI/input |
| Nested renderer | [src/backends/wayland/wayland_backend.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/wayland/wayland_backend.cpp): OpenGL iff dmabuf `renderDevice`, else QPainter |
| Nested hotplug gap (source inference, not a confirmed bug) | [src/backends/wayland/wayland_backend.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/wayland/wayland_backend.cpp): `WaylandBackend::createVirtualOutput` returns `createOutput` with no `m_outputs` append and no `outputAdded` (unlike `createOutputs`); `removeVirtualOutput` requires `m_outputs.removeAll`. No fix and no behavior change here; runtime untested |
| VirtualBackend dynamic outputs (verified by fetch) | [src/backends/virtual/virtual_backend.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/virtual/virtual_backend.cpp): `createVirtualOutput` -> `addOutput` -> `createOutput` (`m_outputs.append` + `outputAdded`) + `outputsQueried`; `removeVirtualOutput` -> `removeOutput` (`outputRemoved` + `outputsQueried` + `unref`); also `setVirtualOutputs` |
| DrmBackend dynamic virtual outputs (verified by fetch) | [src/backends/drm/drm_backend.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/backends/drm/drm_backend.cpp): `createVirtualOutput` (new `DrmVirtualOutput`, `m_virtualOutputs.push_back`, `addOutput`, `outputsQueried`); `removeVirtualOutput` (`removeOne`, `removeOutput`, `outputsQueried`, `unref`); `applyOutputChanges` applies config to `m_virtualOutputs` |
| Screencast virtual-output path (verified by fetch) | [src/plugins/screencast/screencastmanager.cpp](https://github.com/KDE/kwin/blob/8438567a741826da8b7536a8b10eb3af8fc8820d/src/plugins/screencast/screencastmanager.cpp): `ScreencastManager::streamVirtualOutput` calls backend `createVirtualOutput`, streams it, removes on stream `finished`; gated on OpenGL compositing (`isSupportedCompositingType`), PipeWire via `ScreenCastStream`. It delegates to the backend path, so the nested registration inference still applies there; runtime untested |
| kscreen-doctor | [src/doctor/doctor.cpp](https://invent.kde.org/plasma/libkscreen/-/blob/v6.7.4/src/doctor/doctor.cpp) (libkscreen v6.7.4): requests the KDE output-management protocol; nested use needs a private bus + backend helpers, exact plumbing untested. Enable is not disconnect |
| Nested full-stack launch | Script `loadScript`/`Scripting` + planner `com.omnitiler.Planner` on a private bus, effect built for the exact ABI with a manual plugin path before start, KCM in the same env/private config; compositor-only launch gives no Plasma watcher/systemd/login. [Isolation/lifecycle contract](../live-kwin-testing.md#nested-kwin-isolation), [dev procedure](../dev-loop.md#linux-kde), [effect discovery](../installation.md#kwin-upgrades-policy-and-limitation). Required `--no-global-shortcuts` disables child global shortcuts; `invokeShortcut` cannot prove physical delivery |
| Headless Plasma | Session services plus remote view/input required; `--virtual` alone is not enough |

- Keep fresh private `HOME`, all `XDG_*` homes/runtime and `KDEHOME`, a
  user-owned `0700` runtime, private `dbus-run-session`, and an absolute
  parent socket through `--wayland-display`. The proven nested shape retains
  `--socket nested-kwin-spike --width 640 --height 480 --no-global-shortcuts
  --no-kactivities --no-lockscreen`, adding `--output-count 2`; never
  `--windowed`. Host `kwinrc` hash/mtime are read-only pre/post diagnostics.

## 5. QEMU 11.0.3 pinned findings

Base `QE`: `https://github.com/qemu/qemu/blob/v11.0.3`.
Packaging: [package.nix](https://raw.githubusercontent.com/NixOS/nixpkgs/54ba4bcec4043e72a4006d825e0d7aff5562008f/pkgs/by-name/qe/qemu/package.nix) (QEMU 11.0.3 at nixpkgs pin `54ba4bcec4043e72a4006d825e0d7aff5562008f`).

| Claim | Pinned file + symbols |
| --- | --- |
| Geometry | [hw/display/virtio-gpu-base.c](https://github.com/qemu/qemu/blob/v11.0.3/hw/display/virtio-gpu-base.c): `max_outputs` capacity; `enabled_output_bitmask = 1` initially; `outputs[i]` with BOTH xres+yres sets bit + geometry; [qapi/virtio.json](https://github.com/qemu/qemu/blob/v11.0.3/qapi/virtio.json): xres/yres since 11.0 |
| Disable event | Same file: zero width/height (`virtio_gpu_ui_info`) disables and sends `VIRTIO_GPU_EVENT_DISPLAY` |
| GTK close/resize | [ui/gtk.c](https://github.com/qemu/qemu/blob/v11.0.3/ui/gtk.c): `gd_tab_window_close` reparents the tab with no ui_info zero (close is not unplug); `gd_set_ui_size` -> `ui_info` supports mode updates; guest kscreen owns scale/arrangement |
| QMP | [qapi/ui.json](https://github.com/qemu/qemu/blob/v11.0.3/qapi/ui.json): `query-display-options` is backend-only; `display-update` addresses only; no disconnect-head operation |
| GL/suspend | virgl enables the local GTK GL path, not real-GPU timing/passthrough; pausing QEMU is not suspend |

## 6. krfb v26.08.2 (no host command)

- [krfb/main-virtualmonitor.cpp](https://github.com/KDE/krfb/blob/v26.08.2/krfb/main-virtualmonitor.cpp): `--resolution` / `--name` / `--scale` / `--port` / `--password`, no `--address` option.
- [krfb/rfbserver.cpp](https://github.com/KDE/krfb/blob/v26.08.2/krfb/rfbserver.cpp): default listen address `0.0.0.0`. No safe loopback-only host command exists without a new reviewed bind/firewall solution; a listener password is not isolation.
- The PipeWire framebuffer path ([framebuffers/](https://github.com/KDE/krfb/tree/v26.08.2/framebuffers)) drives `stream_virtual_output` against the KWin lifetime-bound path (sec 4 screencast row).
- Guest-side krfb virtual output can synthetically remove/recreate an output inside the guest (needs guest tools + viewer + OpenGL/PipeWire + identity/arrival proof); it does not prove a cable/EDID link.

## 7. Check mapping (exact triage wording)

`#31`/`#33` are base single-output work, not separate multi-output gates.
`#52` is K (known-issue-allowed), not M. D06/D13 are batch-1 extras outside
Proposal B and do not recategorize `#52`.

| Checks | What the route must show |
| --- | --- |
| #1 Reference comparison G-05/G-06/G-37/G-D2 + D28 fullscreen carry (M) | Cross-output carry including a real game; game stays native. A game runs in the guest but cannot prove native cost/non-interference; adequacy per leg is a user call (sec 3 sleep row) |
| #2 Two-candidate output move/send/migration (M) | Only check needing 3 outputs: wide source + two destinations |
| #14 R-WS-08/11 item 1 remainder (M): scope, hotplug, presets | Scope/presets are static-capable; hotplug legs stay RUNTIME-PENDING (sec 3) |
| #15 R-WS-01/14 item 2 (M) | Send-ring follow/stay/wrap/fill across joined heads |
| #34 Reliability gates gaming + work-area (M); N4 scaling + overlays (M) | DYNAMICALLY change mode + scale with maximized/fullscreen/fixed-float windows; overlays retained, no fullscreen geometry writes |
| #42 Q4 migration; #43 item 5 R-MOV-08/R-OUT-04; #44 K1 visual remap; #46 directional + shortcut + convergence; #47 cross-output drops; #49 reconciliation | Static multi-output core across joined heads; #46 needs real shortcuts (VM only, sec 3) |
| #48 KWin restart surviving Planner (M) | KWin restarts while the Planner survives (fresh-adopt, tray recovery). A session restart kills the planner and does NOT satisfy this check |
| #50 process-loss + sleep multi (M); #30 single; N3 sleep + gaming (M) | Guest suspend is guest-only; host-suspend-with-VM-alive is a fidelity gap, not host proof; per-gate adequacy is a user call |
| #51 output hotplug displacement/return (M) | RUNTIME-PENDING on every route (sec 3) |
| D06 newcomer focused output; D13 local-wrap/output crossing | Also need two outputs; verification-only batch-1 extras |

## 8. Setup prerequisites (nothing added; proposed names marked *)

- [nixos-module.nix](../../nixos-module.nix) installs script + effect + settings and a system `xdg/kwinrc` enablement key through `programs.omnitiler.enable = true`; import the existing `nixosModules.default` from [flake.nix](../../flake.nix). It does not install or start the planner/tray. Add their package and guest-local services explicitly, or use [home-manager-module.nix](../../home-manager-module.nix) (`programs.omnitiler.planner.enable`, default true, and `programs.omnitiler.tray.enable`) inside the guest. The latter requires a Home Manager input, which this flake does not currently carry. Build the effect against the guest's exact KWin package, not a copied host plugin.
- Guest prerequisites: full Plasma Wayland/login services, libkscreen (`kscreen-doctor`), ordinary Wayland test clients, and the planner/tray session integration above. Optional synthetic hotplug adds guest `kdePackages.krfb`, PipeWire and a guest VNC viewer such as `tigervnc`; it must stay contained in a guest with no network forwarding/listeners exposed to the host.
- The flake has no `nixosConfigurations` and no `system.build.vm` / `vm-*` package attrs today.
- VM mechanics live in [qemu-vm.nix](https://github.com/NixOS/nixpkgs/blob/54ba4bcec4043e72a4006d825e0d7aff5562008f/nixos/modules/virtualisation/qemu-vm.nix): `virtualisation.qemu.options` carries extra QEMU args, `system.build.vm` builds the run script. Proposed and not present: a `vm-two-output` package attr exposing that build, and a `just/linux.just` recipe (existing recipes in [linux.just](../../just/linux.just); no VM recipe exists).
- New host tool: QEMU, absent from [devenv.nix](../../devenv.nix). Proposed addition: `pkgs.qemu_kvm` (includes GTK and `qemu-img`); restart the development session after adding it. The `system.build.vm` wrapper can reference its pinned executable directly, but still introduces that runtime dependency. No separate viewer is needed for local GTK. The host-krfb alternative instead requires proposing `kdePackages.krfb`, `tigervnc` and `kdePackages.libkscreen`. No dependency changes are made here.

## 9. Illustrative device fragment (display-only, not runnable)

Replaces (not duplicates) the default VGA. No disk/kernel/machine/session;
not a launch command.

```text
-vga none -device '{"driver":"virtio-vga-gl","id":"vga","max_outputs":2,"outputs":[{"name":"OT-L","xres":1280,"yres":800},{"name":"OT-R","xres":1280,"yres":800}]}' -display gtk,gl=on,show-tabs=on
```

Check #2 uses the same shape with `"max_outputs":3` and three `outputs` entries.

## 10. Open gaps

- No runs on any route; every RUNTIME-PENDING cell needs proof before claims.
- Adequacy is decided per gate by the user: synthetic hotplug and guest suspend may or may not satisfy #50/#51/N3, and game-in-guest may or may not satisfy #1/#34 legs. Nothing here reclassifies gates to hardware-only.
- After route selection: guest profile, proposed `vm-two-output` package, and proposed `just/linux.just` recipe via the `system.build.vm` wrapper.

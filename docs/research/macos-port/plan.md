# macOS port: tier-2 implementation plan

Status: Phase 0 in progress, 2026-10-14. Tier 2 was selected 2026-10-03;
language/threading/native-Spaces/Meta/signing/floor decisions are now adopted
in the table below. Read-only display/Space/current queries have live evidence;
AX writes, input delivery, signing, and Space mutations remain unverified.
The [active record](../../changes/macos-phase-0.md) and
[setup runbook](../../macos-dev-environment.md) carry the current evidence.
Upstream source text is not runtime proof,
and a dispatched setter is not proof of an observed effect.
Labels used below: FACT (read from pinned source),
CLAIM (upstream text asserts, not verified here), PROPOSAL (our
inference/plan, not source), UNKNOWN (not established).

## Selected policy (user-approved) vs proposals

- SELECTED (user 2026-10-03, recorded in
  [decisions](../../decisions.md#scope-and-platform-goals)): default to tier 2, public
  plus private APIs with SIP left enabled (AeroSpace / yabai-without-SA
  class). No Dock injection, no reduced SIP. Re-evaluate deeper tiers only
  if tier 2 cannot solve a problem well.
- SELECTED: lower-level lower-jank yabai-style FIRST; an optional
  higher-level public-API alternative (AeroSpace-style offscreen parking
  class) may be evaluated later as a configurable alternative. No dual
  backend now. Native Spaces first, yabai primary reference; parking is
  deferred to a potential later user setting, not an initial fallback.
- Tier-2 scope is public AX plus scoped private joins/reads/writes that
  appear in surveyed source without Dock injection: `_AXUIElementGetWindow`
  identity join plus SLPS focus and SLS query/order/Space-move helpers.
  Each further private symbol needs its own justification; the prior
  public-only default is stale and replaced.
- PROPOSAL (adapter design and milestones below): one
  per-login signed Rust app in the user's graphical session; retained
  `tiler-core` Engine for layout policy; a Mac adapter owning AX
  observation/actuation, focus readback, visuals, menu/settings, and
  login/update lifecycle. No KWin D-Bus transplant, no universal IPC hop.
- SELECTED (2026-10-14): all Rust; dependency selection and unsafe exceptions
  follow [Unsafe Code](../../principles.md#unsafe-code). Main thread
  AppKit/UI, dedicated input, per-app AX, engine-owned state via channels.
  `tiler-protocol` only if external IPC is needed remains a proposal.
- The higher-level route and any injection/reduced-SIP route remain
  unselected alternatives, not release contracts.

## What Windows delivered (reuse intent, do not transplant mechanics)

- Reuse retained Engine layout/focus/move, minimum-aware projection, visual
  intent and the shared stale-destination-focus send fix. Skip redundant
  writes; measure action latency and native readback, not dispatch time.
- Windows now delivers select/send-and-follow/trailing-empty workspaces,
  independent restoration plus a same-executable watcher, active border,
  underlay, maximise/fullscreen, float/sticky float. Mac phases target those
  behaviors, then drag/drop preview, output support and settings parity.
- Win32 hooks, `SW_HIDE`, `SPI_SETWINARRANGING` and `AttachThreadInput`
  stay in the Windows adapter. Parking changes geometry, so the Windows
  visibility-only ledger cannot be copied unchanged; a Mac restore preimage
  must carry geometry.

Evidence: [workspaces](../../changes/archive/windows-managed-workspaces.md),
[latency](../../changes/archive/windows-workspace-action-latency.md),
[minimum sizes](../../changes/archive/windows-minimum-size-hints.md),
[send fix](../../changes/archive/windows-send-split-axis.md) and the Windows
border/underlay/maximise/fullscreen/float/sticky archive records. Shipped
Windows slices still have user-owned physical and multi-output checks;
their delivery is not evidence of macOS capability.

## Historical source-evidence pins (2026-10-04)

Source links below are canonical GitHub blob URLs pinned to the checkout
SHAs in the table. Dates are committer dates from `git log -1 --format=%cs`.
These preserve the original research evidence; the newer
[source-availability snapshot](prior-art.md#source-checkout-snapshot-2026-10-14)
does not advance these pins or their line citations.

| Repo | HEAD SHA | Date | Read scope |
| --- | --- | --- | --- |
| yabai | `dd845723416f5fe92af49fad5ebab00369e07edd` | 2026-06-14 | `src/sa.h`, `src/sa.m`, `src/window.c`, `src/window_manager.c`, `src/space_manager.c`, `src/display_manager.c`, `src/event_loop.c`, `src/message.c`, `src/workspace.m`, `src/yabai.c`, `src/view.c`, `src/misc/extern.h`, `CHANGELOG.md` |
| AeroSpace | `74a1bf17e82d70e0a21945ba04bb4f590bf19f83` | 2026-10-01 | `Sources/PrivateApi/include/private.h`, `Sources/AppBundle/tree/MacApp.swift`, `Sources/AppBundle/tree/MacWindow.swift`, `Sources/AppBundle/layout/refresh.swift`, `Sources/AppBundle/util/accessibility.swift`, `README.md`, `Package.swift` |
| Amethyst | `6508ee2cacf8f9b357e1a3249a8f28ba5c94db2a` | 2026-08-19 | `Amethyst/Model/Space.swift`, `Amethyst/Managers/HotKeyRegistrar.swift`, `Amethyst/Events/HotKeyManager.swift`, `Amethyst.xcodeproj/project.pbxproj` |
| glazewm (macOS backend) | `5709ad0a3c7c386bbc3e38166a865ffc12937515` | 2026-04-09 | `packages/wm-platform/src/platform_impl/macos/{ffi,native_window,keyboard_hook,application_observer,display}.rs`, `packages/wm-platform/src/dispatcher.rs` |
| rift | `41f1c051f21f4bb69b3faf506c5bdc8f92100f62` | 2026-10-02 | `src/sys/{skylight,cgs_window,window_server,observer,event_tap,carbon,accessibility,screen}.rs`, `src/actor/{spaces,wm_controller}.rs`, `src/ui/{drag_preview,stack_line,mission_control}.rs`, `src/layout_engine/engine/persistence/*` |
| komorebi-for-mac | `bdf10452f10ebf15ec1978cb05d0aac79ef5611f` | 2026-05-04 | `komorebi/src/{window,workspace,skylight,reaper,main,accessibility/mod,accessibility/private,border_manager/border,border_manager/ns_window,input_event_listener,core_graphics/mod,macos_api}.rs` |
| glide | `2837add2ce3911743e869ed43da59014714b7e59` | 2026-09-29 | `src/sys/{event,observer,window_server,screen}.rs`, `src/actor/{wm_controller,space_manager}.rs`, `src/ui/permission_flow.rs` |
| paneru | `b1b6abbd3f1a4be138152b6f0389c9ff1b27a269` | 2026-10-02 | `src/manager/{skylight,windows,app,manager}.rs`, `src/platform/input.rs`, `src/{overlay,accessibility_prompt}.rs`, `src/ecs/{focus,layout,display,restore}.rs`, `src/config.rs`, `Cargo.toml` |
| hammerspoon | `23e387e2805a9890066366e0ac96c71b27f0cfd5` | 2026-07-08 | `Hammerspoon/HSuicore.m`, `extensions/{hotkey/libhotkey,spaces/libspaces,spaces/spaces.lua,window/libwindow,eventtap/libeventtap,axuielement/common}.m` |

## Capability evidence (tier-2 default)

### Phase 0 local evidence (2026-10-14)

The [active change](../../changes/macos-phase-0.md) records the partial
milestone and exact continuation. Read-only SLS enumeration returned a display,
its Space list and matching current Space. Secure Input reported inactive.
AX app enumeration worked, but window reads failed with trust false; no AX
join/observer/frame/focus/timeout effect has been verified. Persistent untrusted
self-signed signing now passes strict verification with an identical bundle ID
and designated requirement across two clean rebuilds; Accessibility trust is
false and grant survival remains unverified. Carbon delivery and tap
suppression/recovery are unverified. Owned window membership is proven through
the Child holder; direct Space move/restore is implemented but untested, while
switch/create/destroy remain unsupported. No API failure is
attributed to SIP from these results. Native/portable Cargo build/test/strict
clippy and formatting pass; configured Linux/Windows CI results are pending.
The unsafe module is isolated but still needs public-binding minimization.

Initial workspace probes target native Spaces. Lower-level order-out remains
research; parking is deferred to a potential later setting.
Source shape is not acceptance proof; each needs the managed-workspace
spike on owned windows first.

### 1. Enumeration, AX observers, CGWindowList, SkyLightCGS queries

- FACT: per-app `AXObserverCreate` plus run-loop source is the shared
  observation substrate. glazewm
  [application_observer.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/application_observer.rs#L68-L84)
  creates via `AXObserver::create` and reports null as an error; glide
  [observer.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/observer.rs#L50-L69) builds with
  `AXObserver::create` and installs the run-loop source in
  [observer.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/observer.rs#L75-L88), with
  [add_notification](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/observer.rs#L104-L117); rift
  [observer.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/observer.rs#L52-L80) mirrors the
  create path and
  [add_notification](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/observer.rs#L154-L182);
  AeroSpace [accessibility.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/util/accessibility.swift#L372-L377)
  wraps `AXObserverCreate`; hammerspoon Carbon hotkeys and event taps are
  separate (see section 6).
- FACT: `CGWindowListCopyWindowInfo` supplements AX with bounds/PID/order
  metadata (not pixel capture). rift
  [window_server.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/window_server.rs#L497-L517)
  queries with `OptionOnScreenOnly | ExcludeDesktopElements`, with a TODO
  noting `CGWindowListCopyWindowInfo` "does not appear to order windows
  properly" (supports the refuse-ambiguous-joins rule); glide
  [window_server.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/window_server.rs#L124-L126)
  expects a non-null array of window-info dicts; paneru reads the list in
  [manager.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager.rs#L586-L586).
  CLAIM (AeroSpace comment, not used code): layer-0 filter sketch in
  [private.h](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/PrivateApi/include/private.h#L6-L20).
- FACT: private SkyLight CGS query surface appears in surveyed source with
  no Dock-injection code on those paths (absence is not SIP-on proof;
   runtime permission/SIP behavior is UNKNOWN until the spike): rift
  `SLSWindowQueryWindows` / `SLSWindowIterator*` in
  [window_server.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/window_server.rs#L138-L189),
  constraints via `SLSWindowIteratorGetConstraints` with
  `SLSPackagesGetWindowConstraints` fallback in
  [window_server.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/window_server.rs#L210-L230);
  hammerspoon `SLSCopyManagedDisplaySpaces` in
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L53-L60);
  paneru binds `SLSMainConnectionID` and window-query helpers in
  [skylight.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/skylight.rs#L28-L100).
- Recommended mechanism: per-app AX observer loop on owned apps first,
  joined to CG snapshots for bounds/PID/ordering; CGS queries only for read
  paths the spike proves stable. Confidence: high for observer + CG join
  shape; medium for CGS query stability (version-coupled). On-screen-only
  CG snapshots cannot be the complete hidden-workspace membership source;
  retain identities and re-enumerate AX/Space membership before reconciliation.
  Adapter owns
  observer lifetime, per-app run-loop pumping, join refusal on ambiguity,
  and never treating a missing AX window as closure.

### 2. AX-to-CG identity join (one join helper, not the whole private scope)

- FACT: `_AXUIElementGetWindow` is the tier-2 identity join, bound and
  called across surveyed trees (Swift/ObjC plus all five Rust trees):
  AeroSpace (Swift) declares it in
  [private.h](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/PrivateApi/include/private.h#L26-L26)
  and calls it in
  [accessibility.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/util/accessibility.swift#L362-L369)
  (null-ID rejected); rift binds in
  [skylight.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/skylight.rs#L330-L330) and calls in
  [window_server.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/window_server.rs#L73-L82)
  (error and zero-ID rejected); glide binds in
  [window_server.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/window_server.rs#L205-L210)
  and calls in
  [window_server.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/window_server.rs#L57-L66)
  (null-ID path noted at L55-L56); paneru binds in
  [skylight.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/skylight.rs#L25-L26) and calls
  in [windows.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/windows.rs#L146-L153)
  (zero-ID rejected); komorebi-for-mac binds in
  [private.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/accessibility/private.rs#L16-L16)
  and calls in
  [mod.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/accessibility/mod.rs#L30-L36);
  glazewm binds in
  [ffi.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/ffi.rs#L60-L65)
  and calls in
  [native_window.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/native_window.rs#L39-L39);
  hammerspoon declares it in
  [common.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/axuielement/common.m#L10-L10)
  and snapshots it at window construction in
  [HSuicore.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/Hammerspoon/HSuicore.m#L657-L661)
  (second call site at `HSuicore.m:894` for snapshot path).
- CLAIM (AeroSpace [README](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/README.md#L124-L124)):
  this is its only private API; everything else is public AX. That is an
  upstream claim about AeroSpace, not a scope limit on our tier 2: our
  tier 2 also intends scoped SLPS/SLS read/write paths below.
  UNKNOWN: version-coupling risk of the symbol across macOS releases; the
  spike must re-verify per floor release.
- Recommended mechanism: adopt this join; any further private linkage needs
  its own call-site justification. Confidence: high for join shape.
  Adapter owns null/zero-ID refusal and ambiguous-join refusal.

### 3. Move/resize latency and avoiding animations

- FACT: the shared public-AX write order is size, then position, then size
  again. AeroSpace [MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L411-L419)
  cites issues #143/#335 with size/position/size; glazewm
  [native_window.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/native_window.rs#L177-L187)
  writes `AXSize`, `AXPosition`, `AXSize`.
- FACT: the yabai non-SA baseline is the same class plus the enhanced-UI
  workaround. [window_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L741-L761)
  wraps size/position/size in `AX_ENHANCED_UI_WORKAROUND`; the macro in
  [helpers.h](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/helpers.h#L516-L529) reads
  `AXEnhancedUserInterface` (constant at
   [helpers.h](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/helpers.h#L171-L171)) and toggles
  it off/on around the body. AeroSpace
  [MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L424-L436)
  (yabai-commit/Rectangle-PR lineage) and glazewm
  [native_window.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/native_window.rs#L267-L304)
  do the same toggle. PROPOSAL: same write order plus bounded readback;
  never reassert endlessly. UNKNOWN: measured latency/jank on the target
  host; source order is not a latency proof.
- FACT: `SLSDisableUpdate`/`SLSReenableUpdate` batching is a separate
  private candidate with an explicit stability warning, not the default.
  komorebi-for-mac binds them in
  [skylight.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/skylight.rs#L10-L15)
  ("undocumented and I don't know how stable they are") and batches in
  [workspace.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/workspace.rs#L1363-L1383).
  PROPOSAL: evaluate only if AX write-order plus enhanced-UI toggling
  proves too janky, with its own version-coupling note. The yabai SA
  animation proxy path
  ([window_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L472-L525)
  driven through SA swap calls [sa.h](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.h#L24-L25))
  is SA-gated and excluded from tier 2.
- Recommended mechanism: public AX size/pos/size with enhanced-UI toggle
  first; conditional `SLSDisableUpdate` batching only on measured jank.
  Confidence: high for baseline shape; low for jank outcome before
  measurement. Adapter owns write batching, readback comparison, and
  per-app settability matrix. Default needs no Screen Recording: CG list
  metadata and AX writes are not pixel capture (section 11).

### 4. Focus (key-focus vs raise vs frontmost activation)

- Key-focus, raise (z-order), and frontmost activation are distinct. Do not
  promise `AXRaise` is raiseless or that a private call avoids activation
  without readback.
- FACT: plain app activation has a focus side effect by construction.
  komorebi-for-mac [window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L737-L785)
  calls `activateWithOptions` then sets `kAXMainAttribute`; AeroSpace
  [MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L130-L148)
  raises (`isMain` + `AXRaise`) before
  `activate(options: .activateIgnoringOtherApps)`, with a fast path avoiding
  AX requests for slow clients (Godot cited, issue #101).
- FACT: yabai native-window focus defaults to a direct SLPS path, not SA.
  [window_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1324-L1335)
  `window_manager_focus_window_with_raise` runs
  `_SLPSSetFrontProcessWithOptions(window_psn, window_id,
  kCPSUserGenerated)` plus `window_manager_make_key_window` (two
  `SLPSPostEventRecordTo` synthesized key events in
  [window_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1269-L1291))
  plus `AXUIElementPerformAction(kAXRaiseAction)` under `#if 1`; the SA
  `scripting_addition_focus_window` sits in the disabled `#else` branch.
  The no-raise variant at
  [window_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1293-L1322) is
  SLPS-only (`_SLPSSetFrontProcessWithOptions` + make-key, with a 40ms
  pacing hack and same-PSN fast path). Callers use the with-raise form for
  `window --focus` in
  [message.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/message.c#L2084-L2085), display focus in
  [display_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/display_manager.c#L463-L467),
  send-to-Space in
  [window_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L2115-L2116)
  (`space_manager_move_window_to_space` + `SLSSpaceSetFrontPSN`), and
  focus-follows-mouse autorise in
  [event_loop.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L1431-L1433) vs
  no-raise in [event_loop.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L1396-L1396).
  No click/AX-press synthesis on this path; clicks/gestures appear only on
   the Space/display gesture fallback in section 5. Private-symbol version coupling,
  per-app acceptance, TCC consent, latency, and restore behavior are
  runtime UNKNOWN until the spike with readback.
- FACT: private SLPS focus reads distinguish key-focus from frontmost.
  glide [window_server.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/window_server.rs#L212-L239)
  documents that a non-activating panel (Spotlight example) takes keyboard
  focus without activating its app, exposing both `_SLPSGetFrontProcess`
  and `SLPSGetKeyFocusProcess` (bindings at
  [window_server.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/window_server.rs#L271-L284));
  glide writes via `_SLPSSetFrontProcessWithOptions` plus two
  `SLPSPostEventRecordTo` in
  [window_server.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/window_server.rs#L241-L265);
  paneru reads front process in [app.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/app.rs#L381-L386)
  (`_SLPSGetFrontProcess`) and splits focus vs raise in
  [windows.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/windows.rs#L703-L750)
  (`focus_without_raise` at L703-L737 with same-PSN fast path,
  `focus_with_raise` at L741-L750 as SLPS plus make-key plus raise,
  `raise_without_focus` at L753-L757), driven by triggers in
  [focus.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/ecs/focus.rs#L459-L525)
  (`focus_window_trigger` at L459-L488, `raise_window_trigger` at
  L490-L525); `make_key_window` skips macOS 14 Sonoma to avoid a
  serialization crash in
  [windows.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/windows.rs#L470-L480);
  paneru binds both symbols in
  [skylight.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/skylight.rs#L410-L431).
- FACT: `AXRaise`-only has documented limits. glazewm documents in
  [native_window.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/native_window.rs#L306-L332)
  that `AXRaise` after `SetFrontProcess` can steal focus between same-app
  windows (reason its bring-all-to-front is unimplemented), with the
  `SetFrontProcess` call at
  [native_window.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/native_window.rs#L334-L351)
  (bind at [ffi.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/ffi.rs#L67-L79));
  paneru comments `raise_without_focus` is best-effort and "can't lift a
  window above another app's frontmost window" in
  [windows.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/windows.rs#L103-L106), impl at
  [windows.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/windows.rs#L753-L757);
  `focus_with_raise` (SLPS + make-key + raise) is at
  [windows.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/windows.rs#L741-L750).
- PROPOSAL: start with yabai's direct SLPS window-specific focus, testing
   no-raise and with-raise separately; read back the actual AX focused window,
   key-focus process and frontmost process. These paths avoid the broad
   `NSRunningApplication.activate` request, but `_SLPSSetFrontProcessWithOptions`
   still changes frontmost state: "without activation side effects" means
   avoiding unrelated same-app window raises/Space switches, not promising
   unchanged app activation. Neither method-name nor successful dispatch
   proves that goal. The 20ms paneru / 40ms yabai same-app pacing is source
   evidence against assuming instant focus. Use app activation only as a
   separately measured fallback. Confidence: high for source mechanism;
   low for per-app side-effect/latency acceptance. Adapter owns exact focused-
   window readback, stale-target refusal and the per-app acceptance matrix.

### 5. Managed per-monitor workspaces and hiding without Dock injection

Native Spaces is the selected initial workspace model. Probe native-Space
queries and owned-window moves first; order-out remains capability research,
and parking is a possible later user setting, not initial scope.

- FACT: yabai `sa.h` lists Dock-injected implementations, not a global
  exclusivity boundary. [sa.h](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.h#L12-L29) declares
  client stubs; each packs an opcode and sends it to the Dock payload in
  [sa.m](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L442-L447) (example `focus_space`) through
  [sa.m](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L615-L621) (example
  `move_window_to_space`). The same operations have in-process non-SA
  fallbacks in surveyed source: `space --focus` falls back to a synthesized
  gesture path, window-to-Space moves fall back to a compat-ID direct SLS
  path, and window focus defaults to a direct SLPS path (details in 5(b)
   and 4). Yabai's direct create/destroy/reorder/move-to-display paths remain
   SA-gated; Hammerspoon's visibly animated AX create/destroy alternative
   is separate below. CLAIM (upstream
  [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L14-L43)): SIP-enabled
  operation for these fallbacks; dispatch shape below is FACT, SIP-on
  efficacy is runtime UNKNOWN until the spike.
- FACT (a): order-out hiding is dispatched in source but foreign-window
  authority is UNKNOWN. rift [cgs_window.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/cgs_window.rs#L302-L341)
  exposes `order_above` (1), `order_below` (-1), `order_out` (0) via
  `SLSOrderWindow` on `self.connection` (`*G_CONNECTION` from
  `SLSMainConnectionID` at [skylight.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/skylight.rs#L23-L23));
  `from_existing` at [cgs_window.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/cgs_window.rs#L215-L221)
  marks foreign IDs `owned: false`, but every in-tree order call site
  found (drag preview at
  [drag_preview.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/ui/drag_preview.rs#L161-L192),
  mission-control overlay at
  [mission_control.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/ui/mission_control.rs#L961-L961),
  stack lines at
  [stack_line.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/ui/stack_line.rs#L218-L240)) acts on
  caller-owned `CgsWindow::new`/`new_compositor` surfaces. No surveyed call
  orders a foreign app window out via another connection. UNKNOWN:
  Dock/Cmd-Tab/Mission Control acceptance of foreign order-out; needs the
  managed-workspace spike.
- FACT (b): native-Space moves without SA exist in two surveyed sources
  with version branching, type gating, and no post-dispatch readback.
  hammerspoon
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L183-L190)
   branches on Sonoma 14.5+ (gate at
   [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L16-L20)):
  newer uses `SLSSpaceSetCompatID` + `SLSSetWindowListWorkspace`, older uses
  `SLSMoveWindowsToManagedSpace`. yabai adds an async-bridged branch ahead
  of those alternatives in
  [space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L684) (list)
  and [space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L686-L705)
   (single): (1) async bridged operation using
   `SLSBridgedMoveWindowsToManagedSpaceOperation.initWithWindows:spaceID:` when
  `SLSPerformAsynchronousBridgedWindowManagementOperation` resolves (bound
  in [yabai.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/yabai.c#L149-L149)), else (2) direct
  `SLSMoveWindowsToManagedSpace` when `workspace_use_macos_space_workaround()`
   is false, else (3) SA attempt first then compat-ID fallback
   (`SLSSpaceSetCompatID(sid, 0x79616265)` +
   `SLSSetWindowListWorkspace(..., 0x79616265)` + reset to `0x0`).
   Yabai's workaround gate is broader than Hammerspoon's: macOS 12.7+,
   13.6+, 14.5+ and 15+ in
   [workspace.m](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/workspace.m#L17-L26).
   Our tier-2 adapter must omit the SA attempts entirely, not copy a
   "try injection first" path. The compat-ID sequence temporarily mutates
   Space state and resets to zero rather than restoring a queried preimage;
   a spike must establish cleanup on every failure, not assume that is safe.
  CLAIM (upstream [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L14-L14),
  7.1.25 2026-05-08): branch (3) fallback "works with SIP enabled again".
  Dispatch is FACT; SIP-on effect is runtime UNKNOWN.
  Type gating is user-space to user-space unless forced: hammerspoon
  rejects non-user target at
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L163-L167)
  and non-user source at
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L176-L181)
  unless `force` is true (fullscreen float-only noted at
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L154-L155));
  query path rejects non-user/non-fullscreen at
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L116-L121).
  Space-type values (user=0, fullscreen=4, system=2) are documented in
  paneru [skylight.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/skylight.rs#L150-L154),
  and rift treats fullscreen/system spaces as transient in
  [spaces.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/actor/spaces.rs#L1-L21).
  Caveat: hammerspoon dispatch returns `true` without observed membership
  readback in
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L172-L199)
  (already-on-space skips the SLS call yet still returns true; no
  post-move `SLSCopySpacesForWindows` check). Do not treat parked
  user-space construction as a product choice; it is a spike probe only.
- FACT (b2): Space create/destroy/focus via public AX UI automation exist
  in surveyed source with visible-transition cost. hammerspoon
  [spaces.lua](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L723-L730)
  `addSpaceToScreen` opens Mission Control then `doAXPress` on
  `mc.spaces.add`; [spaces.lua](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L779-L794)
  `gotoSpace` opens Mission Control, waits `MCwaitTime`, then AX `Press`
  on the indexed `mc.spaces.list` child (auto-closes MC); and
  [spaces.lua](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L864-L875)
  `removeSpace` opens Mission Control, waits, then AX `AXRemoveDesktop`
  on the indexed child. Module header at
  [spaces.lua](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L6-L10)
  states private API plus AX hacks with unavoidable visual feedback
  (Reduce-motion only minimizes). This is public AX automation, no
   injection, not a low-jank private equivalent.
- FACT (b3): yabai `space --focus` has a non-SA synthesized-gesture
  fallback. [space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L985-L1009)
  tries `scripting_addition_focus_space(sid)` first, else calls
  `space_manager_focus_space_using_gesture`; the gesture path at
  [space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L982) posts
  high-velocity Dock-swipe CGEvents (`kCGSessionEventTap`) per index step
  and, on cross-display, warps the cursor plus `CGPostMouseEvent` click in
  [space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L973-L978), with
  the same click fallback in display focus at
  [display_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/display_manager.c#L463-L480).
  The in-code comment at
  [space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L944-L953) states
  macOS has no Space-activation API. CLAIM (upstream
  [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L43-L43) 7.1.19 plus perf at
  [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L39-L39) 7.1.20): `space --focus`
  works with SIP enabled. Dispatch fallback is FACT; latency, permission
  (synthesized input needs Accessibility consent recording per spike),
  focus-follows, and animation outcome are runtime UNKNOWN. `display --focus`
   named `display_manager_focus_display_with_raise` stays SA-only in
  [display_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/display_manager.c#L483-L495)
  (returns `SCRIPTING_ADDITION` error when SA fails).
- FACT (b4): SIP-on focus-animation suppression is a dispatched setter
  pair, not a proven effect. `skip_window_focus_animation` config plumbing
  is in [message.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/message.c#L1269-L1273); when set,
  front-switch handling at
  [event_loop.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L359-L375) and
  window-focus handling at
  [event_loop.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L663-L669) call
  `SLSSpaceSetFrontPSN` then the gesture focus above. CLAIM (upstream
  [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L31-L35) 7.1.21-7.1.22 and
  [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L23-L23) 7.1.23): removes native
  focus Space animation (cmd+tab/Dock click, gated behind the option) with
  SIP enabled. Setter dispatch is FACT; animation removal is runtime
  UNKNOWN until measured.
  UNKNOWN: move acceptance, focus-follows, and fullscreen/system-space
  behavior on the target host.
- FACT (c): AeroSpace-class offscreen parking, own implementation (not a
  comment about another repo). [MacWindow.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacWindow.swift#L123-L155)
  `hideInCorner` saves a proportional preimage then `setAxFrame` to a
  bottom corner (1px Zoom quirk at L146-L152);
  [MacWindow.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacWindow.swift#L157-L183)
  `unhideFromCorner` restores floating frames proportionally;
  [refresh.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/layout/refresh.swift#L151-L153)
  defines `OptimalHideCorner` and picks per-monitor corners by overlap in
  [refresh.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/layout/refresh.swift#L164-L187),
  hiding invisible workspaces at
  [refresh.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/layout/refresh.swift#L196-L201)
   via public-AX `setAxFrame` ([MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L150-L157)),
  size/pos/size at
  [MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L411-L419).
  Separate-Spaces rationale: focus fast path branches on
  `screensHaveSeparateSpaces` at
  [MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L136-L136).
  Komorebi-for-mac [window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L508-L542)
  stores the restore rect in `WINDOW_RESTORE_POSITIONS` (vacant-entry only)
  then moves to a bottom-corner hidden frame, with the comment "basically
  what Aerospace does in lieu of an actual Hide API" at
  [window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L518-L520);
  restore re-applies the stored rect in
  [window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L556-L577).
  Paneru parks hidden strips offscreen with a visible sliver: magnitude and
  display-adoption rationale in
  [layout.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/ecs/layout.rs#L34-L38)
  (`PARKED_STRIP_SLIVER` = 10), use in
  [display.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/ecs/display.rs#L293-L317), corner
  clamping in [layout.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/ecs/layout.rs#L1640-L1650).
   CLAIM (AeroSpace [guide.adoc](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/docs/guide.adoc#L448-L487)):
   slivers survive, some monitor arrangements leak hidden windows onto other
   displays, and Mission Control thumbnails can be too small. The guide
   [recommends separate Spaces OFF](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/docs/guide.adoc#L490-L526)
   when native multi-monitor fullscreen is unimportant. UNKNOWN: current-host
   behavior. Our acceptance must cover existing display arrangements and
   both separate-Spaces modes, without requiring these setting changes to
   conceal workspace defects.
- PROPOSAL (spike order, not a product default): probe (a) then (b) on
  owned test windows; use (c) parking only if (a)/(b) fail acceptance, gated
  on recovery + shell-behavior proof (restore identity, Mission
  Control/Dock/Cmd-Tab acceptance, forced-kill and quit/update restore with
  a visible manual recovery route). Recommended mechanism per path is the
  path order above. Confidence: medium for source shape on all three; low
  for acceptance before the spike. Adapter owns Space/display snapshots,
  per-display sets, and restore-identity bookkeeping.

### 6. Hotkeys: CGEventTap vs Carbon RegisterEventHotKey, and TCC

- FACT: Carbon provides discrete registration, not an event-filter callback;
   these sources do not establish a general OS-chord takeover capability.
   hammerspoon
  [libhotkey.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/hotkey/libhotkey.m#L257-L267)
  calls `RegisterEventHotKey(..., kEventHotKeyExclusive, ...)` with
  first-come exclusivity handling; Amethyst registers through
  KeyboardShortcuts/MASShortcut in
  [HotKeyRegistrar.swift](https://github.com/ianyh/Amethyst/blob/6508ee2cacf8f9b357e1a3249a8f28ba5c94db2a/Amethyst/Managers/HotKeyRegistrar.swift#L10-L52);
  glide registers via `livesplit_hotkey` `Hook::with_consume_preference(MustConsume)` in
  [event.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/event.rs#L19-L45). UNKNOWN: exact
  capture substrate inside `livesplit_hotkey` and Amethyst deps (not opened
  here).
- FACT: session event taps can intercept (return-null swallows) but need
  their own consent and suffer Secure Input blackout plus timeout-disable.
  glazewm [keyboard_hook.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/keyboard_hook.rs#L134-L167)
  taps `SessionEventTap`/`HeadInsertEventTap` for KeyDown+KeyUp, returns
  null on intercept in
  [keyboard_hook.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/keyboard_hook.rs#L175-L216);
  hammerspoon [libeventtap.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/eventtap/libeventtap.m#L217-L230)
  creates `kCGSessionEventTap`/`kCGHeadInsertEventTap` and blames missing
  Accessibility on failure; paneru installs a `HIDEventTap`/`HeadInsert`
  tap in [input.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/platform/input.rs#L195-L234)
  covering mouse, scroll, gesture and KeyDown, with focus-passthrough sets
  in [input.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/platform/input.rs#L34-L43); rift
  wraps tap creation with reenable/invalidate callbacks in
  [event_tap.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/event_tap.rs#L107-L159), and keeps
  a Carbon listener type in
  [carbon.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/carbon.rs#L162-L178)
  (framework linkage at [carbon.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/carbon.rs#L29-L29)).
  komorebi-for-mac's tap is a passive observer: `HIDEventTap`/`HeadInsert`
  on mouse-up/key-up only in
  [input_event_listener.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/input_event_listener.rs#L84-L112),
  always returning the event in
  [input_event_listener.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/input_event_listener.rs#L26-L82)
  (reap/drag-end supplement, not interception).
- PROPOSAL: spike compares Carbon-style discrete registration against a
  session event tap on the target host: conflict reporting, suppression of
  matched chords, down/up/repeat behavior, TCC services per mode, Secure
  Input blackout, timeout-disable recovery, non-leakage with unmanaged
  foreground (Windows Win+G/Game Bar lesson). Respect lock/Secure Input;
  missing interception never silently means success. Recommended mechanism:
  discrete registration first, tap only where suppression is proven needed.
  Confidence: high for API shape; low for system-chord suppression. Adapter
  owns tap lifetime, re-enable, and conflict reporting. TCC details are in
  section 11; reset template and stable identity live in
  [the runbook](../../macos-dev-environment.md#6-permissions-tcc-resets-and-stable-identity).

### 7. Active border, true underlay, drop preview

- Scope rule (FACT of our own design, not upstream): caller-owned
  overlay/window manipulation (our border/underlay/preview surfaces) must
  not be conflated with authority over arbitrary foreign windows. Ordering
  one owned surface relative to a target is not control of that target.
- FACT: caller-owned borderless overlay pattern. paneru
  [overlay.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/overlay.rs#L213-L236) builds
  `NSWindow` `Borderless` with `Transient | IgnoresCycle |
  CanJoinAllSpaces | Stationary | FullScreenNone`, sets
  `setIgnoresMouseEvents(true)` at L225 and `NSFloatingWindowLevel` at L227,
  one overlay per display with `orderFront` in
  [overlay.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/overlay.rs#L307-L325); komorebi
  border observer in
  [border.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/border_manager/border.rs#L105-L147)
  re-queries the tracked rect and updates the owned `NsWindow`, created
  borderless in
  [ns_window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/border_manager/ns_window.rs#L65-L81)
  with `NSNormalWindowLevel` and `setIgnoresMouseEvents(true)`.
  No surveyed source demonstrates a nonactivating-panel flag beyond
  `setIgnoresMouseEvents`; click-through without activation beyond that is
  UNKNOWN.
- FACT: relative ordering primitives on owned surfaces exist in source:
  rift [cgs_window.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/cgs_window.rs#L302-L341)
  (`SLSOrderWindow` above/below/out on own `G_CONNECTION`); yabai orders
  an owned insert-feedback surface relative to a managed foreign window at
  [view.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/view.c#L42-L42)
  (`SLSOrderWindow(feedback, 1, window_order[0])`) with order-out at
  [view.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/view.c#L114-L114). UNKNOWN: a
  proven primitive placing our fill below specific foreign windows while
  above unrelated lower ones; none established from these sources, and
  foreign-window ordering authority is explicitly UNKNOWN (section 5).
- PROPOSAL: approved fallback stays group outline (never a fill drawn above
  content masquerading as an underlay). Experiment with click-through
  nonactivating panels (active ring, group plane, drag preview) against
  real z-order; renderer choice grants no stacking. Recommended mechanism:
  owned borderless click-through overlay first; underlay fill only on proven
  below-foreign/above-lower primitive. Confidence: high for overlay shape;
  low for true-underlay feasibility. Adapter owns overlay lifetime,
  per-display placement, and acceptance (Mission Control thumbnails,
  Cmd-Tab, Dock, click-through, fullscreen/game suppression, mixed-scale
  multi-display).

### 8. Mixed-DPI and multi-monitor coordinates

- FACT: per-display scale is read from the native screen object. glide
  [screen.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/screen.rs#L95-L113) maps CG displays
  to `NSScreen` for `backing_scale_factor` (struct
  [screen.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/sys/screen.rs#L181-L197)); glazewm
  [display.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/display.rs#L99-L112)
  exposes `scale_factor`; komorebi-for-mac converts AppKit visible frames
  to Quartz coordinates in
  [macos_api.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/macos_api.rs#L261-L283)
  with display bounds in
  [mod.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/core_graphics/mod.rs#L55-L86);
  paneru models menubar height and dock-derived bounds in
  [display.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/display.rs#L23-L48) and
  [display.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager/display.rs#L105-L144).
- PROPOSAL: adapter normalizes to integer core units with per-output scale
  (closest Windows analogue:
  [active_border.rs](../../../crates/tiler-windows/src/active_border.rs#L135-L158)
  `scale_to_physical`/`scale_style`); requested-vs-observed frames are
  compared across scales. Recommended mechanism: native scale read plus
  integer normalization. Confidence: high for read shape; low for
  rounding/origin before host measurement. UNKNOWN: exact rounding/origin
  behavior on the target host; "Displays have separate Spaces" on/off
  changes monitor/fullscreen behavior and both must be tested. Adapter owns
  unit conversion and cross-scale readback.

### 9. Native fullscreen and Spaces coexistence

- FACT: fullscreen detection exists in each tree: yabai reports
  `is-native-fullscreen` via `space_is_fullscreen` in
  [window.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c#L359-L360) and skips tiling such
  windows in [window.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c#L1124-L1125);
  komorebi-for-mac skips reaping fullscreen windows in
  [reaper.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/reaper.rs#L143-L148);
  rift nulls fullscreen/system spaces before workspace mapping in
   [spaces.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/actor/spaces.rs#L11-L21); paneru keeps
  keybindings intercepted on fullscreen spaces but routes layout per its
  fullscreen-space check ([input.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/platform/input.rs#L596-L598));
  glazewm maps maximize to `AXFullScreen` in
  [native_window.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/platform_impl/macos/native_window.rs#L220-L226).
- PROPOSAL (preserved from the prior plan, no invented behavior): native
  mac fullscreen (own Space) vs project fullscreen handling differences
  stay OPEN. Scope against the KDE reference; leave native-fullscreen
  windows and their Spaces untouched until the spike proves otherwise.
  Amethyst native-Space throw/focus-across-screens
  ([HotKeyManager.swift](https://github.com/ianyh/Amethyst/blob/6508ee2cacf8f9b357e1a3249a8f28ba5c94db2a/Amethyst/Events/HotKeyManager.swift#L154-L171))
  and Silica private `CGSSpaceID`/`CGSSpaceType` in
  [Space.swift](https://github.com/ianyh/Amethyst/blob/6508ee2cacf8f9b357e1a3249a8f28ba5c94db2a/Amethyst/Model/Space.swift#L9-L14) are
  the native-Space contrast input, not the selected model. Recommended
  mechanism: detect-and-skip first. Confidence: high for detection shape.
  Adapter owns classification and no-write guarantee on fullscreen/games.

### 10. Crash recovery (snapshots vs stranded-window restore)

Layout snapshots and stranded-window restore are separate problems; an
in-memory preimage plus a reaper callback is not crash recovery.

- FACT: in-memory preimage-before-hide plus live re-enumeration (not
  durable recovery). komorebi-for-mac
  [window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L512-L516)
  inserts the restore rect on vacant entry only, and
  [window.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L557-L577)
  removes it after restore; the reaper in
  [reaper.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/reaper.rs#L61-L122)
  reaps invalid windows and updates the focused workspace. This covers live
  invalidation, not post-crash restore.
- FACT: durable layout snapshots exist in three trees. rift persists engine
  snapshots with schema validation in
  [snapshot.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/layout_engine/engine/persistence/snapshot.rs#L25-L108)
  and restores transactionally in
  [restore.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/layout_engine/engine/persistence/restore.rs#L80-L243);
  paneru plans workspace/strip survival against live state in
  [restore.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/ecs/restore.rs#L158-L230) with config
  gates in [config.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/config.rs#L690-L708) and a
  startup grace tick in
  [restore.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/ecs/restore.rs#L45-L56); glide keeps a
  `restore_file` in
  [wm_controller.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/actor/wm_controller.rs#L96-L137)
  and restores space state in
  [space_manager.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/actor/space_manager.rs#L83-L124).
- FACT (own watcher evidence, Windows): same-executable crash watcher with
  live readiness. Marker helpers
  ([product_hide.rs](../../../crates/tiler-windows/src/product_hide.rs#L36-L54))
  render/parse `watcher-<owner-creation>.ready`; spawn and live-verify in
  [product_hide.rs](../../../crates/tiler-windows/src/product_hide.rs#L1344-L1415)
  (same exe/SID/session, creation match, alive check); commit-before-hide
  ledger in [storage.rs](../../../crates/tiler-windows/src/storage.rs#L221-L253)
  with hide/reveal claims in
  [product_hide.rs](../../../crates/tiler-windows/src/product_hide.rs#L1003-L1081)
  and
  [product_hide.rs](../../../crates/tiler-windows/src/product_hide.rs#L1196-L1344).
  The Windows ledger is geometry-less by design (visibility claims); a Mac
  preimage must additionally be durable and carry geometry plus session and
  lifetime identity (bundle ID / PID / window ID joins are all unstable
  across relaunch; AX pointers must never be serialized).
- PROPOSAL: Mac needs its own TCC/lifetime proof; prove standalone restore
  before managing ordinary user windows. Durable preimage requirements:
  committed geometry per managed window, session/display membership,
  stable-enough identity for re-identification, and a visible manual
  recovery route. UNKNOWN: watchdog shape on Mac; no same-executable
  watcher source established here. Recommended mechanism: durable snapshot
  plus independent stranded-restore probe with forced-kill gate. Confidence:
  high for split shape; low for restore acceptance. Adapter owns snapshot
  durability, identity rejoin, and the manual route.

### 11. Permissions, signer, notarisation, App Store, floor

- FACT: Accessibility consent checks appear in source; MDM can deny it.
  `AXIsProcessTrustedWithOptions` with prompt in glide
  [permission_flow.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/ui/permission_flow.rs#L125-L129),
  glazewm [dispatcher.rs](https://github.com/glzr-io/glazewm/blob/5709ad0a3c7c386bbc3e38166a865ffc12937515/packages/wm-platform/src/dispatcher.rs#L97-L101),
  rift [accessibility.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/sys/accessibility.rs#L30-L39),
  paneru [manager.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager.rs#L901-L907);
  `AXIsProcessTrusted` gate in komorebi-for-mac
  [main.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/main.rs#L60-L62) and
  paneru [manager.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/manager.rs#L892-L892); paneru
  startup prompt in
  [accessibility_prompt.rs](https://github.com/karinushka/paneru/blob/b1b6abbd3f1a4be138152b6f0389c9ff1b27a269/src/accessibility_prompt.rs#L6-L36).
  TCC reset template and the stable-identity rule (same bundle ID, path,
  signing identity) are in
  [the runbook](../../macos-dev-environment.md#6-permissions-tcc-resets-and-stable-identity).
- FACT: mutating taps/posts need Accessibility; passive observation does
  not imply it, and Input Monitoring is a separate service per tap mode/OS.
  Intercepting session taps (glazewm, paneru, rift above) vs komorebi's
  passive observer (section 6) is the source distinction; glide's fallback
  message points at Input Monitoring in
  [permission_flow.rs](https://github.com/tmandry/glide/blob/2837add2ce3911743e869ed43da59014714b7e59/src/ui/permission_flow.rs#L173-L176).
  No surveyed default assumes its permissions; the spike must record TCC
  services per mode. Recommended mechanism: request Accessibility first,
  add Input Monitoring only where the chosen tap mode proves it needs it.
  Confidence: high for check shape; medium for per-mode TCC mapping.
  Adapter owns consent sequencing and refusal handling.
- FACT: `CGWindowListCopyWindowInfo` metadata is not pixel capture; Screen
  Recording is for pixel/preview paths. Rift gates overview previews on
  `CGPreflightScreenCaptureAccess` in
  [mission_control.rs](https://github.com/acsandmann/rift/blob/41f1c051f21f4bb69b3faf506c5bdc8f92100f62/src/ui/mission_control.rs#L1799-L1804);
  komorebi-for-mac gates startup on `CGPreflight/RequestScreenCaptureAccess`
  in [main.rs](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/main.rs#L50-L58)
  (comment: needed to read window titles in its implementation).
   PROPOSAL: request no Screen Recording for baseline tiling; use AX for
   any needed textual metadata, and do not require CG window titles. Owned
   drop-preview drawing is not capture. Captured thumbnails require a
   separate permission check; verify needed non-content CG fields with
   Screen Recording denied on the target OS. Confidence: high for this
   source distinction, medium for target-OS metadata availability. Adapter
   owns keeping capture paths out of the baseline.
- FACT: notarization is a distribution property, not an API-surface
   statement. AeroSpace ships unnotarized by author choice
  ([README](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/README.md#L40-L47)) while using mostly
   public API plus one private helper. Its
   [development guide](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/dev-docs/development.md#L28-L38)
   uses a persistent self-signed code-signing certificate; its
   [release script](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/build-release.sh#L5-L10) accepts
   an identity, [signs the CLI](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/build-release.sh#L47-L51)
   and [verifies both signatures](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/build-release.sh#L94-L101).
   Amethyst's [build settings](https://github.com/ianyh/Amethyst/blob/6508ee2cacf8f9b357e1a3249a8f28ba5c94db2a/Amethyst.xcodeproj/project.pbxproj#L1042-L1048)
   enable hardened runtime alongside private-framework search paths; this
   does not establish our notarization success. PROPOSAL: Developer ID,
   hardened runtime and notarization per the runbook's S3/S4/S5, with actual
   private-symbol loading/entitlements verified on an artifact. No surveyed
   source establishes a blanket private-API notarization ban or a guarantee
   of acceptance. Our tentative distribution stays
  notarized Developer ID DMG plus Homebrew cask first (open decision);
  notarization is not App Store approval.
- FACT: App Store rules require sandboxing (2.4.5) and public APIs (2.5.1),
  with alternate-desktop review exposure (2.5.8) per the Apple review
  guidelines (sources below). Tier 2 (`_AXUIElementGetWindow`, SLPS/SLS
  focus, CGS queries) is excluded from the Store path by construction.
  Defer any Store promise to a separate signed sandbox spike plus Review
  pass; the higher-level alternative in section 5 does not itself promise
  Store viability.
- FACT (deployment settings, not our floor): AeroSpace
   [Package.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Package.swift#L11-L15) declares
   `.macOS(.v13)`; Amethyst's application configuration uses
   [macOS 11](https://github.com/ianyh/Amethyst/blob/6508ee2cacf8f9b357e1a3249a8f28ba5c94db2a/Amethyst.xcodeproj/project.pbxproj#L1052-L1058).
   AeroSpace's [release script](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/build-release.sh#L26-L27)
   builds arm64/x86_64 CLI slices and
   [checks both app and CLI are universal](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/build-release.sh#L79-L95).
   These establish configuration, not tested support for our private symbols.
   PROPOSAL: macOS 15+ arm64 initially, consistent with the setup runbook and
   existing macOS-15 arm64 CI smoke job. Confirm the actual host first; do not
   claim other OS releases or Intel parity without their own evidence.

## Core and adapter mapping (actual seams, not analogies)

Shared core holds policy; the adapter owns visibility, session/world
mapping, TCC/native IDs, and the event loop. Own-repo source baseline:
`0360eb3` (no product code changed in this research). Exact seams:

- Policy entry: `Engine::handle(&CoreEvent) -> CoreReply` in
   [engine.rs](../../../crates/tiler-core/src/engine.rs#L807-L902) covers
   reconciliation, focus/move, float, resize, send, group and drag operations;
   domain
  lifecycle `session`/`session_mut`/`keys`/`store_committed`/
  `take_usable_session`/`reproject_retained` in
  [engine.rs](../../../crates/tiler-core/src/engine.rs#L383-L401),
  [engine.rs](../../../crates/tiler-core/src/engine.rs#L682-L682) and
  [engine.rs](../../../crates/tiler-core/src/engine.rs#L661-L725).
- Session plans (not constructors): move/focus/resize/drag plan types in
  [session.rs](../../../crates/tiler-core/src/session.rs#L328-L408),
  lifecycle verification and ack in
  [session.rs](../../../crates/tiler-core/src/session.rs#L1250-L1346)
  (`acknowledge`, `cancel_unacked_pending`, `verify_lifecycle`,
  `note_adapter_loss`, `note_postcondition_mismatch`).
- Workspace policy (pure functions): `select_existing`,
  `resolve_send_target`, `plan_trailing` in
   [workspace.rs](../../../crates/tiler-core/src/workspace.rs#L7-L66).
- Visual intent (host-independent math): border/drag/underlay helpers in
   [visual.rs](../../../crates/tiler-core/src/visual.rs#L19-L133)
  (`active_border_state`, `active_border_inner_rect`,
  `drag_preview_rect_valid`, `group_underlay_outer_rect`,
  `group_visible`, `group_underlay_trigger`).
- Minimum-aware projection: meaningful min/max, clamp, and assessment in
  [size_hints.rs](../../../crates/tiler-core/src/size_hints.rs#L108-L219)
  plus `project_with_hints`.
- Contract shape exists but is not a shipped adapter route: `Observation` /
  `Dispatch` / `AdapterAck` / `PostObservation` / `DivergenceKind` in
  [contract.rs](../../../crates/tiler-core/src/contract.rs#L61-L248) and
  lifecycle/focus plan types in
  [contract.rs](../../../crates/tiler-core/src/contract.rs#L302-L746);
  in-tree `tiler-windows` uses only `LifecycleOperation::MoveTiled` and
  `DivergenceKind` in
  [workspace_owner.rs](../../../crates/tiler-windows/src/workspace_owner.rs#L1596-L2012).
  Reconcile entry `new` in
  [reconcile.rs](../../../crates/tiler-core/src/reconcile.rs#L298-L298) is
  test-covered policy, not an adapter call. Do not present the contract
  module as the Mac adapter route without a spike proving its use.
- Closest Windows adapter analogues (intent only, no mechanic reuse):
  `ManagedWorkspaces` membership/hide/focus/cleanup in
  [workspace.rs](../../../crates/tiler-windows/src/workspace.rs#L95-L389)
  (`ensure_output`, `activate`, `select`, `resolve_send`, `assign`,
  `set_hidden`, `focus_target`, `eligible_focus_set`, `plan_cleanup`);
  committed ledger in
  [storage.rs](../../../crates/tiler-windows/src/storage.rs#L221-L253);
  hook classification/queue in
  [winarrow.rs](../../../crates/tiler-windows/src/winarrow.rs#L54-L451);
  border/underlay geometry and eligibility in
  [active_border.rs](../../../crates/tiler-windows/src/active_border.rs#L42-L353)
  and
  [group_underlay.rs](../../../crates/tiler-windows/src/group_underlay.rs#L29-L270).

## Shortcuts and host-setting conflicts

- The 2026-10-03 per-binding conflict decision applies to Mac: settings
  show each binding conflicting with an OS/desktop chord and offer keep
  (override), disable, or rebind, with one-step "compatible" and
  "authentic" presets. Ctrl is the adopted default Meta and the modifier
  is user-configurable; the catalog/conflict spike still inventories the
  actual chords, including Mission Control Ctrl+arrow and Ctrl+digit holders.
  Compatible avoids known OS conflicts; authentic may override only proven
  chords, with explicit unsupported status.
- Apple-owned chords to inventory: Cmd-Space, Ctrl-Up/Down, Cmd-Tab,
  Ctrl-Cmd-Q, Fn-Control tiling chords, and Mission Control/Spaces/System
  Keyboard remappables (Apple sources below).
- Carbon `RegisterEventHotKey` reports conflicts but does not promise
  takeover of system chords; a session event tap can intercept/suppress
  but needs its own consent, suffers Secure Input blackout and
  timeout-disable (FACT pattern, section 6).

## Native coexistence inventory (spike before promising)

- Native tiling (macOS 15): edge drag, Option-accelerated drag, green
  button layouts, Window menu, Fn-Control(-Shift/-Option-Shift) chords
  (Apple tile-windows and tiling-shortcuts sources below).
- Mission Control/Spaces/System Keyboard shortcuts are user-remappable;
  remappability is not a general takeover API. Record exact preimages of
  only owned changed settings; visible off/revert; no private defaults
  writes assumed.
- "Displays have separate Spaces" on/off changes monitor/fullscreen
  behavior; test both (AeroSpace branches on
  `screensHaveSeparateSpaces` in
  [MacApp.swift](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacApp.swift#L136-L136);
  hammerspoon exposes the same flag in
  [libspaces.m](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L34-L39)).
- Stage Manager needs a classification probe. Historical Amethyst reports
  could not distinguish staged-hidden from minimized reliably; that is an
  upstream limitation report, not proof about current macOS. Never infer
  closure or ownership from absence.

## Gaming coexistence

- Remember tiled membership across gaming sessions; suspend geometry
  moves, effects (border/underlay/preview), and shortcut handling while
  a game is active; resume with re-observation on return.
- Windows "Xbox mode" pause/resume is platform-specific; do not invent a
  Mac equivalent. Research native fullscreen games, borderless windows,
  Game Mode auto-engage on Apple silicon fullscreen, and Game Overlay
  (Cmd-Esc, Tahoe 26+) against the Apple Game Mode source below.
- Provide alternate access to displaced OS surfaces where authentic
  bindings collide; anti-cheat safety is a non-guarantee; no injection
  to work around it.

## Phases (owned probes first; Engine only after lifecycle proof)

| Phase | Scope | Exit evidence |
| --- | --- | --- |
| 0. Decisions + seam contract | Apply the adopted inputs below; [setup runbook](../../macos-dev-environment.md), [live protocol](../../live-macos-testing.md), stable signed identity; portable-crate baseline; consumer-driven core policy review (no Win32 mechanics). Capability-first spikes: owned-window AX observation/focus readback, Carbon registration and intercepting tap, native Spaces with SIP enabled. [Active evidence](../../changes/macos-phase-0.md). Phase 1 completes lifecycle/recovery and hotkey acceptance | Decisions recorded; probe contracts and initial capability evidence recorded; reproducible signed `.app` with stable TCC consent; CI allowlist green |
| 1. Lifecycle, recovery, hotkeys | AX observer loop on owned apps; Carbon-vs-tap comparison; sleep/wake/session/Space/display callbacks; independent stranded-restore probe with durable preimages; settable/error/readback matrix | Conflict/suppression/TCC/SecureInput/timeout matrix; forced-kill restore with manual route. No hiding of ordinary user windows before this gate passes |
| 2. Engine tiling, then workspaces + parity | Visible-ordinary-Space tiling first (`Engine::handle` + session plans + visual/size-hint policy); per-app skips; min-size hints (`SLSWindowIteratorGetConstraints` class vs AX probing); fullscreen/float/sticky classification; then per-display native Spaces using verified section 5 paths; active border with outline fallback; drag/drop preview. Parking deferred to a later setting | Requested-vs-observed frames across scale/animation; no fullscreen-game writes; revoke/wake/hotplug recovery; two-display send/switch/trailing-empty; shell acceptance; no game cost; preview without focus theft |
| 3. Settings/menu/distribution/update + parity queue | Native menu/settings; per-binding conflict UX; notarized DMG + cask; update/uninstall lifecycle; mirror the Windows parity queue (maximise/fullscreen/float/sticky) per item | Live settings/readback; TCC survival across update; clean login/exit; quarantine path documented |

## Adopted and open decisions

Decisions adopted 2026-10-14 are durable in
[Current Decisions](../../decisions.md#scope-and-platform-goals). Proposals
elsewhere in this research do not override these selections.

| Decision | Options / status | Selection or remaining recommendation |
| --- | --- | --- |
| macOS support floor | **Adopted: macOS 15+** | Homebrew current Mac-only events: 93.27% on 15+, 71.72% newer-only; developer/event-weighted proxy, not population proof. StatCounter modern-version reporting is unreliable. Sources and private-API risk rationale in decisions; floor runtime validation still required |
| Intel support | **Adopted: Apple Silicon only** | No Intel commitment |
| Stable signer | **Adopted: persistent self-signed dev identity** | Fixed `io.github.omnitiler.macos.probe` bundle ID/path/signer; no paid membership initially. Developer ID/notarized release route remains open |
| Cmd/Meta vs Ctrl/Option mapping | **Adopted: Ctrl default Meta, user-configurable on macOS** | Preserve per-binding conflicts and Compatible/Authentic presets; Mission Control Ctrl+arrow/digit are expected conflicts |
| Thread topology | **Adopted from day one** | Main = AppKit/UI only; dedicated minimal input thread; per-app AX threads with timeouts; engine owns core state; channels. Tap timeout-disable recovery and hung apps are design inputs |
| Workspace implementation | **Adopted: native Spaces first** | yabai primary/general reference. Virtual/parking may later be a user setting alongside native; not initial scope |
| Distribution | Notarized Developer ID DMG plus Homebrew cask first vs alternatives | DMG plus cask first; notarization is not Store approval; open |
| Updates | Manual DMG upgrade first vs Sparkle later | Manual first; Sparkle only after safe-restore/update proof for hidden/stranded windows; open |
| UI language / unsafe | **Adopted: all Rust** | [Unsafe Code](../../principles.md#unsafe-code); [enforcement](../../decisions.md#rust-unsafe-guardrail) |
| Native settings takeover | Visible off/revert with preimages of owned changes only vs alternatives | Default pending; open |
| Native vs project fullscreen | Preserve native fullscreen Spaces/games vs project slot restoration | Define against KDE intent after the capability spike; open |

Other distribution/UI choices in earlier drafts remain proposals if
included here; architectural proposals stay distinct from the selected
tier-2 policy above.

## Confidence, unknowns, risks

- Confidence: tier-2 source shapes (AX observers, `_AXUIElementGetWindow`
  join across Swift/ObjC plus five Rust trees, AX write order with
  enhanced-UI toggle, Carbon-vs-tap choice, SLPS focus split with yabai
  direct-SLPS default, order-out / compat-ID Space-move / AX-automation
  Space create-focus-destroy / parking source shapes, per-display scale,
  snapshot vs preimage split) are read-verified in pinned local source.
  `sa.h` declares Dock-payload client stubs, not an exclusivity proof;
  yabai's direct Space create/destroy/reorder/move-to-display remain SA-only,
  while Hammerspoon demonstrates AX UI automation for create/destroy/focus.
  Labels: upstream SIP-on Space/focus/animation lines in
  [CHANGELOG](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L14-L43) are CLAIMs; dispatch
  branches above are FACT; SIP-on effect, latency, TCC consent for
  synthesized gestures, and shell acceptance stay runtime UNKNOWN until
  the owned-window spike with readback. Nothing in tier 2 injects into
  Dock by source shape, but that absence is not SIP-on proof.
- Unknowns (runtime): per-app AX latency and settability; raiseless-focus
  acceptance matrix; foreign order-out acceptance as primary hiding;
  native-Space move acceptance and focus-follows; AX-automation Space
  ops visual-transition cost and reliability (`MCwaitTime` tuning);
  gesture-focus click/warp side effects and consent; parking shell acceptance
  (Mission Control/Dock/Cmd-Tab previews and slivers); tap suppression of
  real system chords; Secure Input/timeout behavior; mixed-scale rounding;
  update-path TCC survival; anti-cheat compatibility (never provable from
  source).
- Risks (gates, not an exhaustive register): parking recovery/stranding,
  fullscreen-game interference, AX/TCC refusal or permission loss, false
  joins, overlay-above-content, Space/display interaction, shortcut
  conflict or stalled taps, signing or update permission resets, Mission
  Control/Cmd-Tab pollution, private-symbol version coupling. Hold the
  workspace release if recovery or shell behavior fails; use the outline
  fallback if the underlay spike fails.

Verification: portable Rust gates in CI; permission, recovery, input and
visual journeys on an authorized physical Mac with owned test windows first.
Measure input/frame-time/CPU/wakeups in real native and borderless games;
neither notarization nor hosted CI proves anti-cheat compatibility. Keep
bounded structured lifecycle logs, no app content, titles or raw key events.

## Sources (local pins plus retained references)

- Pinned local checkouts table above (SHAs + file:line ranges inline).
- [macOS prior art](prior-art.md) (upstream routes, keyboard mechanisms,
  licenses, gaps) and [general prior art](../../research/prior-art.md)
   (historical upstream mechanism summaries).
- [Setup runbook](../../macos-dev-environment.md) (host, signing, TCC,
   quarantine, CI smoke job). Its public-only default sentence predates the
   user's tier-2 selection; this plan and the recorded decision supersede it.
- [Decisions](../../decisions.md) (Windows 2026-09-30/10-01/10-02/10-03
  selections; tier-2 selection; cross-platform conflict-preset and gaming
  rules).
- [Windows plan](../windows-port/plan.md) (mirrored structure).
- Apple references retained from the prior revision:
- https://support.apple.com/guide/mac-help/mchlef287e5d/mac (tile windows)
- https://support.apple.com/guide/mac-help/mchl9674d0b0/mac (tiling icons)
- https://support.apple.com/guide/mac-help/mh14112/mac (Spaces)
- https://support.apple.com/guide/mac-help/change-desktop-dock-settings-mchlp1119/mac (Desktop/Dock)
- https://support.apple.com/en-us/102650 (system shortcuts)
- https://support.apple.com/en-us/105118 (Game Mode)
- https://developer.apple.com/documentation/appkit/nswindow/order(_:relativeto:).md
- https://developer.apple.com/documentation/appkit/nswindow/windownumber.md
- https://developer.apple.com/documentation/security/app-sandbox.md
- https://developer.apple.com/app-store/review/guidelines/ (2.4.5/2.5.1/2.5.8)
- https://developer.apple.com/documentation/security/customizing-the-notarization-workflow.md
- https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution.md
- https://developer.apple.com/documentation/security/hardened-runtime.md

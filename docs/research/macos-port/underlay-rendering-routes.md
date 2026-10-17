# macOS underlay and border rendering routes

Offline research report commissioned by the user (2026-10-17) for the macOS group
underlay and active border. It ranks the rendering routes, prior art and
discriminating experiments. Claims marked as inference are unverified; the
experiments, not the report, establish behaviour. JankyBorders is GPL-3.0:
learn from its approach, do not copy its code.

## 1. Executive Summary & Recommendation

Under SIP-enabled macOS 15 (Sequoia) and macOS 26 (Tahoe) without process/Dock injection, **AppKit-managed `NSWindow`/`NSPanel` instances cannot reliably be ordered relative to foreign windows within the same `CGWindowLevel**`, nor do they obey raw `SLSOrderWindow` calls on `SLSMainConnectionID()` because AppKit's internal window management bridge (`SLSWindowManagementBridge` / `NSCGSWindow`) intercepts, caches, and overrides raw SkyLight ordering state.

To satisfy both deliverables while minimizing unsafe Rust FFI surface and avoiding new TCC permissions:

* **For the Group Underlay (Recommended: Route 1A — Direct SkyLight Window Ordered Below Bottom-Most Group Member):**
Create a window directly via SkyLight (`SLSNewWindow` or `SLSNewWindowWithOpaqueShape`) on `SLSMainConnectionID()`, match its `CGWindowLevel` and sub-level (`SLSSetWindowLevel`, `SLSSetWindowSubLevel`) to the group's members, and order it immediately below (`mode = -1` / `kCGSOrderBelow`) the **lowest-stacked member window** in the tiled group (determined via `CGWindowListCopyWindowInfo` or `SLSWindowIterator` z-order) using `SLSTransactionOrderWindow` (or `SLSOrderWindow`). Because all tiled group members sit above the lowest member and any non-member window underneath the group sits below that lowest member, placing the underlay immediately below the bottom-most member places it below every member and above every occluded non-member.
* **For the Active Window Border (Recommended: Keep Current AppKit Above-Window Panel OR Route 1A Below-Target SkyLight Window with Outer-Plus-Inner Bleed):**
* **Option 1 (Lowest risk, minimum FFI):** Retain the existing click-through `NSPanel` (or use a direct SkyLight window ordered above the target) with an inner cutout. In macOS 15 and macOS 26, even JankyBorders' `order=above` and `order=below` modes encounter trade-offs with window shadows, native window snapping (`JankyBorders#163`), and Sequoia native tiling (`JankyBorders#115`).
* **Option 2 (Platform parity with KDE/Windows — below-window):** Use the same direct SkyLight window mechanism as the underlay, ordered immediately below (`-1`) the focused foreign `CGWindowID`, drawing a hollow stroked path whose inner edge bleeds 2–4 points underneath the target window's rounded corners so there is no gap between the target's corner curve and the border.


* **TCC & Entitlements:** Neither route introduces new TCC prompts beyond the Accessibility (`AXIsProcessTrusted`) and `CGEventTap` permissions OmniTiler already holds. `CGWindowListCopyWindowInfo` without `kCGWindowListOptionOnScreenOnly` title/image capture reads window numbers, bounds, PIDs, and layers without requiring Screen Recording TCC permission.

---

## 2. Plausible Rendering Routes & Ranking

### Detailed Route Mechanics (Question 1)

#### Route 1: Direct SkyLight-Created Windows (`SLSNewWindow` + `SLSOrderWindow` / `SLSTransaction*`)

* **Creation & Surface:**
1. Obtain connection: `cid = SLSMainConnectionID()`.
2. Build a `CGSRegionObj` (`CFTypeRef`) for the window frame via `CGSNewRegionWithRect(&rect, &frame_region)`. For click-through at the WindowServer hit-test layer, pass an empty region (`CGRegionCreateEmptyRegion()`) as the opaque/input shape or configure window tags.
3. Call `SLSNewWindow(cid, kCGSBackingBuffered /* 2 */, x, y, frame_region, &wid)` or `SLSNewWindowWithOpaqueShape(cid, 2, frame_region, empty_region, options /* 13 */, &tags, x, y, 64, &wid, NULL)`.
4. Disable window shadow (`CFNumber` `0` or tag bit `kCGSDisableShadowTagBit = 1 << 3`) via `SLSSetWindowProperty` / `SLSSetWindowTags(cid, wid, &set_tags, 64)` and set `SLSSetWindowOpacity(cid, wid, false)`.
5. Create a CoreGraphics drawing context backed by WindowServer via `SLWindowContextCreate(cid, wid, options)`, draw the RGBA rectangle or stroked rounded rect with `CGContextFillRect` / `CGContextAddPath`, and call `SLSFlushWindowContentRegion(cid, wid, NULL)` (or `CGContextFlush(ctx)`).


* **Tags (`SLSSetWindowTags` / `SLSClearWindowTags`):**
*Note: Exact bit positions are undocumented by Apple and reverse-engineered across `yabai` (`src/misc/extern.h`), `SketchyBar` (`src/window.c`), and `JankyBorders` (`src/border.c` / `src/extern.h`).*
* Ignore mouse events / click-through: `kCGSIgnoreMouseEventsTagBit` (commonly bit `9`, `0x200`, or set via `SLSSetMouseEventEnableFlags` / input region).
* Exclude from Expose / Mission Control / Window Cycling: `kCGSIgnoreForExposeTagBit` (bit `7`, `0x80` or `1 << 11` / `1 << 47` depending on OS version; see `JankyBorders#115`, `JankyBorders#178`, `JankyBorders#209` for regressions in macOS 15, 26, and 27).
* Sticky across Spaces vs per-Space: `kCGSStickyTagBit` (`1 << 11` / `0x800`) and `kCGSSuperStickyTagBit` (`SketchyBar/src/window.c` line 31 in commit `61eacae` explicitly clears `kCGSSuperStickyTagBit` so the window hides in native fullscreen spaces). Better alternative for Space association: attach the SkyLight window to the target window's Space ID via `SLSMoveWindowsToManagedSpace(cid, window_list_cfarray, space_id)`.


* **Level, Sub-Level, and Relative Z-Order:**
1. Query target window's level (`SLSGetWindowLevel(cid, target_wid, &level)`) and sub-level (`SLSGetWindowSubLevel(cid, target_wid)`).
2. Set carrier window's level and sub-level to match: `SLSSetWindowLevel(cid, carrier_wid, level)` and `SLSSetWindowSubLevel(cid, carrier_wid, sub_level)`.
3. Order relative to `target_wid`: either directly via `SLSOrderWindow(cid, carrier_wid, -1 /* below */ or 1 /* above */, target_wid)` or inside an atomic SkyLight transaction:
`tx = SLSTransactionCreate(cid)` -> `SLSTransactionSetWindowLevel(...)` -> `SLSTransactionSetWindowSubLevel(...)` -> `SLSTransactionOrderWindow(tx, carrier_wid, -1, target_wid)` -> `SLSTransactionCommit(tx, 0)`.


* **Keeping Order When Target or Group Member Is Raised:**
* Subscribe to `SLPS` / `NSWorkspace` / AX focus changes and SkyLight connection notifications (`SLSRegisterConnectionNotifyProc` on codes `806` `kCGSWindowDidMove`, `807` `kCGSWindowDidResize`, `808` `kCGSWindowDidChangeOrder` / frontmost changes).
* **For Group Underlay:** Whenever any window on the active Space changes z-order (`808`) or focus changes, query the on-screen window list (`CGWindowListCopyWindowInfo` with `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements`, which returns windows in front-to-back global stacking order). Find the **last** (highest-index / bottom-most) `CGWindowID` belonging to the tiled group, and re-issue `SLSOrderWindow(cid, underlay_wid, -1, bottom_member_wid)`.



#### Route 2: AppKit `NSWindow` / `NSPanel` Re-levelled, Re-tagged, or Re-parented via SkyLight

* **Mechanics:** Create a borderless, non-activating `NSPanel` (`NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel`), grab its `[panel windowNumber]` (`CGWindowID`), and call `SLSSetWindowTags`, `SLSSetWindowSubLevel`, or `SLSOrderWindow`.
* **Why Re-parenting/Ordering Fails:**
* `[NSWindow addChildWindow:ordered:]` requires an `NSWindow*` object in the same process; it cannot take a foreign process's `CGWindowID`.
* Private `CGSAddActivationRegion` / `SLSSetWindowParent` / `SLSTransactionAddWindowToWindowOrderingGroup` (or `SLSMoveWindowWithGroup`): I cannot verify from authoritative sources whether `SLSTransactionAddWindowToWindowOrderingGroup` permits attaching an uninjected process's window to a foreign process's window without owning the parent window's `CGSConnectionID`. Yabai uses `SLSTransactionOrderWindowGroup` inside its `osax/payload.m` injected into `Dock.app` (which holds the universal window manager connection privilege).
* Furthermore, as detailed in Section 3, AppKit's internal window bridge overrides out-of-band `SLSOrderWindow` calls on `NSWindow` instances.



#### Route 3: Above-Target Overlay with Geometric Cut-Out (Even-Odd / Winding Rule Clip)

* **Mechanics:** Use either the existing click-through `NSPanel` ordered `NSWindowAbove` (or at `NSFloatingWindowLevel` / `kCGFloatingWindowLevel = 3`) or a direct SkyLight window ordered above the target.
* **For Border:** Render only the 3-point band outside the target window's frame (with inner rounded-rect clip matching the target's corner radius).
* **For Group Underlay:** Compute the union rectangle of the tiled group (expanded by gap + border width) and subtract each member window's rectangle (using `CGPath` with `kCGPathEOFill` even-odd winding rule, or clipping out the member rects with `kCGBlendModeClear`).


* **Failure Modes:**
1. **Non-member floating/overlapping window:** If an unmanaged/floating window (e.g., Finder dialog, Picture-in-Picture, Spotlight, or a floating app window) overlaps the tiled group, an underlay at `NSFloatingWindowLevel` will draw grid gap bars **on top of** the overlapping floating window. Even if kept at `kCGNormalWindowLevel` ordered above the frontmost group member, if a non-member window sits *between* two group members in z-order, an above-all-members cut-out will draw gap fill over that non-member window.
2. **Rounded corners:** On macOS 15 and macOS 26, standard windows have continuous-curvature (squircle) rounded corners (radius typically ~10–12 pt on macOS 14/15; altered in macOS 26 Tahoe's Liquid Glass design). A rectangular cut-out leaves the 4 corner triangles of the target window uncovered (exposing whatever is behind the target window's corners), whereas a rounded cut-out requires knowing each target window's exact corner radius (which varies between standard titlebar windows, borderless windows, fullscreen/tiled states, and macOS 15 vs 26; see `Paneru` release notes on autodetecting corner radius and `JankyBorders#140`, `#194`).
3. **Shadows:** An above-target cutout clips the underlay around the member frames, which paints the translucent `#662a82da` fill **on top of** the member windows' drop shadows in the gaps, washing out or tinting the native shadows rather than sitting beneath them.



#### Route 4: Window Levels (`CGWindowLevel`) or Sub-Levels (`SLSSetWindowSubLevel`) Just Below the Target

* **Coarse `CGWindowLevel` (`kCGNormalWindowLevel - 1` or `kCGDesktopIconWindowLevel + 1`):**
* Standard application windows live at `kCGNormalWindowLevel` (`0`).
* Setting an overlay/underlay to level `-1` places it below **all** normal-level windows system-wide.
* **Failure mode:** It works only when no non-member normal-level window sits underneath the tiled group. If a user has a browser or terminal window behind the tiled group on the same Space, a level `-1` underlay is hidden behind that background non-member window, violating the requirement to sit *above* non-member windows underneath the group.


* **SkyLight Sub-Levels (`SLSSetWindowSubLevel(cid, wid, sub_level)`):**
* WindowServer maintains a signed integer sub-level within each `CGWindowLevel` (default `0` for normal windows).
* Setting `sub_level = -1` at `level = 0` places the carrier below all normal windows with `sub_level == 0`, which has the **exact same failure mode** as `level = -1` (it sinks below non-member normal windows that have `sub_level == 0`).
* Calling `SLSSetWindowSubLevel` on *foreign* windows (e.g., setting group members to `sub_level = 1`, underlay to `0`, and non-members to `-1`) does not work reliably from an uninjected process: `karinushka/paneru` documented in Discussion `#86` (Feb 2026) that attempting to control foreign windows' levels/sublevels via `SLSSetWindowLevel` and `SLSSetWindowSubLevel` without Dock injection failed to keep windows stacked as intended.



#### Route 5: CoreAnimation (`CAContext`) or `CAMetalLayer` Bound to a SkyLight Window Surface

* **Mechanics:**
* Instead of CPU-bound `SLWindowContextCreate` (`CGContextRef`), create a direct SkyLight window (`SLSNewWindow`) and bind a GPU-accelerated `CAContext` or `CAWindowServerDisplay` surface, or attach a `CAMetalLayer` / `IOSurface` via `SLSSetWindowAutofill` / `CGSBindSurface` / `[CAContext contextWithCGSConnection:cid options:...]`.
* Specifically, private QuartzCore API `[CAContext contextWithCGSConnection:cid options:...]` creates a `CAContext` whose root `CALayer` (including `CAShapeLayer` or `CAMetalLayer`) can be bound to a `CGSWindowID` via `SLSSetWindowProperty(cid, wid, CFSTR("CAContext"), context.contextId)` or `CGSWindowSetBackdrop`.


* **Trade-offs:**
* Eliminates CPU rasterization when resizing large fullscreen-sized group underlays (where `SLWindowContextCreate` allocates and flushes a full bounding-box bitmap buffer—see `JankyBorders#188` regarding high GPU/VRAM usage on Tahoe when large 2D backing stores are reallocated during resize).
* For a solid `#662a82da` rectangle (underlay) or a simple 3-point rounded border, a `1x1` stretched surface or `SLWindowContextCreate` at `1x` scale (for the unrounded underlay) already uses negligible memory and CPU, making private `CAContext` FFI unnecessary unless animating at 120 Hz.



---

### Route Comparison & Ranking Table (Question 4)

#### Table A: Group Underlay Routes (Ranked Best to Worst)

| Rank & Route | Below-Member & Above-Non-Member Correctness | Private API Surface & Version-Coupling Risk | Tracking Latency (Move/Resize) | Idle CPU/GPU Cost | Game & Fullscreen Safety | Implementation Complexity (Rust) | TCC / Entitlements Required |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **#1: Route 1A** — Direct `SLSNewWindow` + `SLSOrderWindow` (`-1`, lowest member `wid`) + `SLWindowContextCreate` | **Exact.** Sits immediately below bottom-most group member and above any lower non-member window at `kCGNormalWindowLevel`. | **Moderate.** Uses ~8 SkyLight functions (`SLSNewWindow`, `SLSReleaseWindow`, `SLSOrderWindow`, `SLSSetWindowLevel`, `SLSSetWindowSubLevel`, `SLSSetWindowTags`, `SLSSetWindowShape`, `SLWindowContextCreate`). Stable across macOS 14–26 with tag adjustments. | **Low (~1–4 ms).** `SLSSetWindowShape` + `SLSOrderWindow` in a single `SLSTransaction` commits directly to WindowServer. | **Zero.** Static WindowServer backing buffer when idle. (Use `1x` non-HiDPI buffer for sharp unrounded rect to save VRAM). | **Safe** if `kCGSSuperStickyTagBit` is cleared and window is moved to target's `CGSSpaceID` (never appears on fullscreen space). | **Medium.** Requires ~150 lines of FFI declarations in the single unsafe module + CoreGraphics drawing. | **None new** (uses existing AX + EventTap permissions). |
| **#2: Route 5** — Direct `SLSNewWindow` + Private `CAContext` / `CAMetalLayer` | **Exact** (same SkyLight window ordering as Route 1A). | **High.** Adds undocumented `QuartzCore` `CAContext` private selectors on top of SkyLight window lifecycle. | **Lowest (<1 ms).** GPU-composited `CAShapeLayer` / Metal drawable resize without CPU bitmap clear. | **Zero.** | **Safe** (same Space/tag handling as Route 1A). | **High.** Requires Objective-C private `CAContext` bindings in `objc2` plus SkyLight FFI. | **None new.** |
| **#3: Route 4** — AppKit `NSPanel` at `level = kCGNormalWindowLevel - 1` (or `subLevel = -1`) | **Fails when non-members are behind group.** Sinks below *all* normal-level windows, hiding behind any non-member window under the group. | **None (Public AppKit)** if using `level = -1`; **Low** if using `SLSSetWindowSubLevel`. | **Low.** Native `NSPanel` `setFrame:display:`. | **Zero.** | **Safe** via `NSWindowCollectionBehaviorFullScreenNone` (or omitting `FullScreenAuxiliary`). | **Trivial.** Pure `objc2-app-kit`. | **None new.** |
| **#4: Route 3** — Above-Group `NSPanel` with Even-Odd Cutout of Member Rects | **Fails in 3 cases:** (1) Paints over non-members overlapping the group; (2) Exposes/clips rounded corners inaccurately; (3) Tints member drop shadows. | **None (Public AppKit).** | **Medium.** Cutout path must re-rasterize on every pixel of member move, causing visible gap/edge jitter during live drag. | **Zero.** | **Risky** if placed at `NSFloatingWindowLevel`; safe at `NSNormalWindowLevel` (though ordering above all members via AppKit fails per Q3). | **Low-Medium.** Requires syncing exact member rects and corner radii. | **None new.** |
| **#5: Route 2** — AppKit `NSPanel` ordered via `NSWindowBelow` or `SLSOrderWindow` | **Broken.** AppKit ignores/overrides cross-process `NSWindowBelow` and raw `SLSOrderWindow` (see Section 3). | **Low.** | **N/A (Does not order properly).** | **Zero.** | **Safe.** | **Low.** | **None new.** |

#### Table B: Active Window Border Routes (Ranked Best to Worst)

| Rank & Route | Visual & Z-Order Fidelity | Private API Surface & Risk | Tracking Latency | Idle Cost | Game & Fullscreen Safety | Implementation Complexity | TCC / Entitlements |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **#1: Route 1A (Below-Target SkyLight Window)** — `SLSNewWindow` ordered `-1` relative to target `wid` with inner bleed | **Exact parity with KDE/Windows.** Sits immediately below target window and above all windows behind target. Target's own content masks inner bleed; shadow stays intact. | **Moderate.** Same ~8 SLS functions as Underlay Route 1A. Can share the exact same Rust `SkyLightSurface` abstraction. | **Low.** Even if border lags by 1 frame during rapid drag, target window masks the inner edge (unlike an above-window border that bisects the window). | **Zero.** | **Safe** when Space-bound and hidden on fullscreen Space transitions. | **Medium** (shared with Underlay Route 1A). | **None new.** |
| **#2: Route 1B (Above-Target SkyLight Window)** — `SLSNewWindow` ordered `+1` relative to target `wid` | **Above target** (hides system 1px rim border, but overlaps window during fast drag and can obscure edge UI in small popups—see `JankyBorders#186`). | **Moderate.** Same SLS surface as Route 1A. | **Noticeable on fast drag** (border is drawn over window content if it trails by 1 frame). | **Zero.** | **Safe** with Space-binding. | **Medium.** | **None new.** |
| **#3: Current Shipped Route (AppKit `NSPanel` Above Target)** — `NSPanel` at `NSFloatingWindowLevel` or `NSNormalWindowLevel` | **Above target.** Works reliably at `NSFloatingWindowLevel` (except covers floating palettes/overlays overlapping the focused window); at `NSNormalWindowLevel` ordering relative to foreign `wid` is unreliable. | **Minimal / Public AppKit.** | **Low-Medium** (AppKit main-thread layout pass + CA commit). | **Zero.** | Must explicitly hide when target enters fullscreen or game mode. | **Already implemented.** | **None new.** |

---

## 3. Why the AppKit `NSWindow` Ignored `orderWindow:relativeTo:` and `SLSOrderWindow` (Question 3)

Both observations from your live testing have concrete architectural causes in AppKit and SkyLight:

1. **Why `[NSWindow orderWindow:NSWindowBelow relativeTo:foreignWindowNumber]` failed to place the carrier below all group members:**
* **Established Fact (AppKit Architecture):** `-[NSWindow orderWindow:relativeTo:]` calls into `-[NSWindow _reallyDoOrderWindow:relativeTo:findKey:forCounter:force:isModal:]`. AppKit is designed to manage the window list of **its own process** (`NSApplication.sharedApplication.windows`) and window groups (`_NSWindowGroup`).
* **Inference supported by stack traces and WindowServer behavior:** When `otherWin` (`relativeTo:`) is a window number belonging to a **foreign process**, AppKit cannot look up an `NSWindow*` instance for `otherWin` in its internal window table. In modern macOS (macOS 13+ after the introduction of `SLSWindowManagementBridge` and Stage Manager), AppKit routes window ordering through its WindowManagementBridge delegate rather than blindly issuing `CGSOrderWindow(cid, self.windowNumber, mode, otherWin)`. Furthermore, even when `orderWindow:relativeTo:` succeeds in placing the window relative to *one* foreign window at the moment of the call, activating or raising any foreign application triggers WindowServer's application-activation restacking (`_SLPSSetFrontProcessWithOptions` / `CPS`), which brings the active process's windows forward or re-evaluates process-level window groups without keeping a third-party process's `NSWindow` pinned below a specific foreign window.


2. **Why calling `SLSOrderWindow` directly on an AppKit-owned `NSWindow` returned `kCGErrorSuccess` (`0`) without changing global z-order:**
* **Established Fact (`SLSWindowManagementBridge`):** In macOS 13 (Ventura), macOS 14 (Sonoma), macOS 15 (Sequoia), and macOS 26 (Tahoe), initializing AppKit (`NSApplicationLoad` / `[NSApplication sharedApplication]`) registers an internal `SLSWindowManagementBridge` delegate on the main SkyLight connection.
* In `FelixKratz/JankyBorders#62`, `koekeishiya` (author of `yabai`) and `FelixKratz` (author of `JankyBorders`) explicitly documented that `SLSWindowManagementBridge` intercepts and controls window ordering for windows managed through the bridge; when `SLSWindowManagementBridgeSetDelegate(NULL)` was tested in `yabai` to stop `NSWindow` animation flicker, bridged `SLSOrderWindow` calls stopped taking effect as expected.
* Additionally, every `NSWindow` is wrapped by an AppKit `NSCGSWindow` (`_NSCGSWindow`) which participates in AppKit's display cycle (`_NSWindowTransformAnimation`, `CA::Transaction`). When you mutate an `NSWindow`'s z-order behind AppKit's back via raw `SLSOrderWindow(cid, nsWindow.windowNumber, -1, foreign_wid)`, AppKit's cached `_order` / level state in `NSCGSWindow` disagrees with your call, or the WindowManagementBridge rejects/reverts out-of-band relative ordering against a non-bridged foreign `CGWindowID`.
* **Why a window created via `SLSNewWindow` works where `NSWindow` fails:** A window created directly via `SLSNewWindow(cid, ...)` has **no** `NSWindow` or `NSCGSWindow` wrapper in AppKit. AppKit does not know the `CGWindowID` exists, never touches its level, sub-level, or ordering in `CA::Transaction` commits, and `SLSOrderWindow` / `SLSTransactionOrderWindow` talks directly to WindowServer's window table.


3. **Why your `SLSTransaction`-based order returned an error:**
* **Inference (as exact call signature used in OmniTiler was not provided):** There are two common causes for `SLSTransactionOrderWindow` / `SLSTransactionCommit` returning an error on macOS 14–26:
1. Calling `SLSTransactionOrderWindowGroup` instead of `SLSTransactionOrderWindow` on a window that is not a leader of a `CGSWindowOrderingGroup` or across foreign connections without Dock privileges.
2. Wrong FFI signature for `SLSTransactionOrderWindow` or `SLSTransactionCommit`:
* `extern CFTypeRef SLSTransactionCreate(int cid);`
* `extern CGError SLSTransactionOrderWindow(CFTypeRef transaction, uint32_t wid, int order, uint32_t rel_wid);`
* `extern CGError SLSTransactionCommit(CFTypeRef transaction, int synchronous);` (Note: passing an invalid `cid` to `SLSTransactionCreate`, or passing an `NSWindow` whose transaction state is already locked by AppKit's own `CATransaction`/`SLSTransaction`, causes commit failure).







---

## 4. Per-Project Prior-Art Findings (Question 2)

### 1. JankyBorders (`FelixKratz/JankyBorders`)

* **Files & Commits:** `src/border.c`, `src/windows.c`, `src/events.c`, `src/extern.h` (v1.8.4 `af27913` for macOS 26 compatibility; v1.9.0 `a7297ca` May 2026). *(Note: GPL-3.0 licensed; architectural sequence described below without code reproduction.)*
* **Window Creation & Rendering APIs:**
* Creates one SkyLight window per tracked foreign window using `SLSNewWindowWithOpaqueShape` (or `SLSNewWindow` with backing type `2`, passing a frame `CGSRegionObj` created via `CGSNewRegionWithRect` and an empty opaque region via `CGRegionCreateEmptyRegion`).
* Creates a CoreGraphics context via `SLWindowContextCreate(cid, border_wid, NULL)` and draws a stroked rounded path (`CGContextAddPath`, `CGContextStrokePath` / `CGContextDrawLinearGradient`) inside a transaction.
* Uses `SLSSetWindowTags` to disable shadow, ignore mouse events, and exclude from window cycling (`#178` in v1.9.0 `a7297ca`).
* Formerly supported background blur via `CGSNewWindowBackdrop` / `SLSSetWindowBackgroundBlurRadius`, which was **removed in v1.7.0 (`37f9135`, Nov 2024)** due to severe performance degradation on macOS 15 Sequoia.


* **Ordering & Re-ordering on Focus/Raise:**
* Queries the target window's level (`SLSGetWindowLevel`) and sub-level (`SLSGetWindowSubLevel`), applies both to `border_wid` via `SLSSetWindowLevel` and `SLSSetWindowSubLevel`, and orders `border_wid` relative to `target_wid` via `SLSOrderWindow(cid, border_wid, order_mode, target_wid)` (or `SLSTransactionOrderWindow`), where `order_mode` is `-1` (`order=below`, default) or `1` (`order=above`).
* On focus change or window order change, re-applies level, sub-level, and `SLSOrderWindow`.


* **Movement Tracking & Event Sources:**
* Does **not** use the Accessibility (`AXObserver`) API at all.
* Registers raw SkyLight connection callbacks via `SLSRegisterConnectionNotifyProc(cid, callback, event_code, context)` for window move (`806`), resize (`807`), order change (`808`), window created/destroyed (`1325`/`1326` or space/window list notifications), and `NSWorkspace` active space/app notifications (`src/events.c`).
* Uses `CVDisplayLink` / coalesced timers (v1.7.0 `37f9135`) and threaded rendering to throttle redraws during live resize.


* **Spaces & Fullscreen:**
* Attaches each border window to the target window's Space via `SLSMoveWindowsToManagedSpace` and hides the border when the target window enters native fullscreen or minimizes.


* **Supported Versions & Known Breakages (macOS 14 / 15 / 26):**
* Targets macOS 14.0+.
* **macOS 15 Sequoia breakages:** Built-in macOS Sequoia window tiling/snapping attempted to tile the border windows themselves (`JankyBorders#115`, `#163`); system logout hung to a black screen if `borders` had run (`#118`, fixed in v1.8.4 `af27913`); backdrop blur caused severe lag (removed in v1.7.0).
* **macOS 26 Tahoe breakages:** Required compatibility update in v1.8.4 (`af27913`, `#160`); high GPU usage reported during resizes on Tahoe (`#188`); stability issues reported on macOS 26.5 (`#199`, `#200`); with `order=above`, borders obscured Apple Pay / App Store "Get" buttons (`#186`); with `order=below`, square or thin borders can be partially occluded by the window during drag (`#140`, `#180`).



### 2. yabai's Former In-Tree Borders (`asmvik/yabai`, formerly `koekeishiya/yabai`)

* **Files & Commits:** `src/border.c` (removed in commit `5dd912f`, Issue `#1889`, Oct 10, 2023, prior to yabai v6.0.0).
* **Window Creation & Rendering APIs:**
* Originally used `SLSNewWindow` + `SLWindowContextCreate` + `SLSSetWindowTags` + `SLSOrderWindow`.
* Later versions (macOS 12/13) required the Scripting Addition (`src/osax/payload.m` injected into `Dock.app` with SIP partially disabled) to order and group borders reliably with target windows (`SLSTransactionOrderWindowGroup` / `SLSMoveWindowWithGroup`).


* **Why It Was Removed:**
* In Issue `#1889`, `asmvik` (`koekeishiya`) removed borders entirely because macOS 13 (Ventura) and macOS 14 (Sonoma) broke window ordering, Mission Control / Expose visibility, and frame synchronization when driven via AX + OSAX. `asmvik` officially directed users to `JankyBorders`.



### 3. Hammerspoon (`Hammerspoon/hammerspoon` — `hs.canvas` & `hs.drawing`)

* **Files:** `extensions/canvas/libcanvas.m`, `extensions/canvas/CanvasWindow.m`, `extensions/drawing/internal.m`.
* **APIs & Ordering:**
* Uses standard AppKit `NSPanel` / `NSWindow` subclasses (`HSlarcWindow` / `HSDrawingWindow` / `CanvasWindow`) configured with `NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel`, `[window setIgnoresMouseEvents:YES]`, and `[window setOpaque:NO]`.
* `hs.canvas:level()` sets coarse `CGWindowLevel` via `[window setLevel:]` (`extensions/canvas/libcanvas.m` line 2830).
* `hs.canvas:orderBelow([canvas2])` (`extensions/canvas/libcanvas.m` line 3023) calls `[window orderWindow:NSWindowBelow relativeTo:winNum]`. However, Hammerspoon's API only accepts **another `hs.canvas` object** (or `nil` for all windows at the same level) — it does not support maintaining relative ordering beneath a specific foreign process's window across focus changes.


* **Tracking & Spaces:**
* Driven by Lua scripts using `hs.window.filter` (which wraps `AXObserver` and `NSWorkspace` notifications), resulting in noticeable 15–50 ms lag during window drags (noted by users in `AeroSpace#271`).
* Spaces handled via `NSWindowCollectionBehaviorCanJoinAllSpaces`, `MoveToActiveSpace`, and `Stationary`.



### 4. AeroSpace (`nikitabobko/AeroSpace`)

* **Files & Issues:** `README.md`, `Sources/PrivateApi/`, Issue `#85`, Issue `#271`.
* **Findings:**
* AeroSpace **does not render borders or underlays**. Its project policy explicitly rejects built-in borders/overlays and restricts private API usage to a single function: `_AXUIElementGetWindow`.
* AeroSpace's documentation (`goodies`) recommends running `JankyBorders` as an external process via `after-startup-command`.
* Note on interaction: Because AeroSpace emulates workspaces by moving inactive windows to the bottom-right corner of the screen (leaving a 1 px sliver), `JankyBorders` required specific workarounds (`JankyBorders#181`) so borders do not cluster in the bottom corner.



### 5. Amethyst (`ianyh/Amethyst`)

* **Files & Issues:** Issues `#187`, `#1411`, `#1162`, `#1803`.
* **Findings:**
* Amethyst **does not implement active window borders or group underlays** (feature requests `#187` and `#1411` remain unimplemented).
* Its only overlay is the **Layout Name HUD** (`LayoutNameWindow` / `NSPanel`), a centered transient floating `NSPanel` (`NSFloatingWindowLevel`, `ignoresMouseEvents = true`) that fades out after layout switches.



### 6. Rectangle (`rxhanson/Rectangle`)

* **Files:** `Rectangle/FootprintWindow.swift`, `TerminalCommands.md`.
* **APIs & Ordering:**
* Renders a drag-to-snap preview ("footprint") using `FootprintWindow`, a subclass of `NSWindow` (`styleMask: .borderless`, `isOpaque = false`, `ignoresMouseEvents = true`, `level = .floating` or `.statusBar`, `collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary]`).
* Supports either a translucent filled/bordered `NSBox`/`NSView` or an `NSVisualEffectView` (`Blur footprint`).
* Ordered **above** normal windows via `level = .floating` (or ordered front when drag enters a snap zone). It never orders below the dragged foreign window.



### 7. Loop (`MrKai77/Loop`)

* **Files:** `Loop/Window Management/Preview/PreviewController.swift`, `PreviewView.swift` (`Issue #1084`, `Issue #88`).
* **APIs & Ordering:**
* Uses an AppKit `NSPanel` (`PreviewController`) hosting a SwiftUI `PreviewView` with `NSVisualEffectView` blur and rounded stroked border.
* Configured as a non-activating, click-through floating panel (`NSPanel` at `.floating` / `.statusBar` level).
* Animates frame changes by moving/resizing the `NSPanel` directly (`setFrame`) while the user holds the trigger key or radial menu; hides upon release when the target window is resized via AX. It does not render persistent below-window underlays.



### 8. SketchyBar (`FelixKratz/SketchyBar`)

* **Files:** `src/window.c`, `src/bar.c` (`Issue #383`, commit `61eacae`).
* **APIs & Ordering:**
* Same author (`FelixKratz`) and identical low-level SkyLight window engine as `JankyBorders`.
* Creates windows directly via `SLSNewWindow` on `SLSMainConnectionID()`, draws via `SLWindowContextCreate`, moves/resizes via `SLSSetWindowShape` / `SLSMoveWindow`, and orders via `SLSOrderWindow`.
* Uses `SLSSetWindowTags` with `kCGSStickyTagBit` and `kCGSIgnoreForExposeTagBit`, and explicitly clears `kCGSSuperStickyTagBit` (`src/window.c` line 31) so the bar automatically hides in native fullscreen Spaces.



### 9. Rift (`acsandmann/rift`)

* **Files & Docs:** `acsandmann/rift` (Rust, forked from `glide-wm`), `Wiki/Config` (`[settings.ui.stack_line]`).
* **Findings:**
* Rift does not implement a general active-window border or group underlay; users pair it with `JankyBorders` or `SketchyBar` via its Mach IPC interface.
* Rift includes an experimental **stack-line indicator** (`[settings.ui.stack_line]`, `thickness`, `horiz_placement`, `vert_placement`, `spacing`) and a Mission Control overview mode. I cannot verify the exact internal file path of Rift's `stack_line` window creation from public search results alone, though Rift uses `objc2` AppKit windows and private SkyLight APIs.



### 10. Paneru (`karinushka/paneru`)

* **Files & Releases:** `src/platform/`, `src/manager/windows.rs`, `CONFIGURATION.md`, Discussion `#86`, Discussion `#129` (v0.3.6), Releases through v0.4.4 (Oct 2026).
* **APIs & Ordering:**
* Written in Rust using `objc2` (`NSWindow`, `NSView`), CoreGraphics, and private SkyLight calls.
* Implements two overlay features added in v0.3.6+: **Active Window Border** (`[border]`) and **Inactive Window Overlay Dimming** (`[dim]`).
* Also supports native macOS inactive window dimming via SkyLight (`SLSSetDictionaryForCurrentConnection` / window alpha/dimming APIs).
* **Known Issues Documented by Author:** In `CONFIGURATION.md` and Discussion `#86`, the author explicitly notes that the overlay/border features rely on undocumented WindowServer APIs and have known z-order issues (for example, dimming overlays covering Picture-in-Picture windows, and `SLSSetWindowLevel` / `SLSSetWindowSubLevel` failing to keep floating windows reliably layered without Dock injection). Paneru also had to add explicit fixes to hide borders in Mission Control and fullscreen (`v0.3.x`–`v0.4.x` release notes).



### 11. Glide (`tmandry/glide` / `glide-wm/glide`)

* **Files:** `Cargo.toml`, `src/` (`Issue #64`).
* **Findings:**
* Written in Rust using `objc2` (`NSWindow`, `NSView`).
* Glide uses AppKit `NSWindow` for its status/group indicators (for stacked/tabbed containers) at floating/overlay levels, and avoids fragile private SkyLight window-creation APIs where possible (noting in `Issue #64` that private SkyLight behavior is macOS-version-sensitive). It does not implement a below-window group underlay.



### 12. komorebi-for-mac (`LGUG2Z/komorebi-for-mac`)

* **Files & Docs:** `LGUG2Z/komorebi-for-mac` (v0.1.0 Mar 2026, v0.1.1 May 2026), `komorebi.json (macOS)` reference.
* **Findings:**
* Publicly released in 2026 under the source-available Komorebi 2.0.0 License; ongoing nightly development occurs in the private `KomoCorp/komorebi-for-mac` repository.
* Its configuration schema (`komorebi.json (macOS)`) includes `Borders` settings. Because active development lives in the private sponsor repo (`KomoCorp/komorebi-for-mac`), I cannot verify the exact internal Rust/Objective-C file path or whether its border renderer uses `NSPanel` or `SLSNewWindow`.



### 13. FlashSpace (`wojciech-kulik/FlashSpace`)

* **Files & Issues:** `SpaceControl`, `Issue #556`, `Issue #606`.
* **Findings:**
* Written in Swift/SwiftUI using standard Accessibility (`AXUIElement`) and AppKit (`NSWindow`).
* Does not tile windows or draw below-window underlays; it switches workspaces by hiding/showing applications (`NSRunningApplication.hide`/`unhide`).
* Its `SpaceControl` feature is a full-screen SwiftUI/AppKit overlay switcher (`NSWindow`) that takes focus when summoned (`Issue #606`).



### 14. Phoenix (`kasper/phoenix`)

* **Files:** `Phoenix/PHModalWindowController.m`, `Phoenix/PHContext.m`.
* **Findings:**
* Provides `Modal` (`PHModalWindowController`), a standard AppKit `NSWindowController` / `NSPanel` at `NSFloatingWindowLevel` (or custom level) used for HUDs and prompts. Does not implement below-window borders or underlays.



### 15. Swindler (`tmandry/Swindler`)

* **Files:** `Sources/`.
* **Findings:**
* Swindler is purely a Swift state-caching and event-normalization library over `AXUIElement` and `NSWorkspace`. It contains **no** window/overlay rendering code.



---

## 5. Minimal Discriminating Experiments (Question 5)

Each experiment is designed as a minimal self-contained Rust test binary (or subcommand in OmniTiler's single FFI module) that verifies stacking order programmatically using `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID)`, which WindowServer guarantees to return in **global front-to-back z-order**.

### Experiment 1: Direct SkyLight Window (`SLSNewWindow`) Ordered Below Lowest Group Member (Tests Route 1A)

* **Goal:** Prove that a window created directly via `SLSNewWindow` on `SLSMainConnectionID()` can be placed immediately below the lowest-stacked member of a multi-process window group and above a non-member window underneath the group, and remains there when group members are raised.
* **Setup:**
1. Open 3 foreign windows on the current Space: `M1` (e.g., Terminal), `M2` (e.g., Safari), and `BackgroundNonMember` (e.g., Finder, stacked behind `M1` and `M2`).
2. Record their `CGWindowID`s (`wid_m1`, `wid_m2`, `wid_bg`).


* **Execution Sequence:**
1. Call `cid = SLSMainConnectionID()`.
2. Create a region `CGSNewRegionWithRect(&union_rect, &region)` and empty region `CGRegionCreateEmptyRegion()`.
3. Create `underlay_wid` via `SLSNewWindowWithOpaqueShape(cid, 2, region, empty_region, 13, &tags, 0.0, 0.0, 64, &underlay_wid, null)`.
4. Query `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, 0)`. Find the array indices `idx(wid_m1)` and `idx(wid_m2)`. Let `bottom_member_wid = if idx(wid_m1) > idx(wid_m2) { wid_m1 } else { wid_m2 }`.
5. Query `SLSGetWindowLevel(cid, bottom_member_wid, &lvl)` and `sub = SLSGetWindowSubLevel(cid, bottom_member_wid)`.
6. Call `SLSSetWindowLevel(cid, underlay_wid, lvl)`, `SLSSetWindowSubLevel(cid, underlay_wid, sub)`, and `SLSOrderWindow(cid, underlay_wid, -1, bottom_member_wid)`.


* **Readback & Pass/Fail Criterion:**
* Immediately call `CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, 0)` and extract the ordered list of `kCGWindowNumber`s.
* **Pass Criterion 1 (Initial Placement):** `idx(underlay_wid) == max(idx(wid_m1), idx(wid_m2)) + 1` AND `idx(underlay_wid) < idx(wid_bg)`.
* **Pass Criterion 2 (Raise Resilience):** Activate `M2` then `M1` via `_SLPSSetFrontProcessWithOptions` / `AXUIElementPerformAction(kAXRaiseAction)`. In the `SLSRegisterConnectionNotifyProc` callback for event `808` (window order changed), recompute `bottom_member_wid` and call `SLSOrderWindow(cid, underlay_wid, -1, bottom_member_wid)`. Read back `CGWindowListCopyWindowInfo` and assert Criterion 1 still holds with zero frames where `idx(underlay_wid) < max(idx(wid_m1), idx(wid_m2))`.



### Experiment 2: Atomic Transaction Move + Re-order Latency & Error Check (Tests `SLSTransaction*`)

* **Goal:** Determine why your previous transaction call failed and verify whether `SLSTransactionOrderWindow` + `SLSTransactionMoveWindowWithGroup` (or `SLSSetWindowShape`) works on `underlay_wid` (a direct SkyLight window) vs an AppKit `NSWindow`.
* **Execution Sequence:**
1. Run the exact same `tx = SLSTransactionCreate(cid)` -> `SLSTransactionOrderWindow(tx, wid, -1, bottom_member_wid)` -> `SLSTransactionCommit(tx, 0)` on:
* **(Case A)** `[nsPanel windowNumber]` (AppKit-owned window), and
* **(Case B)** `underlay_wid` (created via `SLSNewWindow` in Experiment 1).


2. Log the `CGError` return values of both `SLSTransactionOrderWindow` and `SLSTransactionCommit`, and inspect `CGWindowListCopyWindowInfo` indices before and after.


* **Pass/Fail Criterion:**
* Confirms that Case B succeeds and updates global z-order in `CGWindowListCopyWindowInfo`, isolating the failure in Case A to AppKit's `NSCGSWindow` / `SLSWindowManagementBridge` ownership.



### Experiment 3: macOS 15 / 26 Native Window Snapping & Mission Control Interference Test

* **Goal:** Verify that the SkyLight tags applied to `underlay_wid` and `border_wid` prevent two known regressions reported in `JankyBorders`:
1. macOS 15+ native window tiling/snapping treating the overlay as a tileable window (`JankyBorders#115`, `#163`).
2. Ghost rectangles appearing in Mission Control (`JankyBorders#209`).


* **Execution Sequence:**
1. With `underlay_wid` visible below `M1` and `M2`, drag `M1` to the screen edge (to trigger macOS 15/26 native snap preview) and drag `M1` near `M2` (to test window-to-window edge snapping).
2. Trigger Mission Control (`open -a "Mission Control"`).


* **Pass/Fail Criterion:**
* Native snap previews and window-to-window snapping ignore `underlay_wid`, and `underlay_wid` either hides or does not render as a selectable window tile in Mission Control.



---

## 6. Risks and Open Questions

1. **Non-Member Window Interleaved Between Group Members (Z-Order Interleaving):**
* **Risk:** Suppose `M1` and `M2` are in a tiled group, and a user raises an unmanaged/floating window `F` (at `kCGNormalWindowLevel`), then clicks `M1`. The global z-order from front to back is now `[M1, F, M2]`.
* It is topologically impossible for a single flat underlay window to be *below* both `M1` and `M2` while simultaneously being *above* `F` if `F` is stacked between `M1` and `M2`.
* **Mitigation:** Either (a) when any member of a tiled group is focused/raised, OmniTiler raises all other members of that tiled group (or orders them immediately above non-members via AX / SkyLight) so the group remains contiguous in z-order, or (b) ordering the underlay below the lowest member (`M2`) safely places the underlay below `F` as well (so `F` is never occluded by the underlay).


2. **macOS 15 / 26 Native Window Snapping & Tiling (`JankyBorders#115`, `#163`):**
* **Risk:** WindowServer in macOS 15+ inspects on-screen windows at `kCGNormalWindowLevel` for edge-snapping and native tiling. If `underlay_wid` lacks the proper exclusion tags (`kCGSIgnoreForExposeTagBit` / window cycling ignore bit added in `JankyBorders` PR `#178` / v1.9.0 `a7297ca`), dragging a window can snap to the underlay's bounding box.


3. **VRAM and GPU Scaling for Large Group Underlays (`JankyBorders#188`):**
* **Risk:** A group underlay covering an entire 5K/6K Retina display at `@2x` backing scale allocates ~60–120 MB per backing buffer. Reallocating `SLWindowContextCreate` on every resize step causes high GPU/WindowServer memory churn on macOS 26 Tahoe (`JankyBorders#188`).
* **Mitigation:** Because the group underlay is a solid translucent rectangle (`#662a82da`) with **zero corner radius**, do **not** enable HiDPI (`@2x`) scaling on `underlay_wid`, or only recreate the `CGContext` when the bounding box grows, using `SLSSetWindowShape` + `CGContextFillRect` at `1x` resolution.


4. **System Logout Hang (`JankyBorders#118`):**
* **Risk:** Holding open raw `SLSNewWindow` surfaces or connection notify procs without cleaning them up on `NSWorkspaceWillPowerOffNotification` / `SIGTERM` caused black-screen logout hangs on macOS 15 (`JankyBorders#118`, fixed in v1.8.4 `af27913`). Ensure OmniTiler calls `SLSReleaseWindow(cid, wid)` on shutdown and session termination.

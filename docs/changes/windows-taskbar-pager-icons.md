# Windows taskbar pager miniature icons

## Goal and scope

- Centre an obtainable application icon inside each sufficiently large window
  miniature, preserving the existing pager geometry, labels, selection and
  safety gates. Missing icons leave the existing rectangle unchanged.
- Extend the existing opaque GDI painter, not the overlay hosting model.
- User-requested 2026-10-19 styling: icons at 50% opacity; larger bold workspace
  numbers, followed by centered black outlines instead of offset drop shadows,
  without changing geometry or label centering.
- Active-workspace fill follows the Windows accent independently of border
  theme on/off, falling back to the existing blue when unavailable.
- Classic Win32 icons are in scope. Exact packaged-app artwork is conditional
  on obtainable window/class icons; package/AUMID shell resolution is deferred.

## Approach and bounded units

1. Inspect the current painter and identity/snapshot seams. Accepted finding:
   the painter uses an opaque GDI device surface, not UpdateLayeredWindow.
2. Implement bounded background lookup, identity-keyed cache, centred DPI-aware
   icon sizing and opaque icon composition. Window messages use short
   SendMessageTimeout calls with SMTO_ABORTIFHUNG; no foreign-window query or
   shell extraction occurs in the tile loop or paint callback. Own and release
   copied/extracted icons, DCs and bitmaps explicitly inside minimal sys modules.
3. Verify pure sizing/cache decisions and native compilation, inspect the
   resource/thread boundaries, and update research and product progress records.

## Acceptance and verification

- Obtain icons through window messages, class icons and executable fallback;
  missing, hung or inaccessible clients never prevent pager/tiler progress.
- Cache by full window lifetime identity, prune closed windows, refresh changed
  icons periodically, and suspend queued lookup while the pager is suppressed.
- Preserve miniature borders; omit icons that cannot fit legibly. Paint must
  not access foreign windows or wait for icon discovery.
- Native locked four-package build/test/strict all-target clippy, workspace
  format check and diff check. Inspect portable cfg boundaries separately.
- Icon/number appearance and centered halo physically user-accepted 2026-10-19.
  Wider alpha-edge/icon-refresh/DPI variants and native resource
  stability retain their separate acceptance scope below.

## Material decisions and current outcome

- Use the existing opaque miniature background when composing icon pixels.
  Premultiplied alpha for UpdateLayeredWindow is not applicable to this painter;
  changing to layered hosting is out of scope.
- Styling delivered offline 2026-10-19: after GDI composes source-alpha artwork
  over miniature gray, blend the flattened RGB toward that same gray with artwork
  weight 128/255 (about 50%). Transparent regions stay gray and alpha edges keep
  their gradient; cached bytes remain opaque for the existing SRCCOPY painter.
  Numbers use 10pt Segoe UI Bold (700), up from 9pt regular. User physically
  accepted the softened icons/bold numbers with offset shadow on 2026-10-19.
  The subsequent request replaces that shadow with a zero-offset black halo:
  eight surrounding text copies at one DPI-scaled logical pixel radius (1 physical
  px at 96 DPI; 2 at 144/192 DPI), then white at the unchanged center. Base label
  centering, icon sizing, cells, strip and tray gap stay unchanged. Font handles
  retain existing window-owned cleanup; halo painting creates no GDI object.
  The centered halo was physically user-accepted 2026-10-19; wider DPI acceptance
  remains pending.
- Accent delivered offline 2026-10-19: the active fill now shares the documented
  WinRT Personalization-accent resolver with the border, not a hardcoded blue.
  It ignores the border theme switch but shares the positive-alpha availability
  guard; unavailable queries fall back to `#2a82da`. Both existing theme/color
  broadcasts invalidate one owner cache shared by both surfaces without polling; renderer-only
  notifications wake the owner, and suppressed strips refresh on reshow.
  Pending fill/source survives creation backoff, and normal logs disclose
  accent/fallback transitions even at identical RGB. No painter-time query,
  geometry or saved-settings change. Source/implementation/gate evidence is in
  the [dogfood record](windows-single-output-dogfood.md). Shared border/pager
  accent parity was physically user-accepted 2026-10-19; idle notification repaint
  remains separately unverified. Subsequent same-day black-accent reports reopen
  startup reliability: one incident reports a black pager accent alongside the
  correct border accent. Subsequent diagnostic runs localized native startup
  crashes to activation-factory vtable dispatch; black-value causality remains
  unproven. The owner-lifetime/shared-snapshot correction bypasses the static
  factory cache, and the renderer no longer queries WinRT. Corrected startup
  reliability physically user-accepted 2026-10-19: 10/10 starts succeeded with
  correct accent on both border and pager. Idle refresh and wider variants remain
  pending; see the linked investigation.
- Per-query accent diagnostics and trace-only query/creation stages are now
  delivered for several user-run starts plus an accent-change discriminator.
  Cache hits stay silent, prior transition-only pager-gate diagnostics remain
  unchanged. That diagnostic-only delivery did not alter lifetime/cache behavior;
  the subsequent reliability correction does. New shared-source records report
  one owner resolve per observed version, cached scope-init HRESULT and raw ARGB;
  trace stages remain synchronous and no cache read fabricates a query.
- Omit unavailable icons rather than introduce a new fallback glyph. Host or
  executable artwork may be less specific than a packaged application's icon.
- Implemented: `pager_icons.rs` holds portable sizing/cache/channel policy;
  `pager_icons_sys.rs` owns the worker, lookup and GDI conversion; the existing
  frame/painter/host carry identity and optional shared pixel buffers.
- Lookup order: SMALL2, SMALL, BIG window icons (50 ms per message with
  SMTO_ABORTIFHUNG), small/big class icons, then ExtractIconExW on the verified
  executable. Borrowed handles are copied; all owned native handles are freed.
  ApplicationFrameHost executable artwork is deliberately omitted. Exact
  packaged-app artwork remains a follow-up, not a claimed Calculator result.
- User-approved 2026-10-16 sizing: cap 16 logical px, minimum 8 logical px, with one physical
  border pixel plus one physical padding pixel per side. The smallest square
  miniature showing an icon is 12 px at 96 DPI, 20 px at 192 DPI. Render once
  at 32 px and stretch to the DPI-aware slot; scales above 200% can upscale.
- User-approved 2026-10-16: defer packaged/UWP package-artwork lookup until
  the user has seen Calculator physically; no exact artwork claim is made.
  User 2026-10-17 (now seen): keep the plain-gray miniature as implemented
  for now; the user reviews dogfooding.
- User-approved 2026-10-16 limits cleanup delivered offline: positive cache
  entries refresh after 10 seconds (no Windows icon-change event), misses
  retry after 5 seconds (apps often set icons after window creation).
  Overlay creation and worker spawn retry every 5 seconds without backoff,
  recovering promptly without busy-looping. Removed arbitrary 64-entry caps:
  requests deduplicate until results drain, with departed pending work pruned.
  Pager input keeps only the latest pending click, never a stale-click queue.
  Only drawable miniatures on served outputs request artwork.
  The idle worker waits on a condition variable, not a polling timer.
- Suppression retires queued work and its result generation; no new lookup is
  requested while hidden. Already-started lookup can finish. Shutdown clears
  work and signals stop without joining. ExtractIconExW has no timeout: a slow
  extraction can delay all subsequent icons, but never the tile loop or paint.
- Full HWND/pid/process-creation identity is checked before and after lookup;
  closed/departed cache entries are pruned. Same-process HWND reuse is not
  distinguished by the existing key model; refresh bounds stale cached artwork.
- Native boundary review found and repaired queued-only deduplication, idle
  polling, sparse repaint hashing, missing identity verification and failure-path
  cleanup. Full-content digests now cover artwork changes; generation tests
  cover late results across suppression. Stop predicate changes hold the worker
  mutex before notification, avoiding a lost condition-variable wake.
- All locked four-package build/test/strict all-target clippy, workspace format
  and diff checks passed: 2,158 passed, zero failed, one pre-existing ignored
  (+13 icon tests). Portable cfg boundaries inspected. Cross-platform CI for
  implementation `6d0c9f5` passed all eight jobs, including Windows, Rust and
  macOS ([run](https://github.com/beefsack/OmniTiler/actions/runs/38816412355)).
- Lifecycle/outcome logs contain only bounded source/result counts and retry
  reasons, never window titles, executable paths or other application content.
- Icon/number appearance, centered halo and shared accent parity physically
  user-accepted 2026-10-19; keep this record active for remaining native variants.
- Native 2026-10-17 resource samples plateaued over 5.5 minutes (GDI 17, USER
  29, process handles settling to 204). Calculator lookup missed as expected;
  its hosted window paints a plain gray miniature. An earlier missing-miniature
  report was corrected by independent pixel census of the original capture;
  no source defect was found. Physical appearance acceptance and exact packaged
  artwork remain pending. See the
  [lifecycle evidence](archive/windows-taskbar-pager-lifecycle.md).

## Pending user-run native acceptance

- Stacking correction physically user-accepted 2026-10-19 in the
  [single-output dogfood record](windows-single-output-dogfood.md): miniatures
  now paint back-to-front from the existing native enumeration, including
  retained overlays. Hidden members retain their last observed relative order;
  unknown order falls back deterministically. No additional native/paint/icon
  queries. Native occlusion and icon/number styling passed user physical acceptance;
  the subsequent centered halo also passed physical acceptance. Post-halo offline
  gates: Windows library 1,209/0/1; native painter 6/0; pager filter 64/0;
  locked five-package build/clippy and four-package tests 2,859/0/1 passed, as did
  workspace format, unsafe guardrail and diff checks (see linked record).

Follow [the live Windows guide](../live-windows-testing.md): bind the tested
artifact hash and exact owner identity, capture geometry/DPI and affected-window
baseline, and establish the existing stop/restore route before launch. Use
ordinary medium-integrity applications, not games or protected windows.

1. With the desktop handed over for a bounded scoped run, use two distinct
   classic applications plus Calculator. A user-run dev route is:
   `mise exec -- just dev trace --no-keyboard-snap-takeover --no-mouse-snap-prevention --scope-exe notepad.exe --scope-exe mspaint.exe --scope-exe CalculatorApp.exe --scope-exe ApplicationFrameHost.exe --scope-host-child ApplicationFrameHost.exe=CalculatorApp.exe`.
   This is a proposed acceptance run, not evidence it has been executed.
2. Inspect active and hidden-workspace miniatures: centred recognisable icons
    when available at half opacity, no black/opaque-alpha fringe, unchanged borders,
    larger bold white numbers within a centered black outline at the same position,
   unchanged tray gap and no focus change on pager clicks. Calculator may omit
   its icon; do not accept host executable artwork as exact app resolution.
3. Resize to make a miniature cross the minimum size: icon disappears/reappears
   without changing the miniature or cell geometry. Switch workspaces both ways,
   close/reopen the scoped applications and confirm no old artwork persists.
4. Observe at least 30 seconds of stable display and bounded `pager-icons`
   result counts. Test managed fullscreen hide/return through the existing user
   controls, not a game: no pager or queued icon replay while hidden. Inspect
   resource counts across repeated application open/close cycles for growth.
5. Separately approve any display-scale change; compare 100%, fractional and
   200% sizing, restore the original setting. A helper with a changeable icon
   and deliberately unresponsive window procedure is needed to verify 10-second
   icon refresh and hung-client isolation; ordinary apps do not prove these.
6. End with `mise exec -- just stop`; confirm independent restoration, no pager
   residue, no hook or Mouse Snap setting changes, and restored baseline. Use
   the guide's exact-identity emergency/restore ladder only if graceful stop
   fails. Record appearance and resource results before archiving this note.

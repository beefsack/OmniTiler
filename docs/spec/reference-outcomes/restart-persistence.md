# Restart / persistence (reference outcomes)

Part of the [outcome matrix index](../reference-outcomes.md). Notation, profiles, evidence legend, and variant hooks live in the index; `S()`/`D()` keys below resolve there. Scenarios below are GWT with one Then bullet per profile (14). Startup-adoption and owner-controls sections preserved as subsections.

## 6. Startup adoption

Windows D25/D26 offline disposition 2026-10-13: existing restart-intent and
saved-settings evidence retained; no saved-layout restore or cross-login
identity recovery added. Native workspace set/membership/provable focus and
logout/login outcomes stay TBD
([verification and exact journeys](../../changes/archive/windows-item22-verification.md)).

Column Given bullets are separate fixtures, never H/V ancestry claims. R-START-01..03 assess the original enable-tiling action only; no restart leg is substituted. Where the exact enable toggle has no counterpart the cell is qualified with pinned model/inventory evidence and needs no live test. PaperWM extension enable and karousel script enable over unmanaged free fields count as native enable journeys and are assessed as such; mere owner restart or full session adoption is never substituted. R-CTL-01/02/05/06/07 settings journeys are owner-specific throughout: no first-run/preset/staging model exists in any scrolling inventory, so those cells are qualified owner-specific with pinned inventory evidence and outcome TBD. R-CTL-03 is owner-specific by fixture (Windows TaskbarCreated, owner GUID icon and menu Stop have no constructible counterpart on scrolling hosts). R-CTL-04 uses the established floating-workspace backfill citations: no workspace floating toggle or mode exists to hold a floating default.


<a id="scrolling-backfill-additive-wide-rows-above-preserved"></a>
<a id="r-start-01-scrolling-assessment-enable-over-a-2x2-float-field"></a>
### R-START-01: enable over a 2x2 float field

- Given (tree profiles): Tiling off; 2560x1380 work area, gaps 8; A(8,8,1268,678), B(1284,8,1268,678), C(8,694,1268,678), D(1284,694,1268,678); focus A,B,C,D; minima fit

- Given (scrolling): four unmanaged ordinary floats A(8,8,1268,678), B(1284,8,1268,678), C(8,694,1268,678), D(1284,694,1268,678) under the scrolling model at shipped defaults; focus A,B,C,D; viewport recorded. Action: the original enable (extension/script enable over these free fields), then disable/re-enable.

- When: Enable tiling; disable/re-enable

- Observe: Identity/order/topology vs sequential remap; second-enable stability

- Then COSMIC: Fixture focus A->D raises each floater in turn, so D is topmost regardless of map order; enable admits front-to-back (D,C,B,A), each fresh at focus-MRU long-edge anchors. D seeds the root (empty tree, output-dimension fallback); C/B/A each split D's leaf under the frozen MRU: first split side-by-side `H` (`Orientation::Vertical` on the 2544-wide area), then alternating long-edge bisection, so the 2x2 becomes a nested chain. Disable/re-enable roundtrips (tiling order out, reverse-z back in), identity not guaranteed; `S(S-cos-wstile)` + `S(S-cos-raise)` + `S(S-cos-seq)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)`
- Then Hyprland/Dwindle: Unsupported action parameter here: no tiling-off/workspace-enable toggle in source (workspaces always carry both algorithms; workspace rules carry no tiling default; admission is per-window Dwindle anchor, no centre-cut inference), so the 2x2/nested-chain enable outcome never runs and has no built-in equivalent; `S(S-hyp-float)` + `S(S-hyp-ins)` + `S(S-hyp-wsrule)`
- Then bspwm: Unsupported action parameter here: no tiling-off/enable toggle in source (desktops always lay out tiles; float is per-window), so the 2x2/nested-chain enable outcome never runs and has no built-in equivalent; `S(S-bsp-layout)`
- Then i3: Unsupported action parameter here: no tiling-off/enable toggle in source (workspaces always lay out tiles; float is per-window), so the 2x2/nested-chain enable outcome never runs and has no built-in equivalent; `S(S-i3-wsmode)`
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no tiling-off/enable toggle in source (workspaces always tile via layout plus floating layer; admission is per-window `insertUp`, no centre-cut inference, so the 2x2/nested-chain enable outcome never runs and no Tall geometry is asserted here); `S(S-xmo-layout)` + `S(S-xmo-ins)`
- Then sway: Unsupported action parameter here: no tiling-off/enable toggle in source (workspaces always lay out tiles; float is per-window), so the 2x2/nested-chain enable outcome never runs and has no built-in equivalent; `S(S-sway-wsmode)`
- Then qtile/Columns: Unsupported action parameter here: no tiling-off/enable toggle in source (Columns always tiles plus a floating layer; admission is per-window at the live focus anchor), so the 2x2/nested-chain enable outcome never runs and has no built-in equivalent; `S(S-qti-add)` + `S(S-qti-float)`
- Then awesome/tile: Supported as a per-tag layout switch (floating<->tile via layout.set, no global flag): the floating layout arranges nothing, so A/B/C/D keep frames with c.floating unset; re-tile partitions retained global-client order statelessly (nmaster master, rest stack; no rectangle inference), so the 2x2 is not rebuilt as nested splits; re-tile is deterministic recalc over the same order; exact identity order TBD (fixture gives focus order, not manage/swap list order); `S(S-awe-layout)` + `S(S-awe-tile)` + `S(S-awe-float)`
- Then niri: no-counterpart for the original enable (compositor is always scrolling; no tiling enable/disable verb in the full Action inventory); disable/re-enable legs share the absence: no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: extension mechanism traced: first enable has no prevSpace so `addAll` appends workspace windows in `xz_comparator` order at `length` (not the open-position index), each as its own column; x-buckets {A,C}/{B,D} are fixed by the given frames (x=8 vs 1284), but the within-bucket tiebreak reads `display.sort_windows_by_stacking` order whose direction (bottom-first vs top-first) is host semantics untraced at pin: exact column order TBD (host: Mutter stacking-order direction). Second-enable verbatim prevSpace restore is determined (stable). Selection is the host `NORMAL` tab-list head among indexed windows: which of A-D TBD (host: tab-list order policy untraced). Absolute pixel rects additionally need the recorded viewport value, unstated in the fixture: TBD (fixture: viewport value). `S(S-pap-ins)` + `S(S-pap-rst)` + `S(S-pap-layout)` + `S(S-pap-view)`; order/selection queued (host), rects queued (fixture).
- Then karousel/Lazy: script enable constructs the World and re-admits the four free windows in `Workspace.windows` order (KWin manage/creation order, not focus order) via `addExistingClients` into `addClient` as fresh columns (shapeability plus rules plus exactly-1 desktop/activity gates; no centre-cut inference, so no nested chain); each Tiled opens a new column after the last-focused column else the last with width 1268 retained from the given frames via `preferredWidth` clamped into [min,max] (minima fit). Disable/re-enable repeats the same fresh re-admission (live-only Grid, no persisted layout, no restart verb in the full Actions/definition inventory). Exact column order TBD (fixture: creation/manage order unstated; focus order A-D does not establish it). Focus stays the KWin active window (no script focus write on this path). `S(S-kar-ins)` + `S(S-kar-start)` + `S(S-kar-spc)` + `S(S-kar-min)` + `S(S-kar-rst)` + `S(S-kar-acts)` + `S(S-kwin-resizeable)` + `S(S-kwin-winorder)`.
- Then paneru: no-counterpart (the Operation inventory lists no tiling enable verb; startup matching is session restore, not enable, and is never substituted here). `S(S-pan-cmds)`.
- Then Ours KDE: Clean/tolerance-valid recursive-cut adoption preserved; selected User 2026-10-08; exact fixture TBD
- Then Ours Windows: Clean/tolerance-valid recursive-cut adoption preserved; selected User 2026-10-08; exact fixture TBD
- Variant hook: V-START-SEED.

<a id="r-start-02-scrolling-assessment-enable-over-a-cascade"></a>
### R-START-02: enable over a cascade

- Given (tree profiles): Tiling off; 2560x1380 work area, gaps 8; A(80,80,1000,700), B(120,120,1000,700), C(160,160,1000,700), D(200,200,1000,700); focus A,B,C,D; minima 400x200 each

- Given (scrolling): four overlapping unmanaged ordinary floats A(80,80,1000,700), B(120,120,1000,700), C(160,160,1000,700), D(200,200,1000,700) at shipped defaults; focus A,B,C,D; viewport recorded. Action: the original enable over these free fields.

- When: Enable tiling

- Observe: Centre-cut inference vs long-edge seed; final axes and identity order

- Then COSMIC: No centre-cut inference in source. Same mechanism: focus raises make D topmost and MRU, so D seeds the root and C/B/A each split D's leaf at its long edge (frozen MRU), yielding a nested bisection chain, not a 2x2; first split `H` (`Orientation::Vertical` on the wide area), second `V` (`Orientation::Horizontal` on the tall half), third `H` again; identity order D,C,B,A in admission order; `S(S-cos-wstile)` + `S(S-cos-raise)` + `S(S-cos-last)` + `S(S-cos-axis)` + `S(S-cos-newgroup)`
- Then Hyprland/Dwindle: Unsupported enable action as above (no tiling-off toggle; workspace rules carry no tiling default; per-window Dwindle anchor, no centre-cut inference), so the cascade-chain enable outcome never runs and has no built-in equivalent; `S(S-hyp-float)` + `S(S-hyp-ins)` + `S(S-hyp-wsrule)`
- Then bspwm: Unsupported action parameter here: no tiling-off/enable toggle in source (desktops always lay out tiles; float is per-window), so the cascade-chain enable outcome never runs and has no built-in equivalent; `S(S-bsp-layout)`
- Then i3: Unsupported action parameter here: no tiling-off/enable toggle in source (workspaces always lay out tiles; float is per-window), so the cascade-chain enable outcome never runs and has no built-in equivalent; `S(S-i3-wsmode)`
- Then xmonad/Tall+Navigation2D: Unsupported enable action as above (no tiling-off toggle; per-window `insertUp` anchor, no centre-cut inference, so the cascade-chain enable outcome never runs and no Tall geometry is asserted here); `S(S-xmo-layout)` + `S(S-xmo-ins)`
- Then sway: Unsupported action parameter here: no tiling-off/enable toggle in source (workspaces always lay out tiles; float is per-window), so the cascade-chain enable outcome never runs and has no built-in equivalent; `S(S-sway-wsmode)`
- Then qtile/Columns: Unsupported enable action as above (no tiling-off toggle; per-window anchor, no centre-cut inference), so the cascade-chain enable outcome never runs and has no built-in equivalent; `S(S-qti-add)`
- Then awesome/tile: Same per-tag switch mechanism (no centre-cut inference): the floating layout holds the cascade frames with c.floating unset; re-tile partitions retained global-client order by master/stack shares, not a nested bisection chain; exact axes/identity order TBD; `S(S-awe-layout)` + `S(S-awe-tile)`
- Then niri: no-counterpart for the original enable (compositor is always scrolling; no tiling enable/disable verb in the full Action inventory); cascade legs share the absence: no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: extension enable adopts the four free windows through the existing-window path: with no prevSpace, `addAll` appends workspace windows in `xz_comparator` order at `length`; the cascade x-values (80/120/160/200) are all distinct so `xcmp` never ties and the stacking tiebreak never engages: column order ABCD source-proven. No centre-cut inference exists (no centre branch in the comparator or admission path), so the cascade becomes separate columns, not a nested chain. Widths per column layout from frame/preferredWidth. Selection is the host `NORMAL` tab-list head: selection TBD (host: tab-list order policy untraced). Absolute pixel rects need the recorded-but-valueless viewport: TBD (fixture: viewport value). `S(S-pap-ins)` + `S(S-pap-rst)` + `S(S-pap-layout)` + `S(S-pap-view)`; selection queued (host), rects queued (fixture).
- Then karousel/Lazy: script enable re-admits the four free windows in `Workspace.windows` order (KWin manage/creation order) via `addExistingClients` into `addClient` as fresh columns (same gates as R-START-01; no centre-cut inference, so the cascade becomes separate columns, not a chain); each Tiled opens after the last-focused column else the last with width 1000 retained from the given frames via `preferredWidth` clamped into [min,max] (minima 400x200 fit). Exact column order TBD (fixture: creation/manage order unstated; focus order A-D does not establish it). Focus stays the KWin active window (no script focus write on this path). `S(S-kar-ins)` + `S(S-kar-start)` + `S(S-kar-spc)` + `S(S-kar-min)` + `S(S-kar-acts)` + `S(S-kwin-resizeable)` + `S(S-kwin-winorder)`.
- Then paneru: no-counterpart (no tiling enable verb; startup matching is session restore, never substituted). `S(S-pan-cmds)`.
- Then Ours KDE: Decline centre splits to deterministic long-edge bisection chain, not guaranteed 2x2; selected User 2026-10-08, shared KDE+Windows; exact fixture TBD
- Then Ours Windows: Decline centre splits to deterministic long-edge bisection chain, not guaranteed 2x2; selected User 2026-10-08, shared KDE+Windows; exact fixture TBD
- Variant hook: V-START-SEED.

<a id="r-start-03-scrolling-assessment-enable-with-infeasible-minima"></a>
### R-START-03: enable with infeasible minima

- Given (tree profiles): As START-02 plus E(240,240,1000,700); A/B/C minima 401x246, D(Paint) 864x617, E(Calc) 402x627; focus A,B,C,E,D; usable inner 2544x1364, gap 8

- Given (scrolling): the R-START-02 cascade plus E(240,240,1000,700) with A/B/C minima 401x246, D(Paint) 864x617, E(Calc) 402x627 at shipped defaults; focus A,B,C,E,D; viewport recorded. Action: the original enable over these free fields.

- When: Enable tiling

- Observe: Feasibility fallback, skipped/floated/clamped writes, final origins, overlap/overflow

- Then COSMIC: Same admission mechanism (focus raises make D topmost/MRU, admitted first); tiling allocation/cropping ignores minima and admission maps once at the resolved anchor with no alternative search; fixed-size (min==max) admits floating instead. Origins follow the nested-chain allocation with no skip/float/clamp writes (fixed floats excepted) and no centre inference or feasibility search; exact settled frames/native response TBD (L: client settle timing, live-only); `S(S-cos-min)` + `S(S-cos-wstile)` + `S(S-cos-raise)` + `S(S-cos-axis)`; settle queued (live)
- Then Hyprland/Dwindle: Unsupported enable action as above (no tiling-off toggle; workspace rules carry no tiling default; per-window Dwindle anchor, no centre-cut inference), so the enable outcome never runs and has no built-in equivalent; tiled limits default off (unclamped, no auto-float) is cited for the hint leg only, not as an enable journey; `S(S-hyp-float)` + `S(S-hyp-min)` + `S(S-hyp-ins)` + `S(S-hyp-wsrule)`
- Then bspwm: Unsupported enable action as in START-01/02 (no tiling-off toggle; desktops always lay out tiles, float is per-window), so the enable outcome never runs; hints off by default (opt-in leaf clamp on every reflow) is cited for the hint leg only, not as an enable journey; `S(S-bsp-layout)` + `S(S-bsp-hint)` + `S(S-bsp-min)`
- Then i3: Unsupported action parameter here: no tiling-off/enable toggle in source (workspaces always lay out tiles; float is per-window), so the enable outcome never runs and has no built-in equivalent; tiled render ignores size hints while float clamps is cited for the hint leg only, not as an enable journey; `S(S-i3-wsmode)` + `S(S-i3-min)`
- Then xmonad/Tall+Navigation2D: Unsupported enable parameter as above (no tiling-off toggle; Tall sizing is unconditional with no minimum clamp in the tile path and fixed-size floats separately, so the enable outcome never runs and exact origins/frames are not asserted here); `S(S-xmo-layout)` + `S(S-xmo-admit)`
- Then sway: Unsupported enable parameter as above (no tiling-off toggle), so the enable outcome never runs and has no built-in equivalent; tiled arrange ignores client hints (fraction normalize + 10px zeroing bound; `MIN_SANE` 100x60 gap-reservation only, no hint consult) while float clamp is config min/max (client hints on floating resize only) is cited for the hint leg only, not as an enable journey; fixed-size min==max admits floating instead; `S(S-sway-wsmode)` + `S(S-sway-min)` + `S(S-sway-max)`
- Then qtile/Columns: Unsupported enable action as above (no tiling-off toggle; per-window anchor, no centre-cut inference), so the enable outcome never runs and has no built-in equivalent; `S(S-qti-add)`
- Then awesome/tile: Same per-tag switch; on re-tile the tile consults size hints when arranging and fixed-size floats instead (MIN policy), explicit floats preserved; exact origins/frames/response TBD; `S(S-awe-layout)` + `S(S-awe-tile)` + `S(S-awe-float)`
- Then niri: no-counterpart for the original enable (compositor is always scrolling; no tiling enable/disable verb in the full Action inventory); minimum-handling legs share the absence: no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: minima handling determined: `add_filter` has no size branch and the column layout reads frame/preferredWidth only with a work-area margin clamp, so infeasible minima are ignored at admission and layout with no skip/float/clamp write - origins follow sequential column placement with gaps (overlap/overflow as placed). Column order ABCDE source-proven (x-values 80/120/160/200/240 all distinct, stacking tiebreak never engages). Selection is the host tab-list head: selection TBD (host: tab-list order policy untraced). Exact native settlement of the written frames is client-timing (X11 sync converges, Wayland async may not, per the :616-624 re-read): exact native frames TBD (live). Absolute rects also need the recorded-but-valueless viewport: TBD (fixture: viewport value). `S(S-pap-ins)` + `S(S-pap-rst)` + `S(S-pap-spc)` + `S(S-pap-layout)`; selection queued (host), frames queued (live), rects queued (fixture).
- Then karousel/Lazy: script enable re-admits the five free windows in `Workspace.windows` order (KWin manage/creation order) via `addExistingClients` into `addClient` as fresh columns (same gates; no centre-cut inference); admission still tiles shapeable clients and column `setWidth` clamps each `preferredWidth` (given frame widths) into `[getMinWidth, getMaxWidth]` (floor from the widest client minimum, capped at tiling width), so infeasible minima clamp with overlap/overflow as placed and no skip/float write. Exact column order TBD (fixture: creation/manage order unstated; focus order A,B,C,E,D does not establish it). Exact origins TBD (fixture: order unstated, origins follow sequential placement). Focus stays the KWin active window (no script focus write on this path). `S(S-kar-ins)` + `S(S-kar-start)` + `S(S-kar-min)` + `S(S-kar-spc)` + `S(S-kar-acts)` + `S(S-kwin-resizeable)` + `S(S-kwin-winorder)`.
- Then paneru: no-counterpart (no tiling enable verb; startup matching is session restore, never substituted). `S(S-pan-cmds)`.
- Then Ours KDE: Minimum-infeasible startup fits still decline to sequential long-edge seeding; writable infeasible tiles then use origin+minimum (B6, overlap/overflow possible). Code: [adapter](../../../kwin/src/plan-adapter.ts) `overconstrainedEffective`, `writeGeometries`; [startup fixture](../../../kwin/tests/workspace-send-engine-fixture.test.ts). Exact native fixture TBD; `D(D-place)`
- Then Ours Windows: tile origin, extent at least declared minimum (overlap/overflow possible); `D(D-place)` + `D(D-dec-win)` provisional divergence; exact fixture TBD
- Variant hook: V-START-MIN.

## 11. Owner controls and startup settings

<a id="r-ctl-01-scrolling-assessment-first-run-preset-choice"></a>
### R-CTL-01: first-run preset choice

- Given: Windows settings absent

- Given (scrolling): owner settings absent; shipped profile defaults apply. The prompt/preset journey is the same owner-specific fixture as this scenario.

- When: Start owner; choose Compatible; stop; restart

- Observe: Preset persists; first-run prompt does not recur

- Then COSMIC: Not applicable in the inspected compositor inventory: no first-run prompt or preset writer in the traced autotile/workspace-persist/shortcut hot-reload/tiling-toggle/input-actions/keybindings/daemon Action paths; tiling default is the `autotile` config key; no applicable reference journey. `S(S-cos-ctl-tile)`
- Then Hyprland/Dwindle: No counterpart in source: dispatcher inventory at pin lists no first-run/preset/prompt actions, so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-hyp-shortcut)`
- Then bspwm: No counterpart in source: wm command inventory lists dump/load-state/add-monitor/reorder-monitors/adopt-orphans/get-status/record-history/restart only, no first-run/preset/prompt writer, so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-bsp-ctl)`
- Then i3: No Compatible preset/prompt model in inspected source: closest writer `i3-config-wizard` exits when a config exists (no overwrite, fresh-write only), so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-i3-wiz)`
- Then xmonad/Tall+Navigation2D: No counterpart in source: key inventory at pin lists spawn/kill/layout/focus/swap/sink/quit/restart only, no first-run/preset/prompt writer, so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-xmo-ctl)`
- Then sway: No counterpart in source: command inventory at pin lists `reload`/`exit`/`bindsym`/`bindcode`/`unbindsym`/`unbindcode` but no first-run/preset/prompt actions and no `restart` verb (in-place reload only), so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-sway-reload)` + `S(S-sway-bind)`
- Then qtile/Columns: No counterpart in source: no first-run prompt or preset writer (static Keys/config loaded at startup; restart only dumps group/layout/screen state, reload rebuilds from config), so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-qti-keys)` + `S(S-qti-state)`
- Then awesome/tile: No counterpart in source: no first-run prompt or Compatible preset writer in the inspected inventory (static keys/config; restart is in-place awesome.restart preserving client order, tags recreated from rc), so preset choice/restart persistence never runs (owner-specific, no applicable journey); `S(S-awe-ctl)` + `S(S-awe-keys)`
- Then niri: owner-specific with no counterpart (no first-run/preset/prompt writer in the full Action inventory; width SwitchPreset* verbs only): no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: owner-specific fixture with no counterpart (no first-run/preset/prompt writer in the registered action inventory or the prefs settings UI pages); no applicable reference journey. `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the full Actions/definition inventory lists no first-run/preset/prompt writer); no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: owner-specific (no first-run/preset/prompt Operation in the command inventory); no applicable reference journey. `S(S-pan-cmds)`.
- Then Ours KDE: KDE first-run TBD
- Then Ours Windows: authentic default offered, compatible saves 35 disabled rows; existing-file startup skips prompt; synthetic/native proof [tray record](../../changes/archive/windows-tray-first-run.md)
- Variant hook: V-FIRST-RUN.

<a id="r-ctl-02-scrolling-assessment-stale-prompt-choice"></a>
### R-CTL-02: stale prompt choice

- Given: Owner's first-run prompt open, settings absent

- Given (scrolling): owner first-run prompt open with settings published by another writer; shipped profile defaults apply. Same owner-specific fixture as this scenario.

- When: Publish settings from another writer; accept stale prompt choice

- Observe: Preserve newer settings vs overwrite

- Then COSMIC: Not applicable in the inspected compositor inventory: no prompt/settings-race UI in the traced autotile/workspace-persist/shortcut hot-reload/tiling-toggle/input-actions/keybindings/daemon Action paths; `system_actions`/shortcut maps merge system then user config; no applicable reference journey. `S(S-cos-syscmd)`
- Then Hyprland/Dwindle: No counterpart in source: no prompt/settings-race UI or stale-choice path in the dispatcher inventory, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-hyp-shortcut)`
- Then bspwm: No counterpart in source: no prompt/settings-race UI or stale-choice path in the inspected inventory, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-bsp-ctl)`
- Then i3: No settings-race/stale-choice path in inspected source: wizard guard exits on existing config without comparing writers, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-i3-wiz)`
- Then xmonad/Tall+Navigation2D: No counterpart in source: no prompt/settings-race UI or stale-choice path in the inspected key inventory, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-xmo-ctl)`
- Then sway: No counterpart in source: no prompt/settings-race UI or stale-choice path in the inspected command/config inventory, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-sway-reload)` + `S(S-sway-bind)`
- Then qtile/Columns: No counterpart in source: no prompt/settings-race UI or stale-choice path in the inspected key/config inventory, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-qti-keys)`
- Then awesome/tile: No counterpart in source: no prompt/settings-race UI or stale-choice path in the inspected inventory, so stale-choice preserve/overwrite never runs (owner-specific, no applicable journey); `S(S-awe-ctl)`
- Then niri: owner-specific with no counterpart (no prompt/settings-race path in the full Action inventory): no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: owner-specific fixture with no counterpart (no prompt/settings-race path in the registered action inventory or the prefs settings UI pages); no applicable reference journey. `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the full Actions/definition inventory lists no prompt/settings-race path); no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: owner-specific (no prompt/settings-race Operation in the command inventory); no applicable reference journey. `S(S-pan-cmds)`.
- Then Ours KDE: Windows: discard stale choice, load authoritative file, bytes unchanged; same record; other platforms TBD
- Then Ours Windows: Windows: discard stale choice, load authoritative file, bytes unchanged; same record; other platforms TBD
- Variant hook: V-FIRST-RUN.

<a id="r-ctl-03-scrolling-assessment-notification-icon-lifecycle"></a>
### R-CTL-03: notification icon lifecycle

- Given: Running owner with notification icon

- Given (scrolling): running owner with notification icon under the scrolling host; shipped profile defaults apply. Same owner-specific fixture as this scenario: Windows TaskbarCreated re-registration, one owner GUID icon, and menu Stop.

- When: Lose icon registration; post TaskbarCreated; stop from menu

- Observe: Exactly one icon returns; Stop removes icon and owner effects

- Then COSMIC: Not applicable in the inspected compositor inventory: no tray icon lifecycle in the traced shell/workspace/focus/input-actions/keybindings/daemon Action paths; no applicable reference journey. `D(D-tray-task)`
- Then Hyprland/Dwindle: No counterpart in source: no tray icon lifecycle in the dispatcher inventory (only unrelated xwayland tray atoms), so single-icon return/Stop cleanup never runs (owner-specific, no applicable journey); `S(S-hyp-shortcut)`
- Then bspwm: No counterpart in source: no tray icon lifecycle in the inspected inventory (wm verbs as in CTL-01), so single-icon return/Stop cleanup never runs (owner-specific, no applicable journey); `S(S-bsp-ctl)`
- Then i3: No owner notification-icon lifecycle in inspected source: tray inventory is bar-hosted XEMBED (`tray_output`/`tray_padding`, selection window, trayclients), so single-icon return/Stop cleanup never runs (owner-specific, no applicable journey); `S(S-i3-tray)`
- Then xmonad/Tall+Navigation2D: No counterpart in source: no tray icon lifecycle in the inspected key/mouse inventory, so single-icon return/Stop cleanup never runs (owner-specific, no applicable journey); `S(S-xmo-ctl)`
- Then sway: No owner notification-icon lifecycle in inspected source: tray inventory is bar-hosted (`tray_output`/`tray_padding`/`tray_bindcode`/`tray_bindsym`, `HAVE_TRAY`), so single-icon return/Stop cleanup never runs (owner-specific by fixture: TaskbarCreated, owner GUID icon and menu Stop have no constructible counterpart on this host; no applicable journey); `S(S-sway-tray)`
- Then qtile/Columns: No owner notification-icon lifecycle in inspected source: tray inventory is the bar-hosted X11-only Systray widget (hosts client icons), so single-icon return/Stop cleanup never runs (owner-specific, no applicable journey); `S(S-qti-tray)`
- Then awesome/tile: No owner notification-icon lifecycle in inspected source: tray inventory is the bar-hosted wibox.widget.systray hosting client icons, not an owner icon, so single-icon return/Stop cleanup never runs (owner-specific, no applicable journey); `S(S-awe-ctl)`
- Then niri: no-counterpart by fixture (no tray-icon lifecycle verb in the full Action inventory; TaskbarCreated, owner GUID icon and menu Stop are Windows-owner objects with no constructible counterpart on this host): no applicable journey. `S(S-nir-acts)` + `D(D-tray-task)`.
- Then PaperWM: owner-specific by fixture (same Windows-owner objects; no scrolling-host counterpart assessed); outcome TBD. `D(D-tray-task)`.
- Then karousel/Lazy: no-counterpart by fixture (the full Actions/definition inventory lists no tray-icon lifecycle verb, and TaskbarCreated, owner GUID icon and menu Stop are Windows-owner objects with no constructible counterpart on this host); no applicable reference journey. `S(S-kar-acts)` + `D(D-tray-task)`.
- Then paneru: owner-specific by fixture (same Windows-owner objects; no scrolling-host counterpart assessed); outcome TBD. `D(D-tray-task)`.
- Then Ours KDE: KDE lifecycle is separate
- Then Ours Windows: GUID-delete fixture then posted message re-adds one icon; actual menu Stop cleans up; same record. Real Explorer restart TBD
- Variant hook: V-TRAY-LIFECYCLE.

<a id="r-ctl-04-scrolling-assessment-workspace-tiling-default"></a>
### R-CTL-04: workspace tiling default

- Given: Existing tiled workspace, new-workspace default Tiled

- Given (scrolling): existing tiled columns plus a saved floating default for new workspaces; shipped profile defaults apply. Same owner-specific fixture as this scenario.

- When: Save Floating default; create new workspace; restart owner

- Observe: Existing override stays, new workspace floating, startup saved default

- Then COSMIC: Tiling default is the `autotile` config (`autotile`/`autotile_behavior` keys persist via config): new workspaces inherit the set's `tiling_enabled` (`WorkspaceSet::new` with `autotile`, `add_empty_workspace` with the set flag); `Global` behavior retoggles existing workspaces, `PerWorkspace` applies to new windows/workspaces only. Per-workspace `tiling_enabled` persists only for pinned workspaces (`to_pinned` carries output match/tiling flag/id/name; `persist` writes pinned only; `add_output` recreates pinned shells with the flag respected), so a pinned tiled override stays tiled (unpinned workspaces have no persisted shell; fixture pin status unstated, F; exact override outcome TBD) and a saved Floating default yields floating new workspaces across restart; `S(S-cos-ctl-tile)` + `S(S-cos-persist)`; pin queued (fixture; primary F)
- Then Hyprland/Dwindle: Unsupported action parameter here: no workspace tiling flag or floating default in source (float is per-window; workspace rules carry no tiling default), so new-workspace/existing-override/saved-default restart never runs and has no built-in equivalent (owner-specific, no applicable journey); new workspaces admit per-window, existing overrides N/A; `S(S-hyp-wsrule)`
- Then bspwm: Unsupported action parameter here: no workspace tiling flag or floating default in source (desktop layout tiled/monocle only; float is per-window), so new-workspace/existing-override/saved-default restart never runs and has no built-in equivalent; `S(S-bsp-layout)` + `S(S-bsp-ctl)`
- Then i3: Unsupported action parameter here: no workspace tiling flag or floating default in source (float is per-window; `workspace_layout` default/stacked/tabbed only), so new-workspace/existing-override/saved-default restart never runs (owner-specific, no applicable journey); `S(S-i3-wsmode)`
- Then xmonad/Tall+Navigation2D: Unsupported action parameter here: no workspace tiling flag or floating default in source (layout always tiles plus a floating layer; float is per-window, so new-workspace/existing-override/saved-default restart never runs); `S(S-xmo-layout)` + `S(S-xmo-ctl)`
- Then sway: Unsupported action parameter here: no workspace tiling flag or floating default in source (float is per-window; `workspace_layout` default/stacked/tabbed only), so new-workspace/existing-override/saved-default restart never runs and has no built-in equivalent (owner-specific, no applicable journey); `S(S-sway-wsmode)`
- Then qtile/Columns: Unsupported action parameter here: no per-group tiling flag or floating default in source (static groups, one global floating_layout, float is per-window), so new-group/existing-override/saved-default restart never runs and has no built-in equivalent; `S(S-qti-wsdef)`
- Then awesome/tile: Unsupported action parameters here: layout is per-tag (set/inc, no global tiling flag or floating-default writer); tags are static 1-9 with rc initial floating (layouts[1]); new-workspace/create and saved-default writers have no counterpart, and restart recreates tags from rc (only floating property plus client order persist), so existing-override/new-floating/saved-default restart never runs; `S(S-awe-layout)` + `S(S-awe-default)` + `S(S-awe-ctl)`
- Then niri: owner-specific with no counterpart (no workspace floating toggle or mode exists to hold the default; `ToggleWindowFloating` is per-window only): no applicable journey. `S(S-nir-float)`.
- Then PaperWM: owner-specific fixture with no counterpart (no floating workspace mode and no workspace toggle in the registered action inventory to hold the default); no applicable reference journey. `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the full Actions/definition inventory lists no workspace tiling flag or floating-default writer; `windowToggleFloating` is per-window only); no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: owner-specific (no floating workspace mode; `Manage` is per-window); no applicable reference journey. `S(S-pan-cmds)`.
- Then Ours KDE: selected `D(D-dec-ww)`, default live proof TBD
- Then Ours Windows: tray/UI file readbacks, owner adoption, existing tiled/new floating checks and saved-default startup native proof [record](../../changes/archive/windows-workspace-tiling.md); physical restart journey TBD
- Variant hook: V-WS-TILING.

<a id="r-ctl-05-scrolling-assessment-shortcut-staging-and-apply"></a>
### R-CTL-05: shortcut staging and apply

- Given: KDE focus-right kept; Lock Session on Meta+L

- Given (scrolling): staged Compatible choice with native shortcut state present; shipped profile defaults apply. Same owner-specific fixture as this scenario.

- When: Stage Compatible; ordinary settings Save; Apply Shortcuts; reopen; restart session

- Observe: Staging/Save leave shortcuts untouched; Disable survives; Lock Session unchanged

- Then COSMIC: Closest COSMIC equivalent in the inspected compositor inventory: shortcut state is system `defaults` plus user `custom`, `Disable` masks a default binding, and the compositor hot-reloads on config change; there is no Keep/Authentic/Compatible staging/Save/Apply/Force model in the traced shortcut/system_actions/config-watch/workspace-persist/daemon Action paths, so Save/Apply/restart semantics have no counterpart here; no applicable reference journey. `S(S-cos-shortcut)`
- Then Hyprland/Dwindle: Closest Hyprland equivalent: conflict lookup plus `unbind` exist, but no Keep/Authentic/Compatible staging/Save/Apply/Force model in source, so Save/Apply/restart semantics never run and have no counterpart here (owner-specific, no applicable journey); `S(S-hyp-shortcut)`
- Then bspwm: Closest bspwm equivalent: node `-t` state (incl `~` alternate) and `-g` flags exist, but no Keep/Authentic/Compatible staging/Save/Apply/Force model in source, so Save/Apply/restart semantics never run and have no counterpart here (owner-specific, no applicable journey); `S(S-bsp-ctl)`
- Then i3: Closest i3 equivalent: `bindsym`/`bindcode` defines bindings via `configure_binding`, applied on reload/restart; no Keep/Authentic/Compatible staging/Save/Apply/Force model in the inspected command/config inventory, so Save/Apply/restart semantics never run (owner-specific, no applicable journey); `S(S-i3-bind)`
- Then xmonad/Tall+Navigation2D: Closest xmonad equivalent: keys/mouseBindings define bindings applied on restart/recompile; no Keep/Authentic/Compatible staging/Save/Apply/Force model in source, so Save/Apply/restart semantics never run (owner-specific, no applicable journey); `S(S-xmo-ctl)`
- Then sway: Closest sway equivalent: `bindsym`/`bindcode` defines bindings, applied on reload; no Keep/Authentic/Compatible staging/Save/Apply/Force model in the inspected command inventory, so Save/Apply/restart semantics never run and have no counterpart here (owner-specific, no applicable journey); `S(S-sway-bind)` + `S(S-sway-reload)`
- Then qtile/Columns: Closest qtile equivalent: static Key bindings grabbed at startup and re-grabbed on reload (ungrab/clear/regrab, no Keep/Authentic/Compatible staging/Save/Apply/Force model), so Save/Apply/restart semantics never run and have no counterpart here (owner-specific, no applicable journey); `S(S-qti-keys)`
- Then awesome/tile: Closest awesome equivalent: global/client keys defined via append keybindings applied on restart/reload; no Keep/Authentic/Compatible staging/Save/Apply/Force model in source, so Save/Apply/restart semantics never run (owner-specific, no applicable journey); `S(S-awe-keys)` + `S(S-awe-ctl)`
- Then niri: owner-specific with no counterpart (no Keep/Authentic/Compatible staging/Save/Apply/Force model in the full Action inventory): no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: owner-specific fixture with no counterpart (the prefs keybindings page edits bindings but neither it nor the registered action inventory defines any Keep/Authentic/Compatible staging/Save/Apply/Force model); no applicable reference journey. `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the full Actions/definition inventory lists no Keep/Authentic/Compatible staging/Save/Apply/Force verb); no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: owner-specific (no Keep/Authentic/Compatible staging/Save/Apply/Force model in the command inventory); no applicable reference journey. `S(S-pan-cmds)`.
- Then Ours KDE: KDE selected: explicit own-action clear, native storage authoritative, no Lock relocation while disabled; live restart/physical delivery TBD [record](../../changes/kde-shortcut-conflicts.md)
- Then Ours Windows: Windows Apply validates and atomically saves; Revert discards unsaved edits and reloads the saved file; Close never saves. Per-binding Keep/Disable/Rebind with the interim Win+existing-Shift limit; Compatible resets the catalog then disables 41 OS-conflicting catalog rows with no replacements, including Win+Y ([shortcut record](../../changes/archive/windows-workspace-tiling-shortcut.md)). KDE Force/foreign-holder clearing has no Windows counterpart (unsupported, TBD). Earlier settings synthetic/native proof passed; Win+Y Apply/Revert and physical input remain user-owned [record](../../changes/archive/windows-settings.md)
- Variant hook: V-SHORTCUT-CONFLICT.

<a id="r-ctl-06-scrolling-assessment-conflict-preview-and-disable"></a>
### R-CTL-06: conflict preview and disable

- Given: KDE foreign action has a project chord plus an unrelated chord

- Given (scrolling): conflicting shortcut row with preview/Force semantics; shipped profile defaults apply. Same owner-specific fixture as this scenario.

- When: Keep conflicting row; Apply; preview Force; edit row to Disable; try Force; Apply

- Observe: Draft edit invalidates preview; disabled row causes no foreign clearing; unrelated chord survives

- Then COSMIC: Closest COSMIC equivalent in the inspected compositor inventory: writing `Disable` for one binding in `custom` masks only that default while unrelated chords keep resolving from `defaults`; there is no Keep/preview/Force/draft step in the traced shortcut/system_actions/config-watch/daemon Action paths; no applicable reference journey. `S(S-cos-shortcut)`
- Then Hyprland/Dwindle: Closest Hyprland equivalent: conflict lookup plus `unbind` exist, but no Keep/Authentic/preview/Force/draft step in source, so draft/Disable/unrelated-chord outcomes never run and have no counterpart here (owner-specific, no applicable journey); `S(S-hyp-shortcut)`
- Then bspwm: Closest bspwm equivalent: node `-t`/`-g` exist, but no Keep/Authentic/preview/Force/draft/Disable-masking model in the inspected inventory, so draft/Disable/unrelated-chord outcomes never run and have no counterpart here (owner-specific, no applicable journey); `S(S-bsp-ctl)`
- Then i3: Closest i3 equivalent: `bindsym`/`bindcode` defines bindings via `configure_binding`; no Keep/Authentic/preview/Force/draft/Disable-masking model in the inspected inventory, so draft/Disable/unrelated-chord outcomes never run (owner-specific, no applicable journey); `S(S-i3-bind)`
- Then xmonad/Tall+Navigation2D: Closest xmonad equivalent: `keys` defines bindings; no Keep/Authentic/preview/Force/draft/Disable-masking model in the inspected inventory, so draft/Disable/unrelated-chord outcomes never run (owner-specific, no applicable journey); `S(S-xmo-ctl)`
- Then sway: Closest sway equivalent: `bindsym`/`bindcode` defines bindings, `unbindsym`/`unbindcode` removes; no Keep/Authentic/preview/Force/draft/Disable-masking model in the inspected inventory, so draft/Disable/unrelated-chord outcomes never run and have no counterpart here (owner-specific, no applicable journey); `S(S-sway-bind)`
- Then qtile/Columns: Closest qtile equivalent: Key definitions only, no Keep/Authentic/preview/Force/draft/Disable-masking model in the inspected inventory, so draft/Disable/unrelated-chord outcomes never run (owner-specific, no applicable journey); `S(S-qti-keys)`
- Then awesome/tile: Closest awesome equivalent: key definitions only, no Keep/Authentic/preview/Force/draft/Disable-masking model in the inspected inventory, so draft/Disable/unrelated-chord outcomes never run (owner-specific, no applicable journey); `S(S-awe-keys)`
- Then niri: owner-specific with no counterpart (no Keep/Authentic/preview/Force/draft model in the full Action inventory): no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: owner-specific fixture with no counterpart (the prefs keybindings page edits bindings but neither it nor the registered action inventory defines any Keep/Authentic/preview/Force/draft model); no applicable reference journey. `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the full Actions/definition inventory lists no Keep/Authentic/preview/Force/draft verb); no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: owner-specific (no Keep/Authentic/preview/Force/draft model in the command inventory); no applicable reference journey. `S(S-pan-cmds)`.
- Then Ours KDE: KDE selected: exact draft/owner/presence/active-image revalidation, no disabled-key holder mutation; live outcome TBD [record](../../changes/kde-shortcut-conflicts.md)
- Then Ours Windows: Windows Compatible resets the catalog then disables 41 OS-conflicting catalog rows, including Win+Y, inventing no replacements; actual rebound-chord conflicts are shown. Win+Y Compatible settings/classifier coverage passed offline 2026-10-13; physical OS behavior TBD ([shortcut record](../../changes/archive/windows-workspace-tiling-shortcut.md)). KDE draft/Force preview and disabled-key holder mutation have no Windows counterpart (unsupported, TBD) [record](../../changes/archive/windows-settings.md)
- Variant hook: V-SHORTCUT-CONFLICT.

<a id="r-ctl-07-scrolling-assessment-revert-restores-defaults"></a>
### R-CTL-07: revert restores defaults

- Given: KDE Force previously cleared a noncompiled foreign default chord

- Given (scrolling): previously cleared foreign default chord with staged Compatible choice; shipped profile defaults apply. Same owner-specific fixture as this scenario.

- When: Stage Compatible; Apply; Revert Shortcuts

- Observe: Default conflict still disabled; no automatic restore; separate Revert restores foreign defaults and retains own Disable

- Then COSMIC: Closest COSMIC equivalent in the inspected compositor inventory: removing a `custom` entry re-exposes the system default (no separate restore action in the traced shortcut/system_actions/config-watch/daemon Action paths); there is no preimage/automatic-restore model here; no applicable reference journey. `S(S-cos-shortcut)`
- Then Hyprland/Dwindle: Closest Hyprland equivalent: `unbind`/conflict lookup exist, but no preimage/automatic-restore or separate Revert model in source, so default-conflict restore never runs and has no counterpart here (owner-specific, no applicable journey); `S(S-hyp-shortcut)`
- Then bspwm: Closest bspwm equivalent: node `-t`/`-g` exist, but no preimage/automatic-restore or separate Revert model in the inspected inventory, so default-conflict restore never runs and has no counterpart here (owner-specific, no applicable journey); `S(S-bsp-ctl)`
- Then i3: Closest i3 equivalent: `bindsym`/`bindcode` defines bindings via `configure_binding`; no preimage/automatic-restore or separate Revert model in the inspected inventory, so default-conflict restore never runs (owner-specific, no applicable journey); `S(S-i3-bind)`
- Then xmonad/Tall+Navigation2D: Closest xmonad equivalent: `keys` defines bindings; no preimage/automatic-restore or separate Revert model in the inspected inventory, so default-conflict restore never runs (owner-specific, no applicable journey); `S(S-xmo-ctl)`
- Then sway: Closest sway equivalent: `bindsym`/`bindcode` plus `unbindsym`/`unbindcode`; no preimage/automatic-restore or separate Revert model in the inspected inventory, so default-conflict restore never runs and has no counterpart here (owner-specific, no applicable journey); `S(S-sway-bind)`
- Then qtile/Columns: Closest qtile equivalent: Key definitions only; no preimage/automatic-restore or separate Revert model in the inspected inventory, so default-conflict restore never runs (owner-specific, no applicable journey); `S(S-qti-keys)`
- Then awesome/tile: Closest awesome equivalent: key definitions only; no preimage/automatic-restore or separate Revert model in the inspected inventory, so default-conflict restore never runs (owner-specific, no applicable journey); `S(S-awe-keys)`
- Then niri: owner-specific with no counterpart (no preimage/restore model in the full Action inventory): no applicable journey. `S(S-nir-acts)`.
- Then PaperWM: owner-specific fixture with no counterpart (neither the registered action inventory nor the prefs settings UI pages define any preimage/restore model); no applicable reference journey. `S(S-pap-acts)`.
- Then karousel/Lazy: no-counterpart (the full Actions/definition inventory lists no preimage/automatic-restore or separate Revert verb); no applicable reference journey. `S(S-kar-acts)`.
- Then paneru: owner-specific (no preimage/restore model in the command inventory); no applicable reference journey. `S(S-pan-cmds)`.
- Then Ours KDE: KDE selected: compiled plus discovered defaults/current holders; Revert remains default restoration, not preimage recovery; live outcome TBD [record](../../changes/kde-shortcut-conflicts.md)
- Then Ours Windows: no Force/foreign-default clearing model on Windows, so this fixture is unsupported (TBD). The Windows Revert only discards unsaved edits and reloads the saved file; it never restores foreign defaults [record](../../changes/archive/windows-settings.md)
- Variant hook: V-SHORTCUT-CONFLICT.

## New scenarios (GWT; fixtures/actions/discriminators per the approved expansion record)

Notation, profiles, baselines, legend, and projection rules live in the index. Each scenario below has exactly one Then bullet per profile (14). `S()` tags attach only to the established sub-leg; anything else on that line stays TBD. Column Given bullets are separate fixtures, never H/V ancestry claims. Ours cells cite Engine + adapter source at `6da3d86` plus the new `S(S-ours-kde-rst)` / `S(S-ours-win-rst)` platform keys; selected intent and doc assertions are never evidence.

Restart journeys (config reload is not owner restart):

- COSMIC: client-preserving native journey unestablished; pinned workspace config alone is not restart evidence (`S(S-cos-persist)`).
- Hyprland/sway/niri: reload and exit inventories provide no native client-preserving re-exec (`S(S-hyp-reload)` + `S(S-hyp-shortcut)` + `S(S-sway-reload)` + `S(S-nir-rst)`).
- bspwm: `wm -r` dumps and re-execs with restore (`S(S-bsp-restore)`).
- i3: `restart` saves the layout and re-execs with `--restart` (`S(S-i3-restart)`).
- xmonad: `restart prog True` writes and resumes the windowset; `False` is a separate fresh-start variant (`S(S-xmo-restart)`).
- qtile: `restart` saves metadata on X11; Wayland disables restart support (`S(S-qti-state)`).
- awesome: `awesome.restart()` re-execs, saving order and floating state (`S(S-awe-ctl)`).
- PaperWM/karousel: disable+enable the extension/script with the host session retained (`S(S-pap-rst)` + `S(S-kar-start)`).
- paneru: orderly daemon exit/relaunch; AppExit saves the state and startup loads it for grace-windowed matching (`S(S-pan-rst)`).
- Ours KDE: script stop/start, with a fresh observation and intentional-float id set (`S(S-ours-kde-rst)` baseline); Q3 hydrates settled membership from the same-bus/KWin runtime store before first planning, with NORMATIVE diagnosed-empty behavior for degraded storage (User 2026-10-08). [Offline record](../../changes/archive/kde-intentional-float-restart.md).
- Ours Windows: orderly `tile` stop/start; standalone `restore` is a separate journey (`S(S-ours-win-rst)`).

### R-RST-01: orderly owner restart with apps kept alive

- Given (tree profiles): `WS1=H[A,B*]` 70/30, `WS2=H[C]`, ordinary float F on WS1. Ordinary windows, no rules, scale 1.
- Given (column profiles): `WS1=COL[C1[A],C2[B*]]` with 70/30 widths, `WS2=COL[C3[C]]`, ordinary float F on WS1; shipped defaults apply; viewport recorded.
- Given (paneru): `Space1:{VW1=COL[C1[A],C2[B*]],VW2=COL[C3[C]]}` with 70/30 widths plus ordinary F on VW1; shipped defaults apply.
- Given (Ours KDE, independent degraded-store variant): F has settled explicit
  float intent and the private runtime store exists. Reset independently from
  the healthy-store fixture; retain the same live clients and login session.
- When: orderly owner restart with apps kept alive, using the profile's native journey from the inventory above (plain config reload is explicitly not this journey).
- When (degraded-store variant): stop the owner, replace only its private store
  contents with malformed bytes, then restart the owner. No namespace or
  native-client mutation is part of this leg.
- Observe: layout/ratios/workspaces/float/focus recovered vs fresh adoption.
- Observe (degraded-store variant): diagnosed-empty startup vs blocked/retried
  startup; separate storage acknowledgement after the next settled membership
  update. Ours KDE follows NORMATIVE D4 below (User 2026-10-08); reference and Windows store
  failure counterparts/outcomes remain TBD, not votes against recovery.
- Then COSMIC: no-counterpart for this owner restart with clients alive in the inspected compositor inventory (traced `persist`/`to_pinned`/`add_output` cover pinned workspaces only with no window/layout dump or re-exec verb; `Action` inventory carries `Terminate` with no restart/re-exec verb); no applicable reference journey. `S(S-cos-persist)`
- Then Hyprland/Dwindle: no-counterpart for this owner restart with clients alive (no re-exec verb in the dispatcher inventory; reload keeps the live tree only and exit stops the compositor). `S(S-hyp-reload)` + `S(S-hyp-shortcut)`.
- Then bspwm: 70/30 ratios, WS1/WS2 membership, F float frame, sticky flags and B focus all recovered (split ratios, focused node, history/stack and float rectangles round-trip the dump). `S(S-bsp-restore)`.
- Then i3: 70/30 percents, WS1/WS2 membership, F floating frame and B focus all recovered (percents, focused flag with focus activation, and floating geometry round-trip the layout file). `S(S-i3-restart)`.
- Then xmonad/Tall+Navigation2D: window order, floating map, layout ratio and stack focus all resumed from the file with Tall rendering recalculated. `S(S-xmo-restart)`.
- Then sway: no-counterpart for this owner restart with clients alive (command inventory carries `reload` and `exit` with no restart verb; reload is in-place config only). `S(S-sway-reload)`.
- Then qtile/Columns: X11-only restart dumps QtileState and re-execs (backend-gated; Wayland raises, so no Wayland restart journey): group/layout names, screen assignment and current screen restore while Columns layouts are fresh from config (widths reset to initial_ratio, 70/30 lost) and live windows re-admit in X server query-tree/stacking order via per-window rule match (ordinary float F has no rule match, so re-tiles; fixed/splash/utility/dialog re-float); each stealable newcomer takes focus in scan order, so B focus stays TBD (F: pre-restart stacking/admission order unstated in the fixture; query-tree order is the pinned X stacking contract, not a host input). `S(S-qti-state)` + `S(S-qti-rstadmit)` + `S(S-qti-reload)` + `S(S-qti-add)` + `S(S-qti-float)`; focus queued (fixture; primary F).
- Then awesome/tile: In-place re-exec restores client order via the atexit root-property round-trip (stacking preserved by ordered reparent, saved order permuted back onto the scanned set) plus the persisted floating state (floating xproperty only), with tags recreated from rc (initial floating layout, first tag selected), so tile shares recalculate at shipped mwfact 0.5 and the 70/30 ratio is not recovered (50/50); WS1/WS2 tag membership round-trips through the per-client _NET_WM_DESKTOP index mapping (written on every tag/untag, re-read at manage before the rules run), so the WS1 set (A,B,F) is shown and C stays on WS2's tag; ordinary float F stays floating with its live frame kept (reparent preserves geometry, no fresh tiling); B focus stays TBD: post-restart focus is the last visible client taking rule-focus in query-tree scan order, and the tree preserves pre-restart stacking whose top (last-raised) and bottom ends depend on the unstated A/B/F admission order and B's focus-raise path (F), scanned in the server's query-tree listing order (server behavior external to the pinned call contract, not a missing host input). `S(S-awe-ctl)` + `S(S-awe-tile)` + `S(S-awe-default)` + `S(S-awe-layout)` + `S(S-awe-float)` + `S(S-awe-manage)` + `S(S-awe-hist)` + `S(S-awe-rst)`
- Then niri: no-counterpart for this owner restart with layout recovery (Quit exits and LoadConfigFile reloads config only; no layout dump or re-exec verb). `S(S-nir-rst)`.
- Then PaperWM: controlled disable+enable stages SaveState (monitors/spaces/targetX plus stacking, no float member) and re-adds via prevSpace restore where present else the `xz_comparator` pass; re-adoption re-floats only above-or-minimized windows via `makeScratch` while everything else re-tiles through `add_filter`. The fixture leaves F's float mechanism unstated (above-flag vs minimized-scratch vs list-only float) and the branch reads live host flags: F placement TBD (fixture: record F's above/minimized host flags at stop). Widths per column layout; selection is the host tab-list head: selection TBD (host: tab-list order policy untraced). `S(S-pap-rst)` + `S(S-pap-readopt)` + `S(S-pap-layout)` + `S(S-pap-view)`; F placement queued (fixture), selection queued (host).
- Then karousel/Lazy: orderly owner restart is script disable+enable with the host session retained (no keyboard restart verb in the full Actions/definition inventory; live-only Grid state with no layout dump or store). Existing windows re-admit fresh via `addExistingClients` into `addClient` in `Workspace.windows` order (KWin manage/creation order): the 70/30 tile topology is lost (fresh columns after the last-focused column else the last) while widths re-derive from the live frames (70/30 per fixture) via `preferredWidth` clamped into [min,max]; WS1/WS2 membership follows the KWin desktops through the exactly-1 desktop/activity gate (0/multi desktops or activities float); ordinary float F re-tiles (still shapeable, with no durable intent store, so prior float origin leaves no trace). Focus stays B (B is the KWin active window and admission focuses only an already-focused window, so no steal). Exact order TBD (fixture: creation/manage order unstated). `S(S-kar-rst)` + `S(S-kar-readmit)` + `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kar-ws)` + `S(S-kar-acts)` + `S(S-kwin-winorder)`.
- Then paneru: orderly daemon exit/relaunch saves the state (periodic plus AppExit atomic saves; version-gated load) and startup matches live windows against SessionRestore within the configured grace (enabled by default, 2000ms): hard window_id/pid/bundle matches restore into their saved workspace/virtual-strip/column positions in saved order with Unmanaged cleared, unique title/bundle/identifier/role/subrole fallbacks match only when no hard key collides, and unmatched windows take the fresh path. Widths are not persisted and re-derive from the live OS frames at spawn, so exact widths TBD (fixture: live-frame values after relaunch unstated; runtime: AX settle timing, live-only). Ordinary floating F is absent from the saved strips (extract saves strip columns only; floating drops strip membership), so an unmatched no-rule F freshly tiles. Restore writes no focus; focus TBD (host: post-relaunch active window). `S(S-pan-rst)` + `S(S-pan-admit)` + `S(S-pan-fresh)` + `S(S-pan-flt)`; focus queued (host), widths queued (fixture+live).
- Then Ours KDE: freshly re-observes/adopts windows and hydrates settled intentional F before first planning; F keeps ordinary-float identity and its current frame with no hydration geometry/stacking/focus writes (NORMATIVE Q3 D1-D4, User 2026-10-08). Fixed-window tile overrides also persist membership in the same store (D7 delivered offline 2026-10-09). [Real Planner/entry fixtures](../../../kwin/tests/float-intent.test.ts) and [private store/bus fixtures](../../../crates/omnitiler/src/float_intent_store.rs) cover success-only persistence, clear and fallback. Missing store is empty; corrupt/unreadable/mismatched reads are diagnosed and proceed empty, so degraded restart can lose intent and fixed clients recompute to untouched floats. Native sticky/overlay flags remain observed. Selected User 2026-10-12 D25 (a) target: host workspace set kept; membership re-observed from host with no moves; focus restored only where provable; layout rebuilt via re-adoption with no saved-layout restore; intentional floats stay floating via existing restart store; no setting ([decision](../../decisions.md#restart-persistence)). Layout/ratios/workspace set/native focus recovery verification-only, native journey pending, TBD. [Q3 record](../../changes/archive/kde-intentional-float-restart.md), [D7 record](../../changes/archive/fixed-window-tile-override-restart.md); native journey queued.
- Then Ours Windows: admitted exact live lifetimes hydrate intentional F and both sticky origins from namespaced on-window markers before first tiling; marker hydration preserves live frames and grants no native-write/recovery authority. Agent-observed 2026-10-11, base `e48aed1` plus delivery commit: two graceful owner restarts and sticky visibility across WS2/WS1 passed with four owned helpers; [revision/artifact/evidence record](../../changes/archive/windows-restart-intent.md). Settings persist without layout restoration; fixed-origin tile overrides are now delivered offline with item 13 ([record](../../changes/archive/windows-fixed-size-admission.md)). Selected User 2026-10-12 D25 (a) ([decision](../../decisions.md#restart-persistence)); membership/set/focus native journey pending, TBD, verification-only; item 8 proves float/sticky identity only. Post-restart un-stick remains TBD. Physical input/app dogfood remain user-owned. `S(S-ours-win-rst)` baseline superseded for float/sticky identity.
- Variant hook: NORMATIVE restart recovery (User 2026-10-08; fixed-window
  tile-override persistence delivered offline; D25 set/membership/focus selected
  User 2026-10-12, verification-only).

### R-RST-02: end session, restore session and apps

- Given (tree profiles): saved session with A/B tiled, ordinary F and `B:max` across WS1/WS2. Ordinary windows, no rules, scale 1.
- Given (column profiles): saved session with A/B columns, ordinary F and a maximized column member across WS1/WS2; shipped defaults apply; viewport recorded.
- Given (paneru): saved `Space1:{VW1,VW2}` session with A/B columns, ordinary F and a host-zoomed member; shipped defaults apply.
- Max prep: B enters through the profile-native route named here (maximize where the model owns one, host zoom or width conversion where that is the native form, fullscreen where the profile is maximize-stateless); maximize-stateless profiles run the max leg as a fresh `B:full` journey. Prep citations establish the route only, never the post-session outcome: COSMIC `maximize_request` (`S(S-cos-maxpolicy)`); Hyprland `MAXIMIZED` (`S(S-hyp-fs)`); bspwm none, EWMH fullscreen ADD/REMOVE/TOGGLE (`S(S-bsp-fs)`); i3 none, client FULLSCREEN message (`S(S-i3-max)` + `S(S-i3-fs)`); xmonad none, `fullscreenEventHook` (`S(S-xmo-layout)` + `S(S-xmo-ewmh)`); sway none, workspace/global fullscreen (`S(S-sway-max)` + `S(S-sway-full)`); qtile maximized float state (`S(S-qti-fs)`); awesome maximized boolean (`S(S-awe-fs)`); niri maximized flag (`S(S-nir-maxfs)`); PaperWM width conversion plus fullscreen re-show (`S(S-pap-widthmax)`); karousel host-driven maximize observation (`S(S-kar-maxfs)`); paneru host zoom plus AX fullscreen marker (`S(S-pan-axfs)`); Ours host-owned maximize classification (KDE exceptions, Windows retained).
- When: end session; restore session and apps (session manager plus apps participate; native IDs are replaced and startup order may differ).
- Observe: layout/workspace/native state persisted vs apps freshly admitted.
- Then COSMIC: Pinned workspace shells are recreated from config (`persist` writes pinned workspaces only; `to_pinned` carries output match/tiling flag/id/name with no window/sticky/float/layout state; `add_output` recreates pinned shells); no window layout persists, so relaunched apps freshly admit through normal admission (fullscreen iff flag at relaunch, else dialog/type/min==max floating, else tiling). Exact app set/relaunch order and resulting overlay/placement TBD (fixture: session-manager participation plus per-app relaunch flags/order unstated; host primary H: relaunch owned by unpinned `cosmic-session` via `COSMIC_SESSION_SOCK` with no recorded pin, outside the pinned compositor inventory; no new source pin per scope). `S(S-cos-persist)` + `S(S-cos-admit)`; app set/order/overlay queued (fixture+host; primary H).
- Then Hyprland/Dwindle: no ended-session restore in source (exit stops the compositor; reload is in-place config only with no re-exec, layout dump or window store in the dispatcher/ipc inventories); relaunched apps freshly admit with no session layout restore: ordinary windows tile via the per-window Dwindle anchor, `suggestsFloat` floats only traits or min==max hints at initial map, static `workspace` rule routes plus silent keeps/no-focus, initial map takes the focus monitor with ordinary newcomer focus, and `B:MAXIMIZED` needs a fresh client request consumed/applied at map with no persisted restore; app starts are external launchers per example/hyprland.lua `hyprland.start` plus `exec_cmd` with `HL_INITIAL_WORKSPACE_TOKEN` env for initial-workspace routing; post-session app set/relaunch order and resulting layout TBD (H: relaunch owned by the external session manager outside pinned source, unpinned, no new pin per scope; F: per-app relaunch flags/order unstated); queued. `S(S-hyp-reload)` + `S(S-hyp-shortcut)` + `S(S-hyp-float)` + `S(S-hyp-ins)` + `S(S-hyp-winws)` + `S(S-hyp-newfocus)` + `S(S-hyp-fs)`
- Then bspwm: no maximize state exists, so the max leg runs as a fresh `B:full` EWMH journey (`set_fullscreen` plus `neutralize_occluding_windows`, EWMH ADD/REMOVE/TOGGLE); ended-session layout is not restored in source: startup restores only from an explicit `-s` state path (same-X-session re-exec; a fresh login supplies none), dumped client leaves are keyed by live X window IDs (replaced across sessions; only internal non-client nodes regenerate), and `adopt-orphans` is an explicit manual `wm -o` verb, never startup adoption - so relaunched apps freshly admit through ordinary manage/rules with no layout/focus restore. Exact app set/relaunch order stays TBD (H: relaunch owned by the external session manager outside pinned bspwm source; F: per-app relaunch flags/order unstated). `S(S-bsp-fs)` + `S(S-bsp-restore)` + `S(S-bsp-ctl)`; queued (full leg; fixture+host; primary H)
- Then i3: No maximize verb exists, so the max leg runs as a fresh `B:full` client-message journey; the layout file covers in-place `restart` re-exec only, never a fresh-login session restore (missing/unreadable falls back to `tree_init` with ordinary manage, losing pre-stop tiles). Relaunched apps freshly admit through ordinary manage (fixed/transient float else tile; fullscreen iff flag at relaunch). Exact app set/relaunch order and resulting layout/overlay/placement TBD (H: relaunch owned by the external session manager outside pinned source, unpinned, no new pin per scope; F: per-app relaunch flags/order unstated); queued (full leg; fixture+host; primary H). `S(S-i3-max)` + `S(S-i3-fs)` + `S(S-i3-restart)`
- Then xmonad/Tall+Navigation2D: no maximize state exists, so the max leg runs as a fresh `B:full` event-hook journey (`ClientMessage` add via `doFullFloat` fullscreen float with `_NET_WM_STATE` converged via `chWstate`, remove/toggle via `doSink`; stack retained). `StateFile` covers runtime restart only (`writeStateToFile`; resume-only `readStateFile` removes the file; `restart prog True` resumes): after a session end there is no resumed windowset, so startup scans live top-level windows, drops gone ones and freshly admits each relaunched app through `manage` (fixed/transient float else tile) per `S(S-xmo-restart)`; fullscreen needs a fresh `ClientMessage`, no persisted `_NET_WM_STATE` restore exists. Exact app set/relaunch order and resulting overlay/placement TBD (fixture: session-manager participation plus per-app relaunch flags/order unstated; native IDs replaced and startup order may differ; host primary H: restore journey owned by the external session manager outside pinned xmonad source; no new source pin per scope). `S(S-xmo-layout)` + `S(S-xmo-ewmh)` + `S(S-xmo-restart)` + `S(S-xmo-admit)`; queued (full leg; fixture+host; primary H).
- Then sway: no maximize verb exists, so the max leg runs as a fresh `B:full` journey (workspace/global fullscreen via the client-request mode path); no restart or session-restore path exists in source (`reload` is in-place config only, `exit` stops the compositor, no layout dump/store/re-exec verb), so relaunched apps freshly admit through ordinary admission (tiling via the focus-inactive anchor, fixed-size/dialog float, fullscreen iff flag at relaunch). Exact app set/relaunch order and resulting layout/overlay/placement TBD (H: relaunch owned by the external session manager outside pinned source, unpinned, no new pin per scope; F: per-app relaunch flags/order unstated); queued (full leg; fixture+host; primary H). `S(S-sway-max)` + `S(S-sway-full)` + `S(S-sway-reload)` + `S(S-sway-ins)` + `S(S-sway-spc)`
- Then qtile/Columns: restart state covers in-place re-exec only (groups/layouts/screens/scratchpads, no per-window store; Wayland has no restart journey); ended-session membership/layout recovery has no counterpart in the inspected inventory (no session-manager wiring, no layout dump beyond state); relaunched apps freshly admit through ordinary manage with B:max needing a fresh maximized state; exact app set/relaunch order and resulting layout TBD (H: relaunch owned by the external session manager outside pinned source; F: per-app relaunch flags/order unstated). `S(S-qti-state)` + `S(S-qti-add)` + `S(S-qti-fs)`; queued (host; primary H).
- Then awesome/tile: Atexit order plus the floating xproperty cover in-place re-exec within the X session only; ended-session membership/layout recovery has no counterpart in the inspected inventory (no session-manager wiring, no layout dump beyond order/floating); relaunched apps freshly admit through ordinary manage (fixed-size float else tile, with transient affecting tags/screen rather than implicit float, newcomer focus via the shipped global rule) with `B:max` needing a fresh maximized boolean (no persisted state restore); exact app set/relaunch order and resulting overlay/placement TBD (H: app relaunch owned by the external session manager outside the pinned source; F: per-app relaunch flags/order unstated); queued. `S(S-awe-ctl)` + `S(S-awe-manage)` + `S(S-awe-float)` + `S(S-awe-fs)`
- Then niri: Quit exits and LoadConfigFile reloads config only (no layout dump, re-exec verb, or store in the full Action inventory); startup spawns fresh commands with no layout restore while the session launcher starts the compositor only; relaunched apps freshly admit through ordinary scrolling admission. Exact app set/relaunch order and resulting layout/overlay/placement TBD (H: relaunch owned by the external session manager plus per-app participation outside pinned niri source, unpinned, no new pin per scope; F: per-app relaunch flags/order unstated). `S(S-nir-rst)` + `S(S-nir-ins)` + `S(S-nir-spc)` + `S(S-nir-maxfs)`; queued (session; primary H).
- Then PaperWM: SaveState covers controlled restarts only; ended-session topology recovery TBD (session wiring untraced at pin). `S(S-pap-rst)`; queued.
- Then karousel/Lazy: relaunched apps re-admit through the script startup path (`addExistingClients` into `addClient` in `Workspace.windows` order as fresh columns with no layout restore; live-only Grid, no session-dump/store verb in the full Actions/definition inventory). Post-session app set/launch order, cross-session identity rematch and resulting layout TBD (H: session restore lives in the unpinned external Plasma session manager ksmserver plus per-app session participation, outside pinned KWin/karousel source). `S(S-kar-rst)` + `S(S-kar-acts)` + `S(S-kar-start)` + `S(S-kwin-winorder)`.
- Then paneru: strip/column metadata persists in the state file; cross-session window-identity rematch, host-zoom state and resulting layout are TBD. Host zoom remains an applicable host-owned state. `S(S-pan-rst)`; queued.
- Then Ours KDE: host-maximized members classify as tile exceptions while settings/gaps restore at startup; windows freshly re-admit and host-max restore is TBD. Q3 intentional markers cannot cross a new bus/KWin namespace (NORMATIVE D1, User 2026-10-08), verified offline. Selected User 2026-10-12 D26 (b): restore owned already-persisted gaps/settings; re-observe host workspace set/membership/maximize state; no invented layout restore; listed 0.1 known limitation intentional-float memory does not survive logout/login under unchanged session-scoped REQ-RST-01d ([decision](../../decisions.md#restart-persistence)). Verification-only; actual logout/session-manager journey remains TBD. `S(S-ours-kde-rst)` baseline; [store fixtures](../../../crates/omnitiler/src/float_intent_store.rs); queued.
- Then Ours Windows: the settings file carries gaps/preset durably while the ledger is runtime with explicit standalone restore; restart freshly observes and host-max restore is TBD. Selected User 2026-10-12 D26 (b): restore owned already-persisted gaps/settings; re-observe host set/membership/maximize; no invented layout restore; listed 0.1 known limitation intentional-float memory does not survive logout/login with reopened apps as new identities ([decision](../../decisions.md#restart-persistence)). Verification-only; native dogfood/live check pending. `S(S-ours-win-rst)`; queued.
- Variant hook: NORMATIVE D26 selected User 2026-10-12 (b) with listed cross-login known limitation; no invented layout restore, verification-only.

### R-RST-03: native float geometry changes while the owner is stopped

- Given (tree profiles): A tiled and F intentionally floating on WS1;
  record F's live frame. Ordinary resizable clients, no rules, scale 1.
- Given (column profiles): one column containing A and intentional float F
  on the same workspace; record F's live frame and viewport. No H/V ancestry
  is asserted. Paneru uses one virtual workspace in a retained native Space.
- When: stop the tiler owner with clients alive; move/resize F through the
  host to a different valid frame; start the owner in the same login session.
  This is a stopped-owner gap, not the direct native re-exec in R-RST-01.
- Observe: intentional-float identity and whether the current live frame is
  preserved or a saved pre-stop frame is written back; record any geometry,
  focus or stacking writes to F during adoption.
- Then COSMIC: client-preserving stopped-owner journey unestablished;
  pinned workspace config serializes pinned workspaces only with no float
  identity or geometry; whether the live moved frame survives or a saved
  frame writes back is TBD (no client-preserving journey to compare
  against; store timing-dependent).
  `S(S-cos-persist)`.
- Then Hyprland/Dwindle: No counterpart in source for this stopped-owner gap: exit stops the compositor with no re-exec, layout dump or window store in the dispatcher/ipc inventories and reload is in-place config only, so no stopped-owner save/re-adopt journey exists and no serialized float frame exists to compare against the live drift; the drift outcome never runs (owner-specific, no applicable journey). `S(S-hyp-reload)` + `S(S-hyp-shortcut)`.
- Then bspwm: dump round-trips the float frame (`floatingRectangle`
  dumped/restored) on the direct re-exec journey, but native movement
  during this stopped-owner gap is a different journey; whether the live
  moved frame survives or the saved pre-stop frame writes back is TBD
  (store timing-dependent, live-only). Dump fields round-trip
  focus/history/rectangles with no tile-override member.
  `S(S-bsp-restore)`.
- Then i3: layout save/re-exec round-trips percents, focus and floating
  geometry on the direct restart journey only; this stopped-owner gap with
  a host move in between is not that journey, so the resulting frame is
  TBD (store timing-dependent, live-only). Layout-file fields carry
  percent/focus/rect/floating-geometry with no tile-override member.
  `S(S-i3-restart)`.
- Then xmonad/Tall+Navigation2D: resume restores the saved windowset
  (incl the floating map) on the direct resume journey; stopped-owner
  native movement and resulting frame TBD (client timing, live-only). A
  malformed StateFile yields no resume (failing `Read` parses to
  `Nothing`), but that fresh-start leg does not establish this drift gap.
  The two-member StateFile (windowset plus ext-state) carries no
  tile-override member.
  `S(S-xmo-restart)`.
- Then sway: No counterpart in source for this stopped-owner gap: `exit` stops the compositor with no re-exec, layout dump or window store in the inspected command inventory and `reload` is in-place config only, so no stopped-owner save/re-adopt journey exists and no serialized float frame exists to compare against the live drift; the drift outcome never runs (owner-specific, no applicable journey). `S(S-sway-reload)`.
- Then qtile/Columns: restart metadata carries groups/layouts/screens only
  with no per-window float geometry and no tile-override member, so there
  is no saved F frame to compare; native journey applicability and drift
  outcome TBD (no serialized float frame; store timing-dependent). `S(S-qti-state)`.
- Then awesome/tile: restart persists client order and the explicit
  floating state (explicit/implicit precedence) while tile shares
  recalculate; that journey does not establish this stopped-owner geometry
  leg, so whether the live moved frame survives is TBD (store
  timing-dependent, live-only). Persisted members are client order plus
  registered floating properties, with no tile-override member.
  `S(S-awe-ctl)` + `S(S-awe-fixed-dynamic)`.
- Then niri: stopped-owner journey and drift outcome TBD; config reload is
  not owner restart (quit plus config reload only, no layout dump or
  re-exec verb); no serialized float frame exists to compare. `S(S-nir-rst)`.
- Then PaperWM: disable/enable has a counterpart (SaveState stages
  monitors/spaces/targetX only, no float member); re-adoption re-derives
  float from live host flags (above/minimized re-float via scratch) while
  list-only floats have no staged counterpart, and the fixture does not
  specify F's float mechanism, so whether F re-floats or re-tiles is TBD
  (host-state boundary, live-only). The staged maps carry no
  tile-override member. `S(S-pap-readopt)` + `S(S-pap-rst)`.
- Then karousel/Lazy: script disable+enable re-admits every live client
  fresh (`addExistingClients` into `addClient`, re-evaluating current
  shapeability plus rules plus the exactly-1 desktop/activity gate with no durable intent store), so a still-shapeable F
  re-tiles as a fresh column after the last-focused column else the last; the prior float origin is never consulted, but the fresh width derives from the moved frame (`preferredWidth` wraps the live frame width) clamped into [min,max];
  no geometry, stacking or focus write is issued to F on this path.
  No float-intent or
  tile-override store (no persist verb in the full Actions/definition inventory).
  Exact order TBD (fixture: creation/manage order unstated). Exact width value TBD (fixture: moved frame value chosen in the stopped gap, unstated). Exact focus TBD (fixture: host move/resize path unstated, so host-side refocus cannot be resolved; the script writes no focus on this path).
  `S(S-kar-readmit)` + `S(S-kar-ins)` + `S(S-kar-min)` + `S(S-kar-ws)` + `S(S-kar-acts)` + `S(S-kwin-winorder)`.
- Then paneru: daemon exit/relaunch has a counterpart (durable state file
  plus grace-windowed startup matching); F's drift outcome TBD (identity
  rematch timing-dependent, live-only). A version-mismatched state file
  loads as absent, but that startup leg does not establish this drift gap.
  The staged versioned state carries no tile-override member. `S(S-pan-rst)`.
- Then Ours KDE: intentional F's identity and current moved/resized live frame
  survive through membership-only adoption (NORMATIVE Q3 D2, User 2026-10-08); no hydration
  geometry, stacking or focus writes to F. Implemented offline with counted
  setters and real Planner [entry fixtures](../../../kwin/tests/float-intent.test.ts).
  D7's additive tile membership stores no frames and adds no writes to F;
  only matched explicit tile overrides receive normal tile placement.
  Native stopped-owner journey remains TBD; degraded store follows D4.
- Then Ours Windows: intentional identity retention is selected by R-RST-01;
  stopped-owner drift outcome TBD. Windows handoff item 8; behavior unchanged.
- Variant hook: NORMATIVE Q3 D2 membership-only adoption selected for KDE
  (User 2026-10-08);
  saved float geometry restoration is a review alternative, not a setting.

### R-RST-04: distinguish intentional and automatic fixed floats on restart

- Given (tree profiles): E and F are live ordinary clients with min=max
  640x480 on a tiled-designated workspace. E has explicit ordinary-float
  intent; F floated automatically from fixed-size admission with no explicit
  float command. Record these distinct origins before stopping the owner.
- Given (column profiles): the same E/F origin fixture on one workspace,
  with the viewport recorded; no H/V tree is asserted. Paneru uses one
  virtual workspace in a retained native Space. Fixture applicability is TBD
  where the profile lacks these distinct origins.
- When: stop the owner with E/F alive; both clients clear their fixed-size
  constraints to become resizable; restart the owner in the same login
  session. Reset independently from R-RST-03; no explicit tile command.
- Discriminating D7 variant (independent reset): fixed T is explicitly tiled
  successfully before stop; T loses hints while stopped, restarts, then gains
  fixed hints while workspace tiling is disabled; enable tiling. Control: N
  was non-fixed when explicitly tiled, gains fixed hints while stopped, then
  restarts. Reference outcomes for these added legs are TBD for every profile;
  native Ours legs remain TBD.
- Observe: E's intentional membership vs F's fresh admission using current
  hints; whether automatic float origin accidentally became durable intent.
- Then COSMIC: fixed admission floats fixed-size maps while ordinary maps
  tile (admission-only classification); stopped-owner journey and origin
  recovery after the hint clear TBD (no stopped-owner re-admission
  evidence; client timing, live-only). Config persistence covers pinned
  workspaces only with no tile-override member; T/N discriminator legs TBD
  (same gap timing). `S(S-cos-fixed-admission)` + `S(S-cos-persist)`.
- Then Hyprland/Dwindle: `suggestsFloat` floats min==max hints at initial
  map only; fixture E/F origins are constructible, but stopped-owner
  journey and origin recovery after the hint clear TBD (no hint-change
  recompute in source; client timing, live-only). No float or tile state
  dump exists (config reload only, no re-exec verb); T/N discriminator
  legs TBD. `S(S-hyp-float)` + `S(S-hyp-reload)`.
- Then bspwm: fixed-size admission floats while ordinary admission tiles;
  origin recovery under this stopped-owner journey TBD (R-RST-01 restore
  alone does not establish the discriminator; client timing, live-only).
  Dump fields round-trip focus/history/rectangles with no tile-override
  member; T/N discriminator legs TBD.
  `S(S-bsp-admit)` + `S(S-bsp-restore)`.
- Then i3: fixed-size min==max admission floats while tiled allocation
  ignores hints; post-admission hint updates never re-admit tiles (float
  clamp only). Origin recovery after hint changes during this
  stopped-owner gap TBD (store/client-timing-dependent, live-only).
  Layout-file fields carry percent/focus/rect/floating-geometry with no
  tile-override member; T/N discriminator legs TBD. `S(S-i3-min)` +
  `S(S-i3-fixed-runtime)` + `S(S-i3-restart)`.
- Then xmonad/Tall+Navigation2D: fixed/transient check floats at manage
  only; later status changes only via manual float/sink. Origin recovery
  after hint changes during this   stopped-owner gap TBD (client timing,
  live-only). The two-member StateFile (windowset plus ext-state) carries
  no tile-override member; T/N discriminator legs TBD.
  `S(S-xmo-float)` + `S(S-xmo-restart)`.
- Then sway: `wants_floating` floats fixed-size (min==max) at map while
  tiled arrange ignores hints; the runtime hint path handles urgency only
  with no re-admission. Fixture applicability is established, but
  stopped-owner journey and origin recovery TBD (client timing,
  live-only). No float or tile state dump exists (in-place reload only);
  T/N discriminator legs TBD.
  `S(S-sway-max)` + `S(S-sway-min)` + `S(S-sway-reload)`.
- Then qtile/Columns: fixed-size rules float at admission while tiled
  placement ignores hints and hint refresh never promotes tiles.
  Fixture applicability is established, but stopped-owner journey and
  exact origin recovery TBD (client timing, live-only). QtileState carries
  groups/layouts/screens/scratchpads with no tile-override member; T/N
  discriminator legs TBD. `S(S-qti-float)` + `S(S-qti-min)` +
  `S(S-qti-state)`.
- Then awesome/tile: explicit vs implicit floating exists (hint signals
  recompute implicit floating unless an explicit state overrides), but
  recovery after hint changes during this stopped-owner gap TBD (client
  timing, live-only). Persisted members are client order plus registered
  floating properties, with no tile-override member; T/N discriminator
  legs TBD.
  `S(S-awe-fixed-dynamic)` + `S(S-awe-ctl)`.
- Then niri: `compute_open_floating` classifies at the open callsites only
  (explicit rule, parent, or fixed positive height); later changes only via
  the plain tile-move toggle. Stopped-owner journey and origin recovery TBD
  (client timing, live-only). No layout dump exists at all (quit plus
  config reload only); T/N discriminator legs TBD. `S(S-nir-fixed-open)` + `S(S-nir-flttoggle)` + `S(S-nir-rst)`.
- Then PaperWM: distinct fixed origin has no counterpart (`add_filter`
  admits Normal non-transient windows with no fixed-size branch, so a
  fixed-size client tiles rather than floats; re-adoption re-floats only
  above-or-minimized windows), so the E/F origin fixture is
  fixture-inapplicable here with no applicable journey. The T/N
  discriminator legs share the no-workspace-toggle absence (no workspace
  enable verb in the registered inventory): neither leg has a runnable
  journey. The staged maps carry no tile-override member.
  `S(S-pap-spc)` + `S(S-pap-readopt)` + `S(S-pap-acts)`.
- Then karousel/Lazy: re-admission classifies E and F purely from current
  host flags (`addExistingClients` into `addClient` against live
  moveable/resizeable; no durable origin store), so with fixed hints
  cleared both re-admit Tiled as fresh columns regardless of prior
  explicit/automatic origin; exact order/widths TBD (native geometry,
  live-only). T/N discriminator legs TBD (same gap timing).
  `S(S-kar-readmit)` + `S(S-kar-spc)` + `S(S-kwin-resizeable)`.
- Then paneru: distinct fixed origin has no counterpart (role-gated
  admission with no size predicate; float is rule-assigned; AX exposes no
  min/max hint equality), so the E/F origin fixture is inapplicable here
  with no applicable journey. The staged versioned state
  carries no tile-override member; T/N discriminator legs share the absence
  with no applicable journey.
  `S(S-pan-admit)` + `S(S-pan-rst)`.
- Then Ours KDE: selected R-RST-01 preserves intentional E; Q2 NORMATIVE
  D7 (User 2026-10-08) recomputes automatic F from current hints, so F is newly tiled here.
  Implemented offline with real Planner [two-owner hint-loss fixtures](../../../kwin/tests/float-intent.test.ts),
  under NORMATIVE Q3 D1-D4; E takes no slot and receives no hydration writes.
  Native journey TBD; automatic origin is recomputed, not persisted; fixed-window
  Q2 tile overrides persist in the same membership store (D7 delivered offline
  2026-10-09). In the discriminator, T's matched explicit identity survives
  hint loss and later enable; N gains no durable tile override and recomputes
  fixed-floating without writes. [D7 record](../../changes/archive/fixed-window-tile-override-restart.md).
- Then Ours Windows: intentional E keeps item 8 identity; automatic F is
  recomputed on restart, so hint loss tiles it. Matched fixed-origin tile T
  retains suppression through hint loss/enable; generic non-fixed N writes no
  tile marker and later recomputes fixed-floating. Windows production/stamped
  retained Engine and owned message-only marker tests pass offline 2026-10-11,
  prerequisite `a41fcb5` plus runtime delivery
  ([record](../../changes/archive/windows-fixed-size-admission.md)). Ordinary-app
  combined restart/recovery journey remains TBD; no new recovery authority.
- Variant hook: NORMATIVE intentional membership vs recomputed automatic
  origin (User 2026-10-08; tile-override persistence delivered offline);
  existing R-RST-01 and Q2 D7 intent, not a new hint-classification decision.

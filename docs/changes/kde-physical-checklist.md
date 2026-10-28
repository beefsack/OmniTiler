# KDE physical and user-run checklist

- Scope: one KDE Plasma session, physical keys and pointer, preferably normal resizable [live fixture windows](../../crates/tiler-live-fixture/README.md). Read the [live KWin guide](../live-kwin-testing.md) first. No new implementation or policy is selected here.
- Status: UNRUN. Earlier callback/offline checks and user physical evidence through 2026-10-28 are partial evidence, not passes for this sheet. In particular, renewed toggle diagnostics after `4e504586` remain pending.
- Gate: the [KDE v2 port](../backlog.md#open-work), [native acceptance remainder](kde-live-acceptance.md#remaining-coverage-and-next-step), and the triage queues cited per row. A result closes only the named sub-leg, not the entire backlog item or Stage 8 ownership audit.
- Budget: about 50-60 minutes for setup/restoration, T1, S1-S8 and N1-N2. With two outputs, substitute D1-D3 for optional single-output permutations; allow about 10 minutes. D4 and O1-O5 are optional extensions, not extra mandatory steps.
- Boundary: fixture-only management; preserve foreign windows, desktop identities/count/order/names, views, effects, shortcuts and configuration. No desktop creation/deletion, trailing/append commands, hotplug, logout, lock, forced loss, Steam/game or automatic-float stacking experiments. Known border disappearance during drag is skipped.
- Newest rule: R-MAX-10/11 supersede the old fullscreen Meta+G/Meta+M refusals. R-MAX-02 directional focus remains a separate unresolved policy difference, not a fence to invent here.

## Batched physical run sheet 2026-10-29

- Order: preflight; isolated T1 capture; restore; S1-S5 share A/B; S6-S7 add C; S8 uses existing workspaces; N1-N2 share Settings; conditional D block uses the outputs already present. Do not change display topology for this sheet.
- Follow the guide's [baseline and ownership checks](../live-kwin-testing.md#live-evidence), [helper accounting](../live-kwin-testing.md#kwin-script-lifecycle), [evaluated readiness gate](../live-kwin-testing.md#evaluated-readiness-gate-controller), correlated complete request/reply capture and restoration comparison. A successful startup receipt or quiet journal is not readiness.
- Stop the leg at the first unexpected refusal, parser/ownership/readiness surprise, script error, protected-state change or baseline mismatch. Retain the evidence and restore before considering another leg. Do not repeat around a hold or inert instance.
- Report each ID/sub-leg as PASS/FAIL + the requested observation, or BLOCKED (condition) / NOT RUN. All result lines start UNRUN. Keep artifacts outside the repository; send ordered result lines and artifact filenames, not app content, titles or raw key logs.

### Session setup (once, repo root, inside devenv)

- Preserve source/build identity, runtime float-intent store including absence, project settings, kwinrc, exact shortcut tuples, loaded/active effects, protected-window states/frames/memberships, desktop/output views and pointer, per the guide. Use its accepted baseline checker before startup and after restoration; retain raw complete replies. Do not guess store paths or script IDs.
- Start only from a verified DOWN stack. `just dev-status` is read-only. If a production script is already loaded without an authoritative exact restore route, stop: do not unload it to make room. Use the guide's receipt-bound manual lifecycle or documented scoped `just dev-on --scope-class OmniTilerTestWindow` / `just dev-off` route only after its preconditions hold. Re-prove evaluated readiness after every start. Do not use the obsolete `live-test.sh run` wrapper.
- Fixture T1 and T2, as in the [two-terminal recipe](../../crates/tiler-live-fixture/README.md#smoke-two-terminals-from-the-repo-root-after-an-env-restart):

```sh
RUN_DIR="$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/tiler-live-fixture-XXXXXX")"
chmod 700 "$RUN_DIR"
cargo run --locked -p tiler-live-fixture -- serve --dir "$RUN_DIR"
```

```sh
printf 'socket: '; read -r SOCK
printf 'nonce: '; read -r NONCE
run() { cargo run --locked -p tiler-live-fixture -- send --socket "$SOCK" --nonce "$NONCE" "$@"; }
```

- Keep the exact announcement JSON. Use a fresh `request_id` on every call. Prebuild before timing. T1 needs a fresh server with no windows; after its complete teardown start a fresh server and paste its new socket/nonce into T2.
- Main fixture controls (spawn A/B only after T1):

```sh
run '{"v":1,"request_id":"spawn-s0","op":"spawn","windows":[{"id":"a","size_px":[640,480]},{"id":"b","size_px":[640,480]}]}'
run '{"v":1,"request_id":"get-s0","op":"get","ids":["a","b"]}'
```

- Join caller IDs/PIDs to native KWin UUIDs with the guide's accepted [fixture attribution](../live-kwin-testing.md#live-evidence). `get` supplies observed predicates, not native membership/output/tree proof; Wayland position/visibility/fullscreen fields may be unknown. Retain native joins and full correlated `Describe` replies for member/overlay/operation state. `ReadWorldPresentation` alone cannot prove no pending operations.
- Stage only fixture A/B on one existing workspace WS1: `H[A,B]` means left/right, `V[A,B]` top/bottom, `*` focused. Use normal admission and, if needed, click B / `Meta+O` once to orient the split. Confirm the actual tree and frames before a topology-sensitive row; never infer a tree from appearance alone. Restore the row's starting state between sub-legs.
- Workspace rows require already-existing WS1 and WS2 plus an untouched empty trailing spare wherever filling WS2 would otherwise append a desktop. Verify numbered bindings resolve those exact existing workspaces. No `Meta+0`, `Meta+Shift+0`, desktop-count controls or scripted create/remove calls. If admission, transfer, mode startup or cleanup would add/retire a desktop, mark that row BLOCKED; do not create a test desktop. Leave unrelated windows in place.
- Working keys: `Meta+G` float, `Meta+M` maximize, `Meta+F11` fullscreen, `Meta+O` orientation; arrows/HJKL focus, Shift variants move, Alt variants grow, Alt+Shift variants shrink. Numbered selection `Meta+1/2`; numbered send-follow `Meta+Shift+1/2`; relative follow next/previous `Meta+Ctrl+Shift+Right/Left`. Check the actual assigned chords first; never resolve a conflict by changing another application's binding.
- Focus-sensitive sequences run without clicking a terminal between keys. Observe active focus before terminal readback; terminal focus invalidates that part of the sequence. Refocus the named fixture before the next trial. `invokeShortcut` and callback delivery do not count as physical keys.
- Failure packet for every row: ordered intended keys, actual focused fixture, before/after native flags/frame/output/membership and tree when relevant, journal slice and correlated command/reply identities; include refusal/hold reason and first divergence. Check the unfiltered current-KWin-PID stream for `QT_CATEGORY=kwin_scripting` errors separately from `omnitiler:` lines. Missing evidence is incomplete, not PASS.

### Block T (one isolated admission, about 6 minutes)

- T1 open-animation timing.
  - Setup/steps: follow the existing [single-window capture procedure](../dev-loop.md#user-run-single-window-capture), unchanged, including trace build, journal cursor, video and exactly one normal fixture admission. Do not add shortcuts or extra windows to that capture. This row references the procedure, not a second launcher recipe.
  - Expected: a usable single-window video/journal pair with source-correlated added/debounce/send/reply/write/observed evidence where emitted; this is diagnosis, not a required zero-jump or fix claim. Native presentation timing is not established by setter acceptance or wall-clock deltas.
  - Result: UNRUN - CAPTURED/INCOMPLETE + first-visible/open-effect-end/jump frame offsets, intervals, `match`, missing events and any clock anomaly. On failure retain the unfiltered stream and video rather than guessing an interval.
  - Closes: capture request in [open-animation record, Next](kde-open-animation-timing.md#next) and [dev-loop timing section](../dev-loop.md#kde-open-animation-timing-probe-trace-only), added in `72282bee`. Restore/compare before the main fixture leg.

### Block S (one output; about 25 minutes)

- S1 ordinary unfloat, no focus-change workaround.
  - Setup: WS1 A/B tiled, B focused.
  - Keys: `Meta+G`, wait for floating B; `Meta+G` again, without clicking A or a terminal. Repeat once from restored A/B.
  - Expected: B immediately fresh-admits/retiles; A reflows without a subsequent focus change. Unfloat is fresh admission, not old-slot restore.
  - Result: UNRUN - PASS/FAIL + immediate retile YES/NO and first frame/focus divergence; note any later event that finally triggered tiling, but do not use it to turn the row into PASS.
  - Closes: backlog P0 "KDE Meta+G unfloat does not re-tile until focus changes" (`docs/backlog.md`, original lines 492-498); [restart re-check record](archive/kde-unfloat-immediate-retile.md), [R-FLT-01](../spec/reference-outcomes/floating.md#r-flt-01-toggle-float-then-unfloat).

- S2 renewed native maximize/fullscreen settlement and physical repress.
  - Setup: A/B tiled, B focused; compare each settled native flag before the next press.
  - Keys: `Meta+M` on/off twice; `Meta+F11` on/off twice. Then maximize with `Meta+M`, restore B with its native title-bar restore control, click B and `Meta+M` again, then restore. Repeat the toggle pairs from intentionally floating B; unfloat afterwards. One final trial per chord: hold for 2 seconds, release all keys, then tap to undo only if the observed state requires it.
  - Expected: one native attempt per discrete activation, settled native state agrees with the action; no false `maximize-refused`, fullscreen refusal or `shortcut-failed` on an applied action. Native restore followed by repress makes a fresh attempt. Tiled overlays retain their allocation; ordinary float base survives. A real refusal is a stop, not something to suppress. Held-key observation does not prove every autorepeat ordering.
  - Result: UNRUN - PASS/FAIL by chord/origin + flags at settlement, duplicate/late toggles and any diagnostic/native disagreement; retain command and first fresh accepted observation, not same-stack readback alone.
  - Closes: [settlement correction `4e504586`](archive/kde-native-state-settlement.md#acceptance-and-approach), [B1 diagnostic failure/remainder](kde-live-acceptance.md#native-diagnostics-repeat-signals-and-drag-limits), [R-MAX-04](../spec/reference-outcomes/maximize-fullscreen.md#r-max-04-shortcut-maximize-native-restore-repress). Other B1 origins/races remain open.

- S3 float while focused fullscreen, single and double toggle (R-MAX-10).
  - Setup: fresh `H[A,B*]`.
  - Keys (i): `Meta+F11`, `Meta+G`, observe B still fullscreen, `Meta+F11` to exit. Restore B tiled. (ii) Fresh start: `Meta+F11`, `Meta+G`, `Meta+G`, `Meta+F11`.
  - Expected: (i) float membership changes immediately under fullscreen, B's slot releases and A reflows, zero native writes to fullscreen B from float; exit reveals B floating with durable placement or centered 60% fallback. (ii) second float toggle fresh-admits B invisibly; exit reveals the current tile. Tier 1 majority float-retention selection, not the historical refusal.
  - Result: UNRUN - PASS/FAIL per leg + exit tiled/floating state, A reflow and subject-write evidence. No terminal click before each exit; visual fullscreen alone cannot prove hidden membership.
  - Closes: backlog KDE v2 "Fullscreen float/maximize" (original lines 424-441), [float delivery](archive/fullscreen-float-toggle.md), [R-MAX-10](../spec/reference-outcomes/maximize-fullscreen.md#r-max-10-float-toggle-on-fullscreen); partial Stage 4/5 fullscreen-window shortcut proof.

- S4 maximize replaces focused fullscreen (R-MAX-11).
  - Setup: fresh `H[A,B*]`; repeat from intentional floating B if S2 stayed clean.
  - Keys: `Meta+F11`, then `Meta+M` once; observe native fullscreen cleared and maximize set; `Meta+M` again to restore base.
  - Expected: Hyprland-selected replacement, clear fullscreen then maximize, no intervening confirmation gate/retry; no geometry/focus/stack writes from that transition. Tiled slot/share or floating base and later restoration are tentative recorded details: compare them and report separately, do not silently promote them to a new decision.
  - Result: UNRUN - PASS/FAIL for replacement/diagnostics + tiled/float-base observation, both flags and sibling allocations. Stop on dropped input, false refusal or flag corruption.
  - Closes: same backlog fullscreen item; [maximize delivery](archive/fullscreen-maximize-toggle.md), [R-MAX-11](../spec/reference-outcomes/maximize-fullscreen.md#r-max-11-maximize-toggle-on-fullscreen), [settlement remainder](kde-live-acceptance.md#native-diagnostics-repeat-signals-and-drag-limits); partial Stage 4/5 fullscreen-window shortcut proof. Does not settle R-MAX-02 directional focus.

- S5 rapid physical input / perceived latency.
  - Setup: ordinary `H[A,B*]`, no overlays; same-axis move set to group-with-neighbor, original setting preserved. No deliberate waits inside each burst.
  - Keys: four `Meta+Shift+Left` / `Meta+Shift+Right` pairs (8 moves); settle/read back. Then four repetitions of `Meta+Shift+Left`, `Meta+Right`, `Meta+Shift+Left`, `Meta+Right` (16 move/focus presses: B moves left, focus A, A moves left, focus B). Keep an ordered count, no raw-key logger.
  - Expected: each delivered directional command applies in FIFO order with subject bound when drained; both bursts end at `H[A,B*]`, no stuck geometry, short-height arrivals, dropped commands or perceptible stalls. Input-ordering policy remains Tentative; this is physical delivery/feel evidence, not policy approval.
  - Result: UNRUN - PASS/FAIL + intended/observed command counts, final order/focus, worst perceived delay and first missing/misordered action. Stop after the first failed burst; do not add more spam.
  - Closes: backlog P0 "KDE rapid input drops and stalled geometry" (original lines 529-536), [rapid-input outcome](archive/kde-rapid-input-geometry.md). Conditional cross-output burst is D2.

- S6 float-out survivor shares (D22).
  - Setup: add normal C with `run '{"v":1,"request_id":"spawn-s6","op":"spawn","windows":[{"id":"c"}]}'`. Establish flat `H[A,B,C]` by ordinary moves/drag, then native split resize to approximately 50/25/25; confirm actual shares. BLOCKED if the exact tree cannot be staged safely.
  - Keys: click B, `Meta+G` once.
  - Expected: B leaves the tree, A/C preserve their prior ratio, approximately 67/33 after gap/decor adjustments, not equal halves. Thin disclosed Tier 1 basis: i3/sway one family plus COSMIC observation; no evidenced alternative. Measure from the actual pre-shares, not an assumed pixel width.
  - Result: UNRUN - PASS/FAIL + before/after A:C shares and native frames; note equalization or unexpected focus/geometry writes. Unfloat B after recording.
  - Closes: backlog triage D22 + native queue (original lines 1042, 1052), [decision](../decisions.md#window-state-float-sticky-maximize-fullscreen), [R-FLT-03](../spec/reference-outcomes/floating.md#r-flt-03-float-out-survivor-widths).

- S7 minimize/rejoin (D23), no rehome experiment.
  - Setup: flat A/B/C; click A, C, B in that order. Keep C's MRU identity in the capture.
  - Steps: minimize B using its native minimize control; inspect survivor reflow and focus. Restore B using the fixture `set` operation `mode:"windowed"` (not a taskbar activation), then inspect before clicking B. If desired, use `run '{"v":1,"request_id":"restore-s7","op":"set","ids":["b"],"patch":{"mode":"windowed"}}'` from T2; the terminal click means focus must be assessed as no tiler focus setter, not proof C stayed active. Use O4 for the sole-window leg.
  - Expected: B releases allocation, C is MRU fallback; restore best-effort rejoins near the former neighbor on stored side/group/index with comparable old share, no OmniTiler focus write. Exact-share equality only on the fast path. COSMIC source-backed selection; slot memory is session-only. Do not move a minimized client to another workspace: the newer rehome policy is not implemented.
  - Result: UNRUN - PASS/FAIL + removed allocation, minimize focus, restored tree/share and any restore focus setter; distinguish native activation from tiler action.
  - Closes: backlog D23/native best-effort-rejoin queue (original lines 1043, 1052-1054), [D23 delivery](archive/d23-rework.md), [R-MNZ-01/02](../spec/reference-outcomes/minimize.md#r-mnz-01-native-minimize-of-a-middle-tile). Internal-slot/sole-iconic and rehome coverage not claimed.

- S8 float transfer / workspace return (D16, D17, D19; conditional on existing workspaces).
  - Setup: verify the existing trailing spare gate. Stage A tiled on WS1, C tiled on WS2 (click C, `Meta+Shift+2`, `Meta+1`), B intentional float on WS1 (click B, `Meta+G`). Record B's frame and both tiled allocations. Click B before each send.
  - Keys: `Meta+Shift+2`, `Meta+Shift+1` (numbered follow roundtrip); then `Meta+Ctrl+Shift+Right`, `Meta+Ctrl+Shift+Left` only after verifying WS2 is the next existing workspace in the scoped ring. Finally `Meta+2`, `Meta+1` from B-focused WS1 without a send. Overlay legs, fresh B-focused WS1 each time: `Meta+F11`, `Meta+Shift+2`, observe carry, `Meta+F11` to exit on WS2, `Meta+Shift+1` to return; repeat that sequence with `Meta+M` instead of `Meta+F11`.
  - Expected: D16 sends are membership-only, float frame/intent and overlay flags carried exactly; neither source nor destination tiles reflow, no geometry/overlay writes. Destination overlay exit restores float frame without admission. Follow focuses mover; switch-away/back hides B and restores remembered B/frame. Float retention follows Hyprland/i3/sway plus Tier 2 against COSMIC fresh-retile. Wayland `visible:null` is unknown: visual observation plus native state is needed, not membership exclusion alone.
  - Result: UNRUN - PASS/FAIL per numbered/relative/overlay/return leg + frame/flags, selected workspace, actual focus and any tile reflow. Stop if native movement drops/corrupts an overlay. Stay/sole-source/empty-target variants are O3, not inferred here.
  - Closes: backlog D16 native queue (original lines 1041, 1050-1052), [D16 pending acceptance](archive/d16-floating-transfer.md#outcome-and-pending-acceptance), [D17/D19 pending rows](../backlog.md#single-output-setup), [R-WS-09](../spec/reference-outcomes/workspaces.md#r-ws-09-select-ws2-select-ws1-return-focus-and-viewport).

### Block N (native input/styling and settings; about 8 minutes)

- N1 Stage 6 native styling/input, bounded recheck.
  - Setup: ordinary A/B tiles, original border/underlay/preview settings and system accent recorded. Use the existing OmniTiler Settings controls, not an invented host settings path.
  - Steps/keys: move pointer over A then B; click B, `Meta+Left` / `Meta+Right`; title-bar drag B over A then press Esc (no Meta modifier). Observe cursor and paint. With default border color enabled, change the system accent through its normal UI once; then set an explicit border-color override in OmniTiler Settings and Apply. Repeat focus pair/drag, restore both settings and accent. Inspect the native Plasma pager without adding panels/widgets.
  - Expected: pointer never warps; default border follows live accent, explicit override wins; underlay/preview fills remain independent; native pager stays host-styled. Border disappearance during drag is the known skip, not a new failure or a reason to change it. Repeat pointer/focus/paint observation on each existing output in D1.
  - Result: UNRUN - PASS/FAIL by no-warp/accent/override/independent-fill/pager sub-leg + first stale/wrong paint. If a control is unavailable, mark only that sub-leg BLOCKED; do not edit config files to substitute.
  - Closes: backlog KDE v2 Stage 6 native styling/input remainder (original lines 328-331), [Stage 6 evidence/remaining acceptance](kde-world-v2-port.md#stage-6-host-integration-outcome-2026-10-28). Earlier 2026-10-28 physical pass is not erased.

- N2 Stage 7 three-token labels/warning/navigation (UI portion only).
  - Setup: original workspace mode and KWin per-output checkbox recorded; existing desktops only. Do not Apply a mode change for this UI-only row.
  - Steps: in workspace Settings inspect `per-output-local` = "Displays have separate workspaces - own set per display", `global-unique` = "Displays have separate workspaces - swap across displays", `shared` = "Workspaces span displays". Select each draft and inspect tooltip/warning. If the separate-mode warning/action is visible, click **Open Virtual Desktops**; confirm the page has **Switch desktops independently for each screen**, then close it without changing counts. Otherwise leave navigation NOT RUN. Cancel the draft afterwards. If the checkbox is already off, both separate drafts warn and shared does not; if on, the disabled-setting warning is absent. Do not change the host setting just to force the other warning branch here.
  - Expected: distinct exact labels; navigation does not toggle the host option, create/remove desktops or dirty/save settings by itself; warning matches the observed host setting and refreshes on return. Unknown/unsupported host capability is not a fabricated pass. Effective KWin 6.6 shared fallback is a limitation, not independent-view proof.
  - Result: UNRUN - PASS/FAIL + selected token/label, host-setting state, warning and destination-page identity; missing warning branch NOT RUN. D4 covers saved/restarted behavior where safe.
  - Closes: backlog Stage 7 labels/warning/quick-fix sub-legs (original lines 331-334, 413-414), [Stage 7 delivered boundary](kde-world-v2-port.md#delivered-boundary-and-pinned-facts), [workspace vocabulary](../decisions.md#workspace-vocabulary-2026-10-24). Does not close all-mode native acceptance.

### Block D (conditional: two outputs already present)

- D0 setup, not an acceptance row: use existing adjacent O1 left / O2 right with overlapping vertical spans; do not reconfigure outputs. Stage only fixtures `O1=A`, `O2=H[B,C]` by native title-bar moves. Restore ordinary windowed/tiled flags; verify exact native joins, scopes and tree. If no such topology exists, D1-D3 are NOT RUN. Do not retry a known `scope-held` endpoint or revive parked callback/helper experiments.

- D1 physical edge focus, local-first move and close/reflow (about 4 minutes).
  - Setup: D0; click C, then A so destination MRU differs from edge-nearest B.
  - Keys: `Meta+Right` from A should select B, not C; reverse with `Meta+Left`. On O2 establish `V[B*,C]` (`Meta+O` if needed), `Meta+Shift+Left` once; then observe local wrap. Restore `H[B,C]`, click leftmost B, `Meta+Shift+Left` to cross into occupied O1; return with `Meta+Shift+Right` only once B is at O1's right edge (otherwise stage the edge before that trial). Close only C with `run '{"v":1,"request_id":"close-d1","op":"close","ids":["c"]}'`; inspect both outputs. Recreate C only if another row needs it and the desktop-count gate remains satisfied.
  - Expected: focus chooses edge tile, local R1 wins before crossing: D13 gives `O2=H[B*,C]`, O1 unchanged. Exhausted edge move crosses/follows, fits destination and reflows source; close/reflow affects the member's actual scope. D13 local-wrap basis: COSMIC observation plus i3/sway direction, not bspwm/awesome crossing. Pointer remains unwarped and other output paint current.
  - Result: UNRUN - PASS/FAIL per focus/local-wrap/cross/close leg + named target/tree/output, full arrival frame and focus. No claim for third-output topology discrimination, exact R4 return order or explicit output stay bindings.
  - Closes: backlog dual-output keyboard-delivery remainder (original lines 508-510), [T01/T02 matrix](kde-dual-output-live.md#must-pass-checklist), [D13/R-OUT-02](../spec/reference-outcomes/multi-output.md#r-out-02-perpendicular-move-at-an-output-edge), KDE v2 Stage 8 focus/send/both-output/close physical sub-legs.

- D2 rapid physical cross-output input (about 2 minutes).
  - Setup: only ordinary A remains, singleton on O1; O2's existing current is empty. Close only the other fixture windows via `close` if necessary; verify the existing spare/desktop-count gate and no held endpoint. If this empty-target setup is unsafe, NOT RUN; occupied destinations introduce local moves and are not an eight-crossing substitute.
  - Keys: click A; four `Meta+Shift+Right` / `Meta+Shift+Left` pairs at physical rate, no deliberate waits. Record actual crossings separately from any local/no-target action.
  - Expected: ordered arrivals, complete height/width after convergence, no drops/stalls or stale source paint. This is not approval of Tentative input ordering.
  - Result: UNRUN - PASS/FAIL + actual crossing count, final focus/frame/tree, perceived latency and first mismatch; stop on the first refusal.
  - Closes: [rapid-input physical remainder](archive/kde-rapid-input-geometry.md), backlog P0 rapid input (original lines 529-536); partial dual-output physical-key evidence.

- D3 reviewed physical drag/drop/cancel (about 4 minutes).
  - Setup: restore D0 with fresh normal B/C if D2 closed them; ordinary tiles on both outputs, native title bars reachable; prior rows clean. Use occupied destinations; empty-target sub-leg only on an already-empty existing destination satisfying the desktop-count gate.
  - Steps: title-bar drag A O1 -> O2 occupied edge and drop; drag it back and drop. Start another title-bar crossing, briefly hold at the target edge, re-cross once, then press Esc before drop. No Meta-drag+Esc (known lock-screen chord collision). If convenient, repeat a singleton drop into the existing empty destination and reverse, without adding desktops.
  - Expected: preview agrees with committed placement, no release jump/stale A/B paint; cancel snaps back with original membership; source reflows/removes the source member only on committed transfer. Known border-during-drag skip remains. Observe both outputs and independent underlay/preview appearance.
  - Result: UNRUN - PASS/FAIL per direction/held-recross/cancel/empty leg + preview/drop agreement, source/destination membership, native frame, focus and stale-paint observation. Keep late-reply evidence if encountered; do not manufacture it. Missing empty leg stays NOT RUN.
  - Closes: backlog dual-output reviewed-drag/visible-underlay remainder (original lines 508-510), [T08 inventory](kde-dual-output-live.md#must-pass-checklist), [v2 physical checkpoint](kde-world-v2-port.md#physical-checkpoint-2026-10-27) and Stage 4/5 partial renderer evidence. Physical input is the missing route; parked absolute-wrapper attempts are not retried.

### Optional extensions (choose remaining time, not a second mandatory inventory)

| ID / setup | Exact physical steps | Expected / what to note on failure | Backlog/record sub-leg closed |
| --- | --- | --- | --- |
| D4 all-mode native acceptance, 15-20 min; two outputs, existing backing desktops and a fully verified controller stop/restore/restart route. BLOCKED if any mode requires desktop creation/removal, unsupported independent views or protected-window changes. | Preserve initial mode/checkbox. For each token in N2: save it, orderly stop/start the scoped controller using the setup lifecycle, and re-prove readiness; a Settings Apply alone is not adoption (`workspaceMode` is startup-consumed). With separate modes and host checkbox already on, cursor O1 `Meta+2`, `Meta+1`; cursor O2 repeat. With shared, either output `Meta+2`, `Meta+1`. Reopen Settings to check saved token/label. Inspect existing empty padded cells without filling them. Restore original mode and readiness/baseline after the last restart. Do not toggle the global host checkbox merely to force support in this short run. | Separate modes switch only the chosen output; shared switches both together; equal native counts are padding, not shared semantics. Global-unique has distinct sets/numbering, not a local-token fallback. Record token, selected views, fixture shown/frame/focus observations and warning. Missing independent capability, restart gate or stable pre-existing bindings = BLOCKED, not PASS. Do not infer compositor visibility from `hidden=false`. | KDE v2 native all-mode remainder, [Stage 7 remaining acceptance](kde-world-v2-port.md#evidence-and-remaining-acceptance), [mode decision](../decisions.md#workspace-vocabulary-2026-10-24). Does not close the untested opposite warning branch or full hide/reveal compositor audit. |
| O1 chained admission D03, 3 min; fresh landscape `H[A,B*]`, pointer parked outside eligible clients. | Open ordinary C, D, E in order using fixture `spawn`, without a client refocus between admissions; record native focus at every leg. Separate helper calls from physical input observations. | After-focused `[old,new]` anchor re-applied at current live focus each leg, not a frozen leg-1 anchor. COSMIC source-proven after selection plus Tier 1 majority. Record actual leg-2/3 topology/focus/geometry; unobserved outcomes stay TBD. | Backlog D03 original line 1036, [R-INS-04](../spec/reference-outcomes/insertion.md#r-ins-04-chained-admission-with-a-fixed-pointer). D03 is admission, not directional movement. |
| O2 exact nested move fixtures D09/D10/D12, 5-8 min; conditional on ordinary staging of the exact trees, no tree setter. | Fresh per case: D09 `H[A*,V[B,C]]`, remember C then refocus A, `Meta+Shift+Right`; D10 `V[H[A,B*],C]`, `Meta+Shift+Down`; D12 `H[H[A,B*],C]`, `Meta+Shift+Right`. Confirm each starting tree before keys; if it cannot be staged, NOT RUN. | D09 geometric-middle entry `V[B,A*,C]`; D10 local wrap `V[V[A,B*],C]`; D12 same-axis extraction `H[A,B*,C]`, mover focus retained. COSMIC source for D09/10; COSMIC observed plus i3/sway extract direction for D12. Note exact named leaves, grouping and focus; ordinary pixel adjacency is insufficient. | Backlog D09/D10/D12 original lines 1037-1039, [R-MOV-06](../spec/reference-outcomes/move.md#r-mov-06-move-into-a-nested-perpendicular-neighbor-with-remembered-child), [R-MOV-07](../spec/reference-outcomes/move.md#r-mov-07-orthogonal-escape-across-a-perpendicular-parent), [R-MOV-04](../spec/reference-outcomes/move.md#r-mov-04-same-axis-ancestor-escape). |
| O3 D16 stay/sole/empty matrix, 5-8 min; S8 passed, existing empty nontrailing WS2 and spare. Stay actions are UNBOUND by default. | Only if conflict-free temporary project bindings can be established/restored: bind `omnitiler-stay-workspace-2` to `Meta+Ctrl+Alt+2`, `...-1` to `Meta+Ctrl+Alt+1`, `omnitiler-send-stay-next-right-arrow` to `Meta+Ctrl+Alt+Right`, `...-prev-left-arrow` to `Meta+Ctrl+Alt+Left`. Apply, verify actual tuples. Repeat S8 ordinary-float and single-overlay roundtrips using those stay keys; select destination with `Meta+2/1` to refocus B for the reverse send. Repeat follow pair with sole B source / existing empty target if safe. Restore exact binding tuples. | Same membership-only frame/flags/no-reflow contract as S8; stay keeps source selected and source MRU, or no client focus for a sole source, without invented focus. Relative targets must be proved from the existing ring. Record each origin/verb/target separately; no default stay chord is implied. | D16 native queue original lines 1050-1052, [D16 pending matrix](archive/d16-floating-transfer.md#outcome-and-pending-acceptance), [binding catalog](../../kwin/src/workspace-native.ts). |
| O4 no-focus navigation + D23 sole retention, 3-5 min; two outputs for crossing; sole fixture A on existing WS1, B/C on adjacent output, no overlays. No foreign window is moved/hidden to expose desktop. | Native-minimize A. Confirm no managed focus at the current O1 origin from authoritative capture before keys; if KDE retained/selected another client, BLOCKED. Press `Meta+Left` with no left neighbor, then from the same proven no-focus origin `Meta+Right` toward `H[B,C]` on O2 (C previously MRU). Restore A via fixture `mode:"windowed"`; separately native-minimize/restore sole A to inspect retained workspace/slot. | No-neighbor no-op, no local recovery/A pick; adjacent crossing chooses edge B, not MRU C. Host focus loss must not force refocus. Current KDE shortcut source requires `focusedWin()`; this row can expose that routing gap, not claim it delivered. Sole minimize retains workspace, stored slot and no client focus; restore has no tiler focus write. Record no-focus proof, dispatch/refusal, selected output/target and retained workspace. Reachability failure is BLOCKED, not a policy result. | Backlog P1 no-focus remaining proof; [no-focus discriminator](../spec/reference-outcomes.md#workspace-no-focus-discriminators), [KDE routing](../../kwin/src/world-shortcuts.ts), [R-MNZ-03](../spec/reference-outcomes/minimize.md#r-mnz-03-native-minimize-of-a-sole-workspace-window), D23 native queue. |
| O5 orderly restart D25 / persisted settings D26, 5-8 min; only an already-proven receipt-bound stop/restore route with fixture state alive, no owned-above claims and no desktop churn. Otherwise BLOCKED. | Record tiled A, intentional floating B, current existing views/flags/gaps/settings; orderly stop/start once via the setup lifecycle, fresh cursor/readiness and native joins. Do not use logout/login, forced crash, KWin restart or a pre-existing production script as a restart surrogate. | Host set retained, memberships re-observed without moves, intentional float retained in the same namespace, persisted owned gaps/settings reloaded; focus only where provable. Layout is rebuilt by re-adoption, no saved-tree promise. Note any invented layout/focus, lost float intent or restoration mismatch. | Backlog D25/D26 original lines 1044-1045, 1056; [restart decision](../decisions.md#restart-persistence). Cross-login intent is a known limitation, not tested here. |

### Teardown and result format

- Undo only recorded test setting/binding changes; restore the original mode through the same safe lifecycle if D4 ran. Close/shutdown only this fixture server: `run '{"v":1,"request_id":"bye-final","op":"shutdown"}'`. Use the setup's exact controller stop route; verify script/helper absence, owned processes stopped and effects unchanged. Never unload an unidentified object or sweep residue.
- Capture a fresh complete baseline and compare protected windows, views, desktop identity/count/order/names, shortcuts, config/store bytes/modes and loaded/active effects with the preimage. Check fixture/server absence and expected pointer restoration separately. An unexpected desktop addition/removal is a failure to preserve, not permission to recreate/delete it. Stop and retain mismatch evidence; no broad cleanup or automatic repair. Remove only the fixture's now-empty run directory via its documented teardown.
- Result lines: `ID/sub-leg: UNRUN -> PASS|FAIL|BLOCKED|NOT RUN; observation; evidence filenames`. T1 uses CAPTURED/INCOMPLETE, not a product PASS. Separate native flags, physical delivery, perceived latency and tentative policy observations. No complete backlog item advances solely because one sub-leg passes.

### Command/binding/expectation provenance

- Commands: [fixture quickstart/protocol/limitations](../../crates/tiler-live-fixture/README.md), [KDE dev lifecycle](../dev-loop.md#linux-kde), [live guide](../live-kwin-testing.md). The timing row delegates entirely to the existing timing procedure, including its trace lifecycle.
- Physical chords and empty-default stay/output-send bindings: [world shortcuts](../../kwin/src/world-shortcuts.ts), [workspace catalog](../../kwin/src/workspace-native.ts). Startup-only mode adoption: [world entry config-change handling](../../kwin/src/world-entry.ts). Temporary O3 chords are this sheet's test setup, not shipped defaults; a conflict makes that sub-leg BLOCKED.
- Tier 1 basis is stated only where recorded in the cited normative rows/decisions. Native settlement is an adapter correctness check, not a WM-policy vote. All-mode vocabulary follows the v2 decision, not the superseded v1 global-unique removal/fallback oracle.

### Limitations (explicit, not proven here)

- Stage 4/5 coverage here is bounded to physical float/maximize/fullscreen commands, existing-workspace sends and paint/drag observations. No claim for every fullscreen window action, sticky/multi-desktop changes, late-reply race or R-MAX-02 directional focus.
- No real-game, born-fullscreen initial-hold, activation-token, fixed-size or automatic-dialog/Steam acceptance. No fresh keep-above ownership experiment: [floating-dialog work is BLOCKED](kde-automatic-float-stacking.md).
- No effect installation/loading for hide/reveal or mapped-effect-identity proof. User observation plus native joins helps physical acceptance but does not replace the separate compositor-visibility audit. Two-output and all-mode rows remain conditional; third-output selection, hotplug and Activities investigations are not folded into them.
- No D23 minimized rehome test: the option-B source-slot follow-up is approved but not implemented. Exact native trees and slot internals without authoritative evidence remain TBD, even if the screen looks correct.

## Inventory verification

- 20 runnable/conditional IDs: T1 (1), S1-S8 (8), N1-N2 (2), D1-D3 (3), optional D4/O1-O5 (6). D0 is setup only. Every ID remains UNRUN; T1 is a diagnostic capture, not behavior acceptance.
- Excluded blocked flow: automatic-float/dialog keep-above lifecycle and Steam/modal/fullscreen/game retests. Known skip: border hides during drag. No desktop lifecycle, hotplug, cross-login/forced-loss or undecided-policy tests are required.

## Not in this run, awaiting decision

- Workspace deletion policy: native-bound retention is deliberate; core-directed retirement/adopted-desktop scope is parked ([investigation](kde-world-v2-port.md#empty-native-workspace-lifecycle-investigation-2026-10-29)).
- Fixed-size core shape: ownership/shape approval pending; no admission-focus/keep-above retest ([inventory/proposal](kde-world-v2-port.md#fixed-size-admission-consumer-inventory-and-proposed-core-shape-2026-10-29)).
- Client self-resize contract: prompt retained-tile reconciliation versus defer is parked ([backlog F1](../backlog.md#open-work), [discriminator](../spec/reference-outcomes.md#client-driven-tiled-geometry)).
- R-MAX-02 directional focus from fullscreen differs from the Tier 1 same-output fence; do not select a new rule by testing it here ([current row](../spec/reference-outcomes/maximize-fullscreen.md#r-max-02-fullscreen-focus-and-exit)).

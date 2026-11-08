# Windows port notes (world contract v4)

How the Windows adapter moves to the strict v4 contract in
[world-contract](world-contract.md): source-fit vs
approved-not-implemented vs open. Production sources are current v3
pre-port code, not v4 implemented. Staged port tracked in the
[phase-0b review](../changes/v4-migration.md#phase-0b-windows-platform-fit-review);
measurements in [windows-v4-native-evidence](../changes/windows-v4-native-evidence.md).
No v4 production port work included. Build blocked by expected
`tiler-windows` phase-1 mismatches; no repair attempted.

Source paths below are relative to `crates/tiler-windows/src/` unless
explicitly qualified. These are current v3 routes to port or delete,
not v4 acceptance evidence.

## V4 native coordinate normalization

- Target, not delivered by this note: PMv2 physical-pixel reads are
  native integers, never scaled again. Per-screen v4 `scale` is that
  output's effective DPI / 96 (measured A=120, B=96). Core scales each
  logical setting once, then rounds nearest, halves toward positive
  infinity. See the [scale rule](world-contract.md#scale-and-rounding).
  `GetDpiForWindow` proves only the queried window's DPI, so each
  output needs its own PMv2 window, including empty outputs.

## Handshake (exact v4 declaration)

- Fixed source-fit values (shape approved 2026-11-02; gates remain):
  `hostOwned`/`equalCount`/`birthClear` = `false` (G2 retirement and
  native-creation correlation inapplicable); `nativeSelectorScope` =
  `per-output`, ignored without `hostOwned`; `windowSuppression` =
  `true` (durable hide/reveal plus watcher; per-window gates can
  refuse); `maximize`/`fullscreen`/`above`/`warp` = `true` (mechanisms
  exist, not per-window authorization or completed R12 recovery);
  `nativeSticky` = `false` (W3: no native sticky; logical sticky
  composes through suppression); `tiledFloatTransfer` = `false`
  (correct polarity for the no-remap route).
- `unavailableFacts`, in `FactName` catalog order
  ([catalog](world-contract.md#neutral-classification-vocabulary-and-applicability)):
  `managed,acceptsInput,transient,modal,below,role,appId,title,activationPolicy,layer`.
  No producer on the reviewed production route; `appId`/`title` are
  listed until readers exist (W4 supplies `executable` separately). `class`
  is read; `processId` has a producer. Supported-but-unreadable stays
  `null`, never a permanent unavailable declaration.
- `applicationIdentity` must carry separate nullable own
  `appId/executable/processId`, with executable normalized by slash
  replacement plus ASCII lowercase (`lifecycle.rs`, `native.rs`).
  Window process IDs need lifetime-bound wire tokens, not raw PID reuse.
  Same executable in another process does not exclude its windows.

## Current Windows inventory (port or delete)

- `tiling_sys/world_capture.rs`, `tiling_sys/observe.rs`: keep capture
  (DWM extended frame, iconic/zoom/cloak, captionless full-monitor
  coverage, style/exstyle, bounded `WM_GETMINMAXINFO` hints);
  re-target to v4 raw facts. Drop projected negatives
  (elevated-filter `elevated:false`, unchecked style reads,
  `HWND_BOTTOM`-as-below, `GetParent`-as-parent).
- `tiling_sys/world_native.rs`, `tiling_sys/world_native_frame.rs`: apply
  core-decided booleans with raw immediate pre/post; delete
  toggle/continuation verdicts and local logical-view membership
  (replaced by visibility authority).
- `tiling_sys/world_sequencer.rs`: handshake lifetime/gen, move
  settings/capabilities out of reports. Cursor, hit-test, close and
  gesture sources stay separate from command selection.
- `tiling/classification.rs`, `tiling_sys/shell.rs`: delete
  adapter-side policy (dialog/shell gates, owned-dialog exemptions).
  Settings is ordinary; only native process identity excludes
  own-process UI
  ([amendment](../changes/v4-migration.md#own-process-identity-amendment)).
- `product_hide/sys/`: keep hide-commit-before-`ShowWindowAsync` plus
  dead-owner ledger; v4 needs a new executor (batched nonactivating
  shows, one-shot focus, cross-output hide barrier, `LastWrite`).

## Report rows and raw facts

- `WindowFact`: `win/out/rect/fullscreen/maximized/minimized/hints`
  fit sources; `hostHidden` reads cloak; `type` maps `#32770` to
  dialog. Available neutral inputs: token integrity (`privileged`),
  `WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW`, `GW_OWNER` (bound only to
  verified lifetimes), `WS_THICKFRAME`, `WS_EX_TOPMOST`. `GW_OWNER`
  never proves `transient`/`modal`; `IsWindowEnabled` never proves
  keyboard preference.
- `focused`/`foregroundOut`: v4 needs independently captured neutral
  foreground facts/output incl. unmanaged subjects; token-resolved
  `focused` infers false on filtered subjects.
- `screens`: PMv2 full/work, `complete`, per-output scale, one optional
  report-level `primary` (primary-first order). `identity` is nullable
  QDC/SetupAPI metadata (see below).
- Envelope: `obs` advances synchronously on every invalidating event;
  `reqSeq` monotonic per lifetime; readiness latches only after
  accepted initial settings plus a complete usable report.

## Desired-row application lanes

- Frame/member/clear lanes fit mechanically; keep separate, drop local
  view-attempt/member receipt producers (`applied_homing` is the
  adapter's own association, not native evidence).
- `visibility`: executor retains spent lifetime/`visibility.rev`/
  target; fallback hides only on core `visibility.fallback:true`.
- `state`: core booleans with `StateAttempt` pre/post; sticky emits no
  native target/attempt/fact (W3). `warpCursor`: `SetCursorPos` wired
  with gates; `warp:true` is support, not guaranteed success.
- No `SwitchView`/`Operation` wrappers or nested drag/pointer routes.
  `RendererInput` carries the exact drawn row; core judges staleness.

## Visibility and arrival (W2)

- Shape approved 2026-11-02: exclusive native/output `ArrivalBracket`
  arms. Without `hostOwned`, core compares retained logical home plus
  bracketed observed output/frame/visibility/lifetime/tag evidence to
  the frozen target; `null` is well-shaped but never proves readiness.
- Source-reading: hidden windows stay readable after exact-claim proof
  (`tiling_sys/observe.rs`, `product_hide/sys/mod.rs`);
  hide commits before native calls, so the route fits suppression.
  Production cross-output arrival is unverified.
- Measured (standalone, not product acceptance): one hidden move
  `(+60,+40)` px without reveal, then fresh-recoverer restore
  ([hide-loss](../changes/windows-v4-native-evidence.md#5-hide-cloak-and-hidden-arrival-w2)).
- Measured with qualifications: [cross-output hidden move/reveal and
  fresh crash recovery](../changes/windows-v4-native-evidence.md#follow-up-w2-cross-output-hidden-arrival-2026-11-03).
  Hidden B read DPI 96 and normal reveal stayed on B; recovery restored
  A's saved preimage, not B's frozen target. The probe compared outer
  geometry, not the production DWM frame; its local comparator and
  single visible B read are not an ArrivalBracket. Full B bracketing,
  per-stage evidence, hidden DPI-message timing and recovery safety
  gaps remain unverified. Do not replay the parked harness as accepted
  recovery tooling.

## State and persistent above (W1, K3)

- V4 floating-workspace rows publish `floating:true` with empty `float_origin`,
  no tile allocation and unchanged authority/holds. Audit these existing
  `crates/tiler-windows/src/tiling_sys/` readers during the port:
  - `world_reads.rs:218` float-token classification and `admission.rs:35` D2
    ordinary/tiled pin: scope classification must not become retained explicit
    float intent or reclassify retained admission on retile.
  - `world_native.rs:2535` arrival proof and
    `directional_resolution.rs:85,101`: the float branch skips tiled frame
    checks; preserve output/home evidence and use v4 arrival brackets, never
    treat the bit or untouched authority as proof or frame-write permission.
  - `world_sequencer.rs:946` cross-output float transfer gate and the float
    consumers in `capacity_floats.rs`/`world_tick.rs`: verify scope-only and
    placement-bearing floats, minimized recovery and toggle-back to tiled
    without retaining a false per-window override.
  Adapter changes and native verification remain port work;
  [decision](../decisions/workspaces.md#per-workspace-tiling-floating-and-bindings).
- Approved 2026-11-02, option c (union) plus capacity: every float
  (automatic, explicit float/sticky, capacity, workspace-mode even
  with no tiles) acquires supported persistent above under one
  core-owned latch; durable preimages required; preowned above/below
  untouched; fullscreen-game safety wins.
- Source-reading: explicit tiled-to-float acquires `TOPMOST`
  nonactivating unless preowned (`tiling_sys/float_toggle.rs`); release lowers
  only owned bands under fresh identity gates. Automatic/capacity and
  sticky-on-float do no topmost work today.
- Measured (standalone): forced-loss topmost/hide preimages survived
  real writer loss, restored by a fresh recoverer; preowned zero-write
  and external-clear marker-only restore observed
  ([W1/R12](../changes/windows-v4-native-evidence.md#2-topmost-after-owner-loss-w1r12)).

## Recoverable preimages and storage (R12)

- Rule ([recovery](world-contract.md#freshness-and-recovery)): persist
  original native state atomically before hide/chrome/topmost/sticky
  writes, in a lifetime-bound slot outliving the tiler; restore only
  verified leftovers. Failed preimage disables only that property.
- Source-reading: hide ledger stores identity plus show preimage;
  watcher binds the exact owner (`product_hide/sys/watcher.rs`).
  Fullscreen already stores inert style/max/frame (`tiling_sys/fullscreen.rs`),
  but the watcher does not consume them; float topmost preimage is
  runtime-only (`tiling_sys/run_loop.rs`). Recovery and chrome adoption are port
  gates.
- Measured: inert scalar survival; 422 HWNDs created/closed, zero
  full-value repeats (not a no-reuse proof); changed-HWND stale-record
  refusal. Publication interruption, opaque atomicity and product
  restart adoption stay open. See [storage results](../changes/windows-v4-native-evidence.md#3-hwnd-reuse-and-restart-surviving-storage).

## Fullscreen baseline (R14)

- Rule ([state](world-contract.md#window-state-policy)): neutral exit
  needs known exitability; unknown stays untouched; app-owned exits
  refused per baseline case. No `Alt+Enter` synthesis.
- Source-reading: captionless-covering-monitor predicate, not a
  presentation-mode oracle (`tiling_sys/observe.rs`); project entry saves owned
  style/max/frame and restores mask/frame but verifies style, not
  settled geometry (`tiling_sys/fullscreen.rs`). First-seen app fullscreen has no
  preimage: keep refusal.
- Measured (standalone): app-owned borderless roundtrip, all eight
  channels matching; dedicated DXGI `SetFullscreenState` grant
  (API grant only). App-roundtrip foreground restoration, first-seen
  forced exit, game/exclusive and tiler-owned exitability unproved.
  See [R14 results and limits](../changes/windows-v4-native-evidence.md#1-fullscreen-already-present-at-startup-r14).

## Opaque intent records and storage

- Core defines/decides; adapter stores/returns `data` (<=1024 bytes)
  unchanged in a lifetime-bound slot, atomically before dependent
  writes, with restart-verifiable binding
  ([rule](world-contract.md#opaque-intent-storage)).
- Source-reading: current float/tile/sticky markers are inert scalars,
  not the opaque record; admission rotates tags under fences
  (`tiling/float.rs`, `tiling_sys/assignments.rs`, `tiling_sys/restart.rs`). Neither an external tag-keyed slot
  nor atomic property chunks are implemented for v4. Never match by
  title/PID or store a pointer into the exiting process.

## Output identity matching

- Nullable `manufacturer/model/serial/connector`; core derives
  reconnect identity from the triple, connector only when unreadable
  or duplicated; ambiguous reconnect holds; `connector:null` until
  normalization is proved ([rule](world-contract.md#screenreport)).
- Source-reading: `MonitorArea` has no physical metadata
  (`tiling_sys/monitors.rs`); QDC target-name plus SetupAPI EDID device-key route
  is API-feasible. Connector instance is adapter-local, may be zero.
- Measured (standalone): distinct triples; text-serial retention;
  empty-output scale-only cycle delivered per-output `WM_DPICHANGED`
  with exact restore; reconnect, port move/return, sleep/resume,
  reboot stable (reboot changed adapter LUIDs: not durable keys).
  Identical-monitor fallback and connector normalization open. See
  [physical series](../changes/windows-v4-native-evidence.md#physical-check-series-closing-summary).

## Border and K5 witness

- Rule ([presentation](world-contract.md#presentation-and-renderer-input)):
  apply the accepted descriptor to the live frame through native frame
  events. `FrameWitness` must join subject to the
  capture/invalidation chain; unknown/stale suppresses paint narrowly.
  Delay/coincidence/event names are not witnesses. K5 keeps its
  phase-2 proof gate.
- Source-reading: WinEvents cover show/hide/location/move-size edges;
  current border posts absolute geometry with tag/PID/creation. The
  capture-obs/event/render join is not implemented; callbacks bump
  only a visibility generation, not world `obs`
  (`tiling_sys/observe.rs`, `tiling_sys/world_capture.rs`,
  `tiling_sys/render_bridge.rs`, `render_handoff.rs`, `render_pump/render_core.rs`).
- Measured (standalone): synthetic drag changed the DWM frame; one
  inward resize shrank outer/DWM 160x100 px; transitions bracketed at
  150 ms tolerance. No witness join or physical-drag acceptance. See
  [K5 results](../changes/windows-v4-native-evidence.md#4-border-tracking-and-winevent-coverage-k5).

## Phase-4 port gates (no passes claimed)

- Carry all eight scenario OPEN/TBD gates plus: K5 witness join and
  eligibility coverage; opaque/chrome/topmost crash recovery with
  next-start adoption; untagged-lifetime fencing and HWND-reuse
  handling; state/style/eligibility event coverage; connector/
  identical-monitor normalization; per-case app-fullscreen baseline
  with settled restore proof; beyond-scale-only empty-output
  coverage. `hostOwned:false` exempts native removal/creation gates;
  core `exist:false` is sole removal authority.
- Conformance: exact v4 fixtures in `test-fixtures/world-wire/v4/`;
  codec/generator comparison and adapter acceptance must agree before
  merge. Static checks are not native/recovery/renderer acceptance.

## Additional unverified implementation obligations

- Untagged foreground capture must not mint continuity from HWND/PID
  alone. Unknown/unbound lifetime stays empty or nullable as specified;
  do not synthesize negative foreground facts from admission filtering.
- Empty-output scale measurements used a PMv2 observer on each output;
  current windowless fallback does not establish equivalent coverage.
- Elevated-denial measurements discarded DWM failure HRESULTs and did
  not prove independent restoration. Keep those [gaps](../changes/windows-v4-native-evidence.md#elevated-fixed-harness-matrix-measured-with-acceptance-limitations)
  open; per-window denial never adds an `unavailableFacts` entry.
- Non-normal show-state suppression recovery remains unmeasured.
  Per-window recoverability gates must refuse unsupported cases, not
  change the lifetime's hard capability declaration.

## Build, check and gate commands

- Toolchain per [windows-dev-environment](../windows-dev-environment.md);
  CLI/Rust via root `mise.toml`; user owns installs. Blocked:
  `tiler-windows` build/tests/clippy (expected phase-1 type mismatches).
- Live work under [live-windows-testing](../live-windows-testing.md),
  fixture-first, scoped fixture only. No production tiler,
  registry/policy, interception or crash probes in this note.
- Core-only gates stay host-independent; settlement/recovery/paint
  evidence is separate from codec acceptance.

## Material rough edges (no behavior change selected)

- Projected `WindowFact`s, not raw safety facts; `#32770`-to-dialog
  normalization stays adapter mechanics; shell/XAML checks stay core
  policy, not fact verdicts.
- Synthetic drag/resize never substitutes for physical input, focus
  arrival or shell acceptance.

# Windows v4 native evidence

## Goal and scope

- Investigate native fit of the approved [v4 target](../porting/world-contract.md),
  not implement the Windows v4 port. Current production authority remains v3.
- Priorities: first-seen/app-owned fullscreen (R14), topmost crash recovery
  (W1/R12), native lifetime/storage, border witness/event coverage (K5), hidden
  arrival/suppression (W2), and elevated-window identity/facts (W4/K1).
  The follow-up below adds output identity (R11) and per-output scale/event reads.
- Do not change contract, spec, decisions, shared fixtures or non-Windows code.
  Proposed behavioral discriminators stay in this record until accepted.

## Acceptance and approach

- Separate source inspection, fresh measured readbacks, historical evidence and
  unverified proposals. Setter acceptance is never settlement proof.
- Use disposable live-fixture windows and independent PMv2 native observations;
  retain raw evidence outside the repository. Verify lifetime before every
  mutation and exact restoration/teardown afterwards.
- Fullscreen game/exclusive-mode and elevated/UAC acceptance remain separate
  user-owned steps where the fixture and existing live protocol cannot prove them.
- Bounded units: source/mechanism audit; fixture-only native experiments;
  evidence inspection; integrated findings and pending actions.
- Revised probe approach: a small development-only Rust harness reusing fixture
  ownership patterns and `tiler-windows` native wrappers, with effect-free semantic
  guard/preimage tests. Measure fullscreen baseline, topmost, lifetime/storage,
  native events, visibility and ordinary identity in separately bounded legs.
  Do not use the rejected PowerShell write probes. Writer and recoverer are fresh
  separate processes; persist identity, preimage and intended effect before writes.
- Production restart-marker adoption remains a phase-3 integration item. This
  pass tests marker-shaped scalar/opaque records only, without launching the
  production owner. Product integration and elevated application acceptance remain
  separate from the standalone native measurements.

## Outcome and evidence status (2026-11-02)

- **Native measurements collected for all six areas; product acceptance remains
  incomplete.** Later Rust runs demonstrate app-owned borderless roundtrip, a
  dedicated DXGI fullscreen API grant, forced-writer recovery of topmost and hide
  preimages, preowned zero-policy-write behavior, stale-window refusal, synthetic
  drag/resize and ordinary identity reads. Production recovery, R14 exitability,
  actual HWND reuse, K5 render joins, cross-output hidden arrival and elevation
  remain open.
- The first PowerShell run and subsequent rejected candidates remain diagnostic
  history, not acceptance. Later functional Rust repairs and measurements below
  supersede the stopped-run status, not the limitations of those older results.
- Baseline: `origin/v4` at `e57030579b79634c95594abc89c9311223837707`.
  The [phase-0b review](v4-migration.md#phase-0b-windows-platform-fit-review),
  W1-W4 approval and [fullscreen baseline plan](windows-fullscreen-exit-baseline.md)
  are historical source/decision inputs, not new native results.
- First-run build reported successful: `mise exec -- cargo build --locked -p tiler-live-fixture`.
  Independently rehashed first-run artifact: `target/debug/tiler-live-fixture.exe`, SHA-256
  `4a1bc59f9ba60d02a0e0e085a3f5ad29ac50c55b0244e0e9da2d422def7f95e8`.
- Conditions: native Windows 11, ordinary medium-integrity fixture, mixed-scale
  outputs. Exact process/display identities and raw replies remain outside the
  repository. OS-build and complete PMv2 preflight evidence were not captured.
- External evidence group: `run-20261102-140939`; filenames below identify its
  scripts/readbacks, not repository files. No announcement secrets are reproduced.
  Entry route was `tiler-live-fixture serve`, then `send` protocol requests and
  `pwsh -NoProfile -ExecutionPolicy Bypass -File <probe>` with local parameters.
  Per-leg exit statuses and exact forced-loss provenance were not retained.
- No production tiler, watcher, product ledger/settings, registry/policy changes
  or keyboard interception were exercised. Later Rust testing includes synthetic
  mouse dragging/resizing and a refused public cloak-write request, separately
  recorded below.

### Protocol deviations and rejected claims

| Finding | Inspectable evidence | Consequence |
| --- | --- | --- |
| Observer never established PMv2 | `native-lib.ps1:1-64`; `f1-out.json` has virtualized outer/monitor bounds beside physical DWM frames | Physical geometry/arrival assertions rejected; compare only like-for-like recorded values |
| Identity reads did not gate restoration writes | `f2fresh.ps1:18-26`; `f5fresh.ps1:22-26` computes but does not enforce `identityMatch`, and records creation without comparing it | Exact-window safe restoration unproved; no replay of these scripts |
| Missing nonactivation flags | `f2owner.ps1:44-45`, `f2fresh.ps1:25,37,43` use `0x0003`; `f5owner.ps1:39-40` uses `0x0005` | Both omit `SWP_NOACTIVATE`; foreground preservation unproved |
| Preowned-topmost case wrote TOPMOST again | `f2fresh.ps1:30-40` | Does not test the required untouched/no-write branch |
| No exception-safe leg cleanup; independent reveal not proved before first hide | Probe scripts lack `try/finally`; `f5owner.ps1` hides before `f5fresh.ps1` demonstrates reveal | Recovery prerequisite not met; later cleanup does not retroactively pass it |
| WinEvent hooks registered globally, not for the fixture PID | `f4.ps1:34-38` uses PID/TID zero; 37 callbacks, 33 fixture-PID matches | Broader observation scope than intended; no foreign-window mutation shown, no fixture-only hook-scope claim |
| Writer scripts returned normally, not forced loss | `f2owner.ps1:49-51`, `f3owner.ps1:27-29`, `f5owner.ps1:43-45`; execution clarification confirms no writer kill | Previous crash terminology withdrawn; no forced-crash or product-next-start proof |
| False preimage encoded as property value zero | `f2owner.ps1:41-43`; fresh read also zero | Cannot distinguish stored false from absent/failed read; survival claim rejected |
| Scalar-property probe mislabeled PID/TID | `f3owner.ps1:23` assigns the API return (thread ID) to `pid1` and out-PID to `tid1` | Those fields must not be used as window-identity proof |
| Identity probe used wrong normalization/checking seams | `f6.ps1:19` uses Unicode `ToLowerInvariant`; style success is nonzero rather than checked last-error; one cloak query advertises 16 bytes for a 4-byte value | ASCII examples only; no full normalization, checked-style or buffer-safety proof |

- A disposable nonmutating process's exact-identity forced stop was reported,
  but that does not prove any mutation-owner crash/recovery leg.
- Initial fixture job disappeared between shell invocations; its PID was later
  absent and its pipe unreachable, but its termination cause is unknown.
- Reported final server cleanup: all remaining caller windows closed, list empty,
  shutdown returned `open_windows:0`, then exact server PID and fixture process
  name checks returned absent. `teardown-out.json` directly confirms the
  close/list/shutdown replies; process absence is an execution report, not a
  retained machine-response file. No outstanding fixture resource was reported.
- Other failed setup attempts: stripped JSON argument quoting; unavailable shell
  commands; execution-policy/dot-source and inline-command parsing failures;
  initial C# type/marshalling errors; PowerShell variable collisions. Corrected
  scripts produced the retained observations. The first reuse loop also hit
  asynchronous close/spawn `id is already in use`; the later loop waited for
  caller-id disappearance. These are not product failures or crash evidence.

### First-run write-target audit

- Offline inspection of all retained effect-bearing scripts found every direct
  native window-write argument derived from a fixture `spawn` reply's native
  handle, or a saved file carrying that same handle. Retained fixture replies
  and PID readbacks associate those handles with the announced fixture process.
  No script addresses a separate foreign HWND and no foreign-window write is
  recorded. Missing per-write lifetime checks prevent certifying race-free
  containment beyond that evidence.
- Direct calls: `f2owner.ps1:41,45` writes the test topmost property/band;
  `f2fresh.ps1:25,35,37,43,52` restores/rewrites/removes those values;
  `f3owner.ps1:25` writes the nonzero scalar test property;
  `f5owner.ps1:35,40` hides/moves and `f5fresh.ps1:26` reveals the fixture.
  Other window effects use fixture protocol caller IDs: spawn/close, mode,
  level, geometry, visibility and focus. There is no direct SetWindowLong call;
  fullscreen style changes are performed by the fixture itself.
- `f4.ps1:21-31` callbacks only record event metadata/PID and read rectangles
  for fixture-PID location events. Global registration/unregistration is an
  observation mechanism, not keyboard/mouse interception or window mutation.
  Four callbacks were outside the intended fixture scope; no write branch
  exists in that callback.
- Other effects: external probe/request/preimage/readback files, fixture and
  probe process creation/exit, and the reported exact-identity forced stop of a
  test-created disposable nonmutating process. That stop command was not retained
  in the scripts. No registry/policy, cloak write, synthetic input or third-party
  process termination is recorded.

### Recovery-probe candidate: offline rejection

- External candidate group: `w32v4-repair-20261102-1500`; five scripts named
  `00-native-lib-repaired.ps1` through `04-cleanup-repaired.ps1`, plus offline
  checks. No native window/process calls or live effects were executed by this
  candidate. Parsing all five scripts returned zero syntax errors. Reported
  compile/reflection, random-tag, file-roundtrip and textual-sentinel checks
  passed, but those checks do not exercise identity or recovery semantics.
- Independent inspection rejected live use. The original launched fixture
  identity is not persisted: each entry point compares freshly read identity
  with itself. `Test-RepairOwned` omits original executable/process creation/
  session, and sentinel matching is a caller-id substring rather than the full
  announced window title. Tag attachment lacks an immediate full-identity gate.
- `00-native-lib-repaired.ps1:117,136` names a parameter `$Pid`, colliding with
  PowerShell's read-only `$PID`. A pure parameter-binding reproduction returned
  `Cannot overwrite variable Pid because it is read-only or constant.` without
  entering the function body. Parsing and reflection did not catch this defect.
- PMv2 covers only the paired rectangle read, not subsequent geometry writes or
  display enumeration. Writer identity before termination is not bound to the
  original exact executable/start/session. Preimages are published but their
  schema and original window/tag binding are not validated on recovery.
- Writer post-state is written only in `finally`, which forced termination
  bypasses. Hide recovery would need that missing file. `RestoreProof` exercises
  only a nonmutating writer, not independent reveal/restoration. Recovery runs
  in the surviving controller rather than a fresh process; `RecoverOnly` is
  unused. Hide recovery never restores the original rectangle. Marker removal
  lacks an immediate ownership/value check, and success predicates are incomplete.
- Failure cleanup unconditionally closes created windows, obscuring restoration
  failures instead of preserving uncertain residue. The preowned write count is
  assigned rather than instrumented. These are semantic blockers, not syntax
  failures; the rejected scripts must not be used as a live recovery route.
- Both the first live protocol and this offline candidate remain rejected.
  Forced-writer-loss, fresh recovery, actual restart adoption, PID-scoped event
  sampling, synthetic drag/resize and DXGI measurements are still unperformed.

### Rust replacement: review findings before functional repair

- Added development-only
  [win-probe.rs](../../crates/tiler-live-fixture/examples/win-probe.rs), with a
  Windows-only path dev-dependency on `tiler-windows` and corresponding lockfile
  metadata. It reuses `HeldProcess`, native wrappers and portable lifetime
  predicates; fixture wire behavior and production sources are unchanged.
- First review rejected stale pre-effect facts, effect replay instead of preimage
  restoration, in-process recovery, insufficient process-termination fences and
  failure cleanup. One correction pass added refreshed write guards, exact
  title/process/tag comparisons, preimage restoration, separate recoverer children
  and preservation on failure. None of these live paths has been executed.
- Final independent checks: `mise exec -- cargo test --locked --offline -p
  tiler-live-fixture --example win-probe` passed 19 pure tests; example build,
  strict all-target clippy and fixture fmt check passed. Tests inject fake native
  observations and effects; no native-effect tests or probe entry points ran.
  These checks establish compilation and the exercised pure cases, not the
  correctness of fixture bootstrap, process choreography or restoration.
- Second review blocked live entry: source/artifact/OS provenance is not retained;
  display baseline is only a monitor count; original geometry/visibility/focus/
  topology are not compared after recovery. Process snapshots used for termination
  are not retained as run evidence. The header incorrectly claims bounded HWND
  churn and live recreation coverage that do not exist. This remains an unaccepted
  development example, not a functioning or approved live recovery route.
- Additional source inspection found concrete integration gaps:
  - `bootstrap_tag:735-748` requires the freshly observed facts to authorize the
    absent-tag exception. `LiveRoster::observe:1605-1613` calls
    `rejoin_live:1641-1697`, which always returns that exception flag as false.
    The initial absent live tag therefore rejects bootstrap; fake bootstrap tests
    do not exercise this live adapter path.
  - `recover_from_record:905-906` returns when topmost already equals its preimage,
    before checking/restoring the marker at lines 914-934. External clear or
    interruption between recovery effects can therefore leave marker residue
    while reporting no further work. Marker recovery also does not verify that a
    changed scalar still equals the writer's intended marker before overwriting it.
  - Fresh recoverer launches at lines 1994-2000 and 2088-2094 use unbounded
    `Command::status()`. The send-client timeout at lines 1102-1103 calls
    `Child::kill()` without the exact-identity termination path.
  - Setup publishes the final handoff only after tag/marker writes (1578-1585),
    leaving the out-of-process recovery entry incomplete if setup stops earlier.
    Setup records do not encode the absent tag/marker preimages as those exact
    property changes. No live failure was needed to establish these code paths.
- At that checkpoint no later native measurements existed. R14 app-owned borderless roundtrip/DXGI,
  live preowned/external-clear/set legs, measured HWND churn and stale-record
  rejection after real fixture recreation remain unimplemented or unobserved.
  Production restart adoption remains phase-3 integration work. Areas 4-6 remain
  deferred; no contract or spec outcome is promoted from these tests.

### Later functional Rust repairs and measured results

- Fixed absent-tag bootstrap's live-adapter exception, independent topmost/marker
  reconciliation and marker drift handling. Added four pure regression tests.
  Atomic publication initially failed because Windows could not flush the
  read-only file handle; opening a write-capable handle fixed that actual setup
  failure. Failed setup attempts are retained externally; only their created
  fixture processes were terminated. Later measurement legs completed teardown.
- Latest reported gates: 32 pure example tests passed, both examples built,
  strict fixture all-target clippy and fixture fmt check passed. Final checks
  initially found a redundant closure and formatting drift; function-item and
  formatting-only corrections passed the same gates. No native-effect unit-test
  suite was used as a live result.
- Evidence groups: `win-probe-20261102-03`,
  `win-probe-20261102-m123/{r14,topmost-preowned,topmost-loss,topmost-exttrue,topmost-extclear2,storage-churn,storage-stale}`,
  `k5-20261102-185728`, `winprobe-areas56/{hide1,id1}`, `k5-resize-run{1,2}`
  and `dxgi-probe-2`. Raw records, child spawn/kill replies and readbacks remain
  outside the repository; announcement secrets are not reproduced.

| Focus | Later measured evidence | Limits |
| --- | --- | --- |
| R14 baseline | `r14.json`: a window observed windowed first entered app-owned borderless with outer/DWM/client equal to the full monitor, then app-owned windowed exit matched all eight compared geometry/style/show/visibility/cloak channels. A separately spawned first-seen borderless window also covered its monitor | No forced first-seen exit; foreground was not restored to its original owner; `observed.mode` remained unreliable. No tiler exitability/game/exclusive proof |
| DXGI request | `dxgi-probe2.json`: dedicated own-window D3D11 device/swapchain creation, one windowed Present and SetFullscreenState(TRUE) returned S_OK; GetFullscreenState returned true and native outer covered the output. FALSE request/readback succeeded; original frame, current mode, monitor/work geometry and prior foreground matched afterwards | API grant only, not legacy exclusive/FSO/independent-flip classification. First unpumped/unfocused request was refused with DXGI_ERROR_NOT_CURRENTLY_AVAILABLE. No fullscreen frame/presentation-path measurement; native window still existed immediately after deferred winit drop, followed by process exit and no reported residue |
| Forced-loss topmost/storage | `write-a.json`, `postwrite.json`, `recovered.json` and `topmost-loss.json`: distinct writer stayed alive, exact-created-writer forced termination was followed by a fresh recoverer. Post-loss topmost=true and marker=effect; after recovery topmost=false and marker=preimage. Two verified recovery writes | Marker-shaped opaque payload and native scalar only; no production owner/watcher/core intent adoption. Files flushed before writes support process-kill persistence, not power-loss guarantees |
| Preowned topmost | `topmost-preowned.json`: initial native topmost=true, writer refused with exit 3 and no write record; fresh recovery performed no policy writes both before and after an external clear | Tag/marker setup writes are separate. Zero-policy-write result is not zero total setup effects |
| External clear/set | `topmost-extclear.json`: harness native clear left topmost=false; fresh recovery restored only marker (one write). `topmost-exttrue.json`: external true -> true left the bit unchanged; fresh recovery restored topmost and marker (two writes) | Native clear is a same-controller test intervention, not a production external-clear latch. An unchanged bit does not identify the setter |
| HWND churn/stale record | `storage-churn.json`: 422 created and confirmed closed in 30005 ms, 422 distinct full HWNDs, zero repeats. `storage-stale.json`: forced writer loss left a readable record; close/recreate produced a different HWND with the old tag absent; fresh recovery refused with exit 5 and no recovery writes | Does not prove a reuse interval or exercise actual full-value reuse. Same-process same-HWND/tag mismatch remains pure-test coverage |
| K5 native observation | `k5.json`: 168 polled DWM samples, mean interval 9.9 ms; 14 observed rectangle transitions, all bracketed by fixture-PID-scoped location/move-size events at 150 ms tolerance; 35 callbacks, 18 relevant own-window rows. Synthetic caption drag inserted one down, 14 moves, one up and changed the native DWM frame | Broad temporal bracket, poll aliasing and overlapping stimulus windows do not prove no missed native events. Synthetic border resize was skipped after hit-test miss; no resize input was sent |
| K5 resize retry | `k5-resize.json` in run 2: native thickframe/windowed readback and outer-border HTBOTTOMRIGHT hit; one down, 20 moves, one up shrank outer and DWM width by 160 px and height by 100 px. 172 samples, mean interval 11.1 ms, zero unreadable; 44 callbacks, 22 relevant; 20 observed transitions all temporally bracketed at 150 ms tolerance | One inward corner drag, not all edge/direction or physical-input coverage. Target poll period was 5 ms, not achieved sample interval. First outward target was refused before input; no K5 report/observation/renderer witness join |
| Hide/arrival primitive | `hide-loss.json`: writer published visible/windowed baseline, hid the fixture, then was forcibly terminated with abnormal exit 1. Hidden window remained alive, tagged and uncloaked; saved record survived. Hidden move changed native frame by (+60,+40) px without revealing. Fresh recoverer restored visible=true, exact original outer/DWM frame, show-state 1 and marker preimage with three writes | Single-output bracket only, no frozen core target/output ID/watermark or product ledger/watcher. Non-normal show-state recovery was not tested. Public DWMWA_CLOAK set and clear both returned E_INVALIDARG; cloak remained false, so no cloak-loss recovery result |
| Ordinary identity | `identity.json`: all 11 recorded join predicates true; held process creation/executable/user/session/medium integrity RID 8192 agreed. Checked style/exstyle, class, DWM and cloak readable; owner and parent absent. Production slash/ASCII-lower comparison matched; pure tests preserve non-ASCII case and dot segments | Ordinary medium fixture only; denial/unreadable/elevated matrix unmeasured. Owner/parent absence does not test non-null relationship joins; successful reads do not authorize elevated writes |

- K5 relevant callback-to-next-poll read differences: n=18, min 0.001 ms,
  median 0.058 ms, max 0.276 ms. This is delivery-to-read timing inside the
  observer, not event-generation, render or perceptual lag. The raw callback and
  poll streams are retained for a narrower oracle or nonoverlapping reanalysis.
- The app-level topmost-clear attempt was accepted but natively ineffective after
  a raw native topmost acquisition. The winit setter diffs its cached flags, still
  normal in that case. `extclear-app-attempt.json` and
  `extclear-app-readback.json` retain the accepted reply and unchanged bit. The
  harness native-clear leg measured the intended mechanism without changing
  fixture wire behavior or treating setter acceptance as settlement.
- Added a separate development-only
  [win-dxgi-probe.rs](../../crates/tiler-live-fixture/examples/win-dxgi-probe.rs)
  using cached Windows bindings, not a competing renderer on the softbuffer
  fixture HWND. Pumping the event loop, acquiring foreground and presenting once
  preceded the successful retry; those changes were combined, so the first
  refusal's cause was not isolated. Neither request calls ResizeTarget or chooses
  an arbitrary video mode. Current mode remained unchanged in the measured leg.
- All later completed fixture legs reported closed caller rosters, fixture
  shutdown and no remaining fixture/probe processes. K5 additionally reported hooks dropped,
  cursor restored and mouse button released. Some restoration checks are inline
  verified native reads rather than independent retained full snapshots; no
  stronger claim is made. Final read-only process check found no fixture or probe
  residue. The DXGI immediate HWND-absence check was false; process exit is the
  later cleanup evidence, not an independently retained post-exit HWND read.

The numbered sections below retain the source audit and first-run diagnostic
observations. The later Rust results above are the current measurement summary.

## 1. Fullscreen already present at startup (R14)

- **Verdict: native baseline/request primitives fit; R14 exitability remains a
  gap.** App-owned borderless roundtrip and a dedicated DXGI API grant were
  measured. Neither establishes universal game/presentation-mode detection,
  historical restore-frame recovery, or safe tiler-owned neutral exit.
- Source: [observe.rs:1230-1341](../../crates/tiler-windows/src/tiling_sys/observe.rs)
  reads iconic state, outer/DWM frame, cloak, caption and full-monitor coverage.
  Read failure is unknown/unreadable, not permission to strip chrome. This is a
  captionless-covering-monitor predicate, not a DirectX presentation-mode oracle.
- Source: [fullscreen.rs:286-347,434-468](../../crates/tiler-windows/src/tiling_sys/fullscreen.rs)
  saves owned style bits, maximize state and nonmaximized outer frame before
  project entry. Exit restores that mask/frame and verifies style, but does not
  wait for a settled restored rectangle before clearing metadata. First-seen
  app fullscreen has no such acquired preimage; do not stamp an owned marker.
- Observed (`f1-out.json`): app-driven windowed -> borderless changed caption
  true -> false, style `0x16CF0000` -> `0x16000000`; visible=true and cloak=0.
  DWM frame and fixture physical client size agreed during borderless mode.
  Independently spawned-borderless window had the same borderless style pattern.
  App-driven windowed exit returned the first window's outer rectangle, DWM
  rectangle, style and exstyle exactly to their own recorded preimages.
  This is an app restoring itself, not proof a tiler can restore it.
- `GetWindowPlacement` returned show-state 1 in all three states; its normal
  rectangle tracked the fullscreen rectangle while fullscreen. It did not expose
  the recorded pre-entry rectangle in this case. Fixture `observed.mode` was
  sometimes `windowed` during native borderless coverage and null for the spawned
  case: neither mode value establishes native exitability.
- Way forward: preserve the existing refusal. For an adopted window observed
  windowed first, persist a separate native baseline before subsequent fullscreen:
  owned chrome mask, maximize/placement information and physical outer frame,
  bound to the window lifetime. Keep application baseline distinct from project
  fullscreen ownership and opaque core intent. Prove settled frame/chrome/state
  restoration per case before advertising `fullscreenExitable:true`.
- Not forcing first-seen fullscreen exit avoids the missing-baseline write, but
  is not a claim that every fullscreen game is detectable or that hiding/focusing
  it is harmless. Unknown facts require the existing narrow safety vetoes.
- User-owned: browser F11, player, borderless game, already-fullscreen startup,
  and true-exclusive cases separately. Fixture exclusive mode was not launched:
  it selects a video mode and can change desktop display state; it also would
  not prove true exclusive rather than Windows fullscreen-optimization/flip-model
   presentation. Never use an anti-cheat process as a probe subject.
- Source-only DXGI feasibility: the listed MSVC/Windows SDK route can support
  DXGI/D3D without installing a new tool. Missing Rust feature bindings alone
  are not an impossibility proof. The current fixture has a main-thread winit
  owner and a softbuffer surface/present path
  ([win.rs:109-130,920-973](../../crates/tiler-live-fixture/src/win.rs)); do not
  attach a competing renderer to that existing HWND. A dedicated Windows-only
  own-window rendering path requested DXGI fullscreen in the later measurement
  above, without changing the shared fixture. Presentation-path observability
  and application-specific restore acceptance remain open. A fullscreen-state
  boolean must not be labeled proof of legacy exclusive versus optimization/
  independent-flip/composed presentation.

## 2. Topmost after owner loss (W1/R12)

- **Verdict: standalone forced-loss primitive fits; production recovery gap.**
  Persisted topmost/marker preimages survived real writer loss and were restored
  by a fresh process; production next-start/watcher integration remains unproved.
- Source: [run_loop.rs:1481-1523](../../crates/tiler-windows/src/tiling_sys/run_loop.rs)
  restores only runtime-map preimages on graceful stop, then clears the map.
  [watcher.rs:123-263](../../crates/tiler-windows/src/product_hide/sys/watcher.rs)
  restores/reclaims the hide ledger. It does not provide topmost/chrome recovery.
  R12's watcher-chrome extension is a target requirement, not current behavior.
- Observed (`f2-owner-out.json`, `f2-fresh-out.json`): native topmost false -> true
  -> false followed fixture level setters. After a writer's normal exit a fresh
  process saw topmost=true and restored false. The external preimage file remained
  readable; atomic/durable publication was not tested. The zero-valued native
  preimage property provides no survival evidence.
- Preowned test read true before/after a redundant TOPMOST call. It did not
  leave the window untouched. Redundant true -> true also demonstrates why a
  topmost bit cannot identify another setter's ownership. External clear produced
  false; no core external-clear latch or fullscreen suspension was exercised.
- Way forward: use a nonzero tagged/versioned preimage, where false and true
  have distinct nonzero encodings and absence is separate. A native inert scalar
  can bind to a live window; a larger external slot needs atomic publication,
  corruption checks and exact live tag binding. Persist before writes; on restart
  restore only verified project leftovers. Preowned above requires zero writes.
  Core retains the approved acquisition/release/suspension/external-clear latch;
  mechanical recovery must not invent a second stacking policy.
- An external true -> true set is not detectable from the bit. The approved
  observation-based clear/below latch does not require identifying the setter.
  No contract change is proposed to infer that identity.

## 3. HWND reuse and restart-surviving storage

- **Verdict: storage survival and changed-HWND stale refusal measured; actual
  full-value reuse and product restart acceptance remain open.** Existing
  tagged-member design fits the identity requirement in source.
- Source: [native_guards.rs:17-69](../../crates/tiler-windows/src/native_guards.rs)
  explicitly states HWND+PID+process-creation does not detect same-process HWND
  reuse. A nonempty matching live member tag is also required.
  [assignments.rs:59-137](../../crates/tiler-windows/src/tiling_sys/assignments.rs)
  drops stale state on tag mismatch and reissues the token after admission and
  post-stamp identity checks. No title/PID-only restart matching is acceptable.
- Observed (`f3-owner-out.json`, `f3-fresh-out.json`): nonzero test-only property
  `0xA1B2` read back as 41394 from another process after writer exit while the
  fixture window survived. After window close `IsWindow=false`; querying the old
  handle returned zero. This supports inert scalar survival, not opaque bytes,
  real product-marker adoption, read-error classification or atomic publication.
- Observed (`f3-loop-out.json`): 816 creations in 45051 ms; 816 distinct full HWND
  values and zero full-value reuse. This is not a minimum reuse interval or a
  Windows no-reuse guarantee. Increasing handle values alone do not establish
  how underlying handle slots/generation bits are allocated. The initial loop
  failed on asynchronous caller-id reuse; final `closeFinal` returned unknown id
  because it had already closed, with `absentAtEnd:true`.
- Way forward: preserve original lifetime evidence before admission rotates tags;
  verify PID/process creation plus live window tag before returning storage or
  recovering state. Retire/remint untagged subjects conservatively; do not reuse
  historical tokens based on matching handle/process. For opaque <=1024-byte
  intent prefer one atomic external record keyed by a nonzero live window nonce,
  or prove a complete atomic property-publication protocol. Do not store a pointer
  into the exiting process. Reject unverifiable records without native writes.
- Real float/tile/sticky restart markers, mismatched-tag discard, same-process
   full HWND reuse and forced-loss publication interruption were not exercised.
- Source-only refinement: current float/sticky/tile-override markers are inert
  on-window scalars, not an external opaque record
  ([float.rs:116-169](../../crates/tiler-windows/src/tiling/float.rs)). Fresh
  admission rotates the member tag before marker adoption
  ([assignments.rs:108-137,186](../../crates/tiler-windows/src/tiling_sys/assignments.rs));
  adoption validates the new admitted lifetime and reads the surviving markers
  ([restart.rs:307-457](../../crates/tiler-windows/src/tiling_sys/restart.rs)).
  Closing/recreating a window and finding no marker proves absence, not rejection
  of a stale external record. That latter discriminator belongs to the proposed
  opaque-storage design, not a current product mechanism.
- Real marker testing must start a scoped owner, wait for fixture admission,
  settle exact-owner `workspace --float` / `workspace --sticky` actions, stop and
  independently restore, then start a fresh scoped owner and join adoption logs
  to native markers. Settled unfloat/sticky-off followed by restart separately
  tests marker removal. Hand-planting stale properties is not this product path.
  Native `tests_restart.rs` uses message-only windows and real SetProp/GetProp;
  those tests were inspected, not executed as effect-free offline checks.
- Scope does not make an owner launch fixture-local in every respect:
  [cli.rs:56-73](../../crates/tiler-windows/src/tiling/cli.rs) fences managed
  subjects but defaults keyboard takeover and session Mouse Snap prevention on.
  [run_loop.rs:378-646](../../crates/tiler-windows/src/tiling_sys/run_loop.rs)
  installs global observation hooks and normal-owner input/render/tray machinery;
  saved settings also control the default-on taskbar pager. A dedicated launch
  plan must account for those effects and independent recovery. No owner was
  launched for this investigation.

## 4. Border tracking and WinEvent coverage (K5)

- **Verdict: synthetic native drag/resize and temporal event coverage measured;
  K5 causal gap remains.** No local report-observation/native-event/renderer
  witness join or physical drag/resize acceptance was established.
- Source: [observe.rs:416-570](../../crates/tiler-windows/src/tiling_sys/observe.rs)
  handles foreground/create/destroy/show/hide/location and separate move-size
  edges. Delivery PID/tag reads are explicitly hints, never proof of the event's
  original lifetime; destroy avoids reading a potentially recycled handle.
  Visibility generation advances for show/hide, not every v4 invalidating event.
  Existing subscriptions do not by themselves cover every style/cloak/eligibility
  change. See the [phase-0b border gap](v4-migration.md#phase-0b-windows-platform-fit-review).
- Observed (`f4-out.json`): ten programmatic fixture position/size requests;
  37 callbacks total, 33 with matching fixture PID. Those included 26 location,
  3 foreground, 1 create, 2 show and 1 hide callbacks. Location event timestamp to
  callback TickCount difference: n=26, min=0 ms, max=15 ms, mean=2.31 ms.
  These are coarse event-delivery differences, not DWM/render latency or
  request-to-settled-frame measurements. No retained per-event lifetime join.
- Requests were issued before an explicit 12-second drain loop; synchronous
  client/native calls may pump messages. No one-to-one event/write oracle was
  defined, so 26 events for ten requests proves neither complete delivery nor
  event loss. Final callback and later read had equal virtualized outer frame;
  no physical render/witness proof follows. No minimize/cloak stimulus was
  supplied, and close occurred after unhook: zero state/cloak/destroy counts
  establish no coverage result.
- Way forward: bind accepted border source to verified native lifetime, captured
  `obs`, frame/foreground and a synchronous invalidation watermark. Split anchor
  motion from eligibility invalidation; hide/refuse stale or unwatched anchors.
  Build a PID-scoped, continuously pumped event probe with before/after reads
  around each measured step and explicit expected invalidations. Then measure
  real drag/resize separately. Geometry coincidence or elapsed time cannot close
  K5. No FrameWitness wire extension is justified by these observations.

## 5. Hide, cloak and hidden arrival (W2)

- **Verdict: standalone hide-loss recovery and single-output hidden motion fit;
  product recovery/cross-output arrival remain gaps.** Public cloak requests were
  refused, not silently treated as hide or universal cloak unavailability.
- Source: [product_hide/sys/mod.rs:1751-1793](../../crates/tiler-windows/src/product_hide/sys/mod.rs)
  commits a hide claim before ShowWindowAsync or nonactivating SetWindowPos;
  [watcher.rs](../../crates/tiler-windows/src/product_hide/sys/watcher.rs) supplies
  independent dead-owner ledger recovery. The probe bypassed this product path.
  [observe.rs:1246-1259](../../crates/tiler-windows/src/tiling_sys/observe.rs)
  can read hidden windows only after callers prove the exact hidden claim.
- Observed (`f5-owner-out.json`, `f5-fresh-out.json`): visible=true -> false on
  hide, `IsWindow=true` while hidden; outer rectangle changed after a hidden
  SetWindowPos. Another process still read hidden/alive, then reveal produced
  visible=true with show-state 1. Fixture reported a changed output scale and
  physical client size, while logical client size stayed equal. No exact
  PMv2 output/frame bracket, tag fence, focus preservation or geometry-restoration
  conclusion is accepted. Min/max values were requested-field echoes, not native
  enforcement/readback proof.
- Other, visible windows had DWM cloak mask 0/HRESULT 0 in F1/F6; F5 did not
  query cloak on the hidden window. No cloak write/uncloak or restore-after-crash
  was exercised. Product hide is not cloaking. The private
  COM cloak spike is not live-fixture evidence; the README's public DWM cloak
  recipe is unimplemented and does not prove foreign-caller support.
- Way forward: use the existing recoverable product visibility mechanism for
  `windowSuppression`, retaining minimized/maximized preimages and independent
  restore. W2 output-arm arrival must bracket fresh output/frame/visibility with
  matching live tag/process lifetime and observation watermark, compared by core
  to its frozen target. A window can move while hidden; posting hide/show or
  echoing logical membership cannot prove arrival. Unknown evidence must not
  advance focus/departure. Cloak productization is not required by this contract.

## 6. Elevated windows and executable identity (W4/K1)

- **Verdict: ordinary-window identity mechanism measured; elevated read/write
  behavior needs a user-owned step.** No elevated window was launched.
- Source: [native.rs:78-116](../../crates/tiler-windows/src/native.rs) opens a held
  process with limited-query/synchronize rights and exposes identity/integrity;
  [ffi/process.rs:202-281](../../crates/tiler-windows/src/ffi/process.rs) reads token
  integrity, creation time and QueryFullProcessImageNameW. Success can vary by
  access rights/target; a readable executable does not authorize window writes.
  [observe.rs:1104-1137,1312-1317](../../crates/tiler-windows/src/tiling_sys/observe.rs)
  filters nonmedium processes before assigning `elevated:false`. Do not export
  that constant as a raw v4 fact about all windows.
- Observed (`f6-out.json`): ordinary fixture image query succeeded, process
  creation read succeeded, token RID 8192 (medium), session query succeeded,
  and native HWND PID matched. Class read succeeded; DWM frame and correctly
  sized cloak query returned HRESULT 0; cloak=0, alive/visible=true. No elevated,
  protected, denied-query or cross-integrity write result exists in this run.
- Source normalization is exact slash replacement + ASCII lowercase in
  [lifecycle.rs:42-45](../../crates/tiler-windows/src/lifecycle.rs). Probe ASCII
  examples agreed, but its Unicode lowercase helper is not contract conformant.
  Use the production rule and add non-ASCII and dot-segment negative cases;
  do not resolve filesystem aliases, dot segments or Unicode case.
- Way forward: capture supported raw privilege facts before policy filtering;
  per-window unreadable remains null, not permanently unavailable. Declare only
  truly unsupported route facts in ordered `unavailableFacts`: reviewed route
  lacks `managed,acceptsInput,transient,modal,below,role,activationPolicy,layer`,
  and appId/title until readers are added. Produce owner/parent from independent
  native relationships with lifetime joins; do not invent a parent from popup
  ownership. Executable remains required nullable/local-wire-only, not persisted
  or logged as identity text. Style error checking must distinguish valid zero.
- Source/API expectation, not this run's measurement: integrity restrictions may
  permit some metadata reads while refusing property/style/message operations.
  Do not infer a universal elevated permission matrix. A later separately scoped
  elevated-fixture procedure must measure each API/result after user UAC action;
  protected/anti-cheat subjects and secure desktop remain excluded.

## Adjacent open gates

- Before the follow-up below,
  [monitors.rs:4-84](../../crates/tiler-windows/src/tiling_sys/monitors.rs) proved
  production PMv2 enforcement, full/work/primary enumeration and topology-dirty
  plumbing, not an empty-output scale-only notification or physical identity
  producer. The earlier pass changed no scale, display mode or topology.
- QueryDisplayConfig target metadata plus SetupAPI EDID was a proposed
  manufacturer/model/serial/connection route, per the existing platform-fit
  review. Serial absence stays null; DISPLAY ordinal/connectorInstance alone is
  not proved durable physical identity. The follow-up narrows, not closes, these
  gates.

## Follow-up: output identity and empty-output scale (2026-11-02)

- Scope: measure ordinary-process QueryDisplayConfig target metadata and
  read-only SetupAPI EDID; compare identity across process restart and bounded
  display lifecycle changes. Separately observe effective DPI and native
  notifications with one PMv2 probe window per output, including an output
  without other application windows where available.
- Acceptance: retain native before/after readbacks, distinguish serial absence
  and duplicate/zero metadata, test scale-only versus resolution invalidation,
  and verify restoration of every display setting changed. Unsupported physical
  reconnect, sleep/resume and reboot legs remain explicit user steps.
- Approach: a standalone development probe independent of the temporarily
  unported `tiler-windows` crate; effect-free build and rollback review before
  bounded native measurements. No contract/spec/decision edits are part of this
  investigation.
- **Partial evidence only; display power was restored by physical input, not by the probe.**
  R11 metadata reads and process-restart stability fit the nullable shape;
  durable connector fallback and lifecycle acceptance need further evidence.
  Per-output PMv2 DPI/notifications fit on two outputs, but an otherwise-empty
  output was not established. Neither item is fully accepted.

### Provenance and measurement limitations

- External group: `outid-v4-run-20261102-2044`; raw identities and the standalone
  Rust harness remain outside the repository. Repository reference revision:
  `03310546ea9e75217c4cd4d67ca29c52153202d0`. No production tiler was launched.
  Native Windows 11 reports identify medium integrity (RID 8192).
- Post-run artifact SHA-256 independently checked:
  `10856b9efadcd86dcb0e52b0b8e38c516a33d7d122c52180da8eddef7909686b`.
  Reported offline build, eight tests and strict clippy passed; their transcripts
  were not retained. Reports retain PIDs, OS and integrity, but not executable/
  process-start/session bindings or a launch sidecar. An exact-identity forced
  stop was not demonstrated. These preflight omissions limit acceptance; the
  current artifact hash does not retroactively prove every launched artifact.
- Initial review found handled-Ctrl-C, zero-window timer, aligned-token-buffer,
  partial-mode-preimage, topology-readback and DPI-payload deficiencies. The
  measurement harness was corrected before these legs. It remains a disposable
  probe, not an accepted production or general recovery implementation.
- Identity/save-state commands were not PMv2: their monitor DPI and rectangles
  can be virtualized. Only the PMv2 observer's `GetDpiForWindow` establishes DPI
  here. Same-reader before/after geometry equality is not a physical-pixel proof.
- Window enumeration counted visible windows globally, not per output. Its
  `empty_of_ordinary_apps` result cannot establish whether either output was
  empty or occupied. No foreign windows were moved/closed to create that case.

### R11 identity readbacks and matching

| Readback | Output A | Output B | Meaning |
| --- | --- | --- | --- |
| QDC target EDID IDs/device path/friendly name | Present; IDs valid | Present; IDs valid | Ordinary-process read succeeded; friendly name not forced |
| SetupAPI device-key EDID | Readable; parsed | Readable; parsed | Device keys opened `KEY_READ`; manufacturer/product nonzero |
| Numeric serial | Nonzero | Same value as A | Serial alone is duplicated, not a uniqueness proof |
| Text serial/model descriptors | Present | Present | Parser read them, but retained reports record presence only; text contents/stability are unproved |
| `connectorInstance` | 2 | 0 | Zero is a real observation; instance alone is not a connection identity |
| Manufacturer/product/numeric-serial triple | Distinct from B | Distinct from A | Physical triples distinguish this pair; identical-monitor fallback was not exercised |

- Offline `verify-evidence.jq` exactly matched each QDC device path's normalized
  embedded device-instance portion to one SetupAPI instance ID, with matching
  product IDs and manufacturer IDs after byte-order normalization. This is
  retained-string correlation, not a live SetupAPI interface-detail join.
  A producer must not mix QDC raw manufacturer byte order with parsed EDID order.
- `identity-1.json` and `identity-2.json` are separate PIDs; the latter's diff
  has no added/removed/changed paths and identical retained EDID device rows.
  `identity-final.json` repeats that stable result after scale, resolution and
  power requests. Text serial equality was not compared because text was omitted.
- Distinct adapter/target provenance and device paths were readable, but no
  durable normalized `connector` string was established. Adapter LUID/target IDs
  are connection observations, not a reboot-stability guarantee; DISPLAY ordinal
  and a lone connector instance remain invalid substitutes. Until proved, an
  unreadable/unproven connector stays null and ambiguous matching holds.
- EDID bytes/device registry storage are device-derived, not tiler-process
  state. That inspection supports process independence, not actual reboot,
  sleep/wake, physical reconnect, port-swap or identical-monitor stability.
  No such lifecycle was completed; power requests did not remove an active path.

### Per-output DPI and notifications

- One nonactivating PMv2 observer HWND per output was correctly placed; each
  observation report confirms two distinct actual outputs. Initial effective
  DPI was A=120 (scale 1.25), B=96 (scale 1). An eight-second baseline recorded
  none of the three watched messages.
- Scale stimuli used the undocumented `DisplayConfigGetDeviceInfo(-3)` /
  `DisplayConfigSetDeviceInfo(-4)` source-adapter/source-ID packets, with checked
  32/24-byte layouts, valid relative ranges, saved preimages and bounded holds.
  The ABI was inspected in [DPIScaler](https://github.com/EndpointFocus/DPIScaler/blob/master/DpiHelper.h)
  and [SetDPI](https://github.com/imniko/SetDPI/blob/master/DpiHelper.cpp), with
  a corroborating implementation. This is a test stimulus, not a new product
  API dependency. No direct registry/policy write was used for scale.
- Counts below cover the entire apply-and-restore pair, not a guaranteed number
  per change. Native mode reads remained unchanged during scale stimuli.

| Stimulus and readback | A observer: DPI / DPI messages / display messages / settings messages | B observer: DPI / DPI messages / display messages / settings messages |
| --- | --- | --- |
| A scale 125% -> 100% -> 125%; 3 s hold | 120 -> 96 -> 120 / 2 / 2 / 4 | 96 unchanged / 0 / 2 / 4 |
| B scale 100% -> 125% -> 100%; 3 s hold | 120 unchanged / 0 / 2 / 4 | 96 -> 120 -> 96 / 2 / 2 / 4 |
| B enumerated resolution 1920x1080 -> 1600x900 -> original, same refresh; 3 s hold | 120 unchanged / 0 / 2 / 4 | 96 unchanged / 0 / 2 / 4 |

- DPI messages are `WM_DPICHANGED`, display messages `WM_DISPLAYCHANGE`, settings
  messages `WM_SETTINGCHANGE`. DPI wParam X/Y matched raw `GetDpiForWindow`
  readbacks at each affected transition; the other output received no DPI message.
  Both observers received display/settings messages even for scale-only changes
  in these runs. These are observed notifications, not a universal broadcast
  guarantee or proof of every Settings/custom-scaling path.
- `scaleA-set.json`, `scaleB-set.json` and `setmode-D2.json` each retain successful
  apply readback and exact changed-setting restoration. Resolution kept position
  and orientation unchanged. `scale-final.json` matches both original source
  records exactly. Offline comparison of `save-state.json` with
  `save-state-final.json` found equal GDI devices/modes/primary and equal monitor
  facts excluding handles, including full/work areas under the same reader.
- All observer reports natively confirmed both HWNDs absent after destruction
  and unchanged foreground. No remaining probe process was reported; no retained
  exact-process exit binding or power-HWND absence readback establishes more.

### Power restoration failure and remaining lifecycle legs

- `power.json`: own-HWND monitor-power OFF request, timed ON request, OS OFF/ON
  notifications, then another OFF notification 1190 ms after ON. `power2.json`
  repeated with a shorter off interval and again reported OFF 1183 ms after ON.
  The second run began with OFF. The cause of the return to OFF is unknown;
  actual panel illumination was not observed.
- The probe's `status:completed` checks that ON was sent, not that ON remained
  observed. That completion/restoration claim is rejected. The diagnostic repeat
  after the first uncertain restoration deviated from the restoration-stop rule;
  it supplies no recovery acceptance. No further live work follows this finding.
  The last probe-recorded OS display-power state was OFF. Physical mouse input
  then woke both outputs and normal display operation was confirmed by the user.
  Scale and resolution readbacks were restored by the probe; display power was
  not. A monitor-power OFF request is not a self-restoring software probe.
- Topology validation returned success without applying anything. The saved
  `QDC_ALL_PATHS` set includes inactive alternatives and is not a proven-safe
  active-topology restore recipe; no no-op apply or software detach/reconnect
  was attempted. Do not replay that all-paths blob as a reconnect experiment.
  Physical reconnect, safe sleep/wake and reboot comparison remain user steps.
- Contract impact: no demonstrated shape change. Keep nullable raw identity,
  core-owned triple/connector matching and ambiguous holds, plus per-screen
  DPI/96. Remaining obligations are producer normalization and missing evidence,
  not permission to invent a connector, trust a duplicate serial, or declare
  empty-output notification acceptance from these measurements.

## Proposed discriminators (not spec edits)

| Minimal action sequence | Distinction to measure | Unsupported outcome |
| --- | --- | --- |
| Start fixture borderless before observation; attempt neutral exit; separately adopt windowed, capture baseline, let app enter/exit borderless | First-seen missing baseline versus separately acquired native baseline and app self-restoration | Tiler-forced restore/exitability TBD; existing first-seen refusal unchanged |
| Start A normal and B preowned topmost; atomically save A's nonzero preimage, acquire, force verified owner loss, recover in fresh process; externally clear A before repeat | Exact leftover ownership, B zero writes, external-clear latch versus redundant true -> true set | Production recovery/preowned/latch acceptance TBD; standalone recovery and zero-policy-write branch measured |
| Writer changes tagged topmost and scalar marker; force writer loss; clear topmost externally or interrupt recovery after topmost restoration; start fresh recoverer | Independent property reconciliation versus early return after one property already matches | Interrupted-recoverer and production repeatability TBD; standalone marker restore after external clear measured |
| Persist nonzero window tag plus opaque intent; force writer loss; close/recreate in same fixture process, then feed old external record | Process continuity versus exact native-window continuity | Interrupted publication, actual full-HWND reuse/tag mismatch and product opaque intent TBD; changed-HWND stale refusal measured |
| Settle production float/sticky on admitted fixture; stop/restore; restart scoped owner; separately settle removal and restart again | Surviving on-window marker adoption versus settled removal; absent after recreation is not stale-record rejection | Real restart adoption/removal and forced-loss survival TBD |
| Capture accepted border witness; move/resize fixture, then change focus/style/visibility without a new reply | Anchor-only motion versus eligibility invalidation tied to captured observation | K5 causal join, missed-event and physical lag outcomes TBD |
| Hide tagged fixture; move it across outputs while hidden; bracket output/frame/visibility/lifetime before reveal and focus | Actual hidden arrival versus requested/logical home or changed lifetime | Exact W2 arrival/focus and forced-loss suppression recovery TBD |
| Query otherwise equivalent medium and user-launched elevated fixture windows, with checked API errors; normalize synthetic non-ASCII/dot-segment identities | Supported unreadable facts versus unavailable route facts; ASCII-only normalization | Cross-integrity read/write matrix and denial handling TBD |
| Create dedicated windowed DXGI subject, pump/focus, request current-mode fullscreen, read grant/output, request windowed and compare baseline | Fullscreen request/grant versus actual presentation path and app restore semantics | Legacy-exclusive/FSO classification and tiler exitability TBD; API grant measured separately |
| Persist normal, maximized and minimized suppression preimages; force writer loss while hidden; restore in a fresh process | Ordinary windowed reveal versus exact non-normal show-state restoration | Non-normal suppression recovery and product watcher integration TBD; normal standalone case measured |
| Read physical output metadata in two processes; power-cycle, reconnect/swap ports, sleep/wake and reboot separately; repeat with identical monitors or missing serial | Physical triple versus connection fallback, duplicate serials and ambiguous reconnect | Process-read stability measured; durable connector normalization, text-serial stability and lifecycle matching TBD |
| Leave an output without ordinary application windows; prove that condition per output; create its PMv2 observer; change only its scale, restore, then change resolution and restore | Output-owned DPI notification versus dependence on application windows or resolution-only events | Two-output observer notifications measured; otherwise-empty-output acceptance TBD |

## Contract impact and exact next action

- No contract/spec/decision change established or proposed. Keep W1-W4, R12/R14
  and K5 as approved; current gaps are implementation/evidence obligations.
  In particular do not weaken first-seen fullscreen refusal, drop the watcher
  chrome requirement, declare privilege unsupported, or extend FrameWitness from
  these probes. Any such choice requires separate product review.
- This bounded native measurement pass has results for all six areas; the
  investigation remains pending, not archived, because product gates are open.
  Exact next implementation step belongs to Windows phase 3 after the approved
  core/protocol cutover: integrate pre-effect baselines/storage, exact-lifetime
  recovery and W2/K5 observation joins, then verify the proposed discriminators
  against production paths. Standalone probes are not that acceptance.
- Remaining native/user acceptance includes actual HWND reuse/tag mismatch,
  cross-output hidden arrival, non-normal suppression recovery, application
  fullscreen exitability/presentation behavior, physical input and the separately
  launched elevated-fixture API matrix. The output follow-up also leaves durable
  connector/identical-monitor and lifecycle matching, text-serial stability and
  otherwise-empty-output DPI coverage open. Displays were physically woken and
  confirmed normal after the power leg. No v4 production port work is included.

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
  actual HWND reuse and K5 render joins remain open. Later sections qualify
  cross-output hidden-motion/reveal and elevated results; production W2 arrival
  and elevated restoration acceptance remain open.
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

- **Verdict: ordinary identity and a later elevated-fixture read/write matrix
  measured; restore-proof and error-reporting limitations remain.** The initial
  ordinary run below launched no elevated window; the later physical-leg section
  records the elevated fixture separately.
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

## Physical-leg baseline, empty-output DPI and elevated fixture

- Scope: retain repeatable, timestamped, read-only per-output snapshots for
  physical lifecycle comparisons; establish otherwise-empty-output DPI delivery;
  measure a medium observer's read/write API results against an elevated fixture.
- Acceptance: snapshot EDID header/text serials only in external evidence, use
  PMv2 physical monitor facts and per-output visible-window accounting, record
  each native API return/error and readback, restore every successful mutation,
  and close all experiment-owned windows/processes. No contract changes.
- Approach: reuse the isolated native probe without `tiler-windows`; retain one
  stable snapshot command across subsequent physical legs.

### Reusable snapshot and accepted baseline

- Added external-only `outid-probe.exe snapshot --dir <external-evidence-dir>`.
  Each invocation creates `snapshot-<epoch-ms>-pid<PID>.json` with the same
  schema. It changes no display setting and reads EDID registry keys only.
  Transient nonactivating own PMv2 windows supply effective DPI and are destroyed
  before completion. Absolute invocation/evidence paths remain outside this
  repository for reuse after session or system restart.
- Per active path, the snapshot retains QDC source/target metadata, raw and
  canonical EDID manufacturer/product IDs, monitor device path, technology,
  connector instance, DISPLAY ordinal, joined SetupAPI header and descriptor
  serials/model, physical full/work rectangles, primary, effective DPI, and
  per-output foreign-window census. Raw descriptor strings stay external.
  Missing fields/errors remain explicit. The join uses exact normalized embedded
  device-instance equality plus canonical manufacturer/product guards, not a
  substring match or a claimed SetupAPI interface-detail join.
- Evidence child: `outid-v4-run-20261102-2044/snap-leg-20261102-210707`.
  Accepted baseline: `snapshot-1793615579607-pid20520.json`; restored-state
  snapshot: `snapshot-1793615633416-pid20616.json`. Earlier snapshots in this
  child are not physical-coordinate baselines: review found monitor/census
  geometry read before PMv2 setup. The correction establishes PMv2 at entry and
  verifies actual thread-context equality before any geometry read; both accepted
  snapshots report `effective_pmv2:true`.
- Both snapshots retain executable SHA-256, arguments, PID/start/session,
  integrity and artifact metadata. Independently checked current artifact hash:
  `801b416982cbd056eb0c0692aaf3582e7bc5c2264b1d2aa0fa981c649458434d`;
  both accepted snapshots carry that hash. Reported offline snapshot build and
  14 tests passed; independent review confirmed the corrected awareness ordering.

| Baseline fact | Output A | Output B |
| --- | --- | --- |
| Connection technology | External DisplayPort | HDMI |
| Primary / effective DPI | Yes / 120 | No / 96 |
| Connector instance | 2 | 0 |
| Header numeric serial | Present, nonzero | Same numeric value as A |
| Descriptor text serial | Present | Present, distinct from A |
| Checked ordinary visible-content windows, excluding observers | 2 | 0 |

- Canonical manufacturer/product/header-serial triples are distinct, as are the
  descriptor text serials. Offline `verify-baseline.jq` confirms complete retained
  QDC/EDID metadata equality between the accepted snapshots, including descriptor
  contents. This closes the previous text-retention gap for these snapshots, not
  physical reconnect, sleep or reboot stability.

### Otherwise-empty-output scale measurement

- The census requires a visible, nonminimized, uncloaked foreign top-level window
  with positive-area geometry intersecting its assigned monitor; explicit shell
  classes are counted separately. No titles/content are retained. B had zero
  qualifying ordinary windows and zero unknown cloak reads; its remaining rows
  were shell/cloaked surfaces. No foreign windows were moved or closed.
- The final isolated cycle changed B from 100% -> 125% -> 100%, holding for 3 s,
  with one correctly placed PMv2 observer per output. `scale-final-set.json`
  records apply/readback match, restore return 0 and `restored:true`.
  `observe-final.json` supplies the following apply-and-restore totals:

| Observer | Effective DPI | WM_DPICHANGED | WM_DISPLAYCHANGE | WM_SETTINGCHANGE |
| --- | --- | --- | --- | --- |
| A, unchanged output | 120 throughout | 0 | 2 | 4 |
| B, otherwise empty output | 96 -> 120 -> 96 | 2 | 2 | 4 |

- Each DPI payload equals its native `GetDpiForWindow` readback. Native modes
  remained unchanged throughout the scale stimulus. Offline comparisons confirm
  original physical full/work rectangles, GDI modes/primary, metadata and DPI on
  both outputs afterwards. Both observer HWNDs and both snapshots' transient
  HWNDs were natively confirmed absent; foreground was unchanged. No remaining
  probe process was reported.
- Earlier preparation in the same child ran two restored scale cycles, only the
  second observer-covered. Those are not pooled into the final cycle's counts.
  Verdict: the per-output PMv2 producer and scale-only notification route fit,
  including an otherwise-empty output under this explicit census oracle. This
  does not establish every scaling route or empty-output resolution coverage.

### Elevated-fixture attempt

- External group: `elevmatrix-live-20261102-214941`. The single elevation launch
  returned "The operation was canceled by the user." Retained status is
  `uac-declined-or-launch-failed`; no numeric cancellation code was captured.
  This is a canceled launch, not an observed per-API UIPI refusal.
- The elevated fixture did not launch. No elevated HWND, matrix read/write,
  native mutation or recovery occurred; no fixture process remained reported.
  No second elevation prompt was issued.

| Requested matrix portion | Result |
| --- | --- |
| Process open/image/creation/token integrity | Not measured: elevation launch canceled |
| Style/exstyle/class/DWM frame/cloak/owner/parent/property reads | Not measured |
| Move/topmost/property/hide-show/cloak writes and native readbacks | Not attempted |
| Independent elevated restoration and protocol shutdown | Prepared and offline-tested only; not live-proven |

- The external driver and native helpers were independently reviewed; targeted
  corrections added per-mutation identity checks, original user/process/window
  bindings and bounded protocol cleanup. Reported final helper build, nine tests
  and both PowerShell parser checks passed. They are preparation evidence only;
  no successful elevated-launch or cross-integrity result is claimed.
- Ready state: display settings match the accepted baseline; read-only snapshots
  can now bracket one physical action at a time. Physical lifecycle comparisons
  and the canceled elevated matrix remain open. No contract/spec change follows.
  A final direct invocation produced `snapshot-1793616902793-pid10080.json` after
  the canceled elevation attempt: PMv2 verified, all transient HWNDs absent, and
  both outputs' QDC/EDID metadata, physical full/work areas, modes/primary and
  effective DPI equal to the accepted baseline.

### Physical HDMI same-port reconnect

- User-reported action: unplug B's HDMI cable at the PC end, wait about 10 s,
  reconnect to the same port; both screens appeared normal. Read-only after-file:
  `snapshot-1793617501278-pid13852.json`, in the same snapshot evidence child.
  Compared with the accepted baseline using `compare-reconnect.jq`; both active
  outputs present, PMv2 verified, transient observer HWNDs confirmed absent.

| Field | Output A | Output B |
| --- | --- | --- |
| QDC/EDID manufacturer and product IDs, raw/canonical | Same | Same |
| Header serial | Same | Same |
| Descriptor text serial/model | Same | Same |
| Connector instance | Same | Same (zero retained) |
| Monitor device path | Same | Same |
| DISPLAY ordinal | Same | Same |
| Output technology | Same | Same |
| Full rectangle / work area | Same / same | Same / same |
| Primary / effective DPI | Same / same | Same / same |
| Source and target adapter LUIDs | Same | Same |
| Source ID / target ID | Same / same | Same / same |

- B has exactly one physical-triple match; its unchanged adapter/target/connector
  tuple also identifies one candidate. Thus both routes would pair B correctly
  for this same-port reconnect; connector instance zero alone is not sufficient
  provenance or a durability guarantee. No contract change is indicated.
- No primary change or lasting relocation was observed: both ordinary windows
  tracked by HWND/PID retained their rectangles and outputs versus the latest
  pre-action snapshot. The disconnected interval and any transient relocation
  were not observed. Other-port, identical-monitor, sleep and reboot legs remain
  unmeasured.

### Physical DisplayPort move to another port

- User-reported action: move A's cable to another DisplayPort output on the same
  GPU; both screens appeared normal. Read-only after-file:
  `snapshot-1793621643164-pid13112.json`, compared with the accepted baseline by
  physical triple. Two active outputs, PMv2 verified, transient HWNDs absent.

| Field | Output A | Output B |
| --- | --- | --- |
| EDID manufacturer/product IDs, raw/canonical | Same | Same |
| Header serial | Same | Same |
| Descriptor text serial/model | Same | Same |
| Connector instance | Changed | Same |
| Monitor device path | Changed | Same |
| DISPLAY ordinal | Same | Same |
| Output technology | Same | Same |
| Full rectangle / work area | Same / same | Same / same |
| Primary / effective DPI / scale | Same / same / same | Same / same / same |
| Source and target adapter LUIDs | Same | Same |
| Source ID / target ID | Same / changed | Same / same |

- A still has exactly one unchanged physical-triple match, while its old
  adapter/target/connector tuple matches no current output. B's physical triple
  and connection tuple both remain unchanged. This supports physical-first
  matching and confirms the monitor device path is not port-independent identity.
- Primary designation, output positions/arrangement, DPI and scale survived.
  Both tracked ordinary HWND/PID pairs remain on their prior outputs compared
  with the preceding HDMI after-snapshot. One outer rectangle is inset by 1 px
  on each edge; the other's rectangle is unchanged. Cause and transient behavior
  during the cable move were not observed; no corrective writes were performed.
  Return to the original port, sleep and reboot comparisons remain open.

### Physical DisplayPort return to original port

- User-reported action: return A's cable to its original port; screens appeared
  normal. Read-only after-file: `snapshot-1793621823847-pid12700.json`.
  Compared with both the accepted baseline and the swapped-port snapshot above;
  two active outputs, PMv2 verified, transient HWNDs confirmed absent.

| Field | A vs baseline | A vs swapped port | B vs both |
| --- | --- | --- | --- |
| EDID manufacturer/product IDs | Same | Same | Same |
| Header serial | Same | Same | Same |
| Descriptor text serial/model | Same | Same | Same |
| Connector instance | Same | Changed back | Same |
| Monitor device path | Same | Changed back | Same |
| DISPLAY ordinal | Same | Same | Same |
| Output technology | Same | Same | Same |
| Full rectangle / work area | Same | Same | Same |
| Primary / effective DPI / scale | Same | Same | Same |
| Source/target adapter LUIDs | Same | Same | Same |
| Source ID | Same | Same | Same |
| Target ID | Same | Changed back | Same |

- A's original connection tuple returned exactly, with one baseline connection
  match; its physical triple continued to pair uniquely throughout. Primary,
  arrangement, DPI and scale remained unchanged. Both tracked windows stayed on
  their prior outputs and retained the swapped snapshot's outer rectangles: the
  previously observed 1 px inset persisted, without further drift. No corrective
  writes or other persistent anomalies were observed. Sleep/reboot and
  identical-monitor fallback remain unmeasured.

### Physical sleep/resume

- User-reported action: Start > Power > Sleep, wait about 15 s, wake by mouse;
  screens appeared normal. Read-only after-file:
  `snapshot-1793621951225-pid20752.json`. Two active outputs, PMv2 verified,
  transient HWNDs absent; comparison used the accepted physical baseline.

| Field | Output A | Output B |
| --- | --- | --- |
| EDID manufacturer/product IDs | Same | Same |
| Header serial | Same | Same |
| Descriptor text serial/model | Same | Same |
| Connector instance | Same | Same |
| Monitor device path | Same | Same |
| DISPLAY ordinal | Same | Same |
| Output technology | Same | Same |
| Full rectangle / work area | Same | Same |
| Primary / effective DPI / scale | Same | Same |
| Source/target adapter LUIDs | Same | Same |
| Source ID / target ID | Same | Same |

- Both physical triples and original connection tuples still match uniquely.
  Arrangement/primary/DPI survived; both tracked windows retained their outputs
  and outer rectangles versus the pre-sleep return snapshot. The 1 px inset
  therefore persisted, with no further drift or corrective writes.
- Read-only System-log evidence, retained externally as
  `sleep-resume-events.json`, contains Kernel-Power 42 and 107 plus
  Power-Troubleshooter 1 for this transition. The latter's SleepTime/WakeTime
  span 25.455 s, confirming a logged sleep/resume cycle; this timestamp interval
  is not an independently measured low-power residence duration. Raw transition
  timing fields are retained without conflating them with sleep residence.
- Reboot remains unmeasured. The external snapshot executable is an ordinary
  on-disk file, not session memory, but resides under temporary storage: a normal
  reboot does not inherently remove it, while temporary-file cleanup may. Exact
  invocation, evidence paths, baseline/latest filenames and artifact hash are
  retained in external reboot-continuation notes, not repository-local paths.

### Elevated-fixture retry: server launched, matrix not reached

- External group: `elevmatrix-live-20261102-232445`. Elevation launch succeeded
  and the fixture server announced its pipe/PID, but the prepared sender produced
  empty spawn/get replies. The controller stopped with `get did not return
  exactly one ok window` before matrix entry or any native window write.
  `elev-get.json` is empty apart from encoding/newline; client exit codes/stderr
  were not retained. Native argument quoting is a suspected transport fault,
  not a measured UIPI or native-API refusal.

| Requested API group | Result / error |
| --- | --- |
| OpenProcess / image / creation / token integrity | Matrix not reached; no per-API error captured |
| Style/exstyle / class / DWM extended frame / cloak | Not measured |
| Owner / parent / window-property read | Not measured |
| Move / TOPMOST set-clear / SetProp-remove | Not attempted |
| Hide-show / cloak set-clear | Not attempted |

- The controller's finally-path shutdown also returned an empty reply and left
  the server alive. Separate medium-integrity cleanup checked the original
  PID/start/session and announcement binding, then sent one serializer-built
  authenticated request directly through the announced named pipe. A first
  cleanup setup failed before sending because stream `ReadTimeout` was unsupported;
  bounded asynchronous read replaced it without repeating a semantic request.
- `cleanup.json` retains the successful reply: `open_windows:0` and
  `shutting_down:true`, followed by verified exact-PID absence. Thus no fixture
  window/preimage required restoration; independent native restoration remains
  unexercised, not passed. No elevated fixture residue was reported. Direct
  protocol shutdown succeeded; it is not evidence about window-write permissions.
- An additional offline driver defect was identified: its `0u` literal is
  unsupported by the invoked host shell; `[uint32]0` is required. That later
  enumeration branch was not reached. Sender transport and shell defects must
  be corrected before relying on this driver for a native matrix.
- Contract impact: no permission matrix established. Missing/unreadable facts
  remain unknown/null, never fabricated false. A per-window denial would not
  justify route-wide `unavailableFacts`; that list describes absent producers.
  This setup failure establishes neither denial nor unavailability.

### Elevated harness transport diagnosis

- Scope: reproduce the failed launch/controller/spawn/get sequence using only
  medium-integrity fixture processes; retain actual client arguments, exits,
  stdout/stderr and window readbacks, then correct the external harness without
  changing its elevated-only mutation guards. No elevated launch is part of this
  diagnosis.
- **Measured cause:** the legacy positional-JSON native call under Windows
  PowerShell returned exit 1, zero stdout, and `send: invalid JSON payload: key
  must be a string at line 1 column 2`. The identical JSON delivered through stdin
  succeeds. This isolates native-argument quote loss, not window creation or
  working-directory dependence. `legacy-rc-capture` retains the reproduction.
  The earlier elevated run did not retain its native stderr, so attribution of
  that historical empty reply uses this reproducer, not a recovered old error.
- Error handling amplified the fault: with `ErrorActionPreference=Stop`, native
  stderr can throw before the exit-code capture. The repaired canonical external
  `Invoke-ElevMatrix.ps1` supplies JSON through the existing client's stdin,
  captures stderr and immediate exit code under function-local `Continue`, and
  throws an explicit request/exit/error diagnostic. Per-request logs retain
  response status and lengths without nonce text. Unsupported `0u` literals were
  replaced with `[uint32]0`.
- A medium launch with spaces in its path also exposed unquoted `Start-Process
  -ArgumentList` values. Both controller launch branches now use absolute
  executable/controller/output paths, quoted `-File`/`-OutDir` values and explicit
  working directories. The canonical driver is fixed in place; the old broken
  copy is clearly suffixed, with no second active fixed variant.
- Early isolated-script proofs established the transport cause but did not
  establish the actual controller sequence. The accepted proof uses the real
  extracted controller through fixture startup, the unchanged spawn/get requests,
  and PID/title/HWND discovery, with a medium transport-proof mode that exits
  before the elevated-only owner/matrix stages. Native integrity guards remain
  unchanged.
- Accepted evidence: `med proof spaced/run 2`. All six actual controller requests
  (spawn, get, list, close, list, shutdown) returned exit 0, `ok:true`, empty
  stderr. Exactly one visible fixture window matched announcement, protocol
  snapshot and native HWND/PID/title observations. Close emptied the roster;
  shutdown returned `open_windows:0`, and fixture-PID/HWND absence was verified.
  The controller log completes cleanup. `controller_gone:false` in the wrapper
  status was sampled before that finally-path finished; later controller absence
  was reported, not separately retained as a machine-response artifact.
- Independent offline review confirmed the same-controller path, quoted absolute
  launch arguments, no elevation in proof mode, preserved high-integrity guards,
  six successful retained replies and zero parser errors. It found no blocker to
  the next elevated transport attempt. Elevated owner capture/dry-run, API matrix,
  restoration and watchdog behavior remain untested by this medium proof.
- The prior medium direct-pipe shutdown already showed that the old elevated
  endpoint was reachable for that operation; no alternative window-creation
  design is needed on evidence collected so far. No general cross-integrity
  native-window permission claim or contract change follows.

### Elevated fixed-harness matrix: measured, with acceptance limitations

- External group: `elevmatrix-fixed-retry-01`; one elevated launch reached the
  matrix and protocol shutdown. The wrapper returned exit 0 and
  `completed-clean`; that status establishes neither full restore acceptance nor
  authoritative error reporting for every API. No further run followed.
- Reference revision: `fa318dbb8b85977bd16274094b7ede1b3e962e93`.
  Post-run independently hashed artifacts, not launch-time attestations:

| Artifact | SHA-256 |
| --- | --- |
| `tiler-live-fixture.exe` | `4a3845830a6ef1ea1858ba26a38dd9c227a385886223f40f9cb797eee4d3c509` |
| `fixture-matrix.exe` | `2c2b9b5485c58db2836af67b919079cd2fd774301aae309b54f6effb0b6db280` |
| `fixture-owner.exe` | `61e9d0e969073bb5c08ade66ff5f71f5f16d6dd9d2c66152d0aae34a27e67a3f` |

- Native matrix reads report caller integrity RID 8192 and target RID 12288.
  Original executable/process-creation/session/user/HWND/full sentinel identity
  matched at entry and all eight medium per-mutation gates. Elevated final
  restoration gates also matched. Exact identities remain external.
- `matrix-read.json` and `matrix-write.json` retain the following results.
  `le` below means captured Win32 last-error, not necessarily an API-defined
  error channel. Successful DWM reads establish HRESULT success, not a meaning
  for their incidental `le=0`.

| Read API / fact | Measured result |
| --- | --- |
| `OpenProcess` limited-query/synchronize; image, creation, token integrity/user, session; HWND PID/title | Succeeded and matched original identity; elevated integrity was read, not inferred from a filter |
| `GetWindowLongPtrW` style/exstyle | Nonzero readable values, checked last-error 0; topmost=false |
| `GetClassNameW`, `GetWindowTextW` | Nonempty class and exact fixture sentinel |
| `GetWindowRect`, `GetClientRect` | Successful rectangle reads |
| `DwmGetWindowAttribute` extended frame / cloaked | Both succeeded; frame readable, cloak mask 0 |
| `IsWindowVisible` | true |
| `GetWindow(GW_OWNER)`, `GetParent` | NULL handles with last-error 0; no non-null relationship binding tested; no owner-as-parent inference |
| `GetPropW` fresh run-tag property | NULL, captured last-error 2; NULL alone is absence-or-failure, not a denied-read classification |

| Medium-to-high write API | Return / captured error | Observed result |
| --- | --- | --- |
| `SetWindowPos` modest move | Failed, last-error 5 (`ERROR_ACCESS_DENIED`) | Same-reader outer rectangle unchanged |
| `SetWindowPos` TOPMOST / NOTOPMOST | Both failed, last-error 5 | Exstyle unchanged; topmost=false throughout |
| `SetPropW` nonzero scalar | Failed, last-error 5 | No nonzero property value read back |
| `RemovePropW` | Failed/NULL, captured last-error 2 | Still NULL; no successful set preceded it |
| `ShowWindow` hide / show | Previous-visibility return false for both; captured last-error 5 for both | Visibility true before, between and after; no hide/show roundtrip occurred |
| `DwmSetWindowAttribute` cloak / clear | Both HRESULT failures; HRESULT values discarded. Captured last-errors 0 / 6 are not authoritative DWM failure codes | Cloak mask 0 before, between and after; no cloak effect observed |

- No successful medium mutation required reversal. Failed writes plus unchanged
  readbacks support the measured denial/no-effect outcomes, not a universal
  elevated-window permission matrix. `ShowWindow` returns previous visibility,
  not success; the unchanged visibility is the effect evidence. DWM failures
  cannot be labeled access-denied or invalid-handle from these last-errors.
- **Independent restore prerequisite not established.** The controller captured
  `preimage.json`, ran an elevated idempotent dry-run before medium writes, and
  logged exit 0. However, its argument list supplied `--out` twice and the native
  parser takes the first occurrence. Capture, dry-run and final restore all wrote
  `owner-capture.json`, overwriting earlier reports. Nominal `owner-dryrun.json`
  and `owner-restore.json` do not exist; only the final restore report survives.
- Source inspection additionally found that the recoverer records post-state but
  does not compare it with the preimage before returning 0. It also returns 0
  after a mid-restore identity refusal. Thus dry-run exit 0 and wrapper
  `values-restored` are not semantic restore proofs. This is an unresolved
  harness acceptance defect, not a passing recovery test.
- Final retained `owner-capture.json` records successful geometry/level calls,
  visible=true, unchanged style/exstyle/client/outer values, cloak mask 0 and
  NULL property readback matching their preimages. Its cloak write failed even
  at high integrity, with the HRESULT again discarded. No successful cloak or
  nonzero-property restore was exercised. DWM bottom differs by 1 px from the
  initial preimage; its cause was not isolated. Neither native helper establishes
  PMv2, so compare only intra-API readings; exact physical geometry restoration
  is not accepted from this leg.
- **Shutdown established.** Main authenticated fixture `shutdown` returned exit
  0, `ok:true`, empty stderr. Spawn/get also returned exit 0 and `ok:true`.
  `elev-done.json` and `status.json` report fixture absence; the controller log
  subsequently confirms process absence and no matching fixture HWND. A later
  read-only process check also found the fixture and native helpers absent.
  Redundant finally-path shutdown returned connection error 2 after the pipe
  was gone; it does not invalidate the successful main shutdown. No fixture
  residue remains reported.
- Independent offline review confirmed the matrix/artifacts and identified the
  overwrite and exit-status defects. No corrective live rerun was performed.
- **Contract comparison:** readable privileged/executable/style/class facts must
  retain their measured values; this target's privilege is true, not false.
  Supported-but-unreadable facts remain null and hold only dependent actions.
  Write denial does not negate a readable fact or justify a new route-wide
  `unavailableFacts` entry. Existing absent-producer declarations remain as in
  the contract; this standalone reader does not install production producers.
  No contract shape change is established. Options are to retain these qualified
  measurements with recovery/error-detail gaps open, or obtain stronger evidence
  later with distinct report paths, retained HRESULTs, checked postconditions and
  PMv2. Broader elevated control is not supported by this run.

### Physical reboot comparison (2026-11-03)

- User-reported action: reboot, sign in, confirm both screens normal; monitors
  left unchanged. The retained snapshot binary was present, so no rebuild was
  needed. The stable read-only command returned exit 0 and created
  `snapshot-1793624674297-pid6716.json` in the existing snapshot evidence child.
  Artifact hash matches the accepted pre-reboot hash above; two outputs,
  effective PMv2 verified, all transient own HWNDs natively confirmed absent.
- Compared with both accepted baseline `snapshot-1793615579607-pid20520.json`
  and latest pre-reboot `snapshot-1793621951225-pid20752.json`, pairing by
  canonical manufacturer/product/header-serial triple. Each baseline output has
  exactly one post-reboot match; no duplicate triples or zero identity fields.
  Numeric serial alone remains duplicated across the two outputs.
- External comparison artifacts: `reboot-compare-fields.jq`,
  `reboot-compare-vs-baseline-1793624674297.json` and
  `reboot-compare-vs-presleep-1793624674297.json`. The latter filename refers to
  the latest pre-reboot sleep/resume snapshot, not a pre-sleep sample. Both
  comparisons have the same field-level outcome:

| Field | Output A | Output B |
| --- | --- | --- |
| EDID manufacturer/product IDs, raw/canonical | Same | Same |
| Header numeric serial | Same | Same |
| Descriptor text serial/model | Same | Same |
| Connector instance | Same | Same |
| Monitor device path | Same | Same |
| DISPLAY ordinal | Same | Same |
| Output technology | Same | Same |
| Physical full rectangle / work area | Same / same | Same / same |
| Primary / effective DPI / scale | Same / same / same | Same / same / same |
| Source adapter LUID | Changed | Changed |
| Target adapter LUID | Changed | Changed |
| Source ID / target ID | Same / same | Same / same |

- Physical metadata, arrangement, primary, work areas and DPI survived this
  reported reboot. Source/target adapter LUIDs changed as expected; the old
  adapter/target/connector tuple therefore matches neither current output even
  though both physical triples pair uniquely. LUIDs are not durable physical
  identity keys. No HWND/PID continuity was inferred across reboot, and no
  foreign-window restoration or display-setting write was performed.

### Physical-check series closing summary

| Leg | Accepted observation / remaining limit |
| --- | --- |
| HDMI same-port replug | Physical and connection metadata, arrangement and DPI unchanged after reconnect; disconnected interval unobserved |
| DP alternate port | Physical triple/text serial stable; device path, connector instance and target ID changed; physical-first pairing remained unique |
| DP original-port return | Original connection tuple returned; arrangement/DPI retained; prior 1 px ordinary-window inset persisted without further drift |
| Sleep/wake | Output metadata, arrangement and DPI unchanged; logged sleep/resume corroborated the user report, not independently measured residence |
| Reboot | Physical triple/text metadata, arrangement and DPI stable; source/target adapter LUIDs changed; unique physical pairing survived |
| Otherwise-empty-output DPI | Explicit census established empty B; scale-only 100% -> 125% -> 100% delivered per-output DPI messages and restored settings; not every scaling path or empty-output resolution coverage |
| Elevated fixture | Metadata readable; tested medium writes denied or had no observed effect; protocol shutdown/absence established. Restore prerequisite, exact physical geometry and DWM failure HRESULTs remain unaccepted |

- **Contract impact: no shape change.** These observations support nullable raw
  physical metadata, physical-first unique-triple matching, ambiguous holds and
  per-output DPI/96. They do not prove uniqueness for identical monitors or a
  durable normalized connector fallback. Preserve null for absent/unreadable
  serial/connector facts instead of inventing a key.
- Adapter LUIDs, source/target IDs, DISPLAY ordinals and connector instance alone
  must not be persistent physical-monitor identity keys; nor may a port-dependent
  device path, duplicate numeric serial alone, geometry, primary or DPI stand in
  for physical identity. Connection metadata can describe the current route,
  not promise monitor continuity across port moves or reboot. Both physical
  triples happened to be unique here; ambiguous/identical cases remain open.
- Elevated read success is not write authority. Supported unreadable window
  facts stay null, never false; per-window refusal does not add an
  `unavailableFacts` entry. Existing missing-producer declarations and product
  safety gates remain unchanged. This physical-check sequence is measured, but
  the wider native investigation remains open for its stated evidence and
  production-integration gaps.

## Follow-up: W2 cross-output hidden arrival (2026-11-03)

- **Verdict: qualified native primitives measured; W2 arrival NOT accepted.**
  A fixture window crossed from A (120 DPI) to B (96 DPI) while hidden by
  `ShowWindow`, not cloak, and was revealed on B. A separate forced-writer-loss
  leg left it hidden on B; a fresh recoverer restored the original A preimage.
  Missing arrival bracketing, frame-convention differences and recovery-procedure
  defects below prevent an overall pass. No production tiler was launched.
- Tooling: an isolated development harness extended the earlier hide-loss
  choreography using example-local Windows bindings, without `tiler-windows`.
  In-repository example builds remain blocked by the pre-port product
  dev-dependency. Source, isolated manifest and raw records remain external;
  this is a parked measurement probe, not an accepted reusable recovery tool.
- Reference revision: `e21d11e8`. External groups: `w2-live-20261103-001`
  through `w2-live-20261103-005`; final isolated build group `w2-probe-verify`.
  Final invocation: `w2-probe-verify.exe measure --fixture-exe <fixture-exe>
  --run-dir <fresh-evidence-dir> --stay-ms 8000`. Executable/PID/creation,
  HWND/full sentinel and nonzero tag bindings were retained externally.
  OS build, user/session and integrity were not captured; no stronger
  environment/provenance claim follows.

| Independently rehashed final artifact | SHA-256 |
| --- | --- |
| Isolated harness source | `9ea90175aac8cf8107fa954256be067d0750aa2853c510abc5c35ad80ad83a29` |
| `w2-probe-verify.exe` | `8f2e565b9214e21e649850bffdbbc1d21116b8b37f19ad4646c9115a1662b940` |
| `tiler-live-fixture.exe` | `4a3845830a6ef1ea1858ba26a38dd9c227a385886223f40f9cb797eee4d3c509` |

### Final native readbacks and comparison limits

Rectangles below are PMv2 physical **left/top/right/bottom**, not contract
`[x,y,w,h]`. `w2-target.json` freezes an outer rectangle entirely inside B's
work area; the production adapter reports DWM visible frames instead.

| Stage / retained file | Output / DPI | Visible / cloak | Outer / DWM | Identity and limit |
| --- | --- | --- | --- | --- |
| Before hide, `w2-normal-b.json`, `w2-measure.json` | A / 120 | true / false | `[960,450,1618,977]` / `[968,450,1610,969]` | Handoff/record carry fixture PID/creation/executable, HWND/title/tag; not a separate complete identity snapshot per sample |
| Hidden before crossing | Not separately retained | Hide waited for false | No retained full readback row | Cannot reconstruct the requested before-move output/frame/DPI/lifetime matrix |
| Hidden on B after writer loss, `w2-measure.json` | B / 96 | false / false | `[3191,607,3849,1134]` / `[3198,607,3842,1127]` | Native alive/PID and effect-marker readbacks; first-pass tag/lifetime checks inline, not all persisted per stage |
| Normal reveal on B, `w2-normal-b.json` | B / 96 | true / false | Same B outer / DWM as above | Outer equals frozen target; DWM only contained inside B work area; not a contract-frame equality proof |
| Fresh crash recoverer, `recovered-w2.json`, `w2-measure.json` | A / 120 | true / false | Original A outer / DWM exactly | Show-state 1 and marker preimage restored; four recorded writes; frozen B not restored |

- `w2-preflight-result.json` records a visible offset move on A followed by a
  fresh recoverer restoring the A rectangle. It exercises geometry recovery,
  **not independent hidden-window reveal before the first hide**. Its
  `independent_restore_proven:true` is too broad and is not accepted as the
  hiding prerequisite.
- Mixed DPI: hidden-on-B DPI reads 96 before recovery; A baseline/recovery reads
  120. The harness uses a second `SetWindowPos` pass after cross-DPI settling
  when the first does not reach the exact outer target. Intermediate first-pass
  frames were not retained; this does not isolate DPI message behavior.
  **WM_DPICHANGED while hidden versus only on reveal remains unmeasured.**
- `w2-bracket.json` records wrong-output and stale-frame rejection, an exact
  synthetic frozen construction accepting, and post-recovery A rejecting B.
  These exercise a local comparator, not the core's production bracket path.
  The local epoch is not `obs` or an invalidation watermark. The visible
  `before` is on A, with hide/move/reveal intervening before the sole B sample;
  no visible B lifetime/visibility pair surrounds the output/frame read.
- Frame mismatch matters: B's outer target is 658x527; its DWM frame is 644x520
  with different origin. An exact DWM-frame comparison to that outer target
  must reject, even though both lie within B. Convert/freeze in the production
  frame convention; do not add a tolerance or treat containment as arrival.

### Stopped attempts, procedure defects and cleanup

| Attempt | Stop / subsequent work | Evidence status |
| --- | --- | --- |
| 001 | Hidden outer did not equal fixed client-sized target; window remained hidden. Recovery first failed its geometry postcheck, then a second invocation succeeded; scoped close/list/shutdown reported | Failed restoration was retried rather than stopped/escalated. This violates the restoration-stop rule; later success does not rehabilitate it |
| 002 | After using settled outer dimensions, normal reveal reached B; single-pass rollback missed A. Recovery and scoped teardown reported; a second geometry pass was added | Geometry failure retained, not arrival acceptance; first-pass/corrective-pass frames not retained |
| 003 | Geometry/crash legs completed, but foreground comparison ran before teardown and failed; scoped close returned foreground to baseline | Procedure changed to compare after teardown; foreground preservation throughout the journey is not proved |
| 004 | Teardown checked the wrong reply member (`windows` rather than `ids`); shutdown occurred and absence was reported | Wrapper failure, not native settlement failure; parser corrected |
| 005 | Wrapper exit 0; list empty, shutdown `open_windows:0`, display-read baseline equal, final foreground equal | Qualified final readbacks only; does not erase preceding stops or remaining safety/measurement gaps |

- Second-pass geometry writes lack fresh full HWND/PID/creation/executable/
  title/tag gates immediately before every retry (some only recheck tag).
  This is an unresolved stale-lifetime safety gap, not permission to replay.
  Recovery output filenames are reused within a run, overwriting preflight or
  earlier recovery evidence; prior failed-run write counts are not reconstructible.
- The experiment continued after restoration uncertainty rather than stopping
  under the live-testing restoration rule. No further live run follows
  this audit. Earlier cleanup is reported execution evidence; final retained
  records establish the final leg's tail, not every intervening cleanup state.
- Final read-only process inspection found no fixture or isolated-probe process.
  Seven isolated pure tests passed on independent rerun (record codec/local
  comparison/teardown logic); build reported successful with warnings. These
  checks do not execute native effects or validate recovery choreography.

### Contract implications and remaining discriminators

- No shape change established. W2 forward arrival still needs a fresh
  [output-arm bracket](../porting/world-contract.md#visibility-and-focus-ordering)
  in the production frame convention, matched lifetime/tag/watermark and core
  frozen-target comparison. Hidden output/frame reads demonstrate feasibility,
  not readiness or focus permission.
- R12 rollback and W2 arrival differ: the fresh recoverer intentionally restores
  the saved A preimage, not B. That is a rollback measurement, not a B-arrival
  failure to fix by rewriting the recovery target. After recovery, core must
  reconcile fresh facts and any still-authorized B desire; A must not settle B.
- Proposed minimal rows (outcomes still TBD, no spec edits):
  1. Hide on A; move hidden to B; reveal; take matched visible B samples around
     output/DWM-frame reads, then compare frozen B. Production arrival/focus TBD.
  2. Repeat but substitute A output or pre-move frame for the middle read.
     Production rejection/focus withholding TBD; local comparator rejects.
  3. Force exact writer loss hidden on B; independently recover A baseline;
     submit fresh facts against retained B target. Product rollback and
     reconvergence TBD; standalone A restore measured.
  4. Cross 120 -> 96 DPI while hidden with timestamped per-window DPI messages;
     reveal and compare both frame conventions. Message timing/corrective-frame
     behavior TBD. Normal versus maximized/minimized preimages remain separate.

## Proposed discriminators (not spec edits)

| Minimal action sequence | Distinction to measure | Unsupported outcome |
| --- | --- | --- |
| Start fixture borderless before observation; attempt neutral exit; separately adopt windowed, capture baseline, let app enter/exit borderless | First-seen missing baseline versus separately acquired native baseline and app self-restoration | Tiler-forced restore/exitability TBD; existing first-seen refusal unchanged |
| Start A normal and B preowned topmost; atomically save A's nonzero preimage, acquire, force verified owner loss, recover in fresh process; externally clear A before repeat | Exact leftover ownership, B zero writes, external-clear latch versus redundant true -> true set | Production recovery/preowned/latch acceptance TBD; standalone recovery and zero-policy-write branch measured |
| Writer changes tagged topmost and scalar marker; force writer loss; clear topmost externally or interrupt recovery after topmost restoration; start fresh recoverer | Independent property reconciliation versus early return after one property already matches | Interrupted-recoverer and production repeatability TBD; standalone marker restore after external clear measured |
| Persist nonzero window tag plus opaque intent; force writer loss; close/recreate in same fixture process, then feed old external record | Process continuity versus exact native-window continuity | Interrupted publication, actual full-HWND reuse/tag mismatch and product opaque intent TBD; changed-HWND stale refusal measured |
| Settle production float/sticky on admitted fixture; stop/restore; restart scoped owner; separately settle removal and restart again | Surviving on-window marker adoption versus settled removal; absent after recreation is not stale-record rejection | Real restart adoption/removal and forced-loss survival TBD |
| Capture accepted border witness; move/resize fixture, then change focus/style/visibility without a new reply | Anchor-only motion versus eligibility invalidation tied to captured observation | K5 causal join, missed-event and physical lag outcomes TBD |
| Hide tagged fixture; move it across outputs while hidden; reveal and bracket output/frame/visibility/lifetime before focus | Actual hidden arrival versus requested/logical home or changed lifetime | Hidden move/reveal and forced-loss A rollback measured with qualifications; production DWM-frame W2 bracket/focus and suppression recovery TBD |
| Query otherwise equivalent medium and user-launched elevated fixture windows, with checked API errors; normalize synthetic non-ASCII/dot-segment identities | Supported unreadable facts versus unavailable route facts; ASCII-only normalization | Elevated metadata reads and access-denied/no-effect writes measured; DWM failure HRESULTs, independent restore proof and production denial handling TBD |
| Create dedicated windowed DXGI subject, pump/focus, request current-mode fullscreen, read grant/output, request windowed and compare baseline | Fullscreen request/grant versus actual presentation path and app restore semantics | Legacy-exclusive/FSO classification and tiler exitability TBD; API grant measured separately |
| Persist normal, maximized and minimized suppression preimages; force writer loss while hidden; restore in a fresh process | Ordinary windowed reveal versus exact non-normal show-state restoration | Non-normal suppression recovery and product watcher integration TBD; normal standalone case measured |
| Read physical output metadata in two processes; physically power-cycle, reconnect/swap ports, sleep/wake and reboot separately; repeat with identical monitors or missing serial | Physical triple versus connection fallback, duplicate serials and ambiguous reconnect | Process/text-serial stability, HDMI reconnect, DP move/return, sleep/resume and reboot measured; reboot changed adapter LUIDs; durable connector/identical-monitor matching and other lifecycle cases TBD |
| Leave an output without ordinary application windows; prove that condition per output; create its PMv2 observer; change only its scale, restore, then change resolution and restore | Output-owned DPI notification versus dependence on application windows or resolution-only events | Otherwise-empty-output scale/DPI notifications measured; resolution coverage on that empty output TBD |

## Contract impact and exact next action

- No contract/spec/decision change established or proposed. Keep W1-W4, R12/R14
  and K5 as approved; current gaps are implementation/evidence obligations.
  In particular do not weaken first-seen fullscreen refusal, drop the watcher
  chrome requirement, declare privilege unsupported, or extend FrameWitness from
  these probes. Any such choice requires separate product review.
- This bounded native measurement pass has results for all six areas; the
  investigation remains pending, not archived, because product gates are open.
  Exact next implementation step belongs to Windows phase 4 after the approved
  core/protocol cutover: integrate pre-effect baselines/storage, exact-lifetime
  recovery and W2/K5 observation joins, then verify the proposed discriminators
  against production paths. Standalone probes are not that acceptance.
- Remaining native/user acceptance includes actual HWND reuse/tag mismatch,
  production cross-output hidden-arrival bracketing, hidden DPI message timing,
  non-normal suppression recovery, application
  fullscreen exitability/presentation behavior and physical input. The fixed
  elevated-fixture matrix now has qualified read/write results and clean
  shutdown; its independent restore prerequisite, physical geometry proof and
  DWM failure details remain unaccepted for the reasons above.
  Durable connector/identical-monitor matching and untested lifecycle cases
  remain open. Repeatable snapshots retain text serials and physical geometry;
  otherwise-empty-output scale/DPI delivery and restoration were measured.
  Replug, port swap/return, sleep/wake and reboot comparisons in this sequence
  are now measured, with the elevated limitations above still unresolved.
  Displays were confirmed normal after reboot; the earlier monitor-power
  restoration failure remains a rejected result, not rehabilitated by this
  series. No v4 production port work is included.

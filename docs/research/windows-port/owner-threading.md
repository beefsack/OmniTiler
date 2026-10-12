# Windows owner threading assessment

- Date: 2026-10-12. User direction: separate rendering from input early,
  before further owner features make separation harder. User selected target
  C on 2026-10-12: complete A (stage 1) first, then B (stage 2) early, for
  latency only.
- Symptom: native edge-resize mouse delivery is less smooth with the owner.
  WH_MOUSE_LL runs on its installing thread; owner work can delay delivery,
  even when the callback itself is cheap. Actual waiting time is not yet
  measured; see [latency evidence](../../changes/archive/windows-resize-hook-latency.md).
- Source locators below are in `crates/tiler-windows/src/`, base `e506350`
  plus the current Windows follow-ups. No shared-core policy change proposed.

## Model before stage 1

| Placement | Responsibilities and source |
| --- | --- |
| One application owner pump; no explicit background Rust worker | Mouse hook installs here (`tiling_sys.rs:20114-20127`), keyboard hook here (`:20313-20318`), both serviced by `pump_wait` (`:365-390`). Out-of-context WinEvents also install here (`:20002-20039`); callback queues START/END/Wake (`:321-361`). This does not imply Windows/libraries create no internal threads. |
| Hook callbacks on that pump | Keyboard consume/pass, held-pair verdicts, Esc sequences, bounded intent queue, Win-up E8 mask (`snapkey.rs:4468-4633,4640-5134,5260`). Mouse Win+drag press binds WindowFromPoint/GA_ROOT plus published eligibility; moves pass; consumed Down requires swallowed Up (`win_mouse.rs:677-690,713-814,824-940,962`). |
| Owner observation and actuation | Enumeration/identity/DWM (`tiling_sys.rs:415-622,1899-1974`), Engine reconcile (`:6136`), geometry writes/readback (`:6346-6780`), foreground prime/AttachThreadInput/setter/readback (`:6813-6862,6941-7074`), workspace hide/reveal (`product_hide.rs:1117,1237,1315,1424-1561`). |
| Owner overlays | Border/underlay/drop-preview state and HWNDs; refresh before/after owner operations (`tiling_sys.rs:20932-20935,21245-21248`); create/paint/layered upload/place/destroy (`active_border_sys.rs:907-970,997-1046,1061-1353`). |
| Owner service work | Tray HWND/notifications (`tiling_sys.rs:20052-20098,20361-20440`), first-run dialog on caller/owner thread (`tray_sys.rs:809-818`), settings polling (`tiling_sys.rs:19642`), synchronous trace append (`:1096-1105`), recovery-ledger/SPI lifecycle (`lifecycle.rs:800-859`). Mouse Snap prevention is a session SPI setting, not a mouse-event classifier (`mouse_snap.rs:28-58`). |
| Separate processes, not owner worker threads | Crash watcher (`product_hide.rs:1578-1616`) and Settings UI child (`tray_sys.rs:436-444`, `settings_ui.rs:1409`). The watcher is process-loss recovery, not an input/render executor. |

## Implemented stage 1, 2026-10-12

- Complete A is delivered offline; see the
  [delivery record](../../changes/archive/windows-input-thread.md) for gates
  and pending physical checks. `input_pump.rs` now installs/pumps/unhooks both
  product LL hooks and owns their synchronous classifiers. Owner observation,
  Engine, actuation, all three overlays, tray/settings/trace, SPI and recovery
  stay on the original owner. Proof commands retain their same-thread path.
- `input_handoff.rs` carries whole immutable snapshots through an eight-item
  ordered inbox, plus bounded sender retry and safety slots. Safety overflow
  escalates to an all-origin windrag clear with paired-Up/Cancel retention;
  owner forces current origins to republish behind clears even when unchanged.
  No invalidation or suspend-clear obligation is explicitly dropped.
- Callback-visible published/applied policy atomics close fresh classification
  before changed policy transport, preserving stored press/mask obligations.
  Refused publication retains one retry generation through a policy revert;
  every apply acknowledges it. Same-process runs start above prior generations.
- Initial and partial-failure installs reapply retained snapshots; controls
  remove retained origins so gate refresh/retry cannot resurrect invalidation.
  Single-outstanding bounded tagged batches carry intents back, while Esc
  sequences are sampled directly through monotonic atomics at START/END.
- Readiness, per-hook backoff, creator-side unhook retries and bounded stop
  reporting are implemented. A process-level pump reservation survives missing
  or unclean stop and dropped handles, preventing overlapping replacements.
- Input timing summaries go to the owner writer. Native timestamp-to-callback
  gap, message dispatch span and queue/transport age have separate exclusions;
  none is end-to-end physical input latency. Stage 1 physical input improvement
  and the owner-wake correction were subsequently user-accepted.

## Implemented stage 2, 2026-10-13

- Option B reaches target C: `render_pump.rs` creates, pumps and owns all three
  carriers, including accent queries, native presentation and verified cleanup.
  Owner retains Engine/observation/foreign writes and recovery; input stays on
  its separate pump. Owned proof commands retain their same-thread path.
- `render_handoff.rs` forbids unsafe code and carries latest-wins geometry,
  retained safety watermarks, versioned immutable carrier IDs and acknowledged
  actual color/geometry/checksum/proof. Fresh tagged identities are enforced;
  untagged targets report process-only proof honestly. Native visibility,
  foreground, band and applicable fullscreen checks run before presenting.
- Auto-reset events wake renderer for work and owner for acknowledgements.
  Acknowledgement-only owner turns drain applied state/service pending finish,
  avoiding a repost feedback loop; normal refresh preserves follow/reassertion.
- Preview finish waits as one bounded pending owner operation, outside callbacks;
  verified hidden acknowledgement precedes fresh revalidation and actuation.
  Missing hide proof refuses narrowly, never executes an unproven drop. This
  bounded failure reading is tentative, pending user review.
- Hidden carrier creation precedes readiness/ID publication. Failed hides keep
  their safety obligation; verified destruction and successful exit are required
  for clean stop. Missing completion retains the process slot. No new recovery
  ledger, generic actor framework or independent owner authority.
- Native gates, checksum fixtures and scoped agent live carrier/finish/restore
  evidence are in the [Stage 2 record](../../changes/archive/windows-render-thread.md).
  Physical visuals, settings/DPI journeys and latency acceptance remain pending.
  Stage 3 single-output evidence follows below; this is not gaming-cost or
  containment acceptance.

## Stage 3 single-output hardening, 2026-10-13

- Combined transport/core stress and retry/teardown seams are delivered;
  [acceptance record](../../changes/windows-owner-threading-acceptance.md)
  separates native gates, agent live evidence and remaining physical checks.
- Input stop now caches terminal reports and requires finished successful exit
  as well as hook-release proof. A stalled exit never enters an unbounded join;
  panic or missing completion cannot release the process reservation as clean.
- Input/render fixed 10 ms exchange timers were removed. Input work/control,
  outbox drain and stop signal its work event; install retries and transient
  contention use deadlines. Message delivery transfers intents before blocking
  and after return. Renderer waits on its existing work event/native messages
  with outstanding hide-retry deadlines only; stop signals internally.
- Owner still blocks on messages/events or its 100 ms maintenance deadline.
  Existing slower native follow/reassertion work wakes renderer when needed.
  Trace-only native wait-return counters distinguish actual wakes from applied
  acknowledgements, context switches and throttled trace samples.
- Five scoped desktop lifecycle runs stopped cleanly. Long burst evidence:
  238 native move/resize requests and ten close/recreate cycles, with final
  convergence and zero geometry mismatches. Final trace-on idle measurement:
  328.125 ms CPU over 32.05 seconds (about 1.024% of one core); input zero wait
  returns, renderer thirty work wakes across in-window slow-cadence samples.
  No idle input/render timeout polling was observed. This is single-output
  development-build agent evidence, not physical latency or gaming acceptance.

## Options

| Option | Thread split | Fixes mouse-delivery symptom? | Cross-thread state | Rough effort / risk |
| --- | --- | --- | --- | --- |
| A: dedicated input | Both LL hooks and synchronous classifier on one input pump; owner keeps everything else | Removes rendering, queries, Engine, actuation and I/O from the LL installing thread. Addresses the identified application head-of-line mechanism; owner action/overlay completion can still lag. | Owner -> immutable bindings/remaps/eligibility/origin/gate generations; input -> bounded discrete intents, Esc sequences, counters. Held pairing/mask state stays input-local. | Medium: `snapkey`, `win_mouse`, owner publication/drain/lifecycle plus tests. TLS/ordering rewrite, not a small thread spawn. |
| B: dedicated render | Renderer creates/pumps/owns all three overlay HWNDs and their paints/uploads/SetWindowPos; owner posts desired state | Partial only. Removes overlay presentation cost, but observation, Engine, foreign-window actuation, logging and services still occupy the hook thread. B alone does not isolate input. | Desired overlay frames/styles/anchors and safety epochs; renderer -> owned HWND identities, applied generation, outcomes/checksums/counters, critical-hide acknowledgements. | Medium-large: carrier affinity, stale-show prevention, asynchronous proof/inspection and shutdown. |
| C: A + B | Input pump + owner pump + render pump; owner alone owns observation/Engine/foreign actuation/recovery | Isolates LL delivery from both owner and render work; also reduces owner presentation work. No absolute guarantee against OS scheduling, native calls or downstream hook-chain delays. | A and B contracts, coordinated by one owner authority. | Largest combined scope; two independently useful migrations, not one combined rewrite. |

**Recommendation: select C, deliver complete A first, then B early.** A attacks
the input symptom directly. B fulfills render separation and improves owner
responsiveness, but is insufficient alone. A alone may suffice for mouse
smoothness; it does not guarantee responsive overlays or action settlement.
If B-first is preferred, retain those residual input costs explicitly.

## Contracts that must survive the split

- **Input decisions remain synchronous:** consume/pass cannot wait for owner
  observation or Engine replies. Preserve modifier/foreground reads and mouse
  root-at-point binding; foreground is not a substitute for WindowFromPoint.
  Unknown/stale eligibility cannot invent a new consumption. After consuming
  Down, keep its paired-Up/repeat/mask obligation despite config/suspend changes
  (`snapkey.rs:4484-4495`; `win_mouse.rs:906-940`). Saturated new Down passes;
  terminal Up/Cancel delivery retains its existing guarantee. Moves remain
  pass-through and do not become an unbounded per-motion intent stream.
- **Published state is not shared mutable TLS:** current SNAP/STATE accessors
  are same-thread (`snapkey.rs:4468-4616`; `win_mouse.rs:713-814`). Replace owner
  calls with bounded publication/control and drains. Build snapshots off the
  input thread; callbacks must not wait on owner-held locks, enumerate windows,
  clone large maps, run Engine, paint or write files. Preserve ordered config
  changes, invalidation, held pairs and monotonic Esc START/END snapshots across
  WinEvents and LL callbacks; moving TLS without this bridge is incorrect.
- **Both LL hooks stay together:** mouse Down reserves the keyboard Win-up mask
  synchronously (`win_mouse.rs:888-894`; `snapkey.rs:4610-4616`). The callback
  E8 mask remains before Win-up pass-through. It is distinct from the owner's
  foreground E8 prime; do not move or merge either silently.
- **Owner retains foreground actuation:** keep the E8 prime, foreground-thread
  AttachThreadInput/detach, one setter and exact readback on owner. Input/render
  threads do not attach themselves to foreign input queues. Owner focus-settle
  pumping no longer pumps LL callbacks under A; new intents still queue without
  recursive Engine dispatch (`tiling_sys.rs:7024-7074`). Validate key-state/reset,
  preheld modifiers, injected-event filtering and mask ordering after migration.
- **Mouse Snap prevention stays owner lifecycle work:** SPI setup/suspend/resume/
  restore under its existing lease, not inside LL mouse callbacks. Workspace
  hide/reveal and foreign geometry/style writes also stay owner-only.
- **Latest-wins applies to replaceable render frames, not safety history:**
  bounded desired slots replace old geometry; ordered Hide/Cancel/suspend/reset
  epochs invalidate pending Shows. Before presenting, recheck current native
  target/foreground, lifetime, visibility and band against the desired epoch;
  stale or unreadable facts hide/refuse narrowly. Latest geometry alone is not
  a game-safety proof. Renderer must not independently run Engine/layout policy.
  Where existing behavior requires hide before actuation (preview finish), use
  an acknowledgement/pending owner operation outside callbacks, with bounded
  failure handling. Do not replay late Shows or claim a hide before its result.
- **Renderer owns window affinity and proof outcomes:** create, pump broadcasts,
  query accent, present and destroy on renderer; HWND ownership is not moved by
  sending an integer handle. Preserve click-through/noactivate/tool styles, full
  DWM/native freshness checks, border below target in its band, underlay beneath
  the lowest renderable group member, and preview's above/topmost plane
  (`active_border_sys.rs:350,720-853,997-1046,1116-1136`). Z-order uses fresh
  foreign anchors, not foreign writes. Publish immutable overlay IDs so owner
  observation/WinEvents exclude them (`tiling_sys.rs:1920-1928,20205-20270`).
  Inspection/logs consume acknowledged applied generations and bounded outcomes,
  never owner's desired state masquerading as rendered pixels.
- **Logging remains off the input pump:** input/render counters and bounded
  outcomes go to the owner writer; no synchronous callback logging. Extend
  timing to queue age, installer dispatch gaps and renderer spans, with honest
  coverage/exclusions; callback duration alone does not measure delivery delay.
  No raw input/content capture or separate logging subsystem proposed.

## Stop, failure and recovery

- Retain one process-level owner, lease and recovery ledger. Threads do not get
  independent ownership claims. Keep ledger commit before hide and watcher start
  before product effects; watcher stops only after verified reveal/restore
  (`lifecycle.rs:800-859`). Transient owned overlays do not justify a new ledger
  schema. Existing fullscreen metadata/recovery semantics remain unchanged.
- Graceful stop: close new-intent admission, invalidate render epochs, cancel
  queued work, uninstall both hooks on their input pump, destroy overlays on
  renderer, then complete owner restore/reveal before retiring the watcher.
  Keep consumed-pair handling until hook release; do not wait indefinitely for
  physical release or invent a new synthetic-key cleanup policy. Startup and
  stop need readiness/completion acknowledgement and bounded failure reporting.
- Never hold owner/publication locks while awaiting thread completion. A hung
  renderer cannot be repaired by DestroyWindow from owner or forced thread
  termination. Do not claim a clean stop on missing acknowledgements. Ordinary
  renderer/input failures degrade narrowly with later retry and new generations;
  a stuck thread requires existing exact-owner process recovery, not duplicate
  overlapping workers. Emergency stop remains exact-process termination
  (`lifecycle.rs:1175-1204`);
  process exit releases hooks/windows, while watcher/independent restore handles
  ledger-recorded external effects (`product_hide.rs:1688-1738`;
  `lifecycle.rs:1043-1152`).

## Gaming relationship and staged delivery

- The [gaming-coexistence alternative](../../changes/windows-gaming-coexistence.md#parked-choices-and-recommendation)
  is unselected for Win+G/F11 containment; [backlog](../../backlog.md) keeps it
  parked. Selection here would be **for latency**, not evidence of containment.
  Record that distinction when target/order is selected; Xbox detection and
  shortcut containment remain separate. Added pumps/queues still need the M16
  minimal/imperceivable idle, active and suspended gaming-cost evidence.
- [Principles](../../principles.md): Simplicity - one owner authority, bounded
  handoffs, no generic actor framework; Resilience - narrow degradation/retry,
  truthful stop/recovery; Observability - acknowledged outcomes and explicit
  queue/dispatch timing; Gaming Compatibility - fresh suppression and measured
  minimal cost, never inferred from thread separation.

| Stage | Independently useful delivery | Offline evidence | User physical check |
| --- | --- | --- | --- |
| 1: complete A | Remove owner work from LL pump; retain existing renderer and all current input semantics. Include startup/stop/failure handling in this stage. | Existing classifier/queue fixtures plus deterministic snapshot/config races, consumed-pair/saturation/terminal edges, Esc ordering, mask reentry, install/retry/stop fault seams. Native common DoD gates. | Same scoped edge drags, dev/release and trace-on/off; chord holds/releases, preheld modifiers, Win+drag/Esc, focus/workspace actions, graceful stop/restore. Establish input improvement separately from settlement speed. |
| 2: complete B, reaching C | Move all three carriers to renderer; reduce owner presentation work without changing pixels, follow or z-order. Include affinity/acknowledgement/recovery in this stage. | Latest-wins/hide/cancel/reset race fixtures, stale target/recycled-ID refusal, applied-state inspection, raster checksum equivalence, creator-thread cleanup and shutdown failures; native DoD gates. | Border/underlay/preview follow and placement, no stale show after cancel/suspend/drop, accent/settings/DPI changes, workspace hide/reveal and stop cleanup. |
| 3: C acceptance/hardening | Single-output combined transport/resilience hardening delivered; physical latency and gaming-cost target remain. Correct concrete integration failures; no shutdown basics deferred here. | Delivered: 3,000-round bounded queue/epoch stress, retry/teardown fault seams and native gates; scoped lifecycle/burst/idle evidence. Publication CI pending. Portable tests cover contracts, not HWND/hook delivery. | Single-output physical visuals/input pending. Multi-output/mixed-DPI, gaming/suspension cost and separately scoped exact-owner emergency/crash recovery remain deferred. |

- Native gates: root mise-selected MSVC locked four-package build/test, fmt,
  strict all-target Clippy and whitespace per [common DoD](../../backlog.md).
  Keep pure handoff/classifier/raster fixtures portable where possible; actual
  install/message delivery/HWND affinity/foreground behavior require cfg(windows)
  seams and physical checks. Native window/hook tests are not offline gates.
- Physical checks follow [live Windows testing](../../live-windows-testing.md).
  This assessment supplies no new performance or lifecycle acceptance.

## Selection and remaining acceptance

- Target C, A then B early, was selected on 2026-10-12. Stage 1 physical input
  improvement was user-accepted; Stage 2 is delivered offline with scoped agent
  live evidence. Stage 3 single-output combined hardening has offline and scoped
  agent live evidence. Physical visual/follow and post-hardening input/latency
  checks remain pending, together with multi-output/DPI and gaming-cost legs.
  Preserve the contracts above rather than changing consumption, fullscreen
  policy or recovery behavior as part of thread placement.
- A latency threading selection does not select a Win+G/F11 containment remedy.
  New behavior tradeoffs discovered in implementation require their own outcome
  discriminator; do not resolve them by weakening pairing or game safety.

## Primary Win32 constraints checked

- [LowLevelMouseProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc):
  installer-thread message delivery, timeout/silent removal, dedicated-thread guidance.
- [DestroyWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroywindow):
  cannot destroy a window created by another thread.
- [AttachThreadInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-attachthreadinput):
  input-state coupling requires message queues and resets keyboard state.

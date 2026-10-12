# Windows render thread (Stage 2)

- Outcome: Stage 2 delivered offline with scoped agent live evidence; physical
  acceptance remains pending below. Stage 3 has not started.

## Goal and scope

- Complete option B, reaching target C: a dedicated renderer creates, pumps,
  presents and destroys the active border, group underlay and drop-preview
  carriers. Owner retains observation, Engine, foreign actuation and recovery.
- Preserve pixels, follow, z-order, foreground and input semantics. No Stage 3
  hardening or Win+G/F11 containment claim.
- Binding design: [owner threading](../../research/windows-port/owner-threading.md),
  especially safety history, native freshness, acknowledged inspection and stop.

## Acceptance

- Safe bounded cross-thread handoff: latest-wins geometry, retained safety
  epochs, immutable overlay IDs, event-driven renderer acknowledgements.
- Preview finish hides with acknowledgement before owner actuation, outside
  callbacks, with bounded failure handling and no stale-show replay.
- Renderer-affine lifecycle and truthful readiness/stop/failure outcomes;
  no overlapping replacement after missing completion.
- Deterministic race, stale/recycled-target, applied-state, raster-equivalence,
  cleanup and shutdown-failure evidence; native four-package build/test, fmt,
  strict all-target Clippy and whitespace gates; publication CI.
- Scoped agent live overlay follow/placement and clean teardown evidence under
  [live Windows testing](../../live-windows-testing.md). Physical visual and latency
  acceptance remains separately user-owned.

## Approach and bounded work

1. Small unsafe guardrail before threading implementation.
2. Inspect carrier/owner coupling; implement renderer, safe handoff and owner
   integration together with targeted deterministic fixtures.
3. Review affinity, safety epochs, acknowledgement ordering and failure paths;
   run native gates and scoped live checks; reconcile concrete findings.
4. Promote decisions and archive the delivery evidence.

## Decisions and evidence

- Guardrail `25de29a`: `forbid(unsafe_code)` on `tiler-core` and
  `tiler-protocol` crate roots and unsafe-free Windows modules `active_border`,
  `group_underlay`, `input_handoff`, `model`, `settings`, `tiling`, `tray`,
  `workspace`, `workspace_owner`. Attribute-only; locked four-package check,
  core/protocol tests, fmt and whitespace passed.
- Tentative, pending user review: enforce the guardrail in existing unsafe-free
  modules now; broader Rust unsafe policy remains a separate pending decision.
- New cross-thread state must use safe standard primitives; Win32 unsafe stays
  at minimal FFI call sites. No unsafe Send/Sync, mutable statics, UnsafeCell or
  cross-thread raw-pointer dereferencing.
- Review exposed preview-band refusal, suppressed recovery/reassertion,
  unacknowledged inspection, missing acknowledgement wake and unchecked native
  teardown. These draft defects were corrected before delivery: ordinary movers
  retain the topmost preview plane, foreign anchors carry their own identity,
  own-border anchors stay group/band-qualified, hides retry until verified, and
  acknowledgement-only wakes do not repost frames. Further integration review
  closed pending-finish test gaps and unreadable-foreground refusal. Native
  verified-hide retries reissue `ShowWindow` independently of cached visibility.

## Delivered architecture

- `render_handoff.rs` forbids unsafe code. Three latest-wins desired slots and
  three bounded applied mirrors use safe standard locks/atomics; posted geometry
  never substitutes for applied pixels. Safety watermarks discard older Shows;
  failed hides retain their obligation with bounded retry and unknown-state
  reporting. Hide reason changes get a fresh epoch without per-tick reposting.
- `render_pump.rs` creates all three carriers hidden before readiness, publishes
  a whole immutable ID snapshot before showing, pumps broadcasts, resolves
  accent and fresh native anchors, presents and destroys on the creator thread.
  Self-heal replaces an ID with a versioned immutable snapshot before presenting.
  Owner observation and WinEvents exclude these IDs.
- Desired frames carry process creation and available window-lifetime tags.
  Renderer rechecks identity, visibility/iconic/cloak, foreground, band and
  captionless-fullscreen facts where applicable. Unreadable foreground refuses
  narrowly; an unfocused mover with unchanged readable foreground still works.
  Underlay uses the lowest live group member, including a visible matching border.
- Owner retains observation, Engine, foreign/foreground actuation, services,
  Mouse Snap lifecycle and the existing ledger/watcher. Product overlays use
  the renderer; owned proof commands retain their same-thread paths.
- Separate auto-reset work and acknowledgement events have owned-handle lifetime
  on both threads (`OwnedHandle::try_clone`). Event creation failure refuses
  startup before renderer effects. The outer owner wait watches input and render
  events; focus-settle waits remain message-only. Acknowledgement-only turns drain
  applied outcomes and service pending finish without an acknowledgement feedback
  loop. Periodic owner refresh retains native follow/z-order reassertion.
- Preview finish occupies one bounded pending owner operation. Only a
  creator-verified hidden acknowledgement satisfies it; fresh owner identity,
  domain, cancellation and suspension gates run before focus/Engine/foreign
  writes. Stop, Esc, cancel, suspension and a new gesture invalidate pending work.
  Tentative, pending user review: a missing hide proof after 500 ms refuses that
  finish without drop or snap-back actuation; an overlapping finish is refused
  narrowly. Unaffected tiling and subsequent gestures remain available.
- Applied acknowledgements include actual geometry/color/checksum, generation,
  safety, hide/identity proof, transport age, renderer span and bounded failure.
  Trace-only `render-applied` logs are written by owner, including coalesced
  Shown-to-Unchanged results. No input/render file logging. Queue age and native
  spans are not end-to-end physical latency measurements.
- Stop closes admission and invalidates desired work before input shutdown;
  creator-side destruction is retried boundedly. Clean completion requires
  verified destruction plus successful thread exit. A missing/unclean completion
  retains the process render reservation, preventing overlapping replacements.
  No new recovery authority, ledger schema or foreign thread-affinity transfer.
- New unsafe is limited to native Win32 event/window/DWM/message-pump FFI and
  owned-handle acquisition. No unsafe Send/Sync, mutable static, UnsafeCell or
  cross-thread raw-pointer dereferencing; handle integers are data only.

## Verification and evidence

- Native toolchain: Rust 1.99.0, `x86_64-pc-windows-msvc`.
- Locked four-package build/test (`tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`): 1,648 passed, zero failed,
  one pre-existing ignored benchmark. Windows library: 502 passed; Windows
  integration: 364 passed. All-workspace fmt, locked four-package strict
  all-target Clippy with warnings denied and `git diff --check` passed.
- Deterministic fixtures cover latest-wins, Hide/Cancel/reset epoch ordering,
  stale/recycled tagged identity refusal, ordinary preview bands, resolved
  underlay identities/bands, unverified-hide retry, applied/unknown snapshots,
  painter checksum equivalence, creator-side lifecycle and missing completion.
  Production pending-finish fixtures cover proof-before-actuation, stale pairs,
  timeout, START/member identity drift, busy finish, Esc/cancel/suspend/new gesture,
  domain drift and stop. No desktop mutation is inferred from these fixtures.
- Scoped agent live evidence, 2026-10-13, owned Notepad and Paint on the physical
  desktop: all three carrier HWNDs share the renderer thread, distinct from
  owner/tray and foreign app threads. Border followed the Engine slot directly
  below its target, hid/reappeared on workspace hide/reveal, retained its HWND,
  and suppressed on owned fullscreen. Accent-resolved border checksum
  `44004db2cf1a4ea5` was stable across the observed builds.
- Group underlay presented below the group and matching border in its ordinary
  band during a native move and hid at completion/fullscreen. Preview was visibly
  shown above the owned foreground in the topmost plane in 60/60 in-hold native
  readbacks, with acknowledged checksum `cfed57a41c0a2fe5`. In the final trace,
  preview generation 77 acknowledged verified hide at safety 6 before the
  subsequent `drag-drop-applied` and one matched geometry write, zero mismatches.
  That drop rewrote the source slot; it was not a cross-slot structural move.
- Final trace artifact SHA-256:
  `DFB796B91789D4C952FFFCEC0FEB9126A656463A302621CBE15CF64B780BC2A1`,
  built from `979d656` plus the delivery draft and trace proof fields. Later
  source correction makes native hide retries independent of cached visibility;
  it does not alter the successful live hide path or painters.
- Every scoped run stopped cleanly: verified renderer destruction, released
  hooks, revealed owned windows, restored Mouse Snap settings, and no owner,
  carrier or ledger residue. Disposables were closed after restoration.
  Native gesture automation and injected input are agent live evidence only;
  physical classifier, visual and latency acceptance is not claimed.
- Independent source review closed all serious findings. The two early draft
  implementation approaches needed the corrections above; no failed product
  experiment or unresolved restoration issue remains.

## Remaining physical acceptance and limits

- Physical border/underlay/preview pixels, smooth follow and z-order; Win+Shift
  underlay, native and Win+drag, Esc cancel, a real cross-slot/group-edge drop,
  cancel/suspend/stop with no stale show, accent/settings/DPI changes, workspace
  hide/reveal, Win+arrow responsiveness and native edge-resize comparison.
- Tagged targets have renderer-enforced window lifetime proof. Untagged border
  targets remain process-creation-fenced with the same native reuse race as the
  previous same-thread path; `id_proven:false` reports this limitation honestly.
- Owner work and native scheduling still contribute latency. Renderer wake
  timeouts/backoff remain bounded fallbacks, not an end-to-end performance claim.
- Stage 2 delivery stops here. Combined stress, multi-output/mixed-DPI journeys,
  gaming cost and broader fault probes belong to Stage 3 and are not accepted by
  this record. No Win+G/F11 containment claim.

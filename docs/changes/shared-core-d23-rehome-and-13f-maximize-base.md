# Shared-core proposal: D23 minimized rehome and 13f maximize base

- Status: proposal ready for cross-stream review, 2026-10-17. Documentation
  only; implementation and native acceptance remain pending.
- Carrier note: KWin carrier line references below name removed
  `kwin/src/plan-adapter*.ts` modules; the review must re-base carriers onto
  the world-route modules per the [world adapter
  guide](../porting/world-adapter-guide.md).
- Approved behavior, not reopened: [D23 option B](../research/d23-minimized-rehome.md)
  and [13f option A](../research/fixed-override-born-maximized.md), recorded in
  [window-state decisions](../decisions.md#window-state-float-sticky-maximize-fullscreen)
  and [fixed-size decisions](../decisions.md#fixed-size-admission).
- Goal: one system-agnostic lifetime rule and one shared maximize-precedence
  rule, with host proof/actuation remaining adapter-owned, per
  [Layering and Simplicity](../principles.md).
- Non-goals: new settings, minimized-window shortcut targeting, native Space
  transfer enablement, a general window state machine, saved-layout restore,
  or changes to automatic fixed D4, ordinary maximize, fullscreen or focus policy.

## 1. D23: forget on confirmed per-window rehome

### Contract and discriminators

- Canonical row: [D23 source-slot discriminator](../spec/reference-outcomes.md#d23-best-effort-restore-and-close-discriminators).
  Tile A on WS1 with B/C grouped; minimize B; confirm B belongs to WS2 while
  still minimized; restore there; focus A; return B to WS1. Destination and
  source return both use ordinary fresh admission, not B's former C-group slot.
- The companion row added there returns B to WS1 *while still minimized*,
  before any destination observation can consume its old slot. Restore must
  still admit fresh. This distinguishes immediate invalidation from cleanup
  deferred until destination restore.
- Omission alone, failed/requested sends, unreadable membership and switching
  the current workspace are not rehome confirmation. Relocating an entire
  workspace is not per-window rehoming. Restore geometry follows ordinary
  admission; exact native frames/shortcut reachability remain TBD.

### Current core and proposed minimal delta

| Source | Current behavior / proposed change |
| --- | --- |
| `crates/tiler-core/src/session/world.rs:188-206,1050-1086` | `MinimizedSlot` stores source output/workspace, old leaf and tree. `Session::converge_observation` creates it on tiled omission and releases the allocation. Keep this mechanism. |
| `session/world.rs:598-607,1264-1402` | `Session::forget_minimized_slot` already deletes revision-neutral bookkeeping. Restore consumes a found slot, checks homing, and otherwise fresh-admits. Reuse deletion and admission; do not transplant the source tree. |
| `crates/tiler-core/src/session.rs:1062-1089,1150-1155,1310-1333` | Pair assembly carries slots and split partitions by remembered home. Do not treat assembly, split or requested transfer as proof that native membership moved. |
| `crates/tiler-core/src/boundary.rs:185-206` | `CoreEvent` has `closed_windows`, but no confirmed-hidden-rehome fact. Add a separate optional-at-wire/empty-by-default collection of typed rehome facts. Never fake a close. |
| `crates/tiler-core/src/engine.rs:755-850,867-924` | Single/pair convergence handles close cleanup after accepted convergence and before retirement/canonical store. Add source-scoped, homing-checked rehome cleanup at these accepted-state boundaries. |
| `crates/tiler-protocol/src/planner_protocol.rs:330-335,1389-1416,4528-4533` | Extend the existing DTO, validation and `CoreEvent` mapping alongside close facts; add codec/Engine tests. KWin's carrier is `kwin/src/plan-adapter.ts:8684-8691`. |

- Proposed fact shape (new API, not an existing type):
  `ConfirmedRehome { window: WindowId, source: DomainKey, destination: DomainKey }`.
  Use `confirmed_rehomes` on the source reconcile envelope. Core accepts only
  valid distinct homes in the current owner/generation context; the adapter
  proves the same native lifetime and committed membership. Destination need
  not already have a core session, and this fact never creates one.
- After successful source convergence has stored any same-observation omission,
  drop a matching source-homed slot before session retirement or pair split/store.
  Missing slot or mismatched source is an idempotent no-op. Rejected/diverged
  input must not consume pending proof. Fence replay against a later restore/
  re-minimize cycle using existing ordered request/lifetime discipline, not an
  unfenced delayed call to `Engine::forget_minimized_slot`.
- A destination restore must not overtake the pending source invalidation.
  Queue confirmations until accepted source reconciliation; preserve the first
  departure even if another confirmed move returns B while minimized. Do not
  collapse the history to "current home equals old home, so nothing changed".
- No destination `MinimizedSlot` is needed: the adapter retains minimized
  membership, but core holds no restore anchor. This is the anchorless state
  in the research, represented by absence rather than a new sentinel tree.
  Fresh admission already handles it. No minimize flag is added to live rows.
- A cross-homed observation already cannot rejoin the old slot
  (`session/world.rs:1298-1308`). That fallback is not sufficient: omitted
  windows supply no observation, and source/destination can be separate sessions.
- If a `WorldEngine` consumer supplies these facts, thread the same type through
  `DomainReport`/`WorldReport` and `world_engine.rs` event construction, not a
  second policy (`crates/tiler-core/src/world_report.rs:335-349,379-389`,
  `world_engine.rs:995-1012`; `crates/tiler-protocol/src/world_protocol.rs`).
  Unrelated world-transfer redesign is outside this proposal.

### What confirms rehome on each host

| Platform | Confirming evidence and required adapter behavior |
| --- | --- |
| KDE/KWin | `desktopsChanged` or `outputChanged` is a wake, not proof alone. Re-read the same live native reference's output and desktop membership; resolve a unique changed domain against the last accepted source home. Existing mover signal adapters are `kwin/src/plan-adapter-entry.ts:5263-5283,6006-6012`; add equivalent tracking for omitted minimized identities rather than relying only on an active send flight. Minimized omission currently records homing at `:2952-2964`. Capture the old home before overwriting that map, retain exact-ref identity, and send the source rehome fact even though B stays omitted from geometry rows. Verified send settlement can use the same path (`:4615-4618`); it must not invalidate at request time. |
| Windows | Confirmation must be a committed change in OmniTiler's managed membership, not an OS virtual-desktop event or an iconic frame. `ManagedWorkspaces::member_loc` and `WindowKey` supply source and lifetime (`crates/tiler-windows/src/workspace.rs:56-74,752-754`); `workspace_owner/sends.rs:246-256` verifies source absence/target presence. Current production admission only assigns fresh members (`tiling_sys/assignments.rs:59-136`), and focused-mover sends do not target minimized windows (`workspace_owner/sends.rs:111-164`). There is no current passive minimized-member rehome detector. Wire the new fact at a verified per-window membership commit if/when reachable; do not add OS-desktop integration or infer reassignment from disappearance. |
| macOS managed/native workspaces | Confirmation must come from a same-lifetime native Space-membership read resolving a unique destination, compared with a retained source. Current `build_scoped_snapshot` excludes minimized windows before `spaces_of`/metadata (`crates/tiler-macos/src/native_tile_backend.rs:559-600`); `partition_scoped_members` sees only admitted windows (`runtime_core.rs:417-458`) and `note_scoped_members` prunes omitted PID mappings (`:579-595`). Add membership-only evidence for minimized omitted identities and retain their source home; keep them out of tile/frame observations. Unknown, multiple or ambiguously displayed homes must not confirm. Manual host moves can supply proof; product sends remain unarmed (`runtime_thread.rs:967-975`). Synthetic single-domain mode needs no rehome change. |

- KWin cleanup should mirror exact-ref close-report delivery, not its close
  semantics (`plan-adapter-entry.ts:5394-5449`): pending source-scoped facts
  survive failed delivery and hidden-source reconciliation. Stop resending
  them only after accepted handling, not on bare dispatch.
- Windows can reuse the close queue's scoped/fanout delivery pattern
  (`tiling_sys/assignments.rs:300-326`, `admission.rs:630-726`), preserving
  `WindowKey`, live member-tag and process-identity checks. Continue omitting
  iconic rows; do not send `closed_windows` or a fabricated visible row.
  Whole-workspace migration carries iconic members (`migration_run.rs:559-572`)
  and must preserve their same-workspace restore context instead.

## 2. 13f: durable explicit tile wins beneath native maximize

### Contract and discriminator

- Existing [R-SPC-13 marker discriminator](../spec/reference-outcomes/special-windows.md#r-spc-13-adopt-an-already-mapped-fixed-client-on-owner-startup)
  already covers restart, hint loss, both born settings and the ordinary
  non-fixed N control; no duplicate spec row is needed.
- Explicitly tile known fixed-origin E, maximize, restart/adopt and disable/
  enable tiling; repeat with/without fixed hints and with each born setting.
  Validated durable override keeps a tiled reserved base and native maximize.
  Unmaximize returns to that tiled base, without acquiring automatic origin.
- Automatic fixed D4 still has a floating base/no reserved tile; generic
  non-fixed maximize routing, fullscreen holds, safety gates and one-shot
  attempt rules are unchanged. Missing/invalid intent still uses the existing
  recompute fallback; hints alone cannot manufacture durable intent.

### Current core and proposed minimal delta

| Source | Current behavior / proposed change |
| --- | --- |
| `crates/tiler-core/src/born_maximized.rs:57-66` | Setting controls ordinary admission clearing. Keep enum, values and defaults unchanged. |
| `crates/tiler-core/src/window_policy.rs:286-339,351-374` | Existing clear predicates know born mode, slot, attempt and domain facts, but not durable fixed-origin intent. Extend this policy surface with one normalized `durable_tile_override` input, preferably an override-aware wrapper preserving ordinary callers. It makes admission-clear false under either mode; existing slot/geometry rules remain separate. No new policy module or state machine. |
| `crates/tiler-core/src/session/world.rs:1038-1046,1342-1397` | `fixed_suppress`/retained override already defeats automatic fixed admission; maximized tiled rows can reserve a normal slot. Keep the tree/admission algorithm. Pin override + maximize + hint-loss behavior in core tests rather than inventing a second reserved-slot representation. |
| `window_policy.rs:151-175` | Reserved membership is not permission to write native geometry: overlays continue to fence setters. Override does not bypass safety, fullscreen, sticky or identity gates. |

- Normalize the new boolean from a settled or validated hydrated explicit tile
  over known fixed origin, independently of current hints. Do not equate it
  with any generic explicit unfloat or ordinary admission pin.
- Share the verdict and fixture truth table. Native clear verification,
  identity/store ownership and lifetime maps stay adapter-owned. KWin's
  pre-admission TypeScript gate cannot call the Rust predicate directly;
  mirror the narrow rule with shared-fixture parity rather than introduce a
  native-clear preflight round trip. Windows can consume the Rust gate.

### Required platform integration

- **KDE/KWin:** fix ordering, not just a final flag check. Foreground and hidden
  admission call `clearMaximizeAtAdmission` before classification
  (`kwin/src/plan-adapter.ts:7973-7987,8300-8307`). D7 plants the validated
  suppress record only in `withFixedSizeFloats` (`:4273-4293`), and hintless
  records intentionally omit wire `fixedSuppress` (`:4309-4319`). Resolve
  durable override identity before either clear path, then exempt that exact
  lifetime from clearing (`:8448-8540`) and feed ordinary tiled reservation.
  Do not run all fixed classification early or key the exemption on current
  fixed hints, `pinned` records, or only the wire flag. Preserve intent-bootstrap
  fencing (`:8715-8717`), enable behavior and cross-workspace override carry.
- **Windows:** interim already exempts `fixed_origin_overrides` before hint
  classification (`crates/tiler-windows/src/tiling_sys/admission.rs:71-125`),
  with reserved-overlay plumbing documented at `:128-148`. At shared delivery,
  replace duplicated precedence with the shared verdict and prove unchanged
  outputs. No interim behavior change is requested by this proposal.
- **macOS:** not currently implemented, not blanket "no change".
  `runtime_core.rs:336-356` explicitly emits `maximized: false` (no maximize
  inference) and `fixed_suppress: false`; D7 durable override hydration is not
  wired there. Native fullscreen already has a distinct hold/retention lane;
  do not relabel zoom/fullscreen as maximize. Consume/test the normalized shared
  rule when relevant, but delivery of durable tile restart intent and any
  truthful maximize counterpart needs macOS-stream scope/evidence first.
  This proposal does not silently add a restart store or broaden native control.

## 3. Risks and acceptance plan

| Area | Regression risk and required evidence |
| --- | --- |
| Core D23 | No forget on omission/request/refusal or workspace relocation; same-turn minimize+rehome forgets; repeated accepted fact is harmless; stale source/owner/generation and recycled lifetime cannot erase a newer slot. Cover separate sessions, pair split, source retirement, empty destination and minimized return-before-restore. Extend `crates/tiler-core/tests/minimize_restore.rs` and Engine tests. |
| Core 13f / protocol | Truth-table override x born mode x hint presence with ordinary/D4 controls; override admits a reserved tile but emits no overlaid geometry writes. Extend `window_policy.rs` tests, `test-fixtures/window-policy.json`, `crates/tiler-core/tests/fixed_size_admission.rs` and `tiler-protocol/tests/planner_close_cleanup.rs`-style codec coverage for rehome facts (absent/invalid/duplicate/contradictory input and acceptance fencing). |
| KWin offline | Extend `kwin/tests/minimize-restore-entry.test.ts`, `minimize-close-cleanup-entry.test.ts`, `plan-close-cleanup.test.ts`: exact-ref domain change while omitted, transient/unreadable/sticky membership, failed send and failed delivery, hidden source and output moves, return-before-restore, no invented close/focus. Extend `born-maximized.test.ts` and `float-intent.test.ts`: pre-clear hydration on foreground/hidden startup, hint loss, both modes, enable cycle, ordinary/D4 controls and reused reference. |
| KWin script/effect | Membership notifications, debounce and send settlement must agree without dropping a pending fact or clearing maximize before hydration. Effect visibility remains presentation-only: `kwin/native-effect/activewindowborder.cpp:578-605` listens for minimized/hidden changes to re-anchor groups. No effect policy change is planned; retain native-effect/static coverage and check stale underlay/border or geometry writes on hidden/overlaid windows. |
| Windows offline | Extend `tiling_sys/tests_d23_minimize.rs` using iconic/hidden fixtures to inject a *confirmed managed per-window membership commit*, source acceptance/failure, close and HWND-reuse controls; do not claim the injection proves shortcut reachability. Extend fixed-admission/marker tests for shared-verdict parity, hintless hydration and generic N. Preserve `tests_workspace_migrate.rs` iconic carry as a negative control. |
| macOS offline | Extend scoped-backend/runtime tests: minimized membership-only evidence, preserved old home, unique changed Space, ambiguous/missing reads, PID/window lifetime reuse and current/hidden sessions; no frame/focus effects. Extend `crates/tiler-macos/tests/window_policy_conformance.rs` for shared predicate parity; no native maximize/D7 acceptance claim from this test alone. |

- Future live KDE journey: native minimized reassignment WS1 -> WS2 -> restore
  -> WS1, plus WS1 -> WS2 -> WS1 without restore; repeat across outputs where
  native support permits. Inspect slot removal separately from rendered shape.
  Run the 13f restart/enable/hint-loss matrix under both settings with D4/N
  controls, hidden adoption, native unmaximize, and group effect visibility.
- Future live Windows journey: first establish an actual supported per-window
  minimized membership route; if none, keep that native cell TBD rather than
  adding targeting. Test iconic whole-workspace migration as a non-rehome
  control. Run R-SPC-13's existing E/N restart matrix under both settings.
- Future live macOS journey: validate membership evidence for a manually
  rehomed minimized window in managed/native mode, hidden source and return
  before restore. If native membership is unavailable, record that limit;
  synthetic mode is a no-cross-domain control. Durable intent/maximize native
  journeys remain TBD until the macOS contract exists.
- Before any such run, follow the platform's `docs/live-*-testing.md` protocol.
  This documentation delivery ran no live journeys; it does not mark any
  native outcome as passed.

## 4. Cross-stream questions and recommendations

| Input needed | Recommendation |
| --- | --- |
| KDE: which native action actually reassigns a minimized client, and does the notification expose a unique home? | Start from native desktop/output reassignment, reread exact-ref membership; leave own minimized-send shortcut reachability TBD and do not broaden commands to manufacture a test. |
| KDE/core: can pending rehome proof survive all existing request cancellation/replacement paths without replay into a new minimize cycle? | Reuse accepted-delivery ordering with source/lifetime fences and the return-before-restore regression; add sequence metadata only if existing ordering cannot provide this guarantee. |
| Windows: is any current supported per-window minimized membership route reachable? | Keep the focused-mover contract; implement/test boundary handling now, but do not claim live coverage or add OS virtual-desktop tracking. Whole-workspace relocation is not a substitute. |
| macOS: are excluded minimized identities' Space homes readable reliably, including same-Space display arbitration? | Add membership-only proof where unique and retain source history; unknown/ambiguous homes leave slots intact. Do not use stale minimized frame geometry to guess an output. |
| macOS/user: when should durable fixed-origin restart intent and a native maximize counterpart be delivered? | Track the missing capability explicitly in the macOS stream; adopt the shared precedence without inventing AX maximize or repurposing fullscreen. Decide expanded macOS implementation scope separately, not the already approved behavior. |

## 5. Suggested ownership, order and completion boundary

1. KDE stream: review the host proof and hydration-order audit, then implement
   the core fact/cleanup and shared predicate/fixtures with protocol and KWin
   integration together. Core and KWin offline tests precede native acceptance.
2. Windows stream: consume the shared predicate without changing the interim
   result; wire confirmed managed-membership cleanup and offline fixtures at
   the reachable commit boundary. Keep unreachable native legs explicitly TBD.
3. macOS stream: add minimized membership evidence for managed/native domains,
   then consume the core fact. Report the separate D7/maximize capability scope;
   do not block the synthetic no-change path or enable parked sends.
4. Each platform records its own native journeys and unsupported cells. Update
   the existing canonical rows after evidence, not from proposal approval.

- Proposal acceptance: approved decisions preserved, concrete shared/adapter
  boundaries and discriminators documented, backlog points here, source claims
  reviewed, Markdown links and `git diff --check` pass. No code behavior changed.
- Exact implementation next action: KDE stream reviews the rehome proof carrier
  and pre-clear override hydration ordering, then starts the core/protocol/KWin
  offline unit above; Windows and macOS reviews supply the listed host gaps.

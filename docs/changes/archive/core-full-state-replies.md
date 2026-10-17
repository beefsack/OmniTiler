# Full-state core replies and caller-owned state

## Status, goal and scope

- 2026-10-17: core and KDE delivered offline under selected D1-D5 and capacity option B. Live KDE acceptance and Windows/macOS behavioral adoption remain separate.
- Covers the two P1 backlog items "Core update replies carry workspace state" and "Research: stateless core with caller-owned state" together.
- Goal: one full desired window/workspace snapshot after each accepted platform report, with safe ordering, recovery and a retained stateful Engine.
- Non-goals: persistence across restarts, new platform capabilities, native experiments, and Windows/macOS behavioral adoption.
- Implementation acceptance: the full world report/reply contract below is implemented together in core, protocol and KDE, with deterministic offline recovery/safety coverage, bounded transport measurements and native-platform follow-up boundaries. Live KDE acceptance remains outstanding until separately tested.

## Recommendation

- Retain the stateful, side-effect-free `Engine`. Add a world-level report/reply boundary around its independent domain sessions and the shared workspace policy. Do not serialize caller-owned core state over KDE D-Bus.
- Send a complete normalized observation on every reported update, not just the affected domain. Return a single globally ordered full desired snapshot, not a touched-domain result or a set of independently applicable pages.
- Separate persistent desired properties from one-shot effects. Fresh host observation remains authoritative for external user actions and safety; an unreachable desired property must become an explicit hold/accepted-host result, not an infinite setter loop.
- D1-D5 below select the changed Planner dependency and transfer settlement contract explicitly; idle polling and blind transfer replay remain excluded.

## Investigation baseline

Source locations below describe the pre-implementation investigation. The implemented entry points and final evidence are recorded under Verification and outcome.

| Area | Current implementation and implication |
| --- | --- |
| Retained layout state | `crates/tiler-core/src/engine.rs:54-135` retains sessions, outer gaps, binding, policy options and per-call diagnostics. `crates/tiler-core/src/session.rs:633-697` retains trees/shares through nodes, window links, focus stacks/last-active, exceptions/float frames, fixed-admission overrides, minimize slots, reconciler, pending desired state and drag capture. Observation cannot reconstruct all of this. |
| Determinism | Core has no logging sink (`crates/tiler-core/src/engine.rs:66-73`); source search found no clock, timer, logging or printing calls in `crates/tiler-core/src`. `crates/tiler-core/src/reconcile.rs:229-245` explicitly binds retained pending/revision/fingerprint state. Replaying an event sequence into the same initial engine already has no native IO or wall-clock dependency. |
| Current boundary | `crates/tiler-core/src/boundary.rs:185-206` carries one domain plus optional target, observations, explicit closes and a command. `crates/tiler-protocol/src/planner_protocol.rs:378-410` emits operation-specific geometry/focus/operation data, not a world/workspace snapshot. |
| Convergence | `crates/tiler-core/src/engine.rs:729-828` converges carried membership before the operation and forgets explicit closed slots only after accepted convergence. Pair convergence is atomic before the command (`:830-833`). `docs/decisions.md:1776-1813` preserves topology, observation authority, bounded stale replanning and no replay after setters. A global envelope need not merge domain sessions/revisions (`docs/decisions.md:3151-3155`). |
| Workspace policy | `crates/tiler-core/src/workspace.rs:21-65` defines facts and `plan_trailing`; minimum count is two (`:3`). `crates/tiler-core/src/workspace_scope.rs:245-252` resolves trailing reuse/create; `:701-721` builds facts from ring, occupancy, visibility and retention, excluding sticky all-desktop occupancy. Policy exists, but callers choose when to invoke it. |
| KDE lifecycle | `kwin/src/workspace-native.ts:628-669` directly invokes native create/remove callbacks. Its broader mapping/integration constraint is documented in `docs/changes/architecture-review-ar6-workspaces.md:7-30`: synchronous selection/trailing/hotplug currently works without Planner; no in-script Rust route exists. This is a product dependency/latency decision, not just moving a helper. |
| KDE input/delivery | `kwin/src/plan-adapter.ts:2241-2260` has one flight and correlation rotation, not a world observation watermark. `:2309-2329` forces send-domain reconciliation and retains undelivered explicit closes. `crates/omnitiler/src/planner_service.rs:516-582` accepts JSON strings, refuses concurrent calls as busy, checks caller identity and enforces byte bounds. No generic reliable event queue exists. |
| Restart | Adapter owner/generation rebinding clears core sessions/gaps (`crates/tiler-core/src/engine.rs:457-474`). Service construction retains a Planner (`crates/omnitiler/src/planner_service.rs:295-323`); poisoned-lock recovery replaces it without an independent core-incarnation handshake (`:356-378`). Adapter generation alone cannot distinguish these engine resets. |
| Byte limits | Codec: request 1 MiB/reply 64 KiB (`crates/tiler-protocol/src/planner_protocol.rs:38-46`); reply fallback is correlated, bounded, and contains no state (`:412-434`). The existing 800-window test proves a valid request already exceeds the reply cap (`:12735-12777`). Service also checks reply size (`crates/omnitiler/src/planner_service.rs:577-582`). No new size/performance measurements were made. |
| Host constraints | `crates/tiler-protocol/src/planner_protocol.rs:344-361` carries overconstrained/client-clamped flags. `docs/decisions.md:1791-1801` selects KDE's interim three bounded reassertions followed by accepting the exact client-held frame. Do not invent a generic pixel tolerance or unlimited exponential retry. |
| Safety/authority | `crates/tiler-core/src/window_policy.rs:151-175` distinguishes reserved allocation from allowed geometry writes for overlays. `kwin/src/plan-adapter.ts:2276-2287` retains exact-lifetime born-fullscreen holds. Floating-workspace frames and overlay exceptions remain protected (`docs/decisions.md:799-820`); game safety overrides parity (`docs/principles.md:47-57`). |
| Windows | `crates/tiler-windows/src/tiling_sys/mod.rs:345-348` owns an Engine in-process. `crates/tiler-windows/src/tiling_sys/admission.rs:803-848` normalizes adapter lifetime/overlay/fixed flags. Shared trailing maintenance builds facts, invokes core, removes IDs and creates spares in the adapter (`crates/tiler-windows/src/tiling_sys/scope_trailing.rs:245-327`). |
| macOS | One engine-owner thread and per-app AX workers communicate in-process; main thread is UI-only (`crates/tiler-macos/src/runtime.rs:5-28`), not a separate planner process. `crates/tiler-macos/src/runtime_core.rs:135-169` owns observed focus, lifetime/domain/correlation/revision/repeat caches. `crates/tiler-macos/src/runtime_thread.rs:1001-1156` merges per-domain replies and withholds overlay writes; planned focus never becomes observed truth. Native domain retirement is separate (`crates/tiler-macos/src/runtime_native.rs:76-135`). There is no Windows-equivalent logical ring in this current runtime; native Space capabilities must not be assumed. |

## Caller-owned core state: options and consequences

| Option | Reliability/testability and cost | Recommendation |
| --- | --- | --- |
| A. Retained Engine + explicit full report/result | Keeps one owner/serialization point, independent domain sessions and existing transactional internals. Replay tests can record initial settings/binding plus complete reports and commands. Restart still bootstraps observation and loses session-only topology/history. Full observations/results add work, but no extra state transfer. | Select for this change. Determinism is already present; the missing reliability contract is observation/delivery, not the location of a Rust value. |
| B. Opaque caller-owned typed state, consumed/returned by a pure transition; service holds it for KDE | Policy and pair atomicity stay in core. In-process moves need not clone or serialize anything. Makes all transition dependencies explicit and can simplify branch/replay harnesses, but mostly renames today's ownership: Windows already holds an Engine, macOS's engine thread already holds one, KDE service still retains state. Requires untangling policy handles, diagnostics, pending/drag state and numerous direct Engine queries. No automatic restart recovery/persistence benefit. | Sensible optional later refactor if a concrete testing/embedding benefit justifies it; do not couple it to full-state delivery now. |
| C. No service-retained core state; actual platform client sends opaque state every call | The same core policy still owns every decision; platforms do NOT reimplement topology. Windows/macOS can move typed values cheaply. KDE script must retain, return and version opaque serialized trees, histories, slots, exceptions and staged state; both observation and state cross D-Bus, and reply carries updated state plus desired output. Adds codec/copy/size/validation and atomic replacement obligations. Crash durability still needs an explicit store and migration contract; a lost response can lose the updated state just as today it loses a reply. | Reject for now. No measured need offsets KDE cost and migration risk. |

- Windows/macOS do not gain a process boundary by choosing B or C. KDE can use B without C: the service is the library caller and owns the opaque value; moving bytes to TypeScript is a separate architecture choice.
- A pure API is not inherently O(state-size): moving a Rust value is not deep copying. Conversely, immutable branching/serialization may traverse all retained state, including minimize trees and drag/pending captures. Benchmark rather than assert a performance regression or win.
- Full desired state is an output projection, not a restorable core snapshot. Shares, MRU, minimized neighborhoods and mapping history cannot be recovered from it. Export/import and restart persistence remain separate work.
- Current core needs no time argument. If future shared deadlines require one, pass explicit monotonic logical time; keep native timers outside core. Do not add unused time or persistence machinery here.

## World report and reply contract

### Shape

```text
Report {
  contract_version, adapter_incarnation, core_generation,
  observation_seq, request_seq, request_id, command_seq?, command?,
  observation: {
    outputs, workspace_mode, ordered_workspace_inventory,
    logical_to_native_bindings, visible_workspaces, focused_window,
    domains: [complete members OR explicit unreadable/hold],
    live_window_inventory: [lifetime, membership, frame, flags, constraints],
    inventory_completeness, closed_lifetimes, gesture_barriers
  },
  apply_feedback: [property/creation/effect result with stable identity]
}
Reply {
  contract_version, adapter_incarnation, core_generation, request_id,
  based_on_observation_seq, state_revision, ready, command_result,
  desired: {
    scope_status: [scope, controlled | untouched | hold, reason?],
    workspaces: [logical_id, native_binding?, scope, order, tiling_mode,
                 existence, protection/hold],
    outputs: [output_id, logical_view, authority/hold],
    windows: [lifetime, domain, allocation?, effective_frame?,
              per_property_authority, hold_reason?],
    presentation?: {version, ...capability-negotiated full sections}
  },
  effects: [id, kind, target_lifetime, preconditions, validity]
}
```

- This is a full snapshot of the controlled world, including hidden and retained domains, every controlled workspace, and explicit untouched/hold records for observed floating/excluded/game windows. It is not the entire desktop's geometry under tiler ownership. Unknown/read-failed scopes are explicitly held, never represented as empty.
- Every known scope has an explicit status. Object-list omission means desired absence only inside a completely enumerated, controlled inventory and subject to property authority/removal gates; omission inside a held or untouched scope authorizes no write or retirement. A hold is never encoded by an empty list alone.
- No tree, shares, core focus history or serialized Engine belongs in this reply. Flattened allocations/effective targets suffice for host application; future presentation data can supply its own derived geometry.
- `existence` is desired presence, not an imperative append/remove list. Desired absence applies only to explicitly managed inventory, never arbitrary host objects. Logical creation identities survive repeated snapshots until binding or cancellation; uncreated logical IDs cannot be used as native IDs.
- D3 selects retained view/membership targets, distinct from one-shot effects. Observed active focus is informational state, never a command to focus.
- Optional presentation sections are independent, versioned and capability-negotiated. For a negotiated full section, empty means clear and omitted means unsupported/not controlled, never "reuse the previous geometry". Unknown required versions refuse before actuation. Initial migration reserves this envelope only; it does not implement border/preview/underlay policy moves.

### Ordering and freshness

1. The backend wrapper owns `core_generation`, a fresh opaque incarnation on process start, Engine replacement/poison recovery or world reset. The adapter owns a separate activation/lifetime namespace. Bind both through authenticated discovery/bootstrap; a delayed arbitrary reply cannot reset generation ordering.
2. The adapter advances `observation_seq` synchronously when relevant native input invalidates an observation, INCLUDING during an outstanding request and before debounce/dispatch. Capture a coherent full report under that sequence; if capture races another invalidation, recapture or mark affected scopes unreadable. Merely numbering requests is insufficient.
3. Every newly accepted report publishes one globally monotonic `state_revision`, even for a semantic no-op. Keep internal per-domain revisions/node identities independent. Bind monotonic request/command sequences to the adapter incarnation and core generation. Retain only the latest request's digest and bounded encoded reply plus request/command high-watermarks: an identical latest duplicate returns that reply WITHOUT a new revision; reused sequence with different input is invalid; older requests get a bounded superseded error, never execution. A fresh report carrying an already consumed command sequence may update observation but reports the command as consumed, never executing it again. No unbounded request/outcome ledger is needed.
4. Apply only a ready reply bound to the current authenticated backend/adapter generation, newer than the latest accepted snapshot revision, whose `based_on_observation_seq` equals the adapter's CURRENT local watermark, not merely its latest dispatched request. Otherwise do no setters and schedule a fresh observation-only catch-up.
5. Repeat freshness/lifetime/safety checks immediately before native setters; a gesture barrier suspends geometry effects even if its final geometry has not yet been reported. Mid-apply invalidation stops remaining unsafe writes. Report partial results; never claim the multi-object apply was atomic.
6. Keep desired-revision acceptance separate from verified per-property host results. Failed partial actuation must not be marked converged. Latest observed host state, not the previous desired/applied cache, is the diff baseline.
7. Encode global counters as bounded decimal `u64` strings on JSON to avoid JavaScript precision loss; compare numerically, not lexicographically. Do not reuse today's one-million domain revision cap for world ordering (`crates/tiler-core/src/contract.rs:43-52`). Before counter exhaustion establish a new bootstrap generation, never wrap in place.
8. Keep service single-flight. Dirty observation work coalesces to the newest full report; explicit user commands are distinct and never silently coalesced/replayed. Busy/invalid/unaccepted input is a bounded error, NOT a ready full-state reply and NOT an acknowledgement that its report was incorporated.

### Bootstrap and recovery

- Discovery returns generation/capabilities with `ready=false`, no applicable desired state. The adapter sends a full topology, workspace mapping, inventory, settings and observation before any ready reply. Empty pre-bootstrap state is never a removal request.
- Readable sections may become controlled while unreadable sections remain explicit holds. Workspace cleanup requires complete occupancy inventory for its scope. Restart may fresh-adopt topology using current policy; it must not claim shares/history survived.
- Generation changes cancel old effects, flights, ordering and desired caches, but not native windows. Re-enumerate and reconstruct native bindings; do not replay old focus/send/raise requests or old logical creation tokens across generations.
- Poison recovery must perform this same generation handshake rather than silently process a partial domain request into a fresh empty engine.

## Gap resolutions and application rules

| Gap | Proposed resolution |
| --- | --- |
| Newer reply based on older input | Global revision handles reply order; echoed observation watermark plus immediate invalidation/gesture fences handle input order. Both are required. Stale results cause observation-only catch-up, not unconditional command replay. |
| Lost reports | Full normalized observation on EVERY report repairs missing earlier membership/topology reports. Keep a dirty watermark until incorporation is acknowledged; after timeout/busy/transport loss, send a fresh observation-only report when transport recovers, not the old command. Carry explicit closes/attempt feedback until acknowledged. Full lifetime inventory must include minimized/hidden windows: omission from a domain is not a close (D23). Only a provably complete live-lifetime inventory can establish missing=closed; otherwise preserve explicit-close obligations. Silent native notification loss with no subsequent trigger requires the audit choice D5. |
| Focus, raise, stacking | Emit command-scoped effect IDs, separate from persistent state. Adapter marks an effect spent BEFORE its first possible native dispatch, then records observed outcome; duplicates and repeated state diffs do not reapply it. Lost/stale effects are not exactly-once delivery and may be dropped; newer relevant user input invalidates them. Native focus dispatch is not observed focus. Existing proof-gated follow stays switch-before-focus. Stacking/raise actions use the same effect contract, not a permanent total ordering to restore every update. |
| Host refusal, rounding, churn | Diff effective targets in platform-normalized coordinates. Feed observed frame, known min/max hints, attempted target and result back. Preserve existing minimum-aware projection, overconstrained no-reassert and client-clamp acceptance. KDE keeps its three bounded attempts then accepted exact host frame; no blanket epsilon and no per-reply attempt-counter reset. Keep canonical allocation separate from accepted effective frame. Suppression is per lifetime/property/target/constraint context and reopens only on meaningful constraint/topology/target change or a fresh explicit command; normal repeated replies never rearm it. Narrow failures hold that property/scope, not the whole system. |
| Floating/excluded/game windows | Explicit per-property authority: ordinary floats preserve frames; excluded/game rows have no automatic frame, focus, stacking, overlay or membership authority. Reserved tiled fullscreen/maximized slots remain allocations without fresh geometry writes. Explicit already-supported floating sends may authorize membership without authorizing frame changes (D16), so one blanket `managed` boolean is insufficient. Re-check native safety before effects as well as diffs. |
| Workspace removal | Desired absence is only eligibility, not permission to delete blindly. Immediately re-enumerate native occupants, including unmanaged/floating/minimized/fullscreen/maximized windows, and re-check visibility, pins, displacement, mappings and minimum count. Unknown occupancy means keep. Sticky all-desktop windows do not occupy every backing workspace. Remove only observed-empty eligible managed workspaces; otherwise report blocked and recompute. Runtime managed/adopted status is distinct from lifetime-owned teardown disposal. |
| Repeated workspace creation | Core chooses a stable logical workspace ID before native creation. Adapter holds at most one in-flight creation for that ID, records the native binding as soon as observed, and reports it in the next inventory. An ambiguous create result requires enumeration/binding recovery before another attempt; never append blindly on every identical reply. Unknown binding holds dependent membership writes. Confirmed failure follows narrow suppression/recovery, not repeated creation. |
| Generation bootstrap | Use the explicit not-ready/full-observation handshake above for service restart AND in-process engine replacement. No uninitialized empty desired list, stale generation reply, or unobserved native absence can trigger cleanup. |
| Full-state byte bound | Replace the 64 KiB cap, not the full-state requirement. D4 proposes 4 MiB request AND reply as explicit application limits. Use compact flattened targets and shared identifiers where useful; no topology blob. Validate input and a conservative worst-case full-reply budget BEFORE any observation convergence, slot cleanup or command mutation, not merely before command commit. If exact serialization can still fail, stage affected changes and publish/commit only after the entire bounded envelope encodes; overflow discards that candidate. Do not require an unconditional whole-world clone. An overflow error contains no applicable state; never truncate or apply partial pages. Boundary fixtures must prove estimator safety including pending creates, long IDs, numeric extrema and negotiated presentation sections. |

### What full-state delivery can and cannot repair

- A lost geometry/lifecycle snapshot is repaired by diffing the next complete desired snapshot, subject to current input/safety/refusal gates. A lost observation is repaired only after a later full observation actually reaches the core.
- Focus/raise/stacking are deliberately at-most-once attempts, not persistent conditions. Their loss is not repaired by stealing focus on the next unrelated report.
- Window transfer/view selection requires retained intent, not merely wrapping an operation-specific result: D3 keeps an unattempted desired target across the next full observation, while explicit attempt/result feedback holds possibly-executed transfers without blind replay.

## Selected decisions (2026-10-16)

- D1=A: retain the stateful Engine, with explicit deterministic replay tests; no caller-owned or serialized core state. A pure typed transition remains optional only for a demonstrated need.
- D2=A: Planner alone owns KDE workspace policy. This supersedes AR6's synchronous no-Planner policy behavior where necessary; native observation and safety remain independent, with no TypeScript fallback policy. Measure offline response/catch-up latency; live acceptance is separate.
- D3=A: retain transfer/view targets until confirmation, newer external user input or native refusal. Stable operation identities and explicit attempt/result feedback fence ambiguous outcomes; never repeat a possibly-executed transfer without fresh unambiguous proof. Focus/raise/stacking remain one-shot.
- D4=A: one bounded 4 MiB request/reply budget, with actual size/latency/memory measurements and narrow recoverable overflow.
- D5=A: event-driven full reports, dirty/ack recovery and anomaly rescans; no idle polling. A missed final native event remains undetectable until another wake.
- Capacity B: preserve existing topology and publish a narrow, recoverable constraint hold with complete held lifetime rows when projection runs out of space. Keep the hold policy in one replaceable location. Admission fallback is explicitly separate work under "Best-effort admission when space runs out"; reference research is R-MIN-04/R-INS-09.

| ID | Options and consequences | Recommendation |
| --- | --- | --- |
| D1: state ownership | A retained Engine; B opaque typed caller-owned transition with KDE service retaining the value; C actual client-carried serialized state over KDE D-Bus. B keeps policy portable but expands this migration; C adds round-trip state replacement/versioning without automatic durability. | A now; keep replay tests explicit. B only for a demonstrated need. Do not select C. |
| D2: KDE synchronous workspace operations | A Planner is the only workspace-policy authority, accepting a round trip and narrow unavailability for policy-changing selection/create/hotplug; native observation/safety and already-observed views still function. B retain synchronous TS policy/mirror or introduce an in-process native bridge. B preserves existing dependency/latency behavior but violates the single-authority goal until a bridge exists and substantially expands scope. | A for a complete core+KDE migration, explicitly superseding AR6's no-Planner behavior where necessary. Measure response/catch-up latency before acceptance; never silently invent a TS fallback. If that tradeoff is rejected, stop interface integration and redesign the synchronous bridge separately. |
| D3: transfer/view ownership | A desired membership/view remains an explicit outstanding core target until confirmed, superseded by newer external user input, or settled by native refusal feedback; lost replies can therefore be repaired. Reports must distinguish observation, explicit external membership/view change, and actuation result/unknown, without guessing that every mismatch is user intent. B preserve strictly one-shot native transfer/switch and observation-wins semantics; least churn, but those lost state updates are NOT repaired by the next snapshot. Unlimited retry is neither option. | A, with stable operation identity, explicit attempt/result feedback, safe hold on ambiguous causality, and no repeat of a possibly executed transfer without fresh unambiguous proof. Confirmed refusal settles the target to observed/held state; newer external input wins. This changes existing transfer settlement/no-replay boundaries and needs explicit selection. Keep focus/raise one-shot in either option. |
| D4: transport budget | A fixed bounded 4 MiB global request/reply; B retain 64 KiB and add atomic chunk assembly with total bound, manifest/revision/checksum, timeout and no apply before complete; C an unbounded reply. B solves transport size but adds protocol/memory complexity and still needs a total bound; C is unacceptable. No candidate guarantees arbitrary window counts. | A. Validate actual bytes for hundreds/thousands of windows and measure latency/memory; 4 MiB is a proposed engineering budget, not a capacity/performance claim. Keep overflow narrow and recoverable after scope/size changes. |
| D5: silent missed host notifications | A event-driven full reports plus dirty/ack recovery and anomaly rescans; B add a low-frequency complete audit while enabled. A preserves current no-idle-polling behavior but cannot detect a missed final native event without another wake. B detects that class but adds recurring enumeration/IPC cost, particularly with AX. | A initially, explicitly retaining that limit; select B only if eventual detection without subsequent input is required, with interval and platform cost measured separately. |

- These selected choices supersede conflicting earlier decisions, including the no-world-sequence restriction, without merging independent per-domain session revisions.
- Minimal action-sequence discriminators are added to `docs/spec/reference-outcomes.md`; unsupported native/reference outcomes stay TBD. Existing R-WS-10 already covers ordinary empty-middle cleanup and is not duplicated.

## Migration plan and dependencies

1. Select D1-D5, especially the Planner dependency and transfer settlement boundary. Confirm KDE/macOS integration implications against their current platform work before code changes. Promote only selected choices to `docs/decisions.md`.
2. Lock the report/desired/effect fixture contract: all three workspace modes, global hidden/foreground inventory, lifetime IDs, unreadable holds, full output shape, freshness/ordering, refusal, creation binding and exact size bounds. Define which existing operation results map to persistent targets versus effects. Do not make production depend on an unaccepted fixture oracle.
3. Core + protocol: world observation/workspace state and deterministic report transition; reuse `plan_trailing`, `resolve_trailing`, scoped rings/history and existing independent sessions. Compute workspace lifecycle on every accepted report, not per-event adapter callbacks. Add generation/readiness, global revision, input watermark, bounded snapshot export and explicit feedback/effect results. Keep diagnostics bounded and redacted; no payload/identity logging to prove ordering.
4. KDE in the SAME interface change: update service generation/bootstrap and all byte-limit mirrors, codec/goldens, adapter full observation capture, dirty catch-up, workspace logical/native bindings, lifecycle actuator and per-property diff. Route affected selection/send/hotplug paths consistently under D2/D3, remove duplicate TS lifecycle decisions only after fixture parity, preserve post-setter proof/safety gates, cancellation and no false success. Native effect remains renderer; future presentation sections remain optional/unimplemented.
5. Keep repository builds coherent: preferably introduce the world-update boundary alongside the internal/domain `Engine::handle` while core+KDE adopt it; keep the latter explicitly transitional for native callers, not a second KDE policy path. If it must be removed, make coordinated mechanical compile fixes before landing, without claiming native behavioral adoption. No knowingly broken native stream or broad unrelated compatibility layer. Remove transitional entry points after both follow-ups.
6. Separate Windows/macOS behavioral adoption, then remove obsolete contracts. Breaking changes pre-0.1 are acceptable, but do not let a compile shim masquerade as migrated behavior.

### Exact proposed follow-up backlog items

- **P1 | Windows: adopt full world-state core replies.** Replace adapter-triggered trailing policy and per-op application with complete observed world reports and host diffs; move scope/history/mapping policy into the world state; preserve recovery ledger/window properties/native safety and D16/D23/overlay semantics. Adapt `TileLoop`, `tiling_sys/admission.rs`, `tiling_sys/scope_trailing.rs` and workspace owner/effect/send paths; remove direct session-state policy queries where the reply replaces them. Cover bootstrap/order, stable creation identities, lifetime/gesture fences, refused targets, partial transfer and follow proof in all workspace modes. Native acceptance remains separately required.
- **P1 | macOS: adopt full world-state core replies.** Replace per-domain reply concatenation/cursors with world revision and input watermark, preserving AppKit/engine/AX thread ownership, per-app generations, latest-wins frame slots, observed-focus authority and native Space capability refusals. Adapt `runtime_core.rs`, `runtime_native.rs`, `runtime_thread.rs`; include full lifetime/completeness/hold facts and per-property feedback. Represent observed Spaces without inventing Windows/KDE logical creation or ordinary-window transfer capability. Native Space operations remain capability-gated until separately established.
- These follow-ups are recorded in `docs/backlog.md`. Neither native platform needs a caller-owned-state migration under D1.

## Verification and outcome

### Delivered implementation

- Code commits: `8e29a0b4` (core/protocol/service and shared wire fixtures), `76c4185f` (KDE observer/actuator migration and portable-check regression gate).
- Core: `WorldEngine` wraps the retained `Engine`; `world_report`, `world_sizing`, `world_publish`, `world_commands`, `world_sends`, `world_tiling_views` and `world_displacement` separate the contract, bounded encoding, projection and workspace policy. Cheap validation/budget gates precede a staged candidate; exact bounded encoding precedes commit. Candidate cloning is conditional on accepted preflight, not performed on rejected input. Native callers retain the transitional domain API; no native-platform sources changed.
- Protocol/service: `tiler-protocol::world_protocol` and `omnitiler::world_service`; authenticated current-KWin-owner `DiscoverWorld`/`DescribeWorld` routes, fresh generations on construction/poison replacement, single-flight admission, bounded redacted ingress/egress summaries. Request counters are decimal strings. Latest duplicate identity uses full typed equality plus a digest pre-check; command consumption uses a high-watermark, not an evictable outcome ledger.
- KDE: production `entry.ts` uses `world-entry`; observation, sequencing, actuation, drag, native toggles and shortcuts have separate modules. Complete reports carry minimized lifetimes, fixed predicate, default tiling, focused window, output geometry/primary facts and bindings. Planner owns all three workspace modes, tiling defaults/overrides, scoped selection/history, lifecycle, sends, migration and displacement/return. Legacy policy modules remain available to their existing offline tests, not as a production fallback.
- Retained explicit float placement has a stable per-property token and repairs a dropped result without rewriting ordinary floats. Transfers/views and policy hotplug targets use unique per-intent identities; possibly-executed ambiguity holds, native read-back proves success, newer explicit input settles. Exact per-output view proof gates follow/focus. Operation results preserve core-derived preview/hover, float placement, group rendering and migration data; the effect remains renderer-only.
- Capacity B is localized in `classify_sweep_hold`: 13 fresh windows in an 800x600 gap-8 domain project; 14 yield complete `constraint-clamp` held rows. Repeated held snapshots do not churn; fitting observations recover. No flattened layout, partial-report workaround or admission fallback was added.

### Accepted offline evidence (2026-10-17)

- At committed revision `76c4185f`, `cargo fmt --all -- --check`, workspace check/build/test and workspace clippy with warnings denied pass. Workspace tests: **2738 passed, 0 failed across 80 suites**. The upstream macOS-only dead-code lint issue was corrected separately before integration; no native-platform source was changed by this work. Workspace results include Linux compilation/testing of the untouched Windows/macOS crates, not their supported native targets.
- KWin typecheck/build/full tests pass: **1703 tests, 248 suites**. `just check-portable` and scanner self-tests pass (4 cases). The scanner now ignores comment-only documentation mentions while still rejecting platform code and scan errors; no dependencies were added.
- Shared fixtures contain exact bytes captured from production TypeScript reports; Rust decodes those bytes and regenerates result goldens consumed by the TypeScript parser/actuator. Coverage includes ordering/correlation, replay/dedup, bootstrap/poison, incomplete and minimized inventories, lost float/transfer replies, refusal and ambiguous attempts, creation binding, occupied removal, all workspace modes, floating boundaries, born-fullscreen slots, defaults, drag preview/drop arrival, migration, hotplug return and unique policy identities.
- Independent review accepted the final three corrections: unique displacement/return identities, retaining third-frame float ambiguity, and updated strict service fixtures. A measured stale-flight wedge was reproduced then repaired: rejecting a consumed stale reply releases only that flight and dispatches observation-only catch-up. No known core/KDE offline acceptance gap remains. Final KDE responsibility splits keep every new production module below the source-size soft cap without changing fixture bytes or behavior.

### Final size and cost measurements

Release codec/core samples use fresh Engines (11 repetitions); sizes are exact compact UTF-8 bytes. Held workloads are not layout-success evidence.

| Workload | Request / reply bytes | Result | Total median / p95 |
| --- | --- | --- | --- |
| 800 windows, 64 domains | 226911 / 178690 | 64 controlled scopes, 800 usable allocations | 97.36 / 103.07 ms |
| 2000 windows, 128-byte IDs, 64 domains | 759891 / 634214 | 64 constraint-held scopes, 2000 complete held rows, zero usable allocations | 100.07 / 104.21 ms |

- 800-window decode/core/encode medians: 2.45/94.35/0.33 ms; held-2000: 6.14/93.77/0.62 ms. Peak process resident high-water was approximately 50 MiB, measured through process status; these are spot offline costs, not live responsiveness acceptance.
- Actual TypeScript entry construction medians: approximately 0.14 ms for two windows and 1.07 ms for 200; golden reply parse 0.018 ms; initial one-row apply 0.002 ms. Mocked-transport catch-up recovered 30/30 stale flights, with no command replay: approximately 0.010 ms handling plus the configured 120 ms debounce plus 0.058 ms report construction. A subsequent explicit command dispatches after acknowledgement. This is mock-time scheduling evidence, not D-Bus/native latency.
- A legal-count 5000-window report padded to exactly 4194304 bytes is accepted; one additional byte refuses before mutation, and the previous request still duplicates at the same revision. An 11188-window payload below the byte cap instead refuses the separate count safety gate; raw byte fit is not semantic acceptance. Reply estimators cover exact encoding, including maximum identifiers and operation results.
- Read-only session-daemon configuration shows a 1000000000-byte message maximum, above the application budget plus framing. Live Qt/KWin delivery and D-Bus end-to-end timing were not tested. No caller-owned-state alternative benchmark is needed under D1.

### Live rejection regression correction (2026-10-17)

- Live diagnostic summaries showed successful discovery followed by repeated observation-only `snapshot-invalid` refusals; explicit workspace actions never reached normal dispatch. Those summaries contain no window payload, so they do not identify individual reported hints.
- Source reproduction found an ingress bug, not a contract redesign: world validation applied the carried-geometry bound and positivity/order requirements to advisory size hints. A normal Qt `QSize` zero minimum or unbounded maximum sentinel therefore invalidated the entire report, contradicting the existing sanitize-at-use policy in `size_hints.rs`. Removed only that hint gate; JSON integer typing, geometry, identity, inventory and safety validation remain enforced. Fixed-vocabulary rejection reasons now reach bounded `msg=` diagnostics without payload logging.
- Earlier offline fixtures used absent or small positive hints. New exact-byte fixtures come from the actual TypeScript producer with Qt zero/default/unbounded hints, partial-zero fixed axes and valid fixed pins; Rust decodes and accepts the observation and a subsequent explicit selection. The regression failed before the fix. Tests also retain malformed-number refusal, advisory sanitization and safe diagnostic redaction. After integrating concurrent changes, workspace tests: **2797 passed**; KWin: **1706 passed**, with typecheck, formatting, portable check and workspace clippy clean.
- User re-check: `just reload` rebuilds/replaces only the worktree Planner and activates the validation fix; generation rediscovery is automatic and old commands do not replay. To load the script-side `msg=` diagnostic as well, use the documented `just dev-off` then `just dev` (foreground), or `just dev-on` (detached). Confirm ready/applied reports, then retry selection, focus/move/resize and ordinary window lifecycle changes; persistent rejection should now identify its fixed reason. No live run was performed for this correction; user confirmation remains required.

### Live adapter no-op and directional refusal correction (2026-10-17)

- The subsequent log showed accepted full reports with advancing revisions, not the earlier hint refusal. Fresh receipt-bound Planner/controller rebuilds confirmed the hint fix was present. Executable and generated-bundle provenance were checked; the public KWin API cannot retrospectively attest an earlier loaded script's evaluated source.
- Live host collections are QML array-likes. Observation decoded them, but native liveness, membership and desktop/output resolution used `Array.isArray` and reference equality, so application stopped at the first window and skipped views/effects despite accepting the reply. Native braced UUIDs also failed raw-ID shortcut lookup. Reused bounded `decodeList` and canonical `normalizeNativeId`, with stable-ID liveness and active-window proof across renewed wrappers.
- Single-output focus/move additionally requested cross-output handling without an adjacent output, producing `operation-refused`. Commands now request crossing only for the observed direction's actual adjacency; edgeless directions stay local, and existing multi-output routing is retained. These are adapter bugs, not a full-state contract redesign.
- Close handling now caches canonical identities while windows are live and promotes missing identities only after a real removal event plus a complete native inventory. It never reads a potentially dead removal payload, never confuses minimized/hidden membership with closure, and retains close obligations across unreadable inventories until acknowledgement.
- Offline regressions use wholly synthetic QML-like collections, renewed wrappers, braced fake UUIDs and generic geometry. They exercise production actuation, entry/shortcut routing, explicit-close acknowledgement and unreadable-inventory recovery. Earlier fakes used real JavaScript arrays, stable wrapper references and bare IDs, so they missed these host-boundary failures. No raw live payload, desktop/window identity or machine information is checked in.
- Final integrated gates pass after concurrent changes: **2802 Rust workspace tests**, **1716 KWin tests across 252 suites**, KWin typecheck/build, Rust formatting, workspace clippy with warnings denied and the portable check. Independent source review accepted the lifetime-safe close correction and retained actuation fences.
- Live callback-path verification applied workspace selection, focus left/right, movement, resize, owned normal-window admission and close/reflow, with authoritative state read-back and zero rejected/refused actions. A final one-client open/close run verified the explicit bare close identity in the outgoing full report, accepted acknowledgement, survivor reflow and zero held rows. Synthetic shortcut invocation proves callbacks, not physical key delivery.
- Initial activation retiled existing windows without a recorded placement preimage; no exact-frame restoration claim is made for that activation. Empty-workspace cleanup reduced the inventory to two. The final verification preserved that two-workspace baseline, both existing window identities, active window, pointer, outputs, configuration and shortcut state; all owned clients and helper scripts were removed, and development returned DOWN. Existing windows were never closed.
- This adapter correction requires rebuilding/reloading the KWin bundle, not only `just reload`: from DOWN use `just dev` or `just dev-on`; from an owned active development session, use the documented teardown/start cycle.

### Remaining work and user-owned live checks

- Beyond the bounded ordinary-action checks above, follow the live-testing guide before further native runs. Remaining acceptance includes authenticated restart/unavailable-Planner behavior; exact 4 MiB Qt/KWin delivery and latency; two-output selection/history/global swaps/hotplug return; follow/stay/append and floating-boundary sends; migration partial refusal; continuous preview/drop/resize and late-reply cancellation; float/sticky/maximize/fullscreen/minimized/game safety; workspace creation/removal races and bounded client-clamp behavior.
- Windows/macOS remain on their compiling legacy domain API. Their full-world behavioral migrations and supported-platform offline/native gates are separate backlog items. Admission fallback remains the separate best-effort-admission item. A missed final host notification still requires a later trigger under D5.
- Exact next action: user-owned live KDE acceptance, then the separately scoped native migrations and admission fallback. Durable decisions are promoted to `docs/decisions.md`; backlog records distinguish offline delivery from those remaining checks.

### Bounded native acceptance continuation, 2026-10-18

- Callback/state samples passed ordinary admission/focus/move/resize,
  tiling-mode toggles, float/sticky/fullscreen/minimize safety, maximized-slot
  enable/restore, Planner-only restart and unavailability, generation rediscovery
  without replay, and close/reflow. The accepted desktop/configuration baseline
  was restored; these samples do not complete the full matrix or physical input.
- Maximized intentional unfloat exposed an asynchronous-readback regression,
  corrected and natively reverified at `fe3cc3a9`. A suspected post-restart float
  no-op was instead the fixture's saved floating default reseeding workspace
  mode after Planner replacement; no admission/capacity defect was established.
- [Current evidence, correction and untested legs](../kde-live-acceptance.md).
- Actual Qt/KWin transport accepted an exactly 4194304-byte synthetic request
  and delivered its 1608803-byte reply in a 904 ms spot sample; duplicate reply
  bytes/revision were stable. One extra request byte was refused before decode;
  the independent count cap also refused its below-byte-cap sample correctly.
  Padding, timings, isolation and exact-reply-boundary limitations are retained
  in the continuation record. Exact-4-MiB reply delivery remains pending.

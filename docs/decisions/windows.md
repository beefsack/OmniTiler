# Windows

### State-centric v4 visibility
- Decision: visibility is per-window desired state (`shown` plus authority) and truthful per-window facts with generic last-write fact. Core owns policy/composition, switch targets, verdicts and completion; correlation only for focus grants. Normative apply is arrive, focus, depart, at most one write per window lifetime/revision/target and no automatic retry. Across outputs, hides wait for outstanding arrival-ready focus from same/earlier revision. v4 carries property-tagged last-write; frame/member attempt lanes migrate separately.
- Why: make core/platform responsibilities intentional and noncompeting.
- Rejected: 2026-10-28 platform-reconciles/model A and 2026-10-29 core-minted work/group IDs and typed lifecycle rows; later focus-failure outcome and other bump-shape decisions stand.
- Assumes / revisit if: follow the [coordinated v4 authority and migration decision](contract.md#contract-authority-and-v4-migration); accepted older protocol details that were superseded do not revive.
- Status: active, 2026-10-30
- Evidence: [direction](../changes/windows-workspace-return-focus.md#state-centric-visibility-direction-2026-10-30), [review](../changes/core-interface-review.md).

### Record model
- Decision: register entries use topic headings, no dated sections/indexes; each has Decision, Why, Assumes/revisit if, Status (`active`/`tentative`, date), optional Rejected and link-only Evidence, about 3-8 lines. Replacements edit in place and name replaced choice under Rejected. Principles capture trade-off; mechanisms belong contract/spec/decisions. Adapter guide becomes normative contract, register current-state one-entry-per-topic. Principles format is Intent, Rule, Yields to (`absolute` when applicable), optional Example.
- Why: keep project records separated by purpose and current register current.
- Assumes / revisit if: migration sequence was principles, stale decision entries, register restructure, contract, backlog trim; pending migration steps are not new product behavior.
- Status: active, 2026-10-30
- Evidence: [review M1-M7](../changes/core-interface-review.md).

### Switch and intent decisions
- Decision: staged switch is a narrow prototype: batch nonactivating arrivals, prove visible, one-shot focus, prove foreground, then hide departures. Arrival-ready authority applies only explicit workspace selection; existing game/fullscreen/unreadable foreground/exact-desktop/final full-view proofs remain, no general bypass. Measure end-to-end and phase latency; regression fails prototype. Failed/unproven staged focus finishes explicitly failed/unproven; account each visibility result, no departure hide under staged authority without foreground proof; only approved fallback hide under fresh gates, else leave visible/blocked. No retry/wait.
- Why: not recorded.
- Assumes / revisit if: R2 authority is only for a still-current explicit intent positively proved never claimed/primed/dispatched; retire old id atomically, at most one dispatch, remint/recheck selection-origin/exact-desktop proof. Use only if visible-proof wait crosses revisions; stop if hard to prove, broader changes needed, or side effects. Unknown-foreground A/B remains deferred pending prototype and physical Alt-Tab SWITCHEND capture.
- Status: active, 2026-10-28
- Evidence: [switch design](../changes/windows-workspace-return-focus.md).

### Windows decision set
- Decision: sticky-off rehomes via core-owned intent resolving output current workspace at execution; explicit `Send` unchanged and KDE/macOS should use same pattern. Synthetic shortcuts for tests require opt-in startup flag and fixture scope; never target non-fixture/agent terminal. Tabbed stacks alone are major Tier 1 feature deferred from 0.1; minimize post-0.1 protocol churn.
- Why: not recorded.
- Assumes / revisit if: compare KDE/macOS sticky path; physical burst baseline follows staged switch. Batch-local selection coalescing dispatches all eligible numbered/trailing/previous/relative commands in order and renders survivor under fresh gates; sends flush selection; superseded catch-up core-only before one dispatch, no render/retry; fail-closed identity binding. Other intake remains per-command.
- Status: active, 2026-10-28
- Evidence: [output targeting](../changes/archive/output-targeting-race-audit.md#parked-boundary), [coalescing](../changes/windows-workspace-return-focus.md#windows-batch-local-coalescing-outcome-2026-10-28).

### Windows 0.1 release gate
- Decision: accept gate D1-D4 exactly as recorded. Retain all 43 original M checks and A/H/X equivalents; exclude only 5/35/38/40/53. Promote W3/W4 to must-pass; applicable K24/26/29/36/37/41/52 remain known-issue-allowed only as listed, without crash or silent tiling stop. Effective base is 45 M / 7 K / 5 excluded. N1 covers optional visual disable/unavailability and carrier-creation failure with tiling continuing; owner/thread loss remains 28/30/50/WX7. WX1-WX8 are must-pass; only Authentic Win+G/Win+F11 exact-chord containment is a listed limitation. Native/physical acceptance and delivery dependencies remain pending; this approval is not release acceptance.
- Why: not recorded.
- Assumes / revisit if: gate proposal dependencies remain outstanding; no approval here asserts release acceptance.
- Status: active, 2026-10-19
- Evidence: [approved gate](../changes/windows-0.1-gate-proposal.md#user-decisions).

### Accepted protocol and phase-one fences
- Decision: the coordinated v4 decision governs the exact cutover and prohibits a shim/dual decoder. Independent per-window suppression capability remains required. Core-minted work/group IDs, typed lifecycle rows and native known-state flags were later superseded by state-centric visibility; names may simplify before freeze. Phase 1 reuses existing completeness/unknown holds without protocol flags; inventory readability gates host-owned domains only, preserving native-free birth/close; test oracle uses commanded view during switch. Alt-Tab SWITCH capture is diagnostic-only, off by default, enabled only by existing `--trace`; never feeds focus, memory or attribution.
- Why: not recorded.
- Assumes / revisit if: do not revive superseded v4 shapes; capture remains non-authoritative.
- Status: active, 2026-10-29
- Evidence: [switch record](../changes/windows-workspace-return-focus.md), [trace capture](../changes/windows-workspace-return-focus.md#trace-only-switch-capture-2026-10-29).

### Pager diagnostics and CLI
- Decision: pager drains at most 128 ordered native events with overflow counts and opaque lifecycle tokens; tick/request/revision identify drain context, not causal attribution. Exact-owner automation CLI forms are test controls, not general end-user CLI; Windows verbs move to shared experimental command names.
- Why: not recorded.
- Assumes / revisit if: stale pager source layout after send-follow reopens only if staleness recurs; prior issue fixed, no new policy.
- Status: active, 2026-10-28
- Evidence: [pager decisions](../changes/windows-pager-workspace-flicker.md#decisions), [CLI record](../changes/archive/windows-workspace-send-follow-stay.md).

### Windows owner-thread architecture
- Decision: selected dedicated input and render pumps for latency, stage 1 complete input pump then stage 2 renderer. Input hook and synchronous classifiers share one dedicated input pump; owner retains observation, Engine, foreign/foreground actuation, rendering/services, Mouse Snap SPI and recovery. Immutable whole snapshots cross bounded ordered handoff; callbacks never wait for owner/locks. Publication/application atomic fence refuses fresh stale-policy consumption but retains press verdicts, consumed pairs/repeats and Win-up obligations; one retry generation. Fixed safety slots preserve clear/invalidation under saturation; overflow clears origins/armed windrag while preserving swallowed Up/Cancel, then forces fresh snapshot. Tagged bounded batches preserve classifier queue/terminal guarantees; atomic Esc sequences preserve START/END order; logs on owner.
- Why: latency; no Win+G/Win+F11 containment claim.
- Assumes / revisit if: creation failure refuses pump before hooks; creator-thread install/retry/unhook/readiness/bounded-stop. Missing/unclean stop retains process reservation, forbids overlapping replacement/new recovery authority.
- Status: active, 2026-10-12
- Evidence: [stage 1](../changes/archive/windows-input-thread.md), [wake correction](../changes/archive/windows-input-owner-wake.md), [thread contracts](../research/windows-port/owner-threading.md).

### Renderer and completion
- Decision: dedicated renderer creates/pumps/paints/places/destroys border, underlay and preview carriers; owner retains observation, Engine, actuation, services/recovery; input classifiers stay on input pump. Three bounded latest-wins frame slots carry geometry/lifetime; safety watermarks invalidate old Shows. Renderer rechecks fresh identity/foreground/visibility/band/fullscreen and reports applied pixels/proof or unknown, never desired-as-rendered. Immutable versioned overlay-ID snapshots exclude carriers from observation/WinEvents; tagged identities prove lifetime, untagged targets retain process-fenced unproven race.
- Why: not recorded.
- Assumes / revisit if: owned events/handles across threads; ack-only owner turns service outcomes without frame repost. Readiness requires hidden carrier; clean stop requires creator-side destruction and exited thread; missing completion blocks replacement. Finish requires creator-verified hide before fresh owner revalidation/actuation. Hide proof timeout at 500 ms refuses finish without drop/snap-back writes; overlapping pending finish narrowly refuses.
- Status: active, 2026-10-13
- Evidence: [render thread](../changes/archive/windows-render-thread.md), [Stage 3 acceptance](../changes/archive/windows-owner-threading-acceptance.md).

### Windows idle and diagnostics
- Decision: input/render waits event/deadline-driven, no fixed exchange quantum; healthy owner blocks on messages, input/render signals, request/settings changes and async completion, with explicit operation deadlines. Modifier-only and saturated recovery have explicit wakes; hide-before-drop retains 500 ms deadline. Request/stop publication atomic; diagnostics add no deadline/reconcile. Preserve visible-pager 2 s topmost-recovery timer for 0.1; native-event replacement only after complete coverage proof. Clean stop requires hook release and finished thread join; cached terminal wait result; retain reservation on missing/unclean completion. Render stop signals its work event. Routine metrics bounded/aggregated on owner existing turns; safety/proof/visibility transitions and failures remain immediate evidence.
  Accepted keyboard/mouse intent batches signal one auto-reset owner event;
  outer owner wait watches it alongside messages while retaining periodic
  fallback. Diagnostics alone do not wake owner. Focus-settle/proof waits
  remain message-only; pending intents do not trigger recursive dispatch/spin
  during foreground settlement. Shared owned-handle lifetime; creation failure
  refuses input-pump startup before hook install. Input transfers callback work
  before blocking and after return; transient transport contention gets bounded
  retry, occupied outboxes resume on owner-drain notification.
- Why: not recorded.
- Assumes / revisit if: no best-effort timer substitute. Native wait diagnostics trace-only; physical chevron/input/preview/game acceptance pending; trace-on idle CPU is not game-cost acceptance.
- Status: active, 2026-10-19
- Evidence: [owner idle](../changes/archive/windows-owner-event-idle.md), [pager coverage](../changes/archive/windows-pager-event-coverage.md), [trace volume](../changes/archive/windows-trace-volume.md).

### Post-0.1 KDE threading exploration
- Decision: Windows pumps materially improved performance; KDE currently has no significant jank and threading is not priority. Later explore moving dynamic drag resizing and drag/move/resize interaction into multithreaded Rust, C++/TypeScript thin shims; first research KWin effect/script threading constraints.
- Why: Windows input/render-thread experience.
- Assumes / revisit if: research may change KWin interaction; linked to Push code down and live sibling reflow backlog.
- Status: active, 2026-10-13
- Evidence: [Layering](../principles.md#layering).

### Rust unsafe guardrail
- Decision: workspace unsafe lint forbids unsafe; FFI crates deny and allow only reviewed boundary roots; CI rejects other allows. Minimal safe FFI module over `windows-sys`, split by native responsibility; safe APIs own buffers/callback state/window-handle lifetimes. Cross-thread state uses safe primitives/owned handles; no unsafe Send/Sync, mutable statics, UnsafeCell or cross-thread pointer dereference.
- Why: not recorded.
- Assumes / revisit if: Rust policy/preference order lives in [Rust and Memory Safety](../principles.md#rust-and-memory-safety). Linux planner, core/protocol roots and tests forbid unsafe; Windows FFI root alone allows; Windows/macOS/KWin effect boundaries allow reviewed roots. No change to existing KWin ABI modules except lint attributes.
- Status: active, 2026-10-13
- Evidence: [Windows FFI](../changes/archive/windows-unsafe-ffi-consolidation.md), [Linux planner](../changes/archive/linux-planner-unsafe-guardrail.md), [render thread](../changes/archive/windows-render-thread.md).

### Multi-output foundation
- Decision: pre-v2 candidate-Engine transaction published retained state/membership only after fresh topology, geometry readback and arrival; empty-destination drops also need source reflow; refusal retains logical membership; partial native writes converge by observation, never replay. Current v2 publishes desired mapping/actionable destination before arrival; settlement requires observed receipts (readback, affinity, membership, view/refill), not desired slot/setter success. Verified transfer stands if later foreground actuation fails, reported as focus failure; no workspace switch/hide.
- Why: not recorded.
- Assumes / revisit if: existing shared selection/pointer outcomes authoritative; domain-scoped hover and occupied two-domain drag add no planner wire fields; physical matrix remains pending.
- Status: active, 2026-10-24
- Evidence: [core world model](../changes/archive/core-world-model-rearchitecture.md#stage-3a-accepted-outcome-2026-10-22), [foundation](../changes/windows-multi-output-foundation.md).

### Windows multi-output refinements
- Decision: migration remaps fullscreen/float nonactivating/move-only; maximized/minimized preserve restore rectangle/show state. DPI transition invalidates adapter placement basis, not physical allocation; fresh insets/rebase once per apply then exact arrival verification, further transitions/refused corrections await native-event reconciliation, not permanent clamp pins. Keep transfer fences and stable-DPI clamp handling. Directional/send-output CLI invoke exact-owner production handlers with proof/safety fences; queue acceptance is not completion/physical proof. Taskbar anchor revalidation event-driven, no secondary timer. Keep disabled fail-closed legacy input-driver stages; containment is not live-fixture proof. T2 staged-clone refusal is adapter-only/test-only/unwired; redo migration under core-owned world model, not narrow migration-authority capability.
- Why: not recorded.
- Assumes / revisit if: real-game incremental cost and native physical topology matrix remain pending.
- Status: active, 2026-10-20
- Evidence: [dual-output follow-up](../changes/windows-dual-output-live.md#cross-dpi-placement-follow-up-2026-10-20), [taskbar anchor](../changes/windows-dual-output-live.md#secondary-pager-restart-correction), [T2 proof](../changes/archive/windows-full-world-state-replies.md#t2-offline-staged-transaction-2026-10-20).

### Windows multi-output sequencing
- Decision: Unpark multi-output after single-output handoff items 1-4, 6, 7, 9-11, 13, 15, 17, 18, with item 18 before 10/11; then proceed to the foundation and dependent items 5, 12, 14, 16, 19.
- Why: not recorded.
- Assumes / revisit if: implementation order is a backlog handoff, not a change to workspace behavior.
- Status: active, 2026-10-10
- Evidence: [backlog handoff](../backlog.md).

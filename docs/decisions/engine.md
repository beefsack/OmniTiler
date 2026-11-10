# Engine architecture and operations

### Core-owned world and platform boundary
- Decision: shared core owns screens/outputs and geometry/work areas, per-screen workspace mapping, windows/state/constraints, focused window/screen, focus history, cursor position and behavior settings. Core policy owns navigation, move/send/open/placement/close/reflow/layout/reconciliation, workspace lifecycle/history, migration, hotplug remap and focus-follows-cursor; core output is desired world state including frames, shown workspace and focus target. Platforms own input, native observation and actuation, lifecycle, permissions, effects and rendering. Host limitations are platform-neutral core options: e.g. shared KDE desktops retain per-screen mapping with equal workspace counts; KDE interprets the model. Native workspace domains are observations, never competing policy or prerequisites for logical destinations. This is an approved boundary, not completed extraction.
- Why: not recorded.
- Assumes / revisit if: native safety and host limitations remain subject to Layering and Gaming Compatibility; first deliverable is inventory of platform-owned behavior to move. Whole-workspace migration is a core mapping change actuated by adapter; default-off narrow migration-authority proposal was not approved. T2 staged-clone refusal stays adapter-only.
- Status: active, 2026-10-20
- Evidence: [core/platform boundary](../principles.md#layering), [world owner adoption](../changes/archive/core-world-model-rearchitecture.md#stage-2b-accepted-outcome-2026-10-22).

### Host-synchronous adapter exception (G4)
- Decision: exception to core-owned policy is limited to (1) mechanical safety vetoes (vanished window/stale handle, stale facts, game/fullscreen safety), and (2) applying a core decision within the same host turn. Any other adapter-side decision needs its own dated decision and a backlog item to move it into core. Keep OS/DE-agnostic logic and policy in or near Rust core; provide needed capabilities through the smallest reliable native integration. No capability, including input, is excluded merely for being native. Existing specific implementation choices stand until changed.
- Why: not recorded.
- Assumes / revisit if: host mechanics require synchronous operation; any additional exception requires separate decision and backlog entry.
- Status: active, 2026-10-31
- Evidence: [adapter guide](../porting/world-contract-v3.md#6-lifetime-freshness-and-scope-vetoes), [review G4](../changes/core-interface-review.md).

### Portable engine and host seams
- Decision: MVP uses portable `tiler-core` Engine behind `LayoutPolicy`; `tiler-protocol` is a thin codec, Linux service is separate, and KWin script observes/actuates while native effect renders. Host-synchronous paths may remain in adapter only where moving them changes latency/failure behavior; G4 limits exceptions. Compile compatibility is not behavioral adoption; deliver core/KDE together and track Windows/macOS adoption separately. Pre-0.1 breaking core changes are acceptable for correctness, performance and reliability.
- Why: not recorded.
- Assumes / revisit if: host synchronous paths change latency/failure behavior when moved.
- Status: active, 2026-09-24
- Evidence: [architecture review](../research/architecture-review/review.md), [Layering](../principles.md#layering).

### Full-state world contract and convergence
- Decision: retain stateful, side-effect-free `Engine` behind report/reply boundary; typed deterministic replay is explicit. No caller-owned/serialized core state crosses KDE D-Bus; pure typed transition is optional only for demonstrated need. Desired output is not restorable snapshot of trees/shares/MRU/minimized history. Each accepted full observation publishes one globally monotonic revision; reports include topology, settings, hidden/minimized lifetimes, explicit closes, unreadable holds and actuation feedback. Replies include complete workspace/output/window desires, per-property authority, retained targets, correlated operation results and one-shot effects. Held/untouched scope omission authorizes no write or retirement.
- Why: not recorded.
- Assumes / revisit if: host backing is optional observation; observed logical membership/view receipts replace required logical/native bindings under logical authority. Preserve truthful settlement, incomplete/unreadable fences, explicit closes, generation/watermark checks, consumed-command fencing and at-most-once ambiguous effects.
- Status: active, 2026-10-16
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md), [world model](../changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24).

### World transport authority and boundedness
- Decision: discovery binds authenticated KWin-owner/adapter identity to fresh core generation and returns not-ready without applicable state; complete report precedes ready apply. Restart, Engine replacement and poison recovery establish fresh generation; old effects/commands never replay. Identical latest request returns same reply/revision only for identical input; reused sequence refuses, older requests supersede, monotonic command high-watermarks prevent reexecution. Global counters are bounded decimal `u64` strings in JSON. Adapter invalidates observation watermark synchronously, including during flight and before debounce. Apply requires exact adapter/request/generation binding, newer world revision and current local watermark; repeat lifetime/gesture/safety checks before setters. Stale replies release flight and schedule observation-only catch-up, never command replay. Desired-revision acceptance differs from observed actuation proof.
- Why: not recorded.
- Assumes / revisit if: one 4 MiB request/reply budget. Validate conservative complete bounds before convergence and commit only after exact bounded encoding; oversize returns no applicable partial state and mutates no candidate state. Event-driven full reports, dirty/ack retention and anomaly rescans recover loss; no idle polling. A silently missed final native notification cannot be detected until another trigger.
- Status: active, 2026-10-16
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md).

### Classification and convergence ownership
- Decision: platforms report normalized classification facts (type/role, owner/parent, modal, resizable/min/max size, fullscreen, cannot-be-controlled, app id/class/title). Core decides manage/float/ignore, user rules, automatic fixed-size floats and game handling. Core compares desired/observed state and decides reapply, adopt host value, refuse or float; core owns anti-fighting/completion. Platforms write/read back/report observations and retain host-mechanical correctness only: coordinate/DPI conversion, invisible-border insets, lifetime/replacement detection. Gesture producers report host gesture start, cursor updates, release/cancel; core owns drag session, target, overlay, release outcome and edge-drag share policy.
- Why: not recorded.
- Assumes / revisit if: host cannot apply core state may require rough edges; work through UX. Frame/convergence extractions and native attempt outcome ownership are independently detailed in source delivery records.
- Status: active, 2026-10-20
- Evidence: [Layering](../principles.md#layering), [world model](../changes/archive/core-world-model-rearchitecture.md).

### KWin-to-Rust migration routes
- Decision: Post-0.1 Push code down in stages: extend out-of-process Planner first (route A), beginning with bounded admission (born-maximized preparation, then D06 output selection), then move decisions before lifecycles; move Settings KCM catalog/conflict logic to Rust (route F), reconciled with KDE catalog migration; measure interaction before native replacement; route C native KWin plugin only for measured latency need. Effect-linked Rust (route B) was initially optional acceleration; the 2026-10-31 native-effect reopening and R21 are scoped to that effect work and do not supersede these route A/F/C steps.
- Why: reaching 0.1 feature parity on KDE, Windows and macOS first validates the plan and focus.
- Assumes / revisit if: refresh migration research inventory first because it predates full-state replies; native replacement requires measurement, and route C requires measured latency need. Track remaining work in backlog.
- Status: active, 2026-10-17
- Evidence: [KWin Rust migration research](../research/kwin-rust-migration.md), [R21 review](../changes/core-interface-review.md).

### KDE effect Rust migration sequence (R21)
- Decision: prefer Rust over effect C++ wherever KWin permits. Reweigh effect-linked Rust beyond optional acceleration against prior build/packaging/distribution findings and current C++ effect complexity. Steps 0-2 of candidate plan (prerequisites/boundary trial; pilot active-border logic and gesture lifecycle; presentation consumer/router in Rust together with v4 anchored border) land before 0.1. Step 3 (shortcut reconciler logic) is "after 0.1 or optionally before" if rearchitecture and cleanup finish before the first release. Effect factory, paint hooks, Qt D-Bus endpoints and KCM widgets stay C++.
- Why: not recorded.
- Assumes / revisit if: timing follows evidence; backlog carries remaining plan. Direction aligns with Rust and Memory Safety principle.
- Status: active, 2026-10-31
- Evidence: [Rust and Memory Safety](../principles.md#rust-and-memory-safety), [review F32, R21](../changes/core-interface-review.md).

### KDE sole world route
- Decision: production KDE operates within existing KDE/Plasma/KWin; no KWin fork or patch. Use one world engine with direct geometry and no Custom Tile topology authority or Legacy fallback. DiscoverWorld/DescribeWorld is sole shipped automatic-tiling route. Custom Tile production research is closed; do not adopt Custom Tiles, reopen only on material KWin change. User 2026-11-09: retire the standalone K35 Custom Tile component acceptance check; product tiling/drag/native safety remains under checks 25/36/47. Remove unshipped legacy routes aggressively before 0.1; KDE plan/workspace-send adapters and DescribePlan transport are removed, production uses DiscoverWorld/DescribeWorld plus float-intent persistence. Portable per-domain core APIs are removed.
- Why: not recorded.
- Assumes / revisit if: world model remains retained core owner; per-domain layout projections are internal machinery in one `WorldEngine`, not adapter-facing second owner.
- Status: active, 2026-11-09
- Evidence: [Custom Tile feasibility](../changes/archive/integrated-plasma-structural-feasibility.md), [KDE legacy route removal](../changes/kde-legacy-route-removal.md), [approved KDE acceptance](../changes/kde-v4-acceptance-checklist.md#approved-acceptance-decisions).

### Logical workspace authority and host-owned observations
- Decision: core logical workspace-to-screen mapping is authoritative; native workspace domains are observations, not prerequisites or competing owners. Default world contract has no native workspace vocabulary. Hosts opt into `host_owned_workspace_domains`; `equal_workspace_count` independently expresses output-spanning limits. Native settlement, incomplete/unreadable fences, closes, generation/watermark checks, consumed-command fencing and at-most-once ambiguous effects remain required. Unsupported tiled-to-floating output crossings preserve committed source state; diagnostic float origin never selects policy.
- Why: not recorded.
- Assumes / revisit if: inherited world machinery may be simplified/replaced; maximize portable core authority without platform policy. Stage 2a established boundary; Windows adoption and cursor-first placement are separate.
- Status: active, 2026-10-21
- Evidence: [world model rearchitecture](../changes/archive/core-world-model-rearchitecture.md#stage-2a-accepted-outcome-2026-10-21).

### World-owner adoption and cursor-first placement
- Decision: Windows `WorldOwner` is sole workspace/layout policy owner; native member/view receipts remain distinct from desires. Scoped inventory excludes never-managed foreign windows without weakening bound-member holds; migration arrival may precede target-view switch and empty refill views may prove with hidden valid members. Startup/restart adopts native markers without durable ring/hidden-view store. Core resolves implicit numbered/relative/previous/trailing selection from cursor/focused target and chooses fresh-admission output/neighbor; first full inventory is startup adoption, later lifetimes route without rehoming retained members.
- Why: not recorded.
- Assumes / revisit if: fresh-admission retained focus survives absent/unknown/stealing-new foreground unless current managed focus supersedes it; cursor sample is bounded per normal snapshot, not a hook or continuous poll, skipped in focused-fullscreen-game lane. DPI transition may reassert same intended physical frame once with renewed safety gates.
- Status: active, 2026-10-22
- Evidence: [stage 2b](../changes/archive/core-world-model-rearchitecture.md#stage-2b-accepted-outcome-2026-10-22), [stage 2c](../changes/archive/core-world-model-rearchitecture.md#stage-2c-accepted-outcome-2026-10-22).

### Action context and output grounding
- Decision: observed output grounding uses full rectangles: integer-rounded center with half-open containment, then largest intersection, then first topology entry. Directional migration source resolves foreground output, pointer output, cached output, then ordered known-output registry; disconnected mappings remain known and stale selected source refuses, never retargets. Migration freezes source/current view before incorporating report changes; numbered/relative send freezes pre-observation retained home.
- Why: not recorded.
- Assumes / revisit if: registry order is reported fact, not primary-output preference. Keyboard sends retain live cache then report-screen order; explicit output remains explicit. Physical-output versus membership-home contract stays parked.
- Status: active, 2026-10-24
- Evidence: [output grounding](../changes/archive/core-world-model-rearchitecture.md#stage-5-output-grounding-and-dead-fence-audit-2026-10-23), [action context](../changes/archive/core-world-model-rearchitecture.md#stage-5-action-context-ownership-2026-10-24).

### Core-derived native settlement
- Decision: adapters report raw frame, clear, membership and view attempts; core derives settlement. Remove external operation-verdict feedback, zero-valued presentation reserve, and retired compatibility routes. Core retains identical-pair clamp suppression and bounded transient retry from raw lifetime-scoped attempts; unreadable proof cannot establish clamp, consumed evidence survives exact-match clears, and retry is event-driven. Core retains minimum-size evidence per lifetime, merging into normalized hints; maxima are not widened, infeasible minima keep proportional behavior, and close/departure/incarnation reset retire evidence.
- Why: not recorded.
- Assumes / revisit if: accepted time-stamped growth can learn an axis minimum only with no axis shrink, readable corroboration, controlled tiled scope and no known DPI transition; unknown DPI is not transition evidence and missing timing never learns. Later report owns suppression pair over immediate post-write readback; later stable mismatch is not cleared by exact immediate readback.
- Status: active, 2026-10-26
- Evidence: [settlement extraction](../changes/archive/core-world-model-rearchitecture.md#stage-5-dead-api-disposition-2026-10-24), [frame and learned constraints](../changes/archive/core-world-model-rearchitecture.md#stage-3b1-accepted-outcome-2026-10-22).

### Gesture lifecycle and native transport
- Decision: adapters report stable nonzero gesture sequence, exact lifetime, move/resize kind, frozen start frame/down point, live/release cursor and terminal phase. Core binds before sweep, retains source/hover, resolves preview and release, and prevents replay; cancellation cannot erase newer gesture. Terminal ack `gestureSettled.committed` proves core mutation only, not native arrival. KDE one native effect owns identity/start/phases/finish; script cannot mint lifecycle or adopt delayed witnesses. Esc is Cancelled only when synchronous finish delivery is causally fenced; otherwise Released.
- Why: not recorded.
- Assumes / revisit if: unavailable capture omits ingress, never invents terminal; indistinguishable Xwayland cancel reports Released. No new hook/timer. Gesture freshness enforcement and physical/game acceptance remain separate.
- Status: active, 2026-10-26
- Evidence: [gesture lifecycle](../changes/archive/core-world-model-rearchitecture.md#stage-4a-accepted-outcome-2026-10-23), [KDE gesture transport](kde.md#kde-gesture-transport).

### Shared presentation and cursor authority
- Decision: core owns border eligibility and the anchored R20 descriptor, group bounds, drop shapes, per-screen pager rings/labels/current/occupied/focused flags, miniature source frames and paint order. Styles come from settings; platforms report neutral foreground, movement/chord, stacking, live pager frames and the causal witness. Border eligibility requires the joined foreground witness. Core retains last-known live pager frames without fabricating geometry or membership; float/tile source slots and adapter pause/style verdicts are retired. Pager order alone does not establish membership: complete inventory immediately prunes an identity with neither a fact nor a held row; held identities and incomplete inventories retain absent identities. Focused-fullscreen presentation suspension is unconditional, including truthful unmanaged fullscreen foregrounds, and hides only that output's border/underlay/pager/drop-preview while preserving other outputs and gesture lifecycle/results. Platforms render applied replies and retain only native freshness/scope/lifetime vetoes, DPI conversion, anchoring, artwork and click-to-command.
- Why: not recorded.
- Assumes / revisit if: core derives shapes; native anchored-border frame following follows R20. Fast wake may hide an obsolete subject but not recompute core geometry. Native/reference order-before-facts delivery and observable ordering consequences remain TBD. Cursor focus/warp policy stays separate; neither setting is enabled by default.
- Status: active, 2026-10-24
- Evidence: [v4 presentation contract](../porting/world-contract.md#worldpresentation-and-anchored-border), [R20](contract.md#anchored-border-r20), [pager-order discriminator](../spec/reference-outcomes.md#pager-order-before-window-facts), [uniform presentation](../changes/archive/core-world-model-rearchitecture.md#stage-5-uniform-wire-presentation-2026-10-24), [cursor focus/warp](../changes/archive/core-world-model-rearchitecture.md#stage-4b-accepted-outcome-2026-10-23).

### Frameless migration placement
- Decision: core publishes cross-output sessionless migration placement as effective frame for membership actuation, not tile allocation or ordinary-float authority. Translate observed source frame by full-output delta, constrain origin to destination work area without resizing, pin oversized axes to work-area origin, and ground from raw observed output so an already-target frame is not translated twice. Windows does not guess remap.
- Why: not recorded.
- Assumes / revisit if: same-output follow ordering, cross-output-only pre-arrival tile slots, intentional-float physical frames and unsupported tiled-to-float source-state transactions remain unchanged.
- Status: active, 2026-10-23
- Evidence: [frameless migration](../changes/archive/core-world-model-rearchitecture.md#stage-3b4-accepted-outcome-2026-10-23).

<a id="cosmic-v1-lifecycle-semantics"></a>
### `cosmic_v1` lifecycle semantics
- Decision: lifecycle plans use portable policy version 1 over opaque `(output, workspace)` domains, preserving independent workspace trees without inventing native workspace semantics. Focused-cell binary admission, physical-axis selection, equal new splits, proportional ordered-N-ary share adaptation, physical-pixel resize and drag zones follow source-evidenced `cosmic_v1`; recursive collapse remains. Center stack drops fail closed because stacks are unselected/compositor-owned. Tiled removal removes a focused leaf from source-domain MRU and selects remaining top; unfocused removal preserves focus; no remaining tiled stack entry clears focus. `direction=None` send uses ordinary admission; separate `follow=true` chooses target focus and `follow=false` source fallback. Float/sticky exceptions and fullscreen/maximize tile overlays follow window-state policy; other lifecycle operation contracts are independent.
- Positive-close reconciliation freezes the existing source-domain tiled MRU/layout selection before ingesting native fallback, excluding the full close cohort. Otherwise readable survivor geometry projects without observed focus, independently of whether the bounded retained close-focus request is permitted. Logical successor and actual foreground remain distinct; sole close retains the shown empty source view and focused-output intent without native clear or other-domain activation.
- Why: not recorded.
- Assumes / revisit if: other lifecycle contracts remain independent; future tiling profile behavior belongs to a separate versioned policy mode sharing portable engine. Missing capability refuses narrowly with logging/recovery, not generic fallback or platform policy.
- Status: active, 2026-09-09
- Evidence: [reference outcomes](../spec/reference-outcomes.md), [lifecycle source](../changes/archive/core-world-model-rearchitecture.md).

### Workspace send and cross-output R4 convergence
- Decision: workspace send and R4 synchronously commit planned topology with complete observation convergence; no surviving pair, ack, verify or pending state. Send moves only focused tiled window and recursively collapses source tree. Validated last-active-leaf target admission uses `map_to_tree` root/output-geometry fallback; empty targets become lone root. R4 preserves owner/generation/revision/correlation/single-flight/visibility/exception/fail-closed target fences, returns both-domain geometry/native assignment, and leaves no pending/status/cancel/abandon path. KWin applies output/desktop membership through its public native API before desired geometry; stale replies authorize no setters and failed/partial proof converges from fresh full observations without command replay or verified-success claim.
- Why: not recorded.
- Assumes / revisit if: local R1/R2/R3 wins. Exhausted horizontal movement crosses to adjacent output's selected logical workspace; 2026-10-07 extension covers all four directions and sole root leaves, selecting reciprocal neighbors on full output rectangles; no output wrap or workspace cycling. Occupied R4 placement uses whole-target D-axis wrap nearest source; explicit output send uses ordinary admission. World focus transfer has no layout/membership writes.
- Status: active, 2026-10-07
- Evidence: [R4 implementation](../changes/kde-dual-output-live.md#rightward-r4-defect-correction-and-re-verification), [status disposition](../changes/archive/decisions-evidence-2026-10-31.md#targeted-baseline-evidence-dispositions), [cross-output focus policy](../spec/reference-outcomes.md#cross-output-focus-edge-discriminator).

### Exhausted cross-output transfer reuse
- Decision: exhausted cross-output moves reuse retained window-transfer bookkeeping. The migration-refill guard and test-bridge clauses in the original record are historical after the world-model rearchitecture.
- Why: not recorded.
- Assumes / revisit if: no additional migration-refill policy is implied by this retained-transfer reuse.
- Status: active, 2026-10-18
- Evidence: [R4 record](../changes/archive/world-split-edge-selection.md).

### R4 own-mover observation tolerance
- Decision: after native transfer setters only, an R4 observation may contain exactly the expected flight set minus the mover, with retained native identity and in-scope output/single-desktop proof. Pre-transfer, domain and non-mover refusal guards remain; arrival/follow still require fresh exact target placement. This permits clean transfer rather than settling through recovery.
- Why: not recorded.
- Assumes / revisit if: source evidence covers COSMIC, Hyprland, i3 and sway crossing to adjacent output with mover followed/focused; no broader tolerance is authorized.
- Status: active, 2026-10-14
- Evidence: [R4 correction and verification](../changes/kde-dual-output-live.md#rightward-r4-defect-correction-and-re-verification).

### MVP directional path
- Decision: COSMIC-style tiling and directional movement are MVP. Once promoted, directional path replaces legacy path; no legacy fallback remains.
- Why: not recorded.
- Assumes / revisit if: promotion is path-specific; no legacy fallback is inferred for paths not promoted.
- Status: active, date not recorded
- Evidence: [core world model](../changes/archive/core-world-model-rearchitecture.md).

### Policy modes and future tiling profiles
- Decision: `cosmic_v1` targets strong source-evidenced behavioral parity, permitting only explicitly recorded functional alternatives, host/game-safety exceptions and infeasible platform capabilities. Missing capability fails closed with narrow refusal/logging and later recovery. Future behavior belongs in a separate versioned mode on the portable engine, not unnamed fallback or adapter policy. After MVP, a selectable profile/type must cover both tiling behavior/algorithm and matching shortcuts, not a cosmetic preset.
- Why: not recorded.
- Assumes / revisit if: COSMIC remains current. No immediate Hyprland mode/backend, exact parity or mode/version/config switching, shortcut override UI or hot-switch semantics selected. Keyboard-layout localization is separate; initial US-keyboard support unchanged.
- Status: active, 2026-09-09
- Evidence: [policy architecture](../changes/archive/core-world-model-rearchitecture.md).

### COSMIC geometry parity boundary
- Decision: native KCM-owned `innerGap` and `outerGap` default to `(8, 8)`, giving effective 8px work-area edge margin and sibling spacing; `outerGap` is not raw COSMIC theme outer value. Retain shares without claiming exact COSMIC parity.
- Why: not recorded.
- Assumes / revisit if: revisit only for reproduced N-ary/deep-layout visual discrepancy or required source-exact fixture failure under shares. No pixel-authority migration, projector correction, numerical-policy or code/test work selected.
- Status: active, 2026-09-16
- Evidence: [COSMIC geometry decisions](../changes/archive/core-world-model-rearchitecture.md).

# World rendering and stress

### Latest-world convergence
- Decision: process every command, but render only the latest desired world; rendering never changes world state. Core owns active-window memory at command/lifecycle decisions. Intentional focus changes replace pending arrival choices promptly; lifetime, displays and settings report promptly.
- Why: intermediate rendering causes slowness and jank.
- Assumes / revisit if: intentional foreground changes can be attributed; click/app activation versus OS fallback remains unresolved. Safety gates, exact-desktop authority and no-forced-refocus remain unchanged. Fresh authority for never-issued focus intent is separate.
- Status: active, 2026-10-27
- Evidence: [Layering](../principles.md#layering), [investigation](../changes/windows-workspace-return-focus.md#prompt-intent-and-render-side-effects-2026-10-27).

### Arrival disturbance reduction
- Decision: reduce disturbance/click rejection before choosing unknown-foreground policy. Reveal arrivals promptly, prioritizing shown proof over unrelated rendering, then investigate eligible focus and departure hide.
- Why: reduce ambiguity before selecting the unknown-foreground policy.
- Assumes / revisit if: measure ordinary and rapid switching, include empty arrivals, physically validate Alt-Tab and complete source-fenced protocol work. Do not block safety, input or newer commands, change safety gates, or adopt phased view authority without review.
- Status: active, 2026-10-27
- Evidence: [measurements and staged design](../changes/windows-workspace-return-focus.md#ambiguity-reduction-measurements-and-staged-design-2026-10-27).

### Stress acceptance
- Decision: stress modes share platform-neutral definitions, oracles, host schedules and bounded observation barriers. Preserve alias/focus oracles, transport/admission fences, evidence separation, effect-aware recovery, exact accepted-world/snapshot binding, strict joins, hidden-window write fences, exact identity/visibility/frame proof and reveal geometry checks.
- Why: not recorded.
- Assumes / revisit if: product behavior and safety fences remain unchanged.
- Status: active, 2026-10-27
- Evidence: [approved contract](../changes/archive/platform-neutral-stress-acceptance.md#approved-technical-decisions), [proof binding](../changes/archive/platform-neutral-stress-acceptance.md#proof-binding-repair-and-final-acceptance).

### Stress runner dispatch
- Decision: deterministic CLI and lower-priority injected-keyboard modes share definitions/oracles; keyboard mode uses injected real shortcuts and test-window churn. Verify shortcut acceptance and coverage for every action first. No batch-file/stdin CLI without user approval if keyboard coverage proves insufficient.
- Why: the deterministic CLI is already faster than typical human input.
- Assumes / revisit if: keyboard mode does not cover a case.
- Status: active, 2026-10-27
- Evidence: [approved contract](../changes/archive/platform-neutral-stress-acceptance.md#approved-technical-decisions).

### KDE renderer freshness
- Decision: gesture transport precedes renderer freshness. Every preview identifies its native gesture; publish each native invalidation and refresh or hide affected surfaces, including anchor remapping. Borders/underlays cover latest invalidation; active-drag preview has only the same-gesture continuity exception and must match the live gesture. No shared-core protocol follows from this.
- Why: not recorded.
- Assumes / revisit if: rapid A/B and in-flight old-paint checks remain; any shared-core need requires a separate decision.
- Status: active, 2026-10-26
- Evidence: [Stage 5 outcome](../changes/kde-world-v2-port.md#stage-5-renderer-causal-freshness-accepted-offline-2026-10-27).

### Focused-fullscreen presentation suspension
- Decision: unconditionally suppress borders, underlays, overlays and drop preview on the output whose focused surface is fullscreen, on every platform. Require truthful fullscreen facts, no app/game/size heuristics. Fullscreen windows otherwise remain ordinary: workspace sends move them, switches hide them and window shortcuts act on them. Publish the suspended output and remaining-output pager content; KDE drops matching previews and invalidates native surfaces on entry/exit. Game suspension is separate.
- Why: not recorded.
- Assumes / revisit if: each host supplies truthful fullscreen facts; no app-name or size heuristics.
- Rejected: construction opt-in and Windows all-output `paused` lane. Alt+Enter remains unbound; fullscreen-exit command is separate.
- Status: active, 2026-10-31
- Evidence: [contract](../changes/kde-world-v2-port.md#focused-fullscreen-presentation-suspension-2026-10-27), [window-state commands](contract.md#window-state-commands-r14).

### Membership and immediate eligibility
- Decision: membership is distinct from immediate layout/action eligibility. Minimized and host-hidden windows may remain members but are ineligible; Windows safety exclusions remain unmanaged. KWin dialog/transient facts use existing float path; native `hidden` is separate through the minimized-member lane. Windows/macOS production behavior is unchanged.
- Why: not recorded.
- Assumes / revisit if: explicit tile intent precedence over automatic transient float and technical names remain tentative; this does not select Activities support.
- Status: active, 2026-10-28
- Evidence: [N5 outcome](../changes/kde-world-v2-port.md#n5-membership-and-immediate-eligibility-2026-10-28).

### Float-toggle intent and output targeting
- Decision: `ToggleFloat` has no intent-time rectangle; core resolves remembered float geometry or current frame at execution, and explicit rectangles mean genuine placement. macOS first float uses current frame in place. Implicit selection/send/move leaves output unset for core execution-time cursor/focus resolution; explicit output requires genuine user choice. Explicit-send current-view sampling remains separate.
- Why: not recorded.
- Assumes / revisit if: prove Windows float-placement parity.
- Status: active, 2026-10-28
- Evidence: [KDE audit](../changes/kde-world-v2-port.md#n6-core-intents-outcome-and-blocker-2026-10-26), [output targeting](../changes/archive/output-targeting-race-audit.md).

### KDE pager
- Decision: KDE uses native Plasma pager; OmniTiler does not render one and core pager-focused-window color does not apply.
- Why: not recorded.
- Assumes / revisit if: not recorded.
- Rejected: KDE pager rendering and styling.
- Status: active, 2026-10-26
- Evidence: [KDE port decisions](../changes/kde-world-v2-port.md#user-decisions-2026-10-26).

### Gesture capture and freshness test boundary
- Decision: lost terminal-capture replies redeliver the byte-identical envelope at 2 s capture-timeout cadence up to `TERMINAL_CAPTURE_MAX_RETRIES`, then at the 25 s host deadline until terminal settles, rotates or stops; these bounds do not prove finish/cancel/settlement. Retain at most 256 gesture identities; a seventeenth source instance rotates broker rather than evicting anti-replay evidence. Keep freshness/visibility logic KWin-independent; test actual facts, broker capture/projection and JS consumer on shared payloads, stubbing only KWin signal delivery/painting. Delivery proof is async D-Bus `Introspect`, never a settle wait.
- Why: preserve anti-replay evidence; otherwise not recorded.
- Assumes / revisit if: physical signal/painting checks remain pending.
- Status: active, 2026-10-27
- Evidence: [Ordering and Races](../principles.md#ordering-and-races), [Stage 5 outcome](../changes/kde-world-v2-port.md#stage-5-renderer-causal-freshness-accepted-offline-2026-10-27).

### Other accepted KDE package decisions
- Decision: accept Stage 6 bounded `windowAt` decoding (count one) and border-selector test seam; Stage 7 read-only per-output desktop detection, navigation-only Virtual Desktops opener and tooltip-distinguished shared labels.
- Why: not recorded.
- Assumes / revisit if: none recorded.
- Status: active, 2026-10-28
- Evidence: [KDE port decisions](../changes/kde-world-v2-port.md).

### Native workspace adoption
- The native-adoption policy, including four-to-four mapping and R19, lives only in [layout-and-admission.md](layout-and-admission.md#native-workspace-adoption-and-r19).

### Active-drag preview continuity
- Decision: retain latest completed core preview for the same active native gesture across pointer/mover motion; X stays visible until Y preview completes, but release acts on current target, not lagging preview. Independently changed/removed targets, layout/structure, focus and fullscreen/suspension invalidate immediately with exact causal structural coverage. Terminal release/cancel/end hides immediately; generation/order fencing and native `{instance,serial}` isolation remain mandatory.
- Why: prefer retaining a valid same-gesture preview over hide/reshow; address lag through performance/latency rather than aggressive hiding.
- Assumes / revisit if: exception is preview-only; borders, underlays and actuation retain exact-current freshness and preview policy remains core-owned.
- Status: active, 2026-10-27
- Evidence: [KDE renderer outcome](../changes/kde-world-v2-port.md#stage-5-renderer-causal-freshness-accepted-offline-2026-10-27).

### Cross-output gesture release placement
- Decision: release honors pointer-targeted slot/orientation through same core resolver as preview, using live release coordinates and retained hover state, never replaying lagging preview. Use destination work area/outer gap; retain member/follow settlement, same-window in-flight and migration-destination fences. Empty destinations and unplaceable points retain existing send admission.
- Why: not recorded.
- Assumes / revisit if: no broader output-targeting or directional R4 change. Ghostty size shortfall is external toolkit/client sizing; no retries/workarounds selected; exact ownership/stale-bounds mechanism unresolved.
- Status: active, 2026-10-27
- Evidence: [evidence and limits](../changes/kde-world-v2-port.md#mixed-scale-cross-output-drop-outcome-2026-10-27).

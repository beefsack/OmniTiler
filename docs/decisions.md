# Current Decisions

Each current decision lives exactly once in its topic file under `docs/decisions/`. Add new decisions to their topic and add a dated index line here. Historical evidence belongs in linked records; tentative entries are not approved policy.

## Dated decisions

- 2026-10-26: KDE gesture-first renderer freshness, unconditional focused-fullscreen suspension, membership versus eligibility, float-toggle/output-targeting semantics, and native Plasma pager ([world rendering](decisions/world-rendering.md#kde-renderer-freshness); [fullscreen](decisions/world-rendering.md#focused-fullscreen-presentation-suspension); [membership](decisions/world-rendering.md#membership-and-immediate-eligibility); [intent](decisions/world-rendering.md#float-toggle-intent-and-output-targeting); [pager](decisions/world-rendering.md#kde-pager)).
- 2026-10-27: no-focus command encoding, shared stress contract, and latest-world convergence ([command encoding](decisions/commands.md#directional-focus-without-a-focused-window); [world rendering](decisions/world-rendering.md#stress-acceptance); [latest-world](decisions/world-rendering.md#latest-world-convergence)).
- 2026-10-28: Windows staged-switch prototype, narrow fresh-focus authority, empty-workspace focus, coalescing limits, active-window memory, cross-output directional focus, and associated Windows decisions ([windows](decisions/windows.md#switch-and-intent-decisions); [focus](decisions/focus.md#workspace-active-window-memory); [cross-output focus](decisions/focus.md#cross-output-directional-focus)).
- 2026-10-29: staged visibility and sticky-off rehome, protocol fences, macOS 0.1 scope, and native adoption ([windows](decisions/windows.md#state-centric-v4-visibility); [windows decisions](decisions/windows.md#windows-decision-set); [macOS scope](decisions/macos.md#macos-0-1-scope-and-gate); [adoption](decisions/layout-and-admission.md#native-workspace-adoption-and-r19)).
- 2026-10-30: state-centric v4 visibility, record model, contract, anti-cheat limits, automatic focus guards, fixed-size float sends, macOS shared directional commands, and cross-platform fullscreen suspension ([windows](decisions/windows.md#state-centric-v4-visibility); [record model](decisions/windows.md#record-model); [contract](decisions/contract.md#state-centric-contract); [gaming](decisions/gaming.md#anti-cheat-mechanism-limits); [focus](decisions/focus.md#automatic-focus-guards); [fixed-size sends](decisions/workspaces.md#workspace-sends-and-floating-state); [cross-output focus](decisions/focus.md#cross-output-directional-focus); [fullscreen](decisions/world-rendering.md#focused-fullscreen-presentation-suspension)).
- 2026-10-31: R10/R11/R12/R14/R18/R20 contract decisions; Rust safety principle; R21 migration timing; licensing; R15 observability; R19 native adoption; and G4 synchronous-adapter exception ([R10 size units](decisions/contract.md#size-units-and-scaling-r10); [R11 output identity](decisions/contract.md#output-identity-r11); [R12 native writes](decisions/contract.md#native-writes-difficult-to-undo-r12); [R14 window-state commands](decisions/contract.md#window-state-commands-r14); [R18 scripting surface](decisions/contract.md#scripting-command-surface-r18); [R20 anchored border](decisions/contract.md#anchored-border-r20); [Rust safety](principles.md#rust-and-memory-safety); [R21](decisions/engine.md#kde-effect-rust-migration-sequence-r21); [licensing](decisions/development.md#licensing); [R15](decisions/observability.md#normal-level-window-identity-r15); [R19](decisions/layout-and-admission.md#native-workspace-adoption-and-r19); [G4](decisions/engine.md#host-synchronous-adapter-exception-g4)).
- 2026-11-01: KWin-free native effect source licensing confirmed MIT after audit; KWin-header-dependent source and effect binary remain GPL ([licensing](decisions/development.md#licensing)).
- 2026-11-01: User decision: directional focus across outputs from maximized/fullscreen origins and onto fullscreen/maximized destination members; fullscreen-state-based game protection ([window state](decisions/window-state.md#directional-maximize-and-fullscreen-operations); [cross-output focus](decisions/focus.md#cross-output-directional-focus); [gaming](decisions/gaming.md#fullscreen-state-based-protection)).
- 2026-11-01: Windows app-owned fullscreen exit remains refused without a restoration preimage; adapter-local adoption-baseline plan and per-case acceptance gate ([window-state overlays](decisions/window-state.md#fullscreen-and-maximize-overlays); [Windows fullscreen exit baseline](changes/windows-fullscreen-exit-baseline.md); [R14](decisions/contract.md#window-state-commands-r14)).
- 2026-11-01: v3 remains authoritative until a single coordinated core/all-adapter v4 cutover; approved v4 migration phases, branch strategy and conformance gate ([contract authority and v4 migration](decisions/contract.md#contract-authority-and-v4-migration); [follow-up record](changes/archive/docs-followups-2026-11-01.md#decision-outcomes-and-remaining-work)).
- 2026-11-01: Vision purpose, gaming, One Core and reliability; Layering and Resilience; contract split and Git-only deletion record model ([purpose](../VISION.md#purpose); [gaming](../VISION.md#gaming-and-full-screen-applications); [One Core](../VISION.md#one-core); [reliability](../VISION.md#reliability); [Layering](principles.md#layering); [Resilience](principles.md#resilience); [contract split and deletion model](principles.md); [v4 authority](decisions/contract.md#contract-authority-and-v4-migration)).
- 2026-11-03: Pending native maximize-on/off is held while newer fullscreen-on is pending or active, then applied after exit; settled maximize preservation and explicit R-MAX-11 replacement remain unchanged ([fullscreen and maximize overlays](decisions/window-state.md#fullscreen-and-maximize-overlays)).
- 2026-11-04: Focusless switches use focus=unproven and complete once their visibility obligations are satisfied, including zero-member and native-owned selections; focus-bearing switches still require proven focus ([focusless switch completion](decisions/workspaces.md#focusless-switch-completion)).
- 2026-11-04: After presentation revision, complete two-way v4 codecs/legacy-envelope removal/version bump, then a development-only KDE vertical slice before scaled metrics, reconnect/rehome and remaining core work. Core quality takes priority; host findings require shared-core design discussion, never KDE-specific workarounds. Sequential platform conformance and live release gates remain unchanged ([v4 migration order and principles](decisions/contract.md#contract-authority-and-v4-migration)).
- 2026-11-08: User decision: retire the tentative KDE lifecycle and accepted vocabulary entry. Lifecycle normalization stays documented in the [KDE port notes](porting/kde-port-notes.md); the name list was stale and not product policy.
- 2026-11-08: User decision: every KDE user command press queues FIFO in press order with no busy refusal; core applies every action and resolves window subjects from its own state, while the platform applies only the latest publication ([KDE rapid input](decisions/commands.md#kde-rapid-input)).
- 2026-11-08: User decision: retire aligned-grid lifecycle, destination replacement and aligned-only refill/spare rules in favor of the common v4 per-output model; retain displacement and physical return ([workspaces](decisions/workspaces.md#common-v4-per-output-workspaces)).
- 2026-11-08: User decision: start the Windows port only after KDE phase 2 is complete offline including v3 removal, the user-run KDE state-target and reconnect live checks pass, and core scales resize minima, keyboard steps and group-edge zones per output; macOS follows Windows ([port start gate](decisions/contract.md#contract-authority-and-v4-migration)).

## Tentative decisions

- Floating/exception/non-leaf command-memory storage remains parked ([Windows decisions](decisions/windows.md#windows-decision-set)).
- macOS preview entry point and preview configuration/observation recovery remain tentative ([development](decisions/development.md#tentative-macos-development-preview); [macOS](decisions/macos.md#fixture-and-scoped-input-defaults)).
- macOS preview presets and native shortcut ownership remain tentative ([shortcuts](decisions/reference-and-shortcuts.md#macos-preview-presets-and-native-shortcut-ownership)).
- KDE unload/restart ownership-restoration question remains open ([fixed-size](decisions/layout-and-admission.md#automatic-fixed-float-stacking-and-gaming-safety)).
- KDE world placement traces remain tentative ([observability](decisions/observability.md#kde-world-placement-traces)).
- Activities defaults remain tentative ([KDE](decisions/kde.md#deferred-activities-support)).
- KDE v2 stage sequencing, capture transport/reset and migration/backing oracle remain tentative ([port record](changes/kde-world-v2-port.md#tentative-technical-decisions)).
- Shared transient three-way classification remains tentative ([port record](changes/macos-world-v2-port.md#shared-transient-classification-2026-10-25)).
- Independent birth-clear authority under geometry holds remains tentative ([contract](decisions/contract.md#core-birth-clear-ownership)).
- macOS fullscreen-game direction remains tentative ([macOS](decisions/macos.md#fullscreen-games)).

## Topics

- [Activation requests](decisions/activation-requests.md): activation intent, urgency and host-specific exceptions.
- [Commands](decisions/commands.md): workspace-tiling intent and command dispatch constraints.
- [Contract](decisions/contract.md): shared core/platform contract, capabilities, persistence and cross-platform wire decisions. <a id="contract"></a>
- [Development](decisions/development.md): project identity, licensing, tooling and development entry points.
- [Engine](decisions/engine.md): portable world ownership, convergence, operation policy and KDE route boundaries.
- [Focus](decisions/focus.md): automatic-focus guards, workspace memory and directional focus.
- [Gaming](decisions/gaming.md): game membership, anti-cheat limits and pause/release policy. <a id="anti-cheat-mechanism-limits"></a>
- [KDE](decisions/kde.md): KDE support, packaging, Planner lifecycle and live-evidence boundary.
- [Layout and admission](decisions/layout-and-admission.md): placement, admission, movement, output selection and native workspace adoption.
- [macOS](decisions/macos.md): native Spaces, input, presentation, lifecycle and macOS 0.1 scope.
- [Observability](decisions/observability.md): logging, tracing, identity and privacy boundaries.
- [Persistence](decisions/persistence.md): restart adoption and durable user intent.
- [Platforms](decisions/platforms.md): supported platform scope, safety and release gates.
- [Reference and shortcuts](decisions/reference-and-shortcuts.md): reference authority, shortcut ownership, conflicts and presets. <a id="reference-tiers"></a>
- [Settings](decisions/settings.md): shared settings model, platform ownership and UI behavior.
- [Visuals](decisions/visuals.md): borders, underlays, pointer presentation and group visuals. <a id="anchored-active-border-v4-r20"></a>
- [Window state](decisions/window-state.md): floating, sticky, maximize, fullscreen and lifecycle transitions.
- [Windows](decisions/windows.md): Windows owner, input, workspace and safety decisions. <a id="rust-unsafe-guardrail"></a>
- [Workspaces](decisions/workspaces.md): workspace authority, views, sends, lifecycle and history.
- [World rendering](decisions/world-rendering.md): latest-world rendering, freshness, presentation and stress acceptance.

## Compatibility aliases

The following link-only aliases preserve existing principle anchors. They add no policy text.

<a id="priorities"></a> [Priorities](decisions/contract.md).
<a id="gaming-compatibility"></a> [Gaming Compatibility](decisions/gaming.md#anti-cheat-mechanism-limits).
<a id="simplicity"></a> [Simplicity](decisions/engine.md).
<a id="focus-and-return-on-investment"></a> [Focus and Return on Investment](decisions/focus.md#workspace-active-window-memory).
<a id="consistency"></a> [Consistency](decisions/reference-and-shortcuts.md).
<a id="layering"></a> [Layering](decisions/engine.md#core-owned-world-and-platform-boundary).
<a id="ordering-and-races"></a> [Ordering and Races](decisions/contract.md#transient-side-effects).
<a id="resilience"></a> [Resilience](decisions/engine.md#world-transport-authority-and-boundedness).
<a id="efficiency"></a> [Efficiency](decisions/observability.md#cross-component-diagnostics).
<a id="host-integration"></a> [Host Integration](decisions/platforms.md#platform-scope).
<a id="observability"></a> [Observability](decisions/observability.md#cross-component-diagnostics).
<a id="testing"></a> [Testing](decisions/reference-and-shortcuts.md#functional-specification-and-unresolved-work).
<a id="rust-and-memory-safety"></a> [Rust and Memory Safety](decisions/windows.md#rust-unsafe-guardrail).
<a id="file-size"></a> [File Size](decisions/development.md#development-environment-tools).
<a id="cross-output-focus-edge-policy"></a> [Cross-output directional focus](decisions/focus.md#cross-output-directional-focus).

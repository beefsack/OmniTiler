# Shared core contract

### State-centric contract
- Decision: core sends desired state and platforms send facts; per-operation tracking exists only where ordering/race rules require correlation.
- Positive-close focus is one retained nullable request with opaque identity, frozen target/domain, immutable issuing revision and distinct `positive-close` cause. Required report `foregroundKnown` distinguishes bound/no-client from unknown with the existing focused string. Raw focus feedback correlates by exact `(id,target,rev)`, not id parsing; current target observation satisfies independently of setter evidence. Adapters spend once per bound id, including safety no-call, and need only the latest publication. No other focus effect migrates to retained state.
- Why: platforms converge to latest desired state; per-operation tracking multiplied correlation machinery.
- Rejected: adapter reassertion budgets, acceptance and replan (KWin three-strike interim and stale-snapshot replan).
- Assumes / revisit if: reapplying desired state is safe for each actuation kind; revisit unsafe kinds.
- Status: active, 2026-10-30
- Evidence: [review P1, F5, F17](../changes/core-interface-review.md).

### Contract authority and v4 migration
- Decision: v3 as implemented is authoritative until one coordinated cutover across core and every adapter; the approved v4 contract replaces it at cutover, with no dual shapes. Sequence: 0 draft a normative v4 section in the [world contract](../porting/world-contract.md), folding in all approved message shapes, ownership, ordering and capabilities, and add v4 wire-conformance fixtures under `test-fixtures/world-wire/`; 0b read-only KDE, Windows and macOS platform-fit reviews feed draft fixes; 0c user approves shape; implement core/protocol and sequential adapter ports in the amended order below, allowing temporarily broken adapters. Delete `world-contract-v3.md` at cutover. Port order is KDE, then macOS, then Windows (including R14 fullscreen baseline) (amended User 2026-11-09); all three must pass conformance before live release acceptance. Migration work lands directly on main (amended 2026-11-03, superseding a separate v4 branch with a dogfoodable main); main may be broken during migration, accepted before release in favor of correctness, quality and simplicity. Fold Windows state-centric visibility and R10/R11/R12/R14/R18/R20 into this cutover. After the KDE port is complete, do R21 KDE effect Rust steps 0-2.
- Implementation order (User 2026-11-04; removal amended User 2026-11-07): finish the common presentation revision unit; then full two-way v4 codecs, removal of legacy envelope keys and the version bump; then a KDE vertical slice covering handshake, report/reply loop, selection, arrival, focus feedback and state targets, in that order; then scaled metrics, physical reconnect/rehome, the scenario runner and remaining v3 removal, informed by KDE findings. Cull obsolete legacy/dead code as it is touched or proved obsolete, in commits separate from behavior fixes with evidence that no live path needs it; do not start a dedicated sweep. The final removal slice covers only what remains. For Rust legacy removal, map each supported legacy assertion to genuine v4 coverage, port uncovered behavior first, then cull legacy implementations in separate independently reviewed units with no-live-path evidence; topic similarity alone is not coverage. Retain immutable recorded fixtures and active migration records, and keep both stage5 proofs enabled. This supersedes deferring all legacy removal to the final slice. The KDE slice is development-only until scaled metrics and rehome land, not release acceptance.
- Port start gate (User 2026-11-08; amended User 2026-11-09 and 2026-11-10): the first non-KDE port starts only after KDE phase 2 is complete offline, including remaining v3 removal, and the user-run KDE state-target live check passes. Since 2026-11-09 that port is macOS, which inherits this gate; Windows follows macOS. User 2026-11-10 deferred the ST desktop/stacking recovery and compositor-boundary (KWin restart) recovery sub-legs until after the macOS start: they no longer gate start and remain open KDE acceptance. The start gate's conditions are now met: KDE phase 2 is complete offline and physical ST passed 2026-11-10 apart from those deferred sub-legs ([results and limits](../changes/v4-phase2-kde-state-targets.md#2026-11-10-physical-state-target-acceptance)). The user-run KDE reconnect check no longer gates the port start (User 2026-11-09): its leg-2 fix and the reconnect merge decision must land before macOS reaches its reconnect/rehome slice and before 0.1. The core per-output scale prerequisite (core scales resize minima, keyboard steps and group-edge zones per output) moves to the Windows port start; KDE and macOS use scale 1. The macOS and Windows ports may run in parallel once the core is judged stable with low risk of breaking changes. Read-only contract refreshes and KDE-local R21 effect steps may overlap.
- Amended start and acceptance boundary (User 2026-11-10): KDE offline completion permits macOS start while ST/RC work continues. macOS slice 5 (state targets) and slice 7 (physical reconnect/rehome) acceptance is not claimed until both KDE ST/RC pass and resulting core changes are absorbed. The reconnect leg-2 fix and merge decision remain pending before acceptance and 0.1; deferred KDE recovery sub-legs remain open.
- Selection boundary: native selector/readback, view-attempt correlation and focusless terminal proof precede arrival/focus feedback. Focus-bearing terminal completion remains deferred to those later slices, with unchanged core completion and no fabricated acknowledgement/feedback. Open populated-selection bindings must permit later selections, commands, capture/reply flow and ordinary tiling, remain bounded under repeated switches, and expose their exact pending/missing outcome as a documented limitation.
- Positive-close rollout amends pre-release v4 in place: required `foregroundKnown` and nullable `closeFocus` are a coordinated strict breaking shape change, with matching codecs, designated active fixtures and adapters, no version bump or dual-shape fallback. Old strict readers reject new replies until updated. Temporarily broken ports remain migration work, not live release acceptance; immutable historical recordings stay unchanged.
- Principles (User 2026-11-04): core quality and correctness remain the priority; KDE platform work must not cut corners. KDE work tests and validates core ideas and designs on a real host. Any core contract/interface design problem starts a discussion aimed at a sensible solution for the core and all platforms, never a KDE-specific workaround.
- Why: early read-only platform-fit review finds host mismatches without concurrent core edits; one exact cutover prevents dual shapes; keeping post-0.1 protocol bumps few keeps the contract coherent.
- Rejected: parallel platform ports during concurrent core/contract churn (the macOS and Windows ports may run in parallel only once the core is judged stable with low risk of breaking changes; KDE stays first); several separate pre-0.1 bumps.
- Assumes / revisit if: each platform can review the draft read-only and port against the shared fixtures in the recorded order; phase findings return to the draft before approval/merge. The two stage5 production-route tests must stay enabled against genuine v4 producer bytes, with no deferred-proof exception. Preserve the legacy recorded fixtures; the legacy `world_wire_fixtures` generator is removed (never run during v4).
- Status: active, 2026-11-01; amended 2026-11-10
- Evidence: User 2026-11-04, 2026-11-07, 2026-11-08, 2026-11-09 and 2026-11-10; [v3/v4 authority rule](../porting/world-contract.md#contract-authority-rule), [Windows visibility decision](windows.md#state-centric-v4-visibility), [R14 baseline](../changes/windows-fullscreen-exit-baseline.md), [R21](engine.md#kde-effect-rust-migration-sequence-r21).

### v4 world contract phase-0c approval
- Decision: approve the v4 world-contract shape, including G1-G3, K1-K5, W2-W4 and M1-M9 with the W1, M3 and M4 amendments recorded below. G2 adds no removal capability: core `exist:false` is sole removal authority with fresh native vetoes. G3 leaves transient/explicit-tile precedence TBD outside the wire. Implementation is phase 1; v3 remains authoritative until coordinated cutover.
- Why: the sequential platform-fit reviews and exact fixtures establish the target shape without claiming native acceptance.
- Assumes / revisit if: port evidence requires a contract change; the eight OPEN scenario gates and native proof obligations remain open.
- Status: approved, 2026-11-02
- Evidence: User 2026-11-02; [target contract](../porting/world-contract.md), [approval outcome](../changes/v4-migration.md#phase-0c-approval-outcome-2026-11-02).

### Persistent-above float policy
- Decision: W1 option c (union): every float is eligible for persistent above, including qualified automatic, explicit float/sticky and capacity-fallback floats. Workspace-floating-mode floats use the same rule, even with no tiles present. One core-owned acquisition/release/fullscreen-suspension/external-clear latch covers every origin; sticky transitions keep the same ownership. Require above capability and durable preimages. Pre-existing below gets no stacking writes, pre-existing above stays untouched, and fullscreen-game safety wins.
- Why: matches the Tier 1 float-above-tiles layer and existing KDE project-float plus automatic-fixed-float behavior; capacity floats should behave consistently without arbitrary complexity.
- Assumes / revisit if: native support or recovery cannot be proved; relative float order remains host-owned, not a new raise/lower policy.
- Status: approved, 2026-11-02
- Evidence: User 2026-11-02, including capacity follow-up; [stacking rules](../porting/world-contract.md#persistent-stacking), [COSMIC float layer](../spec/reference-outcomes/floating.md#r-flt-01-toggle-float-then-unfloat), [KDE stacking evidence](../spec/reference-outcomes/floating.md#r-flt-12-raise-and-lower-overlapping-floats).

### Facts versus classification
- Decision: native attributes and observed causes, with uncertainty, are facts; classification (tiled/floating/excluded) and all other meaning are core policy. Unknown hints do not block independently proven float membership; hint-dependent admission, placement and its prerequisite clear remain fail-closed without inventing empty constraints.
- Why: settings can reclassify, one implementation and cross-platform consistency.
- Rejected: adapters classify membership and report verdicts as facts.
- Assumes / revisit if: each needed native attribute can be reported neutrally.
- Status: active, 2026-10-30
- Evidence: [review P3, F10, G2](../changes/core-interface-review.md).

### Host capabilities and settings
- Decision: hosts declare hard capabilities/limitations, fixed per connection and named by capability, not platform; core derives behavior. A change starts a new handshake. Settings are separate, revisioned and changeable at any time. Before adding capability, test whether host mechanisms can honor existing core policy. Wire includes one version/capability handshake, startup/change settings message and report envelope with facts, command, attempt feedback and renderer input.
- Why: per-report capability flags invite flips; platform-named opt-ins became behavior profiles.
- Rejected: per-report capabilities and platform behavior opt-ins.
- Assumes / revisit if: no hard capability changes often while running; if one does, report it as a fact. Output-shared workspace is capability example, not KDE option.
- Status: active, 2026-10-30
- Evidence: [review F1, F12, F19, P4](../changes/core-interface-review.md).

### Native-only retirement intent
- Decision: For the existing host-owned native-desktop capability, support native desktops shared across outputs while core keeps per-output rings, as well as output-local native desktops. Core remains the sole native-removal authority and continues publishing existing `exist:false` rows for locally retired output/native pairs until deletion or positive output-local re-adoption. The removed logical ID is absence-record correlation only, not a live binding outside the ring. No new wire field or format is required.
- Decision: Global occupancy does not prove local arrival. Local native selection or observed window presence clears the local marker and ordinary adoption assigns a fresh logical ID at native order among live ring rows. Adapters apply fresh global occupant, any-output visibility, protection, completeness and minimum-count vetoes; never delete autonomously or from omission. Native minimum padding/bootstrap and ordinal renumbering belong to the adapter, not core policy.
- Why: Independent logical pruning must not leak native desktops or misinterpret another output's occupants, and the same contract must fit hosts with shared or output-local native desktop identities.
- Assumes / revisit if: Existing wire rows express native-only absence without counting as live ring/spare/occupancy rows. Displaced/held units and settled migration-release adoption remain protected. Generic fake-host and adapter pipeline regressions plus native verification are required; implementation acceptance is incomplete.
- Status: approved, User 2026-11-10.
- Evidence: [contract amendment](../porting/world-contract.md#native-only-retirement-with-per-output-rings); [implementation record](../changes/kde-dynamic-workspaces.md).

### Broker-mediated v4 connection
- Decision: the KDE producer lifetime is the sole v4 `adapter`. KWin sends the real handshake through authenticated `DiscoverWorld`; the broker forwards it through `route_connection_request`, and core binds there. Reports carry that producer lifetime, not a broker incarnation. The same route dispatches separate handshake and settings messages by `type`; only handshake resets, while settings retain their independent revision and reply rules. Broker sequencing, replay, queue ownership and requester-only results remain broker-owned.
- Why: one connection identity and one connection-family route avoid redundant fencing and temporary translation paths.
- Rejected: broker incarnation, broker-synthesised handshake, separate offline seam, dedicated settings method and capture-attached settings.
- Assumes / revisit if: KDE supports processId with fixed own token `kwin-internal`, maps native window pid exactly -1 to that token and positive client PIDs to decimal Id text; zero/other non-positive/unreadable values remain null. Internal windows inherit KWin's -1 sentinel, so resolving the compositor's OS PID would match none, while declaring processId unavailable would lose own-process exclusion. The broker only forwards identity. Native candidate filtering must remain separate from own-process exclusion; other internal surfaces and native unreadable/zero-PID outcomes retain their recorded test obligations.
- Status: approved, 2026-11-06
- Evidence: [connection discriminator](../spec/reference-outcomes.md#v4-broker-connection-discriminator), [KDE port obligations](../porting/kde-port-notes.md#v4-connection-and-recovery).

### Fresh handshake and wait recovery
- Decision: every handshake reaches core and starts fresh, including the same producer lifetime. Remove same-lifetime broker rebind. KWin handshakes only when unbound or after `generation-mismatch`; a bound host-deadline timeout only re-arms the wait. Wait rejects stale generation before producer identity or supersession so a restarted service triggers reconnect.
- Why: core handshake is the sole lifetime reset, and an idle watchdog timeout is not a new connection.
- Rejected: same-lifetime rediscovery without reset and resetting on every bound wait timeout.
- Assumes / revisit if: the KDE port must prove restart/watchdog recovery live; no offline result establishes native recovery.
- Status: approved, 2026-11-06
- Evidence: [KDE recovery obligation](../porting/kde-port-notes.md#v4-connection-and-recovery), [earlier watchdog stalls](../changes/kde-world-v2-port.md).

### Own-process UI and ordinary Settings
- Decision: Settings is an ordinary window on every platform, using normal resizable/fixed-size/dialog classification. Only the tiler's own native process UI is identity-excluded; another process with the same executable or bundle is not. Compare nullable window processId with fixed handshake applicationIdentity.processId, under unavailableFacts semantics; no platform selector or Settings-class predicate. Generic-dialog and generic-XAML rules stay unchanged.
- Why: Settings is a complex window users may resize; Tier 1 WMs classify ordinary application windows, not special built-in Settings surfaces.
- Rejected: the Windows Settings-class exemption of 2026-10-16 (ordinary-member and sends outcomes remain); M1's bundle-or-executable self exclusion, amended with W4 to own-process identity only.
- Assumes / revisit if: Windows Settings becomes resizable and macOS Settings becomes distinguishable from own-process UI through a separate process or neutral marker; both are separate product follow-ups, not phase 1.
- Status: approved, 2026-11-02
- Evidence: User 2026-11-02; [process contract](../porting/world-contract.md#process-identity-and-own-process-exclusion), [fixed-size Tier 1 behavior](../spec/reference-outcomes/special-windows.md#r-spc-08-gain-or-lose-fixed-hints-after-admission), [resolved discriminator](../spec/reference-outcomes.md#raw-self-identity-applicability).

### Transient side effects
- Decision: transient side effects of desired-state application are not world changes. Platforms avoid them where possible and distinguish them by causal evidence, never timing; safety checks use live host state.
- Why: intentional outside changes must reach core promptly, including during desired-state application.
- Assumes / revisit if: hosts expose causal evidence; otherwise document and truthfully degrade.
- Status: active, 2026-10-30
- Evidence: [Ordering and Races](../principles.md#ordering-and-races).

### Command payloads
- Decision: commands carry only user choice or input-carried data (stable identities/live coordinates); core resolves against retained state at execution. Adapters may refuse for live-state safety and report refusal as fact, never substitute an outcome.
- Why: adapter vetoes re-deciding resolved outcomes split policy.
- Assumes / revisit if: every safety refusal can be expressed as a reported fact.
- Status: active, 2026-10-30
- Evidence: [review P6, F10](../changes/core-interface-review.md).

### Intent ownership and persistence
- Decision: core alone owns user intent, defines/validates persisted intent and decides save/restore. Platforms keep the opaque record in host storage (Windows window, KDE session runtime file, macOS where possible) and return unchanged. Facts contain observable state only, absent if unknown; remove adapter intent echoes and retained-frame readback; core keeps last known frame.
- Import boundary: import before first successful admission, preserving pending records through held/unreadable observations; ignore late records after admission and never overwrite newer runtime intent. Adapters return stored intent in or before the first admissible report, whether using native slots or a pre-loaded atomic store. This is ordering, not a new storage capability (User 2026-11-03).
- Why: one implementation and consistent restart behavior; Windows keeps window-lifetime storage without restart identity matching.
- Rejected: core file store keyed by platform identity, losing intent on restart, KDE-only adapter store.
- Assumes / revisit if: each host has a slot surviving tiler restart while window lives; cross-login memory remains a limitation.
- Status: active, 2026-10-30
- Evidence: [review R2, G7](../changes/core-interface-review.md).

### Send source fallback
- Decision: numbered/relative send of a window without retained home resolves source output via action-context chain (foreground, pointer, cache, known-registry order), same as migration, for keyboard and CLI; retain one `SendSource` fallback.
- Why: one intent had route-dependent fallback chains.
- Rejected: cached-live fallback; keeping both chains.
- Assumes / revisit if: action-context output is a sound guess; revisit if keyboard sends land unexpectedly.
- Status: active, 2026-10-30
- Evidence: [review R6, F2](../changes/core-interface-review.md).

### Primary output
- Decision: report one optional primary-output id: host-designated primary display (Windows primary flag, macOS menu-bar display, Plasma priority where readable), absent if unknown. Core fallback is first output in stable order; used for bootstrap adoption, primary-first fallback and displaced-window ties.
- Why: one field had three meanings (first work area, active screen, unset).
- Rejected: per-screen primary flags, active screen as primary, no field.
- Assumes / revisit if: hosts expose designation; otherwise absent. Plasma API readability unchecked.
- Status: active, 2026-10-30
- Evidence: [review R4, F1](../changes/core-interface-review.md).

### Command outcome in reply
- Decision: rejected command is typed outcome of accepted reply (`command_result: Rejected { kind }`); core always incorporates report observation. Adapters have no rejection-recovery path.
- Why: rejection discarded report and caused platform-specific recovery.
- Rejected: adapter observation-only fallback; fixing each adapter under old contract.
- Assumes / revisit if: no rejection kind must withhold observation.
- Status: active, 2026-10-30
- Evidence: [review R3, F7, F18](../changes/core-interface-review.md).

### Decorations
- Decision: typed named decoration fields (border, underlay, pager; tabs own field at tab bump); one presentation revision plus foreground token and common staleness rule. Input names the exact core target and rendered revision. A changed target row means changed identity/home: workspace on output, exact window lifetime in workspace on output, or exact group in workspace on output. Ordinal, trailing, binding, label, occupied and group members are not compared; no rendered-row history is retained. A revision above the latest published presentation revision refuses with `stale-render-target`; every published revision, however old, proceeds to the identity/home check. Refusal happens at command handling, not as a publication hold, and selected commands keep their normal gates. Group ids draw from one generation-scoped monotonic counter; trial and rolled-back placements burn numbers and removed ids are never minted again within that generation. Adapters only map hit position to target.
- Why: adapters used different freshness rules for same pager click.
- Rejected: generic decoration list, per-field freshness, adapter click verification, refusal on any revision change, payload-field comparison, rendered-row history, ignoring the revision, a new diagnostic/token or publication hold, retired-id sets or accepted group-id reuse.
- Assumes / revisit if: every target has stable core id (including generation-monotonic group ids) and renderer carries rendered revision; the generation envelope fence remains required when counters reset on a new generation.
- Status: active, 2026-10-30
- Evidence: [review R8, F3, F16](../changes/core-interface-review.md).

### Size units and scaling (R10)
- Decision: each screen fact carries output scale in adapter units per logical pixel (Windows DPI/96, KDE 1, macOS 1). Settings/constants are logical pixels; core scales gaps, border/underlay, drop zones, resize minimums and keyboard steps per output. Adapters pass unscaled styles. Scale arrives every screen report; core retains none and replans on change.
- Why: settings/constants meant physical pixels on Windows but logical pixels on KDE/macOS; Windows gaps ignored scaling while border followed window DPI.
- Rejected: adapter-unit declaration with Windows gap scaling; status quo/comment-only.
- Assumes / revisit if: each host reports scale/resolution changes (Windows display/DPI/settings events, KDE `screensChanged`, macOS screen-parameter notification); Windows DPI transition rebase preserves core target.
- Status: active, 2026-10-31
- Evidence: [review R10, F21](../changes/core-interface-review.md).

### Output identity (R11)
- Decision: reconnect identity is physical display manufacturer/model/serial; adapters report those and connector as facts, core derives/matches identity. Connector is fallback if identity unreadable or duplicates.
- Why: platform-specific GDI/connector/CG ids made reconnect differ; user means monitor, not port.
- Rejected: connector identity and per-platform keys.
- Assumes / revisit if: hosts expose display identity; connector fallback gap documented; session-local, no restart-persistent map.
- Status: active, 2026-10-31
- Evidence: [review R11, F22](../changes/core-interface-review.md).

### Native writes difficult to undo (R12)
- Decision: before hiding, stripping fullscreen chrome, setting project keep-above or sticky stacking preimage, adapter records original state in host slot outliving tiler; next start restores leftovers. Windows watcher also restores stripped chrome. Advertise per-window suppression only when this rule holds; untouched native state remains untouched.
- KDE native-preimage boundary: a readable file whose bus id or KWin owner differs from the current namespace yields a usable empty seed, exactly like a missing file; none of its old UUIDs authorizes restoration. The first persist atomically replaces that stale file. Corrupt and unreadable files retain their blocking, degraded behavior. A changed session or KWin instance has no surviving windows described by the old namespace (User 2026-11-09).
- Why: only Windows hides were crash-safe; fullscreen chrome and KDE keep-above could be left behind.
- Rejected: rule without Windows watcher extension; no rule and known limitations.
- Assumes / revisit if: host has pre-write slot readable next start. KWin unload without later start leaves state until next start.
- Status: active, 2026-10-31; KDE native-preimage namespace amendment approved, 2026-11-09.
- Evidence: [review R12, F23](../changes/core-interface-review.md); User 2026-11-09; [namespace correction](../changes/archive/kde-native-preimage-namespace.md).

### Window-state commands (R14)
- Decision: maximize/fullscreen/sticky are core world commands on every platform; fullscreen/sticky on/off/toggle, maximize via `ToggleMaximize`. Core owns refusal and sticky-off result; adapters apply authorized native targets and report facts, never own maximize policy. `maximize:true` keeps the existing native overlay route; `maximize:false` with available frame writes selects a core-composed work-area overlay, generalizing the nativeSticky/composed-sticky split. Core retains the base tile allocation or float placement, emits frame authority but no maximized StateTarget, and requires native maximized=null on that route (non-null holds the conflicting row). Fullscreen fact carries ownership and exit capability so neutral exit only targets tiler-exitable fullscreen. Fullscreen-on from maximized enters immediately, preserving maximize underneath; ordinary exit returns to the latest requested maximize state under the [pending-target hold rule](window-state.md#fullscreen-and-maximize-overlays). For settled maximize, the native route issues no unmaximize on entry or extra maximize on exit (host retention remains TBD); composed route retains its owned bit and recomputes the work-area frame on exit. Explicit R-MAX-11 replacement remains separate. Bindings unchanged.
- Why: behavior/refusal was split among KDE and Windows; boolean fullscreen fact could not route neutral exit.
- Rejected: adapter toggles with facts only; defer beyond 0.1.
- Assumes / revisit if: macOS maximize/fullscreen remain in 0.1 scope; sticky unsupported until all-Spaces route proven. M4 is resolved to core-composed maximize, not native Window-menu Zoom: user move/resize clears the overlay; Zoom is ordinary frame drift. Orderly restart drops composed maximize to base without extending `v/float/stickyPrior` or depending on M3. Born-maximized is undetectable and admits ordinarily. [Overlay interactions](window-state.md#fullscreen-and-maximize-overlays) preserve fullscreen precedence and existing exitability. Reassess after the full contract rearchitecture and platform ports if jank/issues appear. Sticky-off preserves prior float/tile status; float-on sticky always tiles; both land current workspace.
- Status: active, 2026-10-31; M4 composed-maximize amendment approved, 2026-11-03.
- Evidence: [review R14, F25](../changes/core-interface-review.md); User 2026-11-03; [M4](../changes/v4-migration.md#core-composed-maximize-amendment); Tier 1 [R-MAX-01](../spec/reference-outcomes/maximize-fullscreen.md#r-max-01-maximize-then-restore) (COSMIC/Hyprland retained-slot overlays; i3/sway no counterpart).

### Scripting command surface (R18)
- Decision: 0.1 has one experimental, non-stable scripting surface on KDE/Windows/macOS, using core `WorldCommand` wire names and core path only. KDE Planner queue, Windows command files renamed to shared verbs, macOS new same-user channel. Probes/native inspection remain platform-specific developer tools.
- Why: one script should work everywhere; existing vocabularies/channels differ.
- Rejected: omit macOS until after 0.1; no public 0.1 surface.
- Assumes / revisit if: same-user trust model holds; stability declaration freezes names for future protocol bumps.
- Status: active, 2026-10-31.
- Evidence: [review R18, F29](../changes/core-interface-review.md).

### Anchored border (R20)
- Decision: v4 border is anchored descriptor (subject lifetime, core outsets, width/radius, eligibility, causal freshness witness), not absolute bounds. Adapters apply to live frame via existing native events, no polling. One core rule replaces KDE hide-during-motion and Windows/macOS trailing; group underlay remains group-defined absolute. `width` and `radius` are the border settings scaled once by the foreground output scale with the existing nearest, halves-upward rule; `outsets` is `[g+w, g+w, g+w, g+w]` from the scaled border gap plus width. An unknown or absent witness, or a witness whose subject or foreground does not join the observed foreground, hides the border with empty reason and null subject, outsets, width, radius and witness under the existing carried hidden geometry-veto spelling; no hide token is added.
- Why: adapter motion behavior was janky; an anchored descriptor avoids replaying stale absolute bounds.
- Rejected: wait for macOS drag evidence and retain absolute bounds in v4; outsets equal to scaled width only; a new hide reason for unknown witness.
- Assumes / revisit if: each native frame event arrives during motion; macOS drag evidence measures smoothness only. Similar shape may fit future group tab strip. K5 native witness join remains OPEN.
- Status: active, 2026-10-31
- Evidence: [review R20, F31](../changes/core-interface-review.md), [offline comparison](../changes/kde-world-v2-port.md#moving-border-offline-comparison-2026-10-28).

- Automatic focus guards live only in [focus.md](focus.md#automatic-focus-guards).

- Anti-cheat mechanism limits live only in [gaming.md](gaming.md#anti-cheat-mechanism-limits).

### Core birth-clear ownership
- Decision: birth/maximize/fullscreen/fixed-size gates integrate into WorldEngine admission/mode/lifetime; commandless birth/exit clear uses `clear_operation` and raw `ClearAttempt`; setter acceptance is not completion. No adapter logical birth state or broker clear-authorization lane.
- Why: not recorded.
- Assumes / revisit if: Windows born outcomes remain identical and preamble-only refusal gates/asynchronous restore are not bypassed. Birth-clear authority under geometry hold: only independently proven birth clear under existing policy may proceed; never bypass fullscreen/protected/refused/unmanaged/game holds, hold geometry writes for fresh facts, settle only by fresh native evidence and raw ClearAttempt.
- Status: active, 2026-11-09
- Evidence: [KDE N6 outcome](../changes/kde-world-v2-port.md#n6-core-intents-outcome-and-blocker-2026-10-26).

### Stage 5 package review decisions
- Accepted Stage 6/7 package choices: [canonical decision](world-rendering.md#other-accepted-kde-package-decisions).
- Tentative N5 explicit-tile precedence: [canonical decision](world-rendering.md#membership-and-immediate-eligibility).

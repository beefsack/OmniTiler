# Workspaces

### KDE workspace-policy authority
- Decision: KDE Planner is sole workspace-policy authority for scoped rings/history, trailing lifecycle, tiling modes, selection, sends, mapping, hotplug displacement and return. There is no silent TypeScript fallback; unavailable Planner refuses policy-changing requests narrowly while observation, safety and observed host state remain usable. Effect remains renderer.
- Why: not recorded.
- Assumes / revisit if: native observation/host safety stays usable without Planner.
- Status: active, 2026-10-16
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md), [world model](../changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24).

### Transfer and view settlement
- Decision: Stable-identity targets remain outstanding until observed confirmation, newer external input or proven native refusal. Arm possibly-executed feedback before dispatch; hold ambiguous causality; never repeat possibly executed transfer without fresh unambiguous proof. Setter return is not confirmation. Focus/raise/stacking are at-most-once attempts; report partial multi-window application honestly. Follow requires arrival proof before switch and switch proof before focus.
- Why: not recorded.
- Assumes / revisit if: this supersedes strictly one-shot settlement, not safety fences.
- Status: active, 2026-10-16
- Evidence: [full-state replies](../changes/archive/core-full-state-replies.md).

### Idle external native selection
- Decision: When complete, known output-local native evidence reports a selection outside core and no retained core view or migration transfer owns that output's selection, adopt the observed selection into the retained view and ordinary scoped history. Never reassert the former core selection against that native switch. Unknown or incomplete selection evidence adopts nothing; existing write/deletion holds and in-flight transfer authority remain unchanged. Shared views retain their common mirrored selection; separate modes adopt only within the output's ring.
- Decision: If lifecycle removes an invalid retained view, prefer a complete known native current in the surviving ring over the ring head when no transfer owns selection. Do not publish a known-unshown fallback without an owning transfer. With no known echo, preserve existing unknown-evidence fallback and holds. Positive local selection of a locally retired native still clears its marker and adopts a fresh logical identity before view reconciliation.
- Why: The next host report is reality for core to continue from, not an instruction to undo native user input. A retained/published view must not disagree indefinitely with known native selection while no effect owns convergence.
- Assumes / revisit if: No new wire shape or native-removal authority. Shown empty desktops remain while shown and become ordinary prune candidates after leaving; the shown-native deletion veto stays.
- Status: active, User 2026-11-11.
- Evidence: [native-selection discriminator](../spec/reference-outcomes.md#idle-external-native-selection); [D2 correction](../changes/kde-dynamic-workspaces.md#idle-native-selection-correction-2026-11-11).

### Focusless switch completion
- Decision: A selection with no required focus target reports `focus:"unproven"` and `outcome:"complete"` once all visibility obligations are satisfied, with `failed:[]` when nothing failed. The same rule covers empty-to-empty selections with zero visibility members and native-owned focusless selections. An explicit selection whose resolved focus target fails route eligibility freezes a binding with no required focus target; visibility completes through the common path and focus reports `unproven`. Under the approved retained-focus target, independent focus authority uses the one global request, never a parallel direct focus effect; eligibility gates are unchanged. Focus-bearing switches still require proven focus for complete; empty/minimized-only workspaces retain no focused client.
- Why: use the existing completion rule without unnecessary branching or new wire vocabulary.
- Assumes / revisit if: no fake focus, fallback, retry, second completion path, new token or fixture change. Unsupported native terminal outcomes remain TBD.
- Status: active, 2026-11-04; retained-focus delivery amendment approved 2026-11-11, not yet implemented
- Evidence: [contract](../porting/world-contract.md#visibility-and-focus-ordering), [discriminator](../spec/reference-outcomes.md#empty-workspace-focus-across-outputs).

### Selection visibility accounting
- Decision: Every frozen visibility member needs its own correlated LastWrite verdict before switch completion. Arrival and proven focus do not substitute for a missing show verdict; that member remains pending and no SwitchEvent is emitted. Supplying the matching show evidence permits the common completion predicate to proceed, without a special arrival-as-accounting path.
- Why: arrival readiness and visibility-write accounting prove different obligations; one per-member rule covers shows and hides.
- Assumes / revisit if: native delivery and equivalent reference-WM receipt protocols remain TBD.
- Status: active, 2026-11-06
- Evidence: [discriminator](../spec/reference-outcomes.md#selection-accounting-without-logical-echoes), [contract](../porting/world-contract.md#normative-apply-rule).

### Capacity hold and best-effort policy
- Decision: Preserve topology when projection cannot allocate positive extents after gaps, using narrow recoverable `constraint-clamp` hold with complete held-lifetime rows; unaffected domains continue. Keep classification in one replaceable core policy location. No admission fallback is implied by this hold; see [layout and admission](layout-and-admission.md).
- Why: not recorded.
- Assumes / revisit if: geometric best-effort admission remains separately governed.
- Status: active, 2026-10-16
- Evidence: [functional spec](../spec/functional-spec.md).

### Workspace scope vocabulary
- Decision: `shared` means linked views, all displays show/switch same workspace (COSMIC Global). `per-output-local` and `global-unique` are separate views (COSMIC OutputBound); `equal_workspace_count` is a host limitation with equal lengths but independent views, never "shared". KDE default is per-output-local; separate modes require per-output desktop setting; warn if disabled, no warning for shared. Native global list can require empty padding; do not add lifecycle beyond existing rules.
- V4 workspace reply rows are unique by `(output,id)`: one logical id may repeat across outputs, but a repeated pair refuses `duplicate-workspace` in both codecs. Preserve shared-ring identity without producer relabeling/deduplication or a fixture change.
- Why: not recorded.
- Assumes / revisit if: all three modes remain valid across platforms; no new setting. Windows stored `shared` token remains unchanged.
- Status: active, 2026-10-24
- Evidence: [workspace mode outcome](../changes/kde-world-v2-port.md#stage-7-workspace-modes-outcome-2026-10-28), [row-identity discriminator](../spec/reference-outcomes.md#shared-workspace-reply-identity); row identity approved 2026-11-06, external reference/native counterparts TBD.

### Separate-view labels
- Decision: Use `Displays have separate workspaces - own set per display` for per-output-local and `Displays have separate workspaces - swap across displays` for global-unique; `shared` remains `Workspaces span displays`. KDE adopts these labels; Windows distinct existing labels need no follow-up; macOS unchanged.
- Why: not recorded.
- Assumes / revisit if: Windows exposes both modes and adopts same labels then.
- Status: active, 2026-10-29
- Evidence: [Stage 7 workspace modes outcome](../changes/kde-world-v2-port.md#stage-7-workspace-modes-outcome-2026-10-28), [label approval](../changes/kde-world-v2-port.md#user-physical-checks-2026-10-28).

### Workspace scope persistence and hotplug
- Decision: Windows item 12 stores schema-v1 `core.workspace.mode` using per-output-local default, global-unique, shared; missing backfills local, invalid refuses. Settings has one three-choice selector. Apply saves immediately but running owner adopts only on restart and reports restart-required drift. Global-unique preserves scoped numbering and KDE visible-elsewhere swap rule, with no new addressing; a saved KDE `global-unique` mode does not fall back to per-output-local. Local/global-unique retain per-output history/rings; shared has one. Shared hotplug mirrors KDE without project displacement/return mapping; disconnected output history is discarded while shared history remains. Startup shared view uses first live output in stable key order, converges other output-bound domains and primes one history without recording startup. Whole-workspace migration refuses shared before writes or native settings mutation.
- Why: not recorded.
- Assumes / revisit if: primary/focused-output startup alternatives remain TBD; shared mode refuses whole-workspace migration before writes.
- Status: active, 2026-10-15
- Evidence: [scope policy](../changes/archive/windows-workspace-scope-policy.md), [hotplug](../changes/archive/windows-workspace-scope-hotplug.md).

### Workspace sends and floating state
- Decision: Sending intentional floats preserves floating state and size, destination tiles untouched, for follow/stay and numbered/relative sends. Same-output preserves exact frame without geometry write; explicit floating+fullscreen/maximized sends carry exact state and return to floating frame after leaving overlay. Sticky refuses. Automatic fixed-size float sends are slotless on every platform, with no admission opt-in, replacement capability or setting. Windows ordinary members born floating/fixed, maximized/fullscreen remain sendable with state preserved; separate-process Settings dialog is ordinary member, hides when workspace not shown, sends without tiling. Same-process owner/control, proof and generic owned-dialog fences remain.
- Why: Tier 1 Hyprland/i3/sway plus Tier 2 xmonad/qtile/awesome retain float against COSMIC fresh re-tile; COSMIC's alternative has no second Tier 1/2 support, so no setting.
- Assumes / revisit if: fullscreen carry is explicit-only; no unsolicited geometry/state writes. Native roundtrip/boundary acceptance remains pending.
- Status: active, 2026-10-30; platform opt-in removal approved 2026-11-12, implemented offline; native acceptance pending.
- Evidence: [floating transfer](../changes/archive/d16-floating-transfer.md), [Windows sends](../changes/windows-all-member-workspace-sends.md), [review R5](../changes/core-interface-review.md), [shared policy implementation](../changes/core-platform-switch-evidence.md#implementation-outcome).

### Workspace lifecycle, focus and history
- Decision: Closing sole window retains shown empty workspace without focused client; workspace return restores remembered focus when verified. Showing occupied workspace without valid remembered focus chooses MRU eligible member, else first eligible by deterministic layout child order; empty/minimized-only retains no focus. Host-caused focus loss is not repaired. Directional focus without focused window uses existing topology and destination remembered tiled selection in all four directions to cross to an adjacent output, else no-op; it adds no current-workspace candidate, local-window, MRU/layout fallback or edge-nearest policy. A shown workspace may remain unfocused.
- Sole positive close also retains source focused-output intent, including with another occupied output/workspace; no other-domain activation or native clear-focus is issued. Readable survivors reflow independently of observed focus or activation eligibility. The bounded [positive-close correction](focus.md#automatic-focus-guards) is close-caused policy, not repair of unrelated host focus loss; native desktop/app foreground stays truthful.
- Why: not recorded.
- Assumes / revisit if: candidate eligibility and fullscreen/game gates unchanged; COSMIC directional outcome remains TBD; focus safety remains authoritative.
- Status: active, 2026-10-26
- Evidence: [reference discriminators](../spec/reference-outcomes.md#workspace-no-focus-discriminators), [directional focus](../changes/archive/directional-focus-without-window.md).

### Workspace target insertion
- Decision: Returning workspace B inserts after remembered target anchor A; no before/after setting.
- Why: not recorded.
- Assumes / revisit if: target anchor semantics remain independent of destination migration insertion.
- Status: active, 2026-10-10
- Evidence: [R-WS-02 return-anchor discriminator](../spec/reference-outcomes/workspaces.md#r-ws-02-backfill-send-back-and-return-anchor-scrolling).

### Windows asynchronous arrival focus
- Decision: Windows remembers accepted asynchronous focus target's exact native identity and membership lifetime as in-flight, without blocking wait/timer/timeout. Foreground WinEvents clear on landing or another readable foreground superseding; target lifetime end, newer accepted setter and generation reset clear/replace. During later nonactivating switch, foreground change to exact in-flight identity is prior landing, not supersession; all others veto. Simultaneous user click on same target is indistinguishable and accepted as ours; unreadable identity never proves landing. Core chooses target; fullscreen/game/lifetime/at-most-once gates remain, no retry/replay.
- Why: not recorded.
- Assumes / revisit if: Replaces tentative queue-completion approach. Fixture bursts/native foreground/pager/genuine supersession were verified; physical acceptance remains distinct.
- Status: active, 2026-10-26
- Evidence: [arrival convergence](../changes/archive/workspace-arrival-focus-convergence.md).

### Windows nonactivating focus application and rejection
- Decision: Remembered-focus view application uses full foreground-identity bracketing, including exact in-flight landing, and same-pass hidden foreground facts; retain existing scope/lifetime/safety/at-most-once gates. Command rejection/catchup-required refusal originally services owed observation-only catchup same turn without queue/replay; from v4 rejection needs no catchup because rejecting reply incorporates observation.
- Why: not recorded.
- Assumes / revisit if: no command queuing/replay; no change to focus safety.
- Status: active, 2026-10-26
- Evidence: [focus restore](../changes/archive/windows-workspace-focus-restore.md#alternative-native-mechanism-2026-10-25), [reply behavior](../changes/archive/windows-directional-focus-startup.md#evidence-and-decisions).

### KDE workspace tiling default adoption
- Decision: KDE tray writes only `defaultTiled` and requests KWin reconfigure; only running-KWin reread/snapshot confirms. Per-workspace overrides are session-only and reset on script reload; shared mode toggles backing workspace across displays. KGlobalAccel workspace-tiling action invoked by tray does not confirm application. No per-workspace history persists.
- Why: not recorded.
- Assumes / revisit if: persistence remains only for saved default, not override/history.
- Status: active, 2026-10-10
- Evidence: [workspace tiling](../changes/archive/windows-workspace-tiling.md).

### Per-workspace tiling, floating and bindings
- Decision: Tiled/floating mode is session-local per managed workspace; startup seeds saved default true, live default changes affect subsequently created workspaces only, overrides reset at owner restart, no binding for default itself. Floating preserves native frame, membership and hide/reveal while disabling domain tiling/navigation/group underlay/drop preview; active border is independent. Retile releases Engine domain without writes, then clean/tolerance-valid recursive fit or sequential long-edge seed, no center inference; preserve exact-lifetime float intent and cancel stale gesture effects on mode flip.
- Floating-mode occupants, including minimized occupants, publish `floating:true` with empty `float_origin`. This is scope classification, not per-window float intent or write authority; existing minimized holds remain. Full conversion into retained per-window floats is not selected.
- Why: removes the "not tiled but not floating" classification and aligns the W1 float union with COSMIC's workspace-floating conversion without adding per-window intent.
- Assumes / revisit if: COSMIC matches for unpinned workspaces; pinned/persistent workspaces are future scope grouped with shared restart store. Intentional float/sticky and maximize/fullscreen exceptions unchanged.
- Status: active, 2026-10-10
- Evidence: [workspace decisions](../spec/functional-spec.md#workspaces), [classification discriminator](../spec/reference-outcomes.md#workspace-floating-classification-diagnostic); classification approved 2026-11-06. Exact native/reference diagnostic counterparts remain TBD.

### Workspace selection keys
- Decision: Meta/Win+1..9 selects an existing workspace without creation; 0 reuses or creates trailing empty. Register US shifted aliases (`Meta+!` through `Meta+)`, and Win equivalents, alongside digit sends while preserving foreign shortcut records.
- Why: not recorded.
- Assumes / revisit if: shortcut registration does not establish physical delivery.
- Status: active, 2026-10-16
- Evidence: [follow/stay](../changes/archive/windows-workspace-send-follow-stay.md).

### Workspace send follow settlement
- Decision: Send follows by default; bindable send-and-stay counterpart is unbound. Relative sends resolve once against scoped ordinal ring before transfer. Exact follow needs fresh mover-absent-source/present-target proof, then switch-before-focus; unrelated async geometry/layout settling does not gate confirmed follow. KWin geometry and membership are non-atomic; setter returns and signal delivery alone are not proof. Cross-output follow-send and same-output follow-send take the same frozen selection binding as an ordinary explicit selection. Window membership settles the transfer independently; the binding owns destination visibility and focus through the existing selection completion. No follow-specific predicate or second completion path. The `{op}-follow` child is removed; under the approved retained-focus target, the one global request carries frozen selection dependencies and waits until the destination is shown. Superseding focus does not cancel unrelated member/visibility obligations; exact `afterShown` schema remains pending. Existing refusal and safety fences are unchanged.
- Why: not recorded.
- Assumes / revisit if: stale, ambiguous, missing, no-op, wrong-target, owner, scope and hook failures do not follow; no retry/replay. Partial successes are reported honestly; rapid-use preference authorizes no ignored errors, retries, queue resets or uncertain-recovery change.
- Status: active, 2026-10-16; retained-focus delivery amendment approved 2026-11-11, not yet implemented
- Evidence: [follow/stay](../changes/archive/windows-workspace-send-follow-stay.md), [floating transfer](../changes/archive/d16-floating-transfer.md), [retained-focus design](../changes/retained-focus-requests.md).

### Workspace history and scoped rings
- Decision: Local/global-unique keep per-output history/rings; shared keeps one. Record every successful observed workspace change regardless of producer, not same-workspace activation or merely focusing another output. Previous is a stable-ID two-view toggle, not MRU; surviving empty IDs remain valid, removed/unassigned/out-of-scope IDs clear with no ordinal reinterpretation/recreation. Ring is every existing scoped workspace including trailing empty and >9; first/last wrap; selection does not create. Reconnect selection never restores history; hotplug observations record normally; disconnect discards that output history and output identity is session-local.
- Why: not recorded.
- Assumes / revisit if: G-04 deliberately keeps per-output history rather than majority global-history models; COSMIC has no history verb. No setting for stable-ID retention or hotplug rules.
- Status: active, 2026-10-10
- Evidence: [workspace history](../changes/archive/windows-workspace-history-ring.md).

### Workspace history shortcut mappings
- Decision: Previous-view is Meta+Ctrl+Tab on KDE / Win+Ctrl+Tab on Windows. Previous/next workspace use Meta/Win+Ctrl+arrows and Meta/Win+Ctrl+H/J/K/L: left/up previous, right/down next. Authentic takes over KDE stock KWin arrow actions and Windows Win+Ctrl+Left/Right; Compatible disables conflicting arrow forms, while letter forms remain. History is the source policy above, not MRU traversal.
- Why: not recorded.
- Assumes / revisit if: Windows native holder mapping is general knowledge and unverified in repository; physical/preset OS journeys remain pending.
- Status: active, 2026-10-10
- Evidence: [workspace history](../changes/archive/windows-workspace-history-ring.md).

### Workspace send shortcut mappings
- Decision: Numbered follow uses Meta/Win+Shift+digits. Relative send-and-follow uses Meta/Win+Ctrl+Shift+arrows and +H/J/K/L. Numbered and relative send-and-stay are bindable, unbound by default. KDE Authentic clears stock `Window One Desktop Down/Up/to the Left/to the Right` arrow holders; Compatible disables our arrows; letters remain. Windows Shift-arrow ownership is unknown. Windows movement/send repeats while modifiers held; toggles remain one action per press.
- Why: not recorded.
- Assumes / revisit if: Windows output-send held repeat uses original modifier/Shift polarity; exact stay Shift distinction remains only as currently approved. Physical containment remains pending.
- Status: active, 2026-10-14
- Evidence: [send follow/stay](../changes/archive/windows-workspace-send-follow-stay.md), [held-key discriminator](../spec/reference-outcomes.md#windows-output-send-held-key-discriminator).

### Workspace migration D1: bindings and scope
- Decision: Four directional active-workspace migration actions are bindable and unbound by default, follow-only; presets keep them empty and claim no foreign conflict. Strictly require `options.perOutputVirtualDesktops === true` and local/global-unique scope. Shared, false, or unreadable capability refuses with reason before any write; never mutate native settings.
- Why: not recorded.
- Assumes / revisit if: no capability fallback is selected.
- Status: active, 2026-10-08
- Evidence: [REQ-WS-12](../spec/functional-spec.md#workspaces).

### Workspace migration D2-D3: target selection
- Decision: Migrate only active workspace. Candidate output needs reciprocal edge-touch and positive overlap across full output rectangles; no output wrap. Unreadable topology refuses; no candidate is a no-op. Select largest shared edge, then ascending left/top tie-break, preserving stable output identity for equal positions.
- Why: not recorded.
- Assumes / revisit if: panel/work-area gaps do not affect adjacency; reference/native unsupported outcomes remain TBD.
- Status: active, 2026-10-09
- Evidence: [output selection](../changes/archive/position-based-output-selection.md).

### Workspace migration D4: destination
- Decision: Retire the aligned trailing-cell destination replacement and its supersession of common insertion.
- Why: The aligned grid is replaced by the [common v4 per-output model](#common-v4-per-output-workspaces).
- Status: retired, 2026-11-08.

### Workspace migration D5: source refill
- Decision: Setting defaults to `last-remaining-workspace`, alternative `most-recently-used-workspace`; KDE `migrationSourceRefill`, Windows `core.migration_source_refill` schema-v1 missing defaults to last-remaining, no migration. Apply affects later migrations. MRU uses per-output history; snapshot previous stable ID before map mutation, eligible only if live, still assigned to source remaining ring and not migrated. Surviving empties qualify; missing/removed/migrated/out-of-scope falls back to last remaining. In both modes preserve a still-scoped live source current. Empty migration allowed, retaining its backing ID.
- Why: not recorded.
- Assumes / revisit if: native lifecycle timing remains user testing.
- Status: active, 2026-10-10
- Evidence: [migration settings](../changes/archive/reference-comparison-implementation.md), [Windows migration](../changes/archive/windows-workspace-output-migration.md).

### Workspace migration D6: focus settlement
- Decision: Retain moved active client only after all member arrivals and both view changes verify. Empty/sticky-active migration uses native output switching without fabricated client activation; minimized clients are not unminimized or explicitly focused. Native-free workspace migration takes one frozen selection binding covering both outputs. Member arrival settles membership; the binding owns target and source-refill visibility and focus through the existing selection completion, and the refill child is removed. Empty migration uses the zero-member completion rule. Native-owned migration keeps its native proof.
- Why: not recorded.
- Assumes / revisit if: no focus before verified arrival/view settlement.
- Status: active, 2026-10-08
- Evidence: [migration contract](../spec/functional-spec.md#workspaces).

### Workspace migration D7: floats and sticky windows
- Decision: Carry workspace-bound intentional/automatic floats with class and origin preserved via native output remap. Sticky all-desktop windows remain on source output, belong to output, and are not migration members.
- Why: not recorded.
- Assumes / revisit if: native carry acceptance remains pending; sticky is not converted to a workspace member.
- Status: active, 2026-10-09
- Evidence: [overlay carry](../changes/archive/migration-overlay-carry.md).

### Workspace migration D8: overlays
- Decision: Carry fullscreen and maximized members, matching Hyprland/sway/i3/niri/COSMIC and native KWin send-to-output. Do not refuse moved members or affected views; issue only native move, no extra size/position/focus writes while fullscreen. Explicit user move, including moving game from wrong output, is not unwanted interference.
- Why: not recorded.
- Assumes / revisit if: physical carry/re-fit/focus checks remain pending.
- Status: active, 2026-10-10
- Evidence: [overlay carry](../changes/archive/migration-overlay-carry.md), [Windows migration](../changes/archive/windows-workspace-output-migration.md).

### Workspace migration D9: history and automatic return
- Decision: Clear previous IDs leaving source scope; discard disconnected-output history and never consult/restore it on reconnect. Remove only explicitly moved ID from automatic hotplug-return associations; migrated IDs lose automatic return.
- Why: not recorded.
- Assumes / revisit if: reconnect selection policy remains unchanged; hotplug-driven observed changes still enter ordinary history.
- Status: active, 2026-10-08
- Evidence: [REQ-WS-12](../spec/functional-spec.md#workspaces).

### KDE aligned workspace lifecycle
- Decision: Retire the full native-position grid, per-output occupied-extent trailing cell and global hole/removal rules.
- Why: Replace platform-specific aligned lifecycle with the [common v4 per-output model](#common-v4-per-output-workspaces).
- Status: retired, 2026-11-08.

### Common v4 per-output workspaces
- Decision: Per-output rings use stable logical workspace identities. Workspace migration inserts the moved logical workspace after destination current, preserving its contents, layout, tiling mode and remembered focus; native backing is resolved separately. Reuse an existing empty final spare: showing/selecting it creates no additional spare. Occupying it supplies the next spare through ordinary lifecycle.
- Why: One common per-output model replaces the aligned-grid special cases.
- Assumes / revisit if: Native backing and physical delivery still require their own evidence; shared mode retains its shared ring.
- Status: active, 2026-11-08.
- Evidence: `world_commands.rs` migration/selection, `workspace_scope.rs::resolve_trailing`, `workspace.rs::plan_trailing`; [selection regressions](../../crates/tiler-core/src/world_selection_await_tests.rs), [workspace policy tests](../../crates/tiler-core/tests/workspace_scope.rs).

### Dynamic workspace lifecycle and KDE pager backing
- Decision: Remove an ordinary empty logical workspace from its ring once no output shows it, including leading and middle empties; retain the trailing empty spare and never prune displaced or held units. Hosts declaring `equalCount` retain equal logical ring lengths as an explicit exception to ordinary-empty pruning; eligible padding prunes when the longest ring shrinks. Occupying the final spare supplies the next spare. Core has no independent minimum-count floor. Each output's trailing spare gets real KDE desktop backing immediately so it appears in the pager.
- Decision: Delete a KDE desktop only when it has no non-sticky windows on any output and no output shows it, preserving conservative protection/readability guards. On-all-desktops windows (panels, docks and user sticky windows) count as no native occupants: deleting a desktop moves none of them. This exclusion applies to both KDE native-occupancy counters, not logical retention: a managed sticky window's retained core home still protects its desktop. Non-sticky windows on a subset of desktops occupy every listed desktop. Never move windows between desktops to compact gaps. Surviving native IDs remain authoritative; KDE ordinal renumbering is acceptable. KDE alone maintains a minimum of two native desktops. In separate mode the shared pager shows the longest list across outputs.
- Why: User: "The pager needs to be consistent with core world state - I think this is critical so the user can know exactly what we are modelling." KDE's global native list already makes every pager show the longest list; equal logical lengths model every listed desktop without extra native desktops. Keep the exception as small and isolated as possible.
- Amendment (User 2026-11-11): For a host declaring the separate `sharedNativeRoster` capability, reconcile padding and retirement by native identity so each surviving pager desktop has a live logical row on each output and globally empty/unshown desktops can retire jointly. Coordinate pure-spare backing so filling the final spare creates one shared native desktop, not one per output. Equal logical lengths remain required under `equalCount`; equal counts alone do not imply a shared native roster. Preserve unknown-occupancy and sibling-live-binding fences, independent rosters, and fresh migration backing. No timers, revision grace periods or occupant relocation for compaction.
- Assumes / revisit if: Logical ring removal and native backing retirement are separate operations. The [implementation note](../changes/kde-dynamic-workspaces.md#implementation-choice-locally-retired-native-markers) records internal locally retired-native markers and core-only native absence authority. Native acceptance remains open. This does not restore the retired aligned grid/global-hole model or change after-current migration insertion/disconnect rules.
- Status: active, User 2026-11-10; amended User 2026-11-11; offline implementation verified, native acceptance open.
- Evidence: [dynamic lifecycle](../changes/kde-dynamic-workspaces.md); [source-only discriminators](../spec/reference-outcomes/workspaces.md#dynamic-lifecycle-discriminators-2026-11-10-source-only).

### KDE common-model output disconnection
<a id="kde-aligned-output-disconnection"></a>
- Decision: On disconnect, preserve displaced layout in separate workspace(s), never merge it into the remaining layout. If the disconnected monitor's active window survives, show its relocated workspace retaining focus; a surviving-monitor active window preserves its view and focus. Commandless policy views bind as focusless selections under their existing `op-dpv-*` parent, with no invented `cmdSeq` or focus effect, and retire through the common completion; native focus stays untouched. Windows rehomed off the disconnected output are never fresh admissions: no automatic-admission focus effect fires and native focus stays untouched, including when they join an existing non-fresh survivor unit (no `op-dpv-*` view is minted there).
- Why: not recorded.
- Assumes / revisit if: Focus continuity deliberately keeps the window in use visible; selection among multiple survivors follows the reconnect rule.
- Status: active, 2026-10-18.
- Evidence: [physical reconnect/rehome](../changes/v4-phase2-kde-physical-reconnect.md#disconnect-rehomed-members-are-not-admissions); earlier [workspace record](../changes/kde-aligned-output-workspaces.md); `world_model.rs` `take_admission_focus`, regression `disconnect_rehomed_members_mint_no_admission_focus`.

### KDE common-model output reconnection
<a id="kde-aligned-output-reconnection"></a>
- Decision: Return displaced workspaces automatically to their original monitor with then-current contents; do not restore a snapshot or pull back individually moved windows. Across overlapping displacement chains, original physical affinity is independent of the current host; an intermediary's return does not reclaim another origin's units. Choose among multiple survivors by nearest disconnect-time geometry, falling back to current primary then output order; never use post-disconnect frame geometry. Show the returning active workspace with focus retained; preserve surviving-output active views/focus without stealing focus. Commandless return-policy views use the same focusless binding under their existing `op-dpv-*` parent, without an invented `cmdSeq` or focus effect, and retire through the common completion with native focus untouched.
- Why: not recorded.
- Assumes / revisit if: Reconnect selection never restores prior-view history; observed hotplug changes enter ordinary history. Spatial proximity is a deliberate COSMIC/six-reference deviation, discriminating only with 3+ outputs. No restart-persistent mapping or return guarantee.
- Status: active, 2026-10-18.
- Evidence: [physical reconnect/rehome](../changes/v4-phase2-kde-physical-reconnect.md), [discriminator](../spec/reference-outcomes.md#kde-dual-output-admission-and-workspace-scope-discriminators).

### Tentative aligned migration D5 source refill
- Decision: Retire the aligned replacement-cell exclusion.
- Why: The [common v4 model](#common-v4-per-output-workspaces) has no replacement cell; [common D5 refill](#workspace-migration-d5-source-refill) applies.
- Status: retired, 2026-11-08.

### Tentative aligned empty-cell and spare lifecycle
- Decision: Retire aligned-cell/global-hole lifecycle refinements; retain existing empty-spare reuse and occupy-to-create-spare in the common model.
- Why: Aligned-only cell policy is replaced by [common v4 lifecycle](#common-v4-per-output-workspaces), not a new selection or sticky policy.
- Status: retired, 2026-11-08.

### Windows owner, workspace storage and recovery
- Decision: Use one normal single per-user/session owner executable with standalone stop/restore. Persist `core.workspace.default_tiled` in schema-v1 Windows settings; older files backfill true. Tray changes only the saved field and settings polling confirms runtime adoption. Recovery ledger v4 stores min/max show state without geometry; readers accept v1-v3. The window-lifetime membership property is inert and distrusted.
- Why: not recorded.
- Assumes / revisit if: multi-output remains separate; physical feel and other output/DPI arrangements remain user-owned.
- Status: active, 2026-10-02
- Evidence: [workspace tiling](../changes/archive/windows-workspace-tiling.md).

### Windows hidden-workspace presentation
- Decision: Managed workspaces use public `SW_HIDE`, nonactivating reveal, and a simple same-executable watcher. Keep `SW_HIDE` for 0.1; investigate only trivial configurability such as off-screen parking after 0.1. Switch-versus-pull remains open.
- Why: not recorded.
- Assumes / revisit if: the user is wary of complexity; add no machinery beyond the recorded simple watcher.
- Status: active, 2026-10-17
- Evidence: [Alt+Tab research](../research/windows-port/alt-tab-hidden-workspaces.md).

### Windows workspace focus and command boundary
- Decision: Workspace focus uses bounded owner-thread E8 prime (unassigned virtual key injected after native safety gates and before one `SetForegroundWindow`, holding last-input foreground right) plus exact foreground readback, without `AttachThreadInput`; Terminal is ordinary. CLI exposes `workspace --select N` / `workspace --send N`, opt-in `--scope-exe` / `--scope-host-child` proof fences, and exact-owner automation send; dispatch acknowledgement is not effect proof.
- Why: not recorded.
- Assumes / revisit if: physical feel and output/DPI arrangements remain user-owned.
- Status: active, 2026-10-26
- Evidence: [focus startup](../changes/archive/windows-directional-focus-startup.md#physical-acceptance-2026-10-26), [workspace tiling](../changes/archive/windows-workspace-tiling.md).

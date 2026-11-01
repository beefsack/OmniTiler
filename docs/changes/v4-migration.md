# v4 migration: contract delta ledger

## Goal, scope and acceptance

- Goal: define v4 as the implemented v3 wire plus approved changes, with exact
  JSON conformance inputs and outputs before codec implementation.
- Scope: normative contract and wire fixtures. Platform-fit review follows the
  draft; implementation and native acceptance are separate work.
- Non-goals: a replacement protocol designed from scratch, tab commands,
  implementation changes, or claims of native conformance.
- Current milestone: milestone 4 complete, ordered independently initialized scenarios.
  Milestone 3 message fixtures and coverage are accepted; canonical number
  spelling is now explicit. Milestone 2 source-checked corrections and the v3
  carry-forward checklist approach remain accepted. Phase-0b KDE source-fit
  review is recorded below. K1-K4 technical resolutions are drafted pending 0c
   approval; their contract/fixture reconciliation is complete. Windows source-fit
  review is recorded below with W1-W4 unresolved; macOS review and phase-0c
  approval remain.
- Acceptance: every current wire record/enum/message has a disposition; every
  delta has an authority reference or a named gap; defaults cite core source;
  preserved shapes keep their current names and representation.
- Approach: resolve the gaps below, draft from this ledger, then check exact
  fixture coverage, JSON validity, links, ASCII and platform fit. Codec execution
  against the new fixtures is a later implementation gate, not phase-0 evidence.

## Reading key and authority

| Short name | Repository source |
| --- | --- |
| WR | `crates/tiler-core/src/world_report.rs` |
| WP | `crates/tiler-core/src/world_presentation.rs` |
| WJ | `crates/tiler-protocol/src/world_protocol.rs` |
| WS | `crates/tiler-core/src/world_sizing.rs` |
| SC | `crates/tiler-core/src/settings_catalog.rs` |
| Review | `docs/changes/core-interface-review.md` |
| Visibility | `docs/changes/windows-workspace-return-focus.md` |

- Citations such as `WR:673-681` expand to the repository file above, not to the
  draft. The codec is handwritten; Rust fields are not automatically JSON keys.
  `WJ:1593-1918` decodes requests; `WJ:1949-2161` encodes replies and delegates
  nested sections to `WS:1441-1808`.
- U = unchanged; C = changed; R = removed; A = added; G = unresolved gap.
  Unchanged means unchanged names, keys, representation and existing semantics,
  subject to the enclosing approved envelope/settings split. A Rust newtype is
  not by itself permission to change its string JSON representation.
- [Baseline](../porting/world-contract-v3.md#current-message-shapes-v3) and
  [cutover authority](../decisions/contract.md#contract-authority-and-v4-migration)
  govern all rows. The main-branch authority-rule paragraph must be preserved
  verbatim, including the working `contract-authority-rule` and
  `approved-not-yet-implemented-v4` anchors.

| Authority | Approved change and source |
| --- | --- |
| Facts | [Facts versus classification](../decisions/contract.md#facts-versus-classification); [classification boundary](../decisions/engine.md#classification-and-convergence-ownership), Review:1145,4773-4778 |
| Split | [Host capabilities/settings](../decisions/contract.md#host-capabilities-and-settings), Review:4746-4765 |
| Intent | [Core intent/storage](../decisions/contract.md#intent-ownership-and-persistence), Review:1145 |
| Primary | [Primary output](../decisions/contract.md#primary-output) |
| Result | [Command outcome](../decisions/contract.md#command-outcome-in-reply) |
| Decor | [Decorations](../decisions/contract.md#decorations), Review:1151 |
| R5 | Review:1148,4771-4796; [fullscreen suspension](../decisions/world-rendering.md#focused-fullscreen-presentation-suspension); [automatic focus guards](../decisions/focus.md#automatic-focus-guards) |
| R6 | Review:1149,4803-4829: typed directions/cause/scope/sequence; remove `SwitchView`/`Operation`; promote three live operations; `HostHidden`; `SetterOutcome`; workspace-reference consolidation |
| R10 | [Scale/logical sizes](../decisions/contract.md#size-units-and-scaling-r10), Review:1153 |
| R11 | [Physical output identity](../decisions/contract.md#output-identity-r11), Review:1154,4818-4829 |
| R12 | [Recoverable preimages](../decisions/contract.md#native-writes-difficult-to-undo-r12); handshake lifetime/generation choice: Review:1155,4754-4758 |
| R13 | Review:1156,4759-4765: per-field settings diagnostics; typed rejection/cause kinds; row-local unreadable holds |
| R14 | [Window-state commands](../decisions/contract.md#window-state-commands-r14), Review:1157; [Windows baseline](windows-fullscreen-exit-baseline.md) |
| R17 | Review:1160; [Testing](../principles.md#testing): exact boundary fixtures and shared acceptance |
| R18 | [Experimental scripting](../decisions/contract.md#scripting-command-surface-r18) |
| R19 | Review:1162,4826-4830; [native backing rules](../decisions/layout-and-admission.md#native-workspace-adoption-and-r19) |
| R20 | [Anchored border](../decisions/contract.md#anchored-border-r20), Review:1163 |
| Visibility | [State-centric visibility](../decisions/windows.md#state-centric-v4-visibility), [retained protocol fences](../decisions/windows.md#accepted-protocol-and-phase-one-fences), Visibility:2777-2879 |

## A. Complete v3 wire disposition

### Messages, envelope and scalar representation

| v3 item and source | Disposition in v4 | Authority |
| --- | --- | --- |
| `WorldReport`, WR:1010-1219; JSON `v,owner,adapter,gen,obs,reqSeq,req,cmdSeq,command,mode,screens,windows,views,closed,hostOwned,equalCount,native,predicate,refill,defaultTiled,focused,complete,gesture,switchTarget,freshPlacement,cursor,actionForeground,actionCached,actionOutputs,focusFollowsCursor,cursorWarp,presentation`; optional keys WJ:1638-1651 | C: extract capabilities/settings; lifetime/generation established by handshake; keep facts/command/attempts/renderer input. Keep unaffected keys, counters and command pairing. Remove `owner` and `views`; G1 reconciliation below. | Split, R12; G1 |
| `WorldReply`, WR:1301-1359; JSON `v,adapter,gen,req,obs,rev,ready,consumed,scopes,workspaces,outputs,windows,transfers,effects,result,gestureSettled,operationSettled,nativeCreations,warpCursor,presentation`, WJ:1949-2161 | C: version/envelope alignment and approved new rows/properties; existing result variants remain. No universal success wrapper. | Split, R12, Result, Decor, Intent, Visibility, R14 |
| Discovery request `{v,adapter,req}`, `test-fixtures/world-wire/discover.json:1`; not-ready full reply WJ:171-173 | C: becomes capability/version handshake; preserve non-actuation before complete facts. Exact new handshake representation is a drafting choice, not an existing v3 message. | Split, R12; [transport](../decisions/engine.md#world-transport-authority-and-boundedness) |
| `WorldProtocolError {kind,message}`, WJ:193-206; error body `{v,outcome,kind,message,gen}`, WJ:235-248 | C: version 4 and closed typed transport kinds; preserve bounded non-echoing body and separate transport/core errors. Do not rename body to `protocol-error` without need. | R13; baseline |
| Version 3, WR:41; 4 MiB request/reply, WR:42-45; structural caps WR:47-83 | C version to 4; U existing limits. New fields need bounded accounting, not a new arbitrary transport cap. | Cutover; [boundedness](../decisions/engine.md#world-transport-authority-and-boundedness) |
| Counter decimal strings, WJ:302-346; rectangles `[x,y,w,h]`, WJ:349-380; points `[x,y]`, WJ:536-566; opaque ids WJ:383-440 | U. New typed ids do not turn geometry into objects or existing strings into tagged objects. Identity metadata is not an opaque id and needs its own validation. | Baseline; R6/R11 authorize type separation only |
| Optional keys, nulls, empty ids and conditional omission, WJ:265-280,481-534,1638-1651; WS:1493-1498,1617-1619,1724-1728 | U unless a specific delta changes the field. No blanket required-everywhere/null-only rewrite. Settings-field validation changes under R13. | Baseline; R13 |

### Report rows and settings-related enums

| v3 record/enum and source | Disposition in v4 | Authority |
| --- | --- | --- |
| `ScreenReport {output,full_rect,work_area,gap,outer_gap,complete,primary}`, WR:673-681; keys `out,full,work,gap,outer,complete,primary`, WJ:624-643 | C: keep name and `out/full/work/complete`; move `gap/outer` to settings; replace `primary` with one report-level optional output id; add scale and identity facts. | Split, Primary, R10, R11 |
| `WindowFact`, WR:706-721; keys `win,out,rect,floating,fitExcluded,fullscreen,maximized,sticky,fixedAuto,fixedSuppress,minimized,hints,observedWs,nativeWs`, WJ:570-622 | C: replace adapter classification/intent projections (`floating,fitExcluded,fixedAuto,fixedSuppress,observedWs`) with neutral raw facts and core-owned intent; unreadable observations remain nullable. K1 specifies closed type normalization, independent native flags and unavailable-vs-unknown applicability, pending 0c approval. Retain `win` as lifetime id, not a new second `lifetime` namespace. Extend fullscreen, visibility and recoverable suppression facts. Tiler-owned completion uses retained core intent plus visibility facts; nativeWs remains native observation. | Facts, Intent, R13, R14, Visibility; [classification](../decisions/engine.md#classification-and-convergence-ownership); correction evidence below |
| `ScreenView {output,view}`, WR:729-732; keys `out,view`, WJ:645-658 | R: no logical view ingress; native selectors keep separate native evidence. See G1 technical reconciliation below. | Visibility:2793-2796; R19 reconciled under G1 |
| `NativeObservation {output,native_order,current_native,occupancy}`, WR:775-780; keys `out,order,current,occupancy`, WJ:660-731 | C only native id typing; otherwise U. Preserve `current:null` non-ordered-surface meaning and incompleteness holds. Do NOT resurrect `orderKnown/currentKnown`: canonical Windows decision explicitly supersedes native known-state flags. | R11; [accepted protocol fences](../decisions/windows.md#accepted-protocol-and-phase-one-fences), lines 39-43 |
| `NativeOccupancy {native,count}`, WR:738-741 | C native id type, U `{native,count}` and null-is-unknown/count coverage. | R11; baseline |
| `PresentationIngress`, WR:633-652; wire keys WJ:1127-1221 | C: settings leave ingress; remove `paused` and projected `shell`; `cloaked` becomes host-hidden vocabulary; unknown foreground facts remain representable, including unmanaged foreground. Keep actual chord/move/order/live-frame facts; core derives foreground eligibility from neutral facts under K1, pending 0c approval. | Split, Facts, R5, R6, R10 |
| `PresentationPagerFrame {window,live,float_frame,tile_frame}`, WR:593-598; keys `win,live,float,tile` | C: remove retained `float/tile` readback; preserve live source (`win,live`). Core retains last-known frame; removing live source is not authorized. | Intent; Review:139-150,609-615 |
| `WindowSizeHints {min_w,min_h,max_w,max_h}`, `size_hints.rs:80-86`; keys `minW,minH,maxW,maxH`, WJ:458-479 | U; keep existing hint sanitization/client-to-frame normalization, not renamed `SizeHints`. | Baseline |
| `WorkspaceMode`, `workspace_scope.rs:42-66` | U tokens `per-output-local/global-unique/shared`; moved setting. Restart timing discrepancy is tracked below, not permission to change behavior. | Split |
| `FixedSizePredicate`, `size_hints.rs:176-204` | U tokens `both-axes-fixed/either-axis-fixed`; moved setting. | Split |
| `MigrationSourceRefill`, `output_selection.rs:365-404` | U tokens `last-remaining-workspace/most-recently-used-workspace`; moved setting. | Split |
| `AnchorFallback`, `admission.rs:16-56`; `NoSpaceResult`, `admission.rs:59-95` | U tokens `largest-splittable-window/closest-splittable-relative/no-search` and `float/allow-overlap`; moved settings with strict per-field validation instead of tolerant wire backfill. | Split, R13 |
| `SwitchTarget`, `cursor_targeting.rs:25-62`; `FreshPlacement`, `cursor_targeting.rs:65-107` | U tokens `cursor-output/focused-output` and `cursor-output-last-focused/cursor-output-cursor-closest/focused-output-last-focused`; moved settings. | Split |
| `BornMaximized`, `born_maximized.rs:16-55` | U tokens `tile-on-launch/keep-maximized-overlay`; moved setting, R13 diagnostics replace silent invalid-token backfill. | Split, R13 |
| Report capability flags `hostOwned,equalCount,birthClear`, WR:1043-1056; WJ:1784-1792,1817-1819 | C location to handshake; keep existing wire names. These names/capabilities are not inventions in the draft. Distinguish hard support from current read completeness. | Split, R12 |

### Desired rows, attempts and enums

| v3 record/enum and source | Disposition in v4 | Authority |
| --- | --- | --- |
| `ScopeStatus {scope,authority}`, WR:366-369; keys `scope,auth,reason`, WJ:1971-1986 | C scope becomes typed, replacing semantic text parsing; keep authority vocabulary and unaffected keys. | R6/R11 |
| `ScopeAuthority = Controlled / Untouched / Hold(HoldReason)`, WR:150-154 | U variants and existing flattened authority/reason encoding. Typed scope is not permission to wrap every authority in `{kind:...}`. | Baseline |
| `HoldReason`, WR:117-142 | U all eight: `Unreadable,IncompleteInventory,GestureBarrier,RefusedTarget,AmbiguousCreation,OccupiedRemoval,ConstraintClamp,Minimized`. Row validation uses `Unreadable`. | Baseline, R13 |
| `WorkspaceDesire`, WR:333-348; keys `id,out,bind,scope,order,mode,exist,hold,trailing?`, WJ:1987-2015 | C typed native binding/scope; U other fields and omit-unless-true `trailing`. Native removal capability is G2, not silently added. | R6/R11, R19; G2 |
| `OutputDesire {output,logical_view,tiling_mode,hold}`, WR:356-362; keys `out,view,mode,hold` | U desired logical view and mode; removal of observation ingress must not remove the desired selection. | Baseline, Visibility |
| `WindowDesire`, WR:286-322; keys WJ:2031-2096 | C add independent visibility/state authority and targets required for approved commands; preserve existing `win,out,ws,alloc,frame`, authority lanes, float diagnostic/placement/clear semantics. Add explicit command sequence where correlation currently parses operation text. | Visibility, R14, R6/R11 |
| `TilingMode = Tiled/Floating`, WR:92-103 | U `tiled/floating`. | Baseline |
| `FloatOrigin = Capacity`, WR:175-191 | U diagnostic-only `capacity`; never functional branching. | Baseline |
| `TransferKind = Window(WindowId)/View/Migration`, WR:259-266 | U. Native-free view completion changes to visibility facts, not logical echoes. | Baseline, Visibility |
| `TransferTarget {operation_id,kind,target_output,target_workspace,follow,held}`, WR:270-280; keys `op,kind,win,out,ws,follow,held`, WJ:2097-2127 | C explicit sequence metadata where needed; preserve retained member/view/migration correlation. No visibility work/group ids. | R6/R11, Visibility |
| `EffectKind = Focus/Raise/Stack`, WR:198-211 | U existing kinds. | Baseline |
| `OneShotEffect {id,kind,target,issued_revision}`, WR:219-250; keys `id,kind,target,rev`, WJ:2128-2142 | C add typed effect cause and explicit command correlation instead of `fx-/af-` inference; arrival-ready focus evidence/order required. No new automatic focus policy in adapters. | R6, R5, Visibility |
| `FrameSetter = Accepted/Refused/Threw`, WR:406-419 | C Rust name to `SetterOutcome`; U existing `accepted/refused/threw` tokens. Visibility no-call representation is new and must not imply a setter was called. | R6, Visibility |
| `FrameAttempt`, WR:456-466; keys `win,target,setter,readback,dpiBefore,dpiAfter,attemptMs,op,pre`, WJ:961-1002 | C type rename of setter only; otherwise preserve the lane and geometry/DPI/one-shot evidence. Do NOT replace it with generic last-write in this bump. | R6; Visibility decision explicitly migrates frame/member lanes separately |
| `MemberAttempt`, WR:503-514; keys WJ:807-838 | C consolidate logical/native `pre/post` into one typed reference set; retain operation, subject, target, setter and timestamp. | R6/R19 |
| `ViewAttempt`, WR:543-555; keys WJ:840-869 | C: one native-reference-only pre/post/baseline set, native selectors only. No logical arm; native-free completion uses visibility/focus facts. | R6/R19, Visibility; G1 reconciliation |
| `ClearAttempt`, WR:573-580; keys `op,win,setter,pre,post,attemptMs`, WJ:899-911 | C setter type name only; U maximize-clear proof and operation id. | R6; [birth clear](../decisions/contract.md#core-birth-clear-ownership) |
| `NativeCreationDesire {operation_id,logical_workspace,target_output}`, WR:792-796; keys `op,ws,out` | C typed ids only; U explicit at-most-once creation authorization. | R11/R19 |
| `NativeCreationAttempt`, WR:818-829; keys `op,ws,out,setter,before,after,created,attemptMs`, WJ:914-959 | C setter/native id types only; U evidence fields and unknown correlated id. | R6/R11/R19 |
| `OperationOutcome`, WR:1234-1269 | U all 14 clear/float outcomes: `ClearContinuedTiled,ClearContinuedFloat,ClearRefusedProven,ClearAbandonedExternalClear,ClearAbandonedFocusChanged,ClearAbandonedViewChanged,ClearAbandonedHomingChanged,ClearAbandonedClosed,ClearAbandonedFullscreen,ClearSuperseded,ClearContinuationRefused,FloatFrameProven,FloatRefusedProven,FloatAbandoned`. | Baseline; visibility must not reinterpret these as visibility settlement |
| `OperationEvent {operation_id,window,outcome,cause}`, WR:1279-1284; keys `op,win,outcome,cause`, WS:1457-1479 | C close cause vocabulary; U existing clear/float terminal semantics. Visibility needs distinct typed terminal accounting on its existing parent, not silently new meanings for these outcomes. | R13, Visibility:2815,2839-2845 |

### Commands: every current world and nested operation

| v3 variant and payload (wire op); source | Disposition | Authority |
| --- | --- | --- |
| `Focus(window,direction)` (`focus`, `win,direction`), WR:864; WJ:1392-1402 | C remove ignored `direction`, keep by-id focus. | R6 |
| `Send(window,target_output,target_workspace,follow)` (`send`, `win,out,ws,follow`), WR:866-871; WJ:1403-1410 | U. | Baseline |
| `SwitchView(output,workspace)` (`switch-view`, `out,ws`), WR:873; WJ:1412-1417 | R; use `SelectWorkspace`. | R6 |
| `SelectWorkspace(output,workspace)` (`select-workspace`, `out,ws`), WR:876; WJ:1419-1424 | U. | Baseline/R6 |
| `SelectNumbered(output?,index)` (`select-numbered`, `out,index`), WR:881; WJ:1426-1441 | U; 1..9 and nullable semantic output. | Baseline |
| `SelectRelative(output?,delta)` (`select-relative`, `out,delta`), WR:884-887; WJ:1442-1454 | U; -1/+1. | Baseline |
| `SelectPrevious(output?)`, `SelectTrailing(output?)` (`select-previous/select-trailing`, `out`), WR:890,894; WJ:1455-1464 | U both. | Baseline |
| `SendRelative(window,source,delta,follow)`, `SendNumbered(window,source,index,follow)` (`send-relative/send-numbered`), WR:901-919; WJ:1465-1503 | C `SendSource` implicit fallback only; U payload keys. Numbered send retains 0..9. | [Send source](../decisions/contract.md#send-source-fallback) |
| `SendSource = Explicit(OutputId)/HomeThenAction/HomeThenCachedLive`, WR:833-844; WJ:498-534 | C remove `HomeThenCachedLive`; preserve explicit and `home-then-action` shapes. | Send source |
| `SetWorkspaceTiling(output,workspace,tiled)`, `ToggleWorkspaceTiling(output,workspace)` (`set-workspace-tiling/toggle-workspace-tiling`), WR:926-936; WJ:1504-1511,1567-1573 | U both; toggle resolves in core. | Baseline |
| `MigrateWorkspace(source_output,target_output,workspace)` (`migrate-workspace`, `from,to,ws`), WR:940-944; WJ:1512-1524 | U. | Baseline |
| `FocusDirectional(window,direction)`, `MoveDirectional(window,direction,same_axis)`, WR:958-968; WJ:1525-1549 | C type direction; U wire ops `focus-directional/move-directional`, keys `win,direction[,sameAxis]`. macOS uses this route. Preserve required empty `win` no-focus encoding; no new nullable-subject redesign. | R6; baseline WJ:1525-1538 |
| `SendDirectional(window,direction,follow)`, `MigrateDirectional(direction)`, WR:974-987; WJ:1550-1566 | C type direction; U wire ops/payload. | R6 |
| `ToggleMaximize(window)` (`toggle-maximize`, `win`), WR:994; WJ:1574-1579 | U command shape; all hosts route policy here. | R14 |
| `Operation(CoreCommand)` (`operation`, nested `command`), WR:1002; WJ:1580-1587 | R wrapper. | R6 |
| Nested `ToggleFloat(window,rect?)`, `ToggleOrientation(window)`, `Resize(window,direction,mode,press)`, `boundary.rs:40-149`; WJ:1265-1284,1322-1334 | C promote to world commands, keep `toggle-float/toggle-orientation/resize` and existing payload keys (`win,rect`; `win`; `win,direction,mode,press`). `rect` is only genuine placement, never adapter preselection. | R6; [float intent](../decisions/world-rendering.md#float-toggle-intent-and-output-targeting) |
| Nested `Reconcile/UpdateGaps/ReleaseDomain`, WJ:1224-1238 | R from world wire with wrapper; internal engine maintenance is not a user intent. | R6 |
| Nested `Move/Focus`, WJ:1239-1264 | R from wire; shared directional commands replace platform route; do not carry adapter-chosen cross-output policy. | R6 |
| Nested `SendToWorkspace/SendToOutput/MigrateWorkspace`, WJ:1335-1358 | R duplicate routes; use existing world send/migrate commands. | R6 |
| Nested `PointerResize`, WJ:1285-1321; `DragDrop/DragPreview`, WJ:1359-1387 | R direct nested wire routes with wrapper; keep gesture-driven internal computation and `DragPreview` reply payload. Only three operations are approved for promotion. Do not silently add direct drag commands to the scripting surface. Source search found no platform constructors outside protocol/core gesture handling. | R6; [gesture ownership](../decisions/engine.md#gesture-lifecycle-and-native-transport) |

### Result, gesture, presentation and transitive types

| v3 item and source | Disposition in v4 | Authority |
| --- | --- | --- |
| `CommandResult::DragPreview {rect,hover,domain,source,target,revision}`, WR:1375-1382; WS:1655-1711 | U existing `op:drag-preview` payload and rect/hover/flattened domain keys. Gesture rendering still needs it even without a direct drag command. | Baseline |
| `CommandResult::Float {window,rect,no_space}`, WR:1390-1394; WS:1712-1730 | U `op:toggle-float`, `win,rect,noSpace?` with true-only omission. | Baseline |
| `CommandResult::Migration {direction,source,target,members,floats}`, WR:1401-1407; WS:1731-1766 | U `op:migrate-workspace` and provenance fields. | Baseline |
| `CommandResult::Directional {outcome,target,rect,cause}`, WR:1426-1431; WS:1767-1791 | C closed cause enum; preserve `op:directional`, `outcome,win,rect,cause` and existing outcome tokens. | R13 |
| `CommandResult::Maximize {window,maximized,clear_fullscreen,was_maximized}`, WR:1436-1441; WS:1792-1807 | U existing `op:maximize`, `win,maximized,clearFullscreen,wasMaximized`. R14 does not authorize deleting this in favor of generic success. | Baseline, R14 |
| `Option<CommandResult>`, WR:1318-1321; WJ:149-155 | C add `Rejected {kind}`. Keep `null` for successful commands whose result is the snapshot and for observation-only replies. No `Applied/NoOp` wrapper is approved. | Result |
| `DirectionalOutcome`, `world_directional.rs:45-86` | U all 16: `FocusRefusedFloating,MoveRefusedFloating,FocusRefusedWorkspaceFloating,MoveRefusedWorkspaceFloating,MoveRefusedFullscreen,FocusRefusedMaximize,FocusFloatRetained,FocusFloatRefusedStale,FocusUnfocusedRetained,MoveFloatRefusedStale,MoveFloatRefusedInvalidGeometry,MoveFloatRefusedConstraints,MoveFloatRefusedUnknownOutput,FocusFloatApplied,MoveFloatApplied,NeedClear`; preserve tokens, including `move-maximize-clear`. | Baseline |
| `GestureIngress`, `gesture.rs:102-135`; WJ:1079-1125 | U `win,kind,phase,id,cursor,startCursor,esc,modal,start`; kind `move/resize`, phases `started/updated/released/cancelled`, nonzero decimal id. | Baseline |
| `GestureSettled`, `gesture.rs:137-149`; WS:1441-1455 | U `null` or `{id,committed}`; core mutation, not native completion. | Baseline |
| `DragHoverPrior`, `session.rs:580-596`; WJ:1004-1063 | U flattened `domainOut,domainWs,leaf,win,rev,prior,priorOut,priorWs`. | Baseline |
| `PriorGroupEdge`, `cosmic_v1.rs:349`; `DragSide`, `contract.rs:1217`; WJ:1021-1043 | U `prior:{group,edge}` and `left/right/top/bottom`, not a flattened edge-only redesign. | Baseline |
| `Direction`, `directional.rs:33`; `SameAxisMove`, `directional.rs:210-247`; WJ:451-455,1240-1244 | U tokens `left/right/up/down`, `group-with-neighbor/swap-with-neighbor`; C directional command Rust fields now use the existing enum. | R6 |
| Resize `mode` string in `CoreCommand::Resize`, `boundary.rs:66-71`; WJ:1265-1284 | U tokens `inwards/outwards/horizontal/vertical` and existing press input. This is not currently a `ResizeMode` enum. No new resize policy. | Baseline |
| `WorldPresentation`, WP:116-127 | C add one rendered presentation revision, retain foreground token/named `border,underlay,pager` and `suspendedOutput?`. Suspension unconditional. | Decor, R5/R20 |
| `BorderPresentation {visible,reason,inner,outer}`, `visual.rs:197-207`; WS:1499-1527 | C absolute rects replaced by anchored subject/outsets/width/radius/eligibility/causal witness. Preserve existing name where practical, not a new generic decoration union. | R20 |
| `BorderHideReason`, `visual.rs:141-169` | C `Cloaked` to `HostHidden`; U others `Disabled,NoTarget,Minimized,Maximized,Fullscreen,HiddenWorkspace,Shell,Suspended`. | R6 |
| `PresentationUnderlay::Found(PresentationUnderlayFound)/Hidden{base_revision,reason}`, WP:89-109; WS:1528-1588 | U absolute found/hidden payload and `status` tag. Existing `baseRev` stays group projection provenance, not a separate click-freshness policy. One common presentation revision governs renderer input. | Decor; R20 explicitly keeps underlay absolute |
| `GroupHideReason`, WP:48-80 | U all 12 `Disabled,Minimized,Maximized,Fullscreen,Floating,Pending,Diverged,RootLeaf,FocusChanged,NoGroup,Idle,Suspended`. | Baseline |
| `ActiveGroupMember {leaf,window,rect}`, `active_group.rs:21-28`; WS:1553-1569 | U `{leaf,win,rect}`. Draft dropping `leaf` is not authorized. | Baseline |
| `PagerScreen`, `PagerWorkspace`, `PagerWindow`, WP:524-555; WS:1589-1639 | U `out,workspaces`; `id,ordinal,label,current,occupied,containsFocused,trailing?,windows`; `win,frame`. Common revision/input adds no gratuitous pager payload rename. | Baseline, Decor |
| `DomainKey {output,workspace}`, `session/world.rs:68-75`; `OutputId/WorkspaceId/NodeId/WindowId`, `directional.rs:76-91`; `Rect`, `geometry.rs:18-26` | U existing JSON scalar/flattened/array representation. Native workspace refs become a distinct type, not another logical id namespace. `WindowId` already identifies a lifetime. | R6/R11; baseline |

### Direct-file types that are not v3 wire records

| Type and source | Disposition |
| --- | --- |
| `AttemptOutcome`, WR:378-395 (`Unattempted/PossiblyExecuted/RefusedProven/SupersededExternal/Applied`) | U internal settlement enum; not adapter ingress, despite its `as_str` documentation. No permission to send verdicts. |
| `RequestFreshness`, WR:1447 onward | U internal freshness classification, not another JSON message. |
| `SafetyFacts`, `window_policy.rs:46-56` | C boundary role: native inputs move into report facts, `cloaked` renamed. Exact native-normalization fields must be sufficient for existing policy, not expose the internal classifier result. |
| `EligibilityRefusal`, `window_policy.rs:89-121` | C `Cloaked` spelling under R6; may supply typed diagnostics under R13, not a required adapter-supplied verdict. |
| `MembershipVerdict`, `RetainedMembership`, `FirstExitDisposition`, `window_policy.rs:181-185,211-214,471` | U core policy/internal outputs. Do not turn these into window facts. Explicit-transient precedence is G3, outside wire scope. |
| `DirectionalVerdict`, `world_directional.rs:26-40` | U internal `Refused/NeedClear`; wire uses the result/outcome records above. |
| `CoreCommand`, `boundary.rs:40-170` | R from public world wire via `Operation` removal, not removal of internal layout machinery. |

### Additions without a v3 counterpart

| Addition | Minimum authorized content; disposition | Authority |
| --- | --- | --- |
| Handshake request/accepted handshake | A version, existing hard capabilities plus proven new hard limits; one adapter lifetime; core-minted generation; not-ready authorizes nothing. No per-report capability flip. | Split, R12 |
| Revisioned settings input and settings response diagnostics | A startup/change values, revision, typed per-field validation, last-good/default fallback. Settings are not repeated as applied styles in report ingress. Exact acknowledgement envelope is a mechanical drafting choice. | Split, R13 |
| Per-output `scale` | A adapter units/logical pixel every screen report; core scales settings/constants and replans on change, without retaining scale. Number encoding/checked rounding must be stated and covered, not assumed. | R10 |
| Manufacturer/model/serial/connector facts | A observed metadata; core derives physical identity and connector fallback for unknown/duplicates. Keep live `out` handle id distinct from reconnect identity. | R11 |
| One optional primary output id | A host-designated primary, unknown absent; stable-order fallback in core. No extra independently invented `output_order` is needed when `screens` already has order. | Primary |
| Fullscreen ownership and exitability facts | A observed fullscreen plus tiler/app ownership and whether core-authorized exit is possible; unknown stays unknown. Windows app-owned exit remains refused pending each approved baseline case. | R14 |
| Fullscreen and sticky on/off/toggle commands | A neutral state commands; preserve `ToggleMaximize`; core resolves refusal and sticky-off float/tile/current-workspace result. New op spellings are mechanical; no binding change or Alt+Enter command shortcut. | R14/R18 |
| Native state targets/authority and raw state actuation facts | A independent fullscreen/maximize/sticky/above StateTarget and StateAttempt lanes; keep existing maximize result. K3 adds persistent stacking support and core ownership, with mechanical R12 preimages, pending 0c approval. No generic `Applied` replacement. | R14, state-centric contract; K3 below |
| Neutral classification vocabulary and hard support | A closed WindowType/ActivationPolicy/FactName, raw native flags/layer and fixed unavailableFacts. Add nativeSelectorScope, above and warp hard capabilities; core derives effective shared mode and unsupported settings diagnostics. Technical resolutions K1-K4 remain pending 0c approval. | Facts, Split, R12; K1-K4 below |
| Anchored border descriptor | A lifetime subject, outsets, width/radius, eligibility and existing causal freshness witness. Literal `"native-frame-event"` is not a causal witness. No polling or new settling marker. | R20 |
| Presentation revision and renderer input | A exact core target plus rendered revision; core rejects changed target row, not unrelated revision. Named typed decorations retained. | Decor |
| Typed cause, command sequence, authority scope, native workspace id/reference | A explicit typed fields replace semantic id parsing; consolidate member/view pre/post reference sets. Serialization choices are mechanical except G1. | R6/R11/R19 |
| `CommandResult::Rejected {kind}` and closed report/command/cause kinds | A rejection inside accepted reply; observation incorporated, no adapter fallback. Retain current result variants/null behavior. | Result, R13 |
| Intent return and save/clear records | A core-defined/validated record stored and returned opaquely per lifetime. Preserve current float/tile override and sticky/prior-float intent, not whole engine/history/geometry persistence. Encoding/version/checks are mechanical; cross-login support not promised. | Intent; current scope evidence `float_intent_store.rs:4-9,384-410`, `native_guards.rs:160-174,214-265` |
| Independent per-window suppression capability/facts | A hard capability separate from native workspace ownership; per-window factual availability/ownership/readability; suppression requires durable preimages. No automatic `complete_window_inventory` capability is established by the decisions. | Visibility; R12 |
| Unsupported tiled-to-floating output transfer limitation | A declared hard capability replaces Windows' post-core `float-no-remap` veto; preserve source state when unsupported. Exact capability name is mechanical; establish the actual host limitation before choosing its polarity or broadening it to ordinary float transfers. | Review:4771-4774; [logical workspace authority](../decisions/engine.md#logical-workspace-authority-and-host-owned-observations) |
| Complete reply instead of direct engine queries | C consumers use published ring/view/mode/member/transfer data. Check existing reply rows before adding fields; the decision does not establish an extra generic query message or serialized engine state. | R5; Review:399-404,4776-4778 |
| Desired `shown` plus independent visibility authority | A core-composed target; never unminimize/app-hide or infer authorization from row presence. | Visibility |
| Property-tagged last-write fact | A visibility initially: target/revision, actual call/no-call evidence, path and identity-bound pre/post. Frame/member lanes remain separate; no visibility operation/work/group id. | Visibility decision; Visibility:2813,2878-2879 |
| Arrival-ready focus and feedback | A explicit-selection-only grant; arrival bracket and foreground proof; one dispatch, failure/unproven outcome; cross-output departure barrier. Exact feedback record is mechanical, not another policy owner. | Visibility:2793-2797,2811-2835 |
| Switch terminal accounting | A complete or completed-with-failures on existing parent; failed windows, proven/refused/unproven focus and fallback use; failed hide retains membership. Distinct from clear/float events. | Visibility:2815,2839-2845 |
| Per-field and per-row diagnostics | A typed settings/held-row diagnostic at narrowest scope; unknown/invalid rows never imply close. No unsolicited generic `diagnostic` message family required by this approval. | R13; [Resilience](../principles.md#resilience) |
| Unsupported native-retirement advertisement | G2, not yet a settled addition. Existing retirement only checks host-owned/completeness, not removal support. | R19 pre-build check; `world_native.rs:350-357` |

## Defaults: source-confirmed, not draft inventions

| Setting family | Core default/type/range and citation |
| --- | --- |
| Inner/outer gap | 8, integer 0..64, SC:279-286,469-503 |
| New-workspace tiled / workspace mode | true / `per-output-local`, SC:504-537; `workspace_scope.rs:42-66` |
| Same-axis move | `group-with-neighbor`, SC:538-554 |
| Born-maximized / fixed-size | `tile-on-launch` / `both-axes-fixed`, SC:555-588 |
| Anchor fallback / no space | `largest-splittable-window` / `float`, SC:589-622 |
| Migration refill | `last-remaining-workspace`, SC:623-639 |
| Switch target / fresh placement | `cursor-output` / `cursor-output-last-focused`, SC:640-673 |
| Focus-follows-cursor / cursor warp | false / false, SC:674-707 |
| Border/underlay enable | true / true, SC:708-724,813-829 |
| Border width/gap/radius | floating-point logical settings: 3.0 (0..32), 0.0 (0..64), 6.0 (0..64), SC:287-311,725-775 |
| Underlay extension | -1.0 (-1..32), follow-border-width sentinel; SC:318-326,845-860. Resolve in core, not literal negative outset. |
| Native paint colors/theme | catalog values `#2a82da`, theme true, `#662a82da`, SC:312-317. Remain native styling unless computation actually needs them. |

- `PresentationIngress::default()` is a disabled/zero ingress, not user defaults
  (WR:632-662). Screen rows have no Rust defaults (WR:673-681).
- Catalog `workspace.mode` is restart-timed (SC:533-534), while the v3 report
  documents live re-scoping (WR:1023-1028). A settings message may arrive at any
  time without changing each setting's application timing. Do not silently turn
  restart-only catalog behavior into live workspace migration.

## B. Reconciliations and open points

| Item | Disposition | Remaining gate | Evidence |
| --- | --- | --- | --- |
| G1: view ingress | Technical reconciliation, not a user decision: the user's Windows visibility removal of `views`/`ScreenView` and logical view-attempt pre/post governs. R19 typing applies where both arms remain meaningful: MemberAttempt uses one logical-or-native reference set; ViewAttempt exists only for native selectors and uses native references only. Native-free completion uses visibility/focus facts. | Pending user approval at phase 0c; no decision change. | Visibility:2793-2796; Review:1162,4826-4829. |
| G2: native removal support | OPEN: defer to phase-0b platform evidence. Draft retains current behavior, no removal capability; `exist:false` remains eligibility only under existing ownership/completeness and fresh native safety checks. | Establish supported host removal mechanisms/consumers before any new gate is proposed. | Review:1162,4829-4830; `world_native.rs:350-357`; search-scoped absence of a non-test macOS existence consumer, not proof of impossibility. |
| G3: transient vs explicit tile | Classification policy, not contract shape; excluded from the wire contract. Existing minimal action sequence remains TBD, no precedence promoted. | Later native/reference testing, outside phase-0 wire wording. | [Native transient versus dialog membership](../spec/reference-outcomes.md#2026-10-24-port-discriminator-stubs); exact row currently at line 6485. `docs/decisions/world-rendering.md:46-50,75-79`. |
| Platform facts and recovery | OPEN: primary-output APIs, raw classification/relationship/fullscreen facts, macOS lifetime-id reuse, per-window suppression/preimage storage and restart ownership. | Phase 0b read-only platform fit; use unknown/hold where evidence is missing, not fabricated facts. | Review R5/R11/R12/R14 and the corresponding contract sections. |
| Scale and anchored border | OPEN: scale-only notification and native event/witness join coverage on each host. | Phase 0b; no polling/settling marker or border-event capability invented. macOS smoothness evidence does not gate shape. | Review R10/R20. |
| Float-no-remap | OPEN: draft capability `tiledFloatTransfer` expresses the existing tiled-source-to-float cross-output limitation, not general float-transfer support. | Phase 0b must establish exact mechanism and capability scope/polarity before 0c. | Review:4771-4774. |

- KDE evidence and host-specific dispositions are in the
  [phase-0b KDE review](#phase-0b-kdekwin-platform-fit-review). They do not close
  another host's evidence gate or turn the eight scenario gates into passes.

- Product behavior already settled, not questions to reopen: core-owned intent,
  capability/settings split, scale units, physical output identity, typed
  rejection with observation incorporation, sticky-off outcome, Windows
  app-fullscreen baseline gating, no Alt+Enter binding, unconditional fullscreen
  suspension, and the two G4 exception classes.
- Platform evidence to collect at phase 0b, not new product decisions: native
  retirement mechanism, primary-output APIs, macOS lifetime-id reuse, raw native
  classification coverage, recoverable suppression, scale-only notification,
  and causal border witness coverage. No live testing is needed for this ledger.

### Accepted mechanical constraints

| Choice accepted at milestone 1 | Constraint |
| --- | --- |
| Envelope for new handshake/settings messages | Use existing short keys and mixed-case conventions where retained; add only a discriminant needed for new message routing. Do not rename every report/reply key to snake case. |
| Settings values/revisions | Preserve core defaults, ranges and application timings, including restart-only workspace.mode; define complete-snapshot/dedup ordering explicitly with later fixtures. |
| Identity/scope/reference schemas | Small tagged records for genuinely new typed unions; existing opaque ids stay strings and geometry stays arrays. Do not create `WindowId` plus a separate `WindowLifetime` mapping. |
| Persistence encoding | Versioned bounded core record for existing intent; invalid/stale records cannot become facts or write authority. No new durable ring, geometry, MRU or restart mapping. |
| Fullscreen/sticky verbs and state write feedback | New names/fields only for R14; preserve existing maximize command/result. On/off may be one set command with boolean; no generic success wrapper. |
| Quantization and scale representation | Specify checked scaling/rounding and boundary examples before goldens; retain existing exact arrival. Do not introduce tolerances, settling markers or new product minima. |
| Fixture representation | Small literal messages plus scenario containers. Fixture-only assertions/ids/step metadata never become wire fields. |

### Milestone-2 mechanical choices for review

- New routing tags only for handshake/settings families; preserve report/reply
  short keys, existing empty/null/omission behavior and all five result payloads.
  Settings values retain their former short keys, with explicit catalog mapping.
  `sameAxis` and `borderRadius` convey existing core catalog settings.
  Canonical encoding follows table key order; duplicate JSON keys refuse.
- Handshake names one adapter lifetime, core returns gen; settings ack is its
  own non-actuating message. Complete revisioned raw settings use accepted,
  stale or revision-reused acknowledgement; per-field typed diagnostics include
  restart-required. No generic asynchronous diagnostic message family.
- New unions are small AuthorityScope/WorkspaceRef/RenderTarget records. Native
  metadata is nullable bounded UTF-8, not an opaque-id restriction. Additional
  text bounds: 4096 bytes, display identity 256, opaque intent string 1024;
  existing 4 MiB and section bounds remain.
- State targets/attempts carry only property/value/revision/correlation needed
  for R14. Visibility retains a target authorization's existing world revision
  in `visibility.rev` across observation-only replies, avoiding reauthorization
  by the globally increasing reply rev; `fallback` explicitly carries core's
  hide-only permission. These are not work/group ids or independent counters.
- Effect causes close all current producer routes, including ffc hover focus;
  command/cause/report diagnostics are typed and preserve existing spellings.
- Opaque intent uses a versioned core JSON string with explicit float/tile
  override and sticky prior mode only. Native storage ownership binds the outer
  live win; no stale embedded adapter identity or restart heuristic.
- Anchored witness contains subject, capture watermark, actual frame and
  foreground joined to the native invalidation chain. Presentation input names
  exact typed target plus common rendered revision; no geometry echo or
  per-decoration freshness policy.
- Positive logical-size scaling rounds nearest, halves upward with checked
  arithmetic. Native normalization details live in the respective port notes,
  not the host-neutral core contract. Boundary examples preserve exact arrival
  and existing bounds, never tolerance or clamping.

### Milestone-3 source-checked corrections

1. Remove WindowFact.observedWs. There IS a production Windows attachment:
   `world_capture.rs:1631-1638` calls `world_native_receipts.rs:31-139`, which
   combines fresh native identity/visibility/frame/tag checks with retained
   `applied_homing` (`:54,80-81,134-135`); `world_diff.rs:228-235` consumes it.
   Thus it is not an independently observed native logical workspace: it
   republishes the adapter's applied logical association. It is removed under
   core intent ownership/G1; raw native evidence remains. Capture constructors
   setting None alone did not establish the absence of a producer.
2. Restart check: `native_guards.rs:160-174,214-265` explicitly restores
   classification only (float, sticky/prior-float, explicit fixed-window tile
   override), with admission/identity/lifetime fences. KDE
   `float_intent_store.rs:4-9,384-410` stores only disjoint live float/tile
   identity sets plus namespace/version, no logical workspace/output. Targeted
   production searches found no logical-membership restart store in those
   adapters. No new ws/out persistence is added and no regression is established
   by these routes; existing native membership is separately re-adopted from
   facts. The existing phase-0b storage/lifetime audit remains OPEN; any newly
   found logical-membership restoration must return as a gap before cutover.
3. Neutral focused fact: WR:1120-1123 says adapter-observed global foreground
   lifetime, not membership or focus proof. Windows capture
   `world_capture.rs:1686-1690` resolves foreground HWND through observed
   lifetime tokens and uses empty when unbound. Contract removes "managed"
   classification from the fact's meaning.
4. Vocabulary verified directly: ffc effect producer
   `world_cursor.rs:150-192` (typed `focus-follows-cursor` is its R6 mechanical
   spelling); `world_engine.rs:70` and `world_commands.rs:88-92` emit
   `focus-unknown`; `world_operations.rs:568-574,602-603,650-668` emit
   `focus-present,no-source,no-adjacent-output` through Directional cause.
   These routes stay; existing R14/input-only additions remain labelled new.
5. Native rounding moved to the KDE/macOS port notes. Direct sources:
   `kwin/src/world-observe.ts:302-326` uses Math.round;
   `crates/tiler-macos/src/topology.rs:80-100` uses f64::round. Contract keeps
   core logical-size scaling only and links those native normalization rules.

### Cutover carry-forward checklist

- Choice: keep inherited detail in the v3 guide during phase 0 rather than
  duplicate its implementation history now. Before deleting that guide,
  consolidate the following into world-contract.md, remove obsolete v3-only
  wording, and retarget every link to the removed file:
  1. `Current message shapes (v3)`: unchanged records/enums explicitly listed in
     the v4 unchanged-vocabulary roster, their exact handwritten codec keys,
     null/empty/omission conventions, and closed variant catalogs. Omit removed
     fields/routes and use the v4 changed tables instead.
  2. `Shared stress acceptance connectors`: shared cases, explicit barriers,
     independent native proofs and hidden-write fence.
  3. `1. One core model and policy` / `Runtime integration`: serial integration
     and requester/subscriber delivery not already stated in v4.
  4. `2. Report: complete normalized facts every time`, especially host-owned
     adoption amendment, minimized-path audit, native creation correlation,
     partial native order, external source rewind, occupancy, hint conversion
     and subsequent-admission timing. Exclude ScreenView/logical view evidence,
     per-report capabilities/settings, projected classification/intent,
     retained-frame readback and paused lane; the v4 rules supersede them.
  5. `3. Identity, sequencing and dedup`, `4. Desired world state: what the reply
     authorizes`, `5. Raw attempts and settlement`, `6. Lifetime, freshness and
     scope vetoes`: remaining unchanged mechanics, float/clear/create one-shot
     behavior, staged-observation and ambiguity rules. Replace prefix parsing,
     parallel native/logical references and lifetime minting with v4 types.
  6. `7. Host limits, coordinates, DPI and gaming`: native backing/adoption,
     released-source migration and exact hint/frame safety; replace scale/style
     and fullscreen opt-in/paused statements with the v4 rules.
  7. `8. Border, underlay, drop preview and pager`: unchanged underlay/group/
     pager payloads, native styling, drop-preview and gesture behavior. Replace
     absolute border and opt-in suspension with v4 anchored/common-revision
     rules. Keep ActiveGroupMember.leaf.
  8. `9. Core-only acceptance baseline and the honest native gap` and
     `Conformance fixtures`: preserve evidence limits and relevant consumers,
     update only when new v4 execution evidence actually exists.
- Cutover gate: world-contract.md must be self-contained for all retained
  contract rules and codec shapes before world-contract-v3.md is removed.

## C. Contract outline and fixture plan

### Contract outline

| Section | One purpose |
| --- | --- |
| Contract authority rule | Preserve main's exact authority text and both existing anchors; label draft versus current implementation. |
| Approved v4 deltas | One compact trace table pointing to rules below and their decisions, not duplicate normative prose. |
| Ownership and G4 boundary | Core policy/model, native mechanics, truthful unknowns, exact two no-round-trip exception classes. |
| Wire conventions and bounds | Existing ids/geometry/counters/nullability, version, limits and canonical encoding. |
| Handshake and lifetime | Hard capability negotiation, generation minting, reset and bootstrap. |
| Settings | Revision, source-backed defaults/ranges/timing, last-good/default diagnostics. |
| Report facts | Full inventories, classification inputs, scale/identity/primary, closes, action context and native domains. |
| Commands and results | Every retained/promoted/new verb and payload, removals, typed rejected result, consumed-command semantics. |
| Desired world and intent | Existing rows/authority, state targets, opaque storage return/save/clear and restart scope. |
| Attempts and settlement | Existing separate frame/member/view/clear/create lanes, typed refs and raw evidence. |
| Visibility and focus ordering | Per-window targets/last-write, arrive-focus-depart, failure/fallback, cross-output barrier and terminal accounting. |
| Freshness and recovery | Request/observation/command fencing, identity, incomplete holds, preimages and narrow fail-open. |
| Presentation and input | One revision/target-row validation, anchored border, absolute underlay, unchanged pager rows. |
| Conformance | Exact fixtures, scenario format, variant coverage, later codec/parser/adapter gates and evidence limits. |

### Fixture layout and ordered-scenario plan

- Root for every path below: `test-fixtures/world-wire/v4/`.
- Each `messages/*.json` file is one complete wire message, no schema/sample
  wrapper, placeholders, comments or mutation instructions. Each scenario step
  names an exact file or contains a literal full input/output, with assertions
  clearly outside wire data. Fresh engines/ids are deterministic per scenario.
- Message files and exact witness pointers are listed in
  [manifest.json](../../test-fixtures/world-wire/v4/manifest.json). Every message
  is standalone wire data; the manifest contains expectations, context, catalog
  coverage and explicit presence/null/empty/omission assertions. Context does
  not imply an ordered engine setup or native proof.

| File(s) | Coverage |
| --- | --- |
| `manifest.json` | One metadata file; all paths, directions, expectations, 7 message families, 24 verbs, 70 records, 65 closed catalogs/342 arms and 57 sentinel/presence assertions after K1-K4 reconciliation. |
| `messages/handshake*.json`, associated `invalid-*` | 13 files: 4 positive, 9 rejected/raw cases. Stable capabilities, core generation, bootstrap and refusal. |
| `messages/settings*.json`, associated `invalid-*` | 16 files: 11 positive/field-diagnostic, 5 rejected cases. Defaults, every setting token, partial validation, restart timing and acknowledgements. |
| `messages/report*.json`, associated `invalid-*` | 198 files: 86 positive, 41 held-row and 71 rejected cases. Every command, facts/native inventory, all raw attempt lanes, gestures, visibility/focus, opaque intent and renderer targets. |
| `messages/reply*.json`, associated `invalid-*` | 134 files: 112 positive, 22 rejected output shapes. Full desires, all original results plus Rejected, holds, effects, operations, state, visibility/switch events and presentations. |
| `messages/error*.json`, associated `invalid-*` | 17 files: 15 positive error kinds and 2 rejected error shapes. Bounded non-echoing diagnostics. |
| `messages/invalid-duplicate-{root,nested}.json`, `messages/invalid-json-truncated.txt` | Included in the family counts above: exactly 2 intentional duplicate-key JSON inputs and 1 labelled malformed-JSON raw input; exempt from positive duplicate/canonical checks. |

- Canonical message files are compact UTF-8 in table/codec key order, without a
  final newline; C0 controls use lowercase `\u00xx`. The manifest is pretty JSON
  with LF, not wire data. `.gitattributes` uses `text=auto eol=lf`; it normalizes
  line endings but does not add final LF. No `.editorconfig` exists. Editors must
  preserve no-final-LF for messages; byte comparison is the enforcement gate.
- `FrameAttempt` preserves required `op`/`pre` JSON keys: ordinary writes use
  `op:""`/`pre:null`; omission is invalid. Rust Option does not mean wire key
  omission (`WJ:961-1002`). Native creation's `created` remains separately
  optional with omitted/null/empty/id vectors (`WJ:914-959`).
- The following scenario files are authored. Rule-to-step coverage and exact
  OPEN/TBD gates are indexed in the manifest; assertions and deterministic model
  inputs remain outside wire data.

| File | Steps | Ordered behavior coverage |
| --- | --- | --- |
| `scenarios/bootstrap-settings.json` | 10 | Handshake, changed/invalid settings, old/duplicate/equivocating revisions and last-good layout. |
| `scenarios/rejected-command-observation.json` | 5 | Rejected command incorporates changed observations and publishes unrelated state. |
| `scenarios/duplicate-consumed-stale-reset.json` | 12 | Cached duplicate bytes, consumed command, reused/stale requests, delayed old-lifetime input/reply and reset. |
| `scenarios/row-hold-close-recovery.json` | 9 | Narrow row hold, incomplete absence, malformed identity, recovery, explicit close; transient/tile precedence TBD. |
| `scenarios/raw-attempt-settlement.json` | 12 | Accepted mismatch holds, unchanged refusal, arrival proof, spent evidence and silent successful birth-clear retirement. |
| `scenarios/visibility-order-failure.json` | 17 | Arrive-focus-depart, failed show, refused focus, explicit fallback and core terminal accounting. |
| `scenarios/visibility-cross-output-supersession.json` | 18 | Earlier-revision cross-output barrier, issued completion, refused hide, no same-authorization retry, supersession and ABA. |
| `scenarios/decorations-target-row.json` | 7 | Unrelated rendered revision accepted; removed target rejected without focus. |
| `scenarios/anchored-border.json` | 8 | Live anchor motion, foreground/replaced-subject veto, absolute underlay; native witness joins OPEN. |
| `scenarios/scale-identity-primary.json` | 8 | Fractional scale/style rounding, duplicate physical identity with distinct connectors; host metadata proof OPEN. |
| `scenarios/send-action-context.json` | 7 | Home first, stale explicit source refusal, isolated foreground/pointer/cache/registry chain; unhomed native fact OPEN. |
| `scenarios/intent-restart-preimage.json` | 15 | Opaque intent return/version refusal, persist-before-hide, lifetime-safe recovery and failed slot; host survival OPEN. |
| `scenarios/window-state.json` | 20 | Maximize, unknown/known/nonexitable fullscreen, sticky prior mode, float-on-sticky and unsupported mechanism; host baseline OPEN. |
| `scenarios/native-domain.json` | 12 | Initial/partial native order, selector/member proof and correlated creation race; removal and notification mechanisms OPEN. |

- Existing convention is byte-locked producer reports and parsed/applied Rust
  replies, not an existing all-platform v4 oracle. Root `report-*.json` are KDE
  broker envelopes with nested producer reports; preserve that distinction.
  Consumers: `kwin/tests/world-bytes.test.ts:362-395,418-453,467-498`,
  `kwin/tests/world-outer-gap-regression.test.ts:149-202`,
  `kwin/tests/world-wire.test.ts:624-646`; regeneration example
  `crates/omnitiler/examples/world_wire_fixtures.rs:73-110,256-345`.
- Phase 1 must add exact codec/generator comparison, not merely `JSON.parse` or
  `jq` acceptance. Later generated TypeScript and parser variant coverage follow
  R17; no v4 codec currently verifies these future expected bytes.

## D. Existing draft assessment

| Draft material | Verified reusable intent | Required correction |
| --- | --- | --- |
| `world-contract.md:3-19` | Draft/current distinction and cutover target. | Authority text from main was not preserved verbatim; restore exact paragraph and both anchors. |
| Ownership/G4/recovery, lines 21-32,379-399 | Core owns policy; two G4 exceptions; narrow fail-open and recoverable preimages match decisions/principles. | Rewrite reviewably, attach rule-local links; not evidence that invented fields are approved. |
| Envelope, lines 45-83 | Handshake/settings/report split and core-minted generation have authority (Split/R12). 4 MiB and counters are inherited. | Wholesale type/key renaming and mandatory metadata on every message are untraceable. Keep unaffected v3 wire keys. |
| Facts/ids, lines 85-141 | Unknown native facts, primary, scale/identity, core intent are approved. | Extra independent `window/lifetime` identities, object geometry, renamed records, blanket null/missing rules and dropping all logical member receipts are not justified. |
| Capabilities, lines 145-162 | `hostOwned/equalCount/birthClear` exist in v3; independent suppression and R14 hard limits are justified concepts. | Draft names do not prove approval; `complete_window_inventory` and mandatory `anchored_border_events` are not established. Test existing mechanisms first; include missing float-no-remap limitation review. |
| Defaults/styles, lines 164-185 | Most numeric defaults match core catalog after checking. | Remove unsupported-default disclaimer; cite SC. Preserve floating style settings/ranges, include radius 6.0, resolve extension sentinel in core, remove duplicate style echo. |
| Commands/results, lines 193-233 | R6 removals/promotions, fullscreen/sticky and unchanged bindings are approved. | `Applied/NoOp` replaces current results without authority. Direct drag command promotion is not among the approved three. Retain existing result payloads/null behavior plus `Rejected`. |
| Generic attempts, lines 260-269 | Visibility property tag/focus feedback are required concepts. | Replacing frame/member/view/clear lanes wholesale is contrary to separate-migration decision; keep existing lanes with specifically approved typing changes. |
| Windows visibility, lines 333-345 | State-centric intent and cross-output barrier are largely supported. | Failed focus does not categorically forbid every departure: separately approved fallback hides may run under fresh gates (Visibility:2825-2829). Preserve suppression ownership and bracket evidence, not just booleans. |
| Presentation, lines 347-365 | Named fields/common revision, anchored border, absolute underlay and unconditional suspension are supported. | A constant event-name string is not a causal witness; do not invent an anchored-event capability as policy. Preserve existing pager/group member shapes. |
| `v4/catalog.json:5-13,52-84` | Useful list of subjects needing coverage, not reusable golden bytes. | Invalid JSON at line 84; geometry/keys/member shapes reinvent baseline; samples do not establish normative completeness. Replace, rather than mechanically repairing malformed pseudogoldens. |
| `v4/scenarios.json:18-31` | Scenario topics are useful coverage reminders. | Not ordered conformance examples: shared report/reply refs are unchanged across incompatible setups; prose setup/assertions do not establish concrete input/output. Failed-focus assertion also omits approved fallback. |

## Accepted evidence and current outcome

- Direct source/doc inspection established the baseline, approved deltas,
  authority-text loss, result mismatch, preserved visibility fallback and the
  restriction against replacing frame/member lanes with generic last-write in
  this visibility change.
- Read-only source extraction found the core defaults and fixture consumers
  cited above. A targeted source search found native retirement eligibility
  gated by host-owned/completeness but no distinct removal capability, and no
  non-test macOS `existence` consumer in the searched files. These absence
  claims remain search-scoped, not platform-fit acceptance.
- Superseded-input validation reported: `jq empty` failed `v4/catalog.json` at
  line 84, column 1032; `v4/scenarios.json` parses and has no duplicate keys.
  Catalog duplicates could not be assessed because its syntax did not parse.
  Both files were reported ASCII. Both are now removed, not repaired.
- Milestone-2 outcome: the unreviewed contract draft was replaced with a
  source-backed normative target, field tables, complete unchanged-type roster,
  exact authority paragraph and explicit carry-forward checklist. G1 is a
  technical reconciliation pending 0c; G2/platform evidence remains OPEN;
  G3 is outside the wire and remains TBD in reference outcomes.
- Milestone-2 verification: `git diff --check` clean; the untracked change note's explicit
  no-index whitespace check emitted no diagnostics; added tracked lines and
  every untracked file had zero non-ASCII bytes. Exact main authority paragraph
  and both anchors verified. Repository link/anchor check reproduced the same
  five known baseline failures (four lifecycle anchors, one missing old KWin
  test file), zero new unresolved. Independent mechanical review identified
  three new bad links and a redundant visibility sample key; all corrected.
- No Rust implementation or fixture bytes changed at milestone 2.

### Milestone-3 outcome and evidence

- Replaced the invalid catalog and prose scenario outline with one manifest and
  378 exact message files (377 JSON, 1 raw text). There are 228 positive or
  settings-field-diagnostic messages, 41 invalid identifiable-row/hold inputs
  and 109 rejected/raw inputs. Every vector has a family, direction and expected
  acceptance class; negative messages remain complete inputs except the labelled
  truncated-JSON case. No mutation instructions or fixture metadata enter wire data.
- Coverage verified against the target draft's explicit inventory: 7/7 message
  families, 24/24 verbs, 70/70 record shapes, 59/59 closed catalogs with 300/300
  arms. Assertions resolve 245 actual record witnesses and 391 typed enum-value
  witnesses; 51 additional assertions distinguish missing, null, empty and
  true-only output fields. Decoded intent-string and flattened DomainKey
  projections are explicitly marked, not invented nested wire objects.
- JSON/bytes: `jq -e .` parsed all 378 JSON files including the manifest and the
  two deliberately duplicate-key negatives. A strict object-pairs parser found
  exactly those two duplicate cases (root `v`, nested settings `gap`); all other
  JSON was duplicate-free. The one raw text file failed JSON syntax as labelled.
  Independent byte comparison passed for all 375 canonical JSON message files,
  including no trailing LF and exact C0 escaping. Recursive key-shape/order
  checks covered all 228 positive messages and every record witness with zero
  errors or unmapped objects. Catalog/arm/file/pointer inventories and typed
  actual-value equality passed; this is not a v4 decoder/type validator.
- Corrections 1-4 are applied and source-checked above: observedWs removed
  despite its Windows applied-homing receipt producer; restart records retain
  existing classification intent without an unproven logical mapping; focused
  is observational; questioned tokens are backed by current producers; native
  rounding lives in port notes. No new restart-regression gap was established by
  the searched stores; the existing phase-0b storage/lifetime audit remains OPEN.
- Final snapshot checks: `git diff --check` clean; all untracked files also
  checked for trailing whitespace/NUL, zero findings; added tracked lines and
  untracked files have zero non-ASCII bytes. Authority paragraph and both
  anchors match the baseline exactly. Repository link/anchor scan checked 7184
  local links and reproduced only the five known baseline failures, zero new.
  No Rust implementation or dependency changes;
  G1 still awaits 0c approval and G2/platform evidence remains OPEN. Ordered
  causal/scenario, codec, native recovery and paint acceptance are not claimed.

### Milestone-4 canonical-number choice

- Number-valued fields use binary64 shortest-round-trip significands expanded
  into ordinary decimal notation, with integral values lacking fraction/exponent
  and signed zero emitted as `0`. The exact tie rule and decoder acceptance are
  stated once in the contract's wire conventions. Existing fractional scale
  vectors (1.25/1.5 in report-scale-identity-primary) and fractional style
  vectors (0.5/1.25/2.5 in settings-alternatives) already cover both requested
  families; their bytes do not need replacement. Manifest metadata points
  to those exact number witnesses as well as the ordered scaling checks.

### Milestone-4 outcome and evidence

- Authored all 14 planned scenario files and manifest rule-to-step coverage:
  160 ordered steps, 124 complete wire inputs, 208 explicit jq assertions and
  8 OPEN/TBD gates. Scenario metadata specifies fresh initialization, generation
  minting, exact inputs, assertion/history semantics and deterministic offline
  native, renderer and storage models; it is not product wire vocabulary.
- Gates remain explicit: transient/tile precedence (G3), native causal witness
  joins, primary/physical identity APIs, unhomed native fact readability, host
  storage survival/ownership, Windows app-fullscreen baseline, removal support
  (G2) and native creation notification correlation. Their outcomes are not
  promoted to acceptance by the deterministic model.
- Source review corrected automatic birth-clear success to silent retirement
  (`world_born.rs:703-707`), not the directional clear lane's `continued-tiled`
  event. Float confirmation/refusal assertions follow `world_settle.rs:1829-1859`;
  native order/id/backing assertions follow `world_model.rs:451-479,818-846`
  and `world_native.rs:725-755,1163-1205,1247-1297`. New policy scenarios use
  their cited approved contract rules, not claims of existing v4 execution.
- Final mechanical verification: all 392 JSON files parse with jq; scenario
  duplicate keys are zero, with exactly the two labelled message duplicate-key
  negatives and one malformed raw input preserved. Scenario inventory, exact
  manifest rule-step/gate maps, history references, file/anchor references and
  all 208 jq assertion compilations pass. Recursive wire key/order checks,
  including messages embedded in model events, report zero errors and zero
  unmapped objects.
- All 375 canonical message files pass compact-byte/no-final-LF checks.
  A binary64 formatter cross-check validates 9642 numeric tokens and all four
  numeric witness pointers/tokens; it is not an independent proof of every
  formatter tie case or a v4 codec execution. Message/catalog coverage remains
  7 families, 24 verbs, 70 records, 59 catalogs/300 arms and 51 distinctions.
- Repository checks: `git diff --check` clean; added tracked lines and all
  untracked files have zero non-ASCII bytes; untracked trailing whitespace/NUL
  checks report zero findings.
  Authority paragraph and both anchors remain exact. The link scan checks
  7185 local links and reports only the same five baseline failures, zero new.
  No Rust implementation or dependency changes; no phase-1 harness, codec,
  native scenario, recovery or paint execution is claimed. G1 still awaits 0c;
  all eight OPEN/TBD gates remain pending, not passed.

## Phase-0b KDE/KWin platform-fit review

- The findings/options in this review describe its initial source-fit result.
  K1-K4 are superseded by the
  [technical resolutions below](#k1-k5-technical-resolution-pending-0c-approval);
  K5 retains an explicit phase-2 proof gate.
- Verdict: the native-workspace route fits; the draft still has classification,
  selector-coupling, stacking and warp gaps. Recovery and anchored-border APIs
  exist, but the current implementation is not v4 recovery/paint acceptance.
- Evidence below is source-only. `K:` means KWin `v6.8.1`
  (`1b798c05c19b1124edb1e16623191fcd2c128fd0`); `F:` means the `v6.6.0`
  floor (`e53bb3be7ecf1735c7739b797f94d66de422ab4d`). Paths after these
  prefixes belong to KWin. Other paths belong to this repository. Current code
  is the implemented pre-v4 baseline, not evidence of v4 execution.
- Citation shorthand: bare TypeScript filenames are under `kwin/src/`;
  `kde_raw.rs`, `world_broker.rs`, `float_intent_store.rs` and
  `planner_service.rs` are under `crates/omnitiler/src/`;
  `presentation_consumer.h` is under `kwin/native-effect/`. A range-only
  continuation uses the preceding file, with the same pin for KWin citations.

### 1. Report production

| Fields/records | Finding and evidence | Verdict |
| --- | --- | --- |
| Handshake/settings; report lifetime, `obs,reqSeq,req,cmdSeq,command,monotonicMs`; completeness/close/attempt envelopes | Broker sequencing and raw capture already separate native facts from ordered transport. v4 must move settings/capabilities once, not preserve per-report policy. Current `hostOwned` flips with readability (`kwin/src/world-observe.ts:1332-1337`); v4 fixes it for the lifetime and uses incomplete facts to hold. | Fits; port work. |
| `ScreenReport.out,full,work,complete,scale` | Output name/geometry and `clientArea(MaximizeArea,output,desktop)` exist; geometry is device-independent, work area uses `geometryF()` (`K:src/core/output.h:247-271`, `K:src/workspace.cpp:2515-2535`). Existing normalization rounds native frames once (`kwin/src/world-observe.ts:302-326`); native renderer does no second DPI transform (`kwin/native-effect/presentation_consumer.h:9-19`). Thus R10 scale is **1**, including fractional display DPR; DPR is not adapter units/logical pixel. | Fits; wording fixed. |
| Scale-only, resolution and output-order invalidation | `screensChanged` wraps output-list changes, not every scale change (`K:src/scripting/workspace_wrapper.cpp:57-60`). Subscribe to each output's `geometryChanged/scaleChanged`, and `screenOrderChanged`; attach/detach with topology. All exist at the floor (`F:src/core/output.h:150-155,250-258`, `F:src/scripting/workspace_wrapper.cpp:56-59`). Current workspace-only hooks are insufficient (`kwin/src/world-entry.ts:4579-4606`). | Fits API; missing port hooks, no polling needed. |
| `OutputIdentity.manufacturer,model,serial,connector`; report `primary` | Script Q_PROPERTYs expose manufacturer/model/serialNumber/name (`K:src/core/output.h:273-323`; `F:src/core/output.h:147-155`). DRM name comes from connectorName, metadata from EDID (`K:src/backends/drm/drm_output.cpp:54-60`; same floor lines). Report name as connector only where its backend meaning is verified, otherwise null; empty/unreadable EDID is unknown. `screenOrder` is ascending backend priority (`K:src/scripting/workspace_wrapper.cpp:446-449`, `K:src/workspace.cpp:2615-2629`; `F:src/workspace.cpp:2579-2593`). No separately proven primary designation: omit `primary`, preserve priority-sorted screen order for core fallback. `set_primary_output` is ignored (`K:src/wayland/outputmanagement_v2.cpp:287-299`). Current active-screen primary (`world-observe.ts:1307-1331`) must go. | Fits nullable identity/optional primary; no active-screen substitution. |
| `WindowFact.win,out,rect,hints,fullscreen,maximized,sticky,minimized,nativeWs` | UUID/output/frame/client/min/max/state/desktops are exposed (`K:src/window.h:172-176,383-387,412-442,566-573,612,673`). Frame-normalized hints use actual client/frame deltas (`crates/omnitiler/src/kde_raw.rs:443-450`). Any maximize axis is currently `maximizeMode != 0` (`:483-485`). Single native desktop is truthful; all-desktop or multiple memberships remain null, never current-desktop guesses (`:498-502`). | Fits, including unknowns; multi-membership is not a fabricated single home. |
| `role,type,appId,class,title` | Native sources: windowRole, NETWM windowType enum, desktopFileName, resourceClass, caption/captionNormal (`K:src/window.h:184-196,334-340,547-559,702-713`). Wayland resourceClass/desktopFileName derive from XDG app id; X11 class and desktop-file derivation differ (`K:src/xdgshellwindow.cpp:847-862`, `K:src/x11window.cpp:414-427,2965-2968`). Type needs a documented native enum-to-text encoding, not an adapter admission label; do not claim desktopFileName is a universal canonical app id. Current raw rows omit these fields. | Fits sources; raw vocabulary/applicability gap K1. |
| `dialog,transient,modal,resizable,owner,parent`; `hostHidden,privileged,shell,tool,ownedDialog,noActivate,cannotControl` | Native dialog/type flags, transient/transientFor, modal, resizeable and hidden exist (`K:src/window.h:237-245,576-604,694-700,755-760`). Bind transientFor only as that known relationship; no invented X11-parent/Windows-owner equivalence. Unavailable relationship/safety facts stay null. `hidden` is narrower than effective visibility; do not combine minimize/desktop/activity into it. `wantsInput` means can accept focus, not noActivate (`:576-582`); managed means KWin controls placement (`:342-348`), not a universal cannotControl verdict. Current normalization manufactures other safety negatives and folds dialog into transient (`kde_raw.rs:425-437`); do not carry that policy into v4. | Gap K1; nullable fields alone do not specify core applicability. |
| `fullscreenOwner,fullscreenExitable` | No native tiler/app provenance property. Return ownership only with verified current-lifetime write evidence, otherwise null. Neutral fullScreen setter/fullScreenable exist, but legacy hack/rules/base implementations can refuse (`K:src/window.h:406-420`, `K:src/window.cpp:4122-4159`, `K:src/xdgshellwindow.cpp:1699-1711`, `K:src/x11window.cpp:3702-3712`). Exitability is availability of a known neutral path, never proof that the requested exit completed; unknown stays null. | Fits; do not infer ownership from fullscreen=true. |
| `visibility,lastWrite,suppressionAvailable`; presentation `minimized,maximized,fullscreen,hostHidden,hiddenWorkspace,shell,frame,foregroundOut` | Effect visible combines shown, current desktop and current activity (`K:src/effect/effectwindow.h:60-65,221-241`, `K:src/effect/effectwindow.cpp:193-199`); Window::isShown excludes hidden/show-desktop/minimize (`K:src/window.cpp:4439-4442`). Use native reads, not desired membership. KDE has no selected recoverable independent hide path: suppressionAvailable=false, suppressionOwned=false when no project hide exists, LastWrite=null; unknown visibility is nullable. Hidden is read-only to script (`K:src/window.h:760`). | Fits native-owned route; suppression not required. |
| `focused`; presentation `foreground` including unmanaged fullscreen | Read activeWindow independently of candidate admission, then its output/frame/state. Ordinary KWin-managed but core-unmanaged fullscreen can be captured; do not require it in the managed roster. Current foreground is filtered through candidate `liveById` (`world-observe.ts:803-816,819-874,1530-1531,1604-1621`). Override-redirect KWin-unmanaged focus is NOT established by these sources: activation rejects non-clients (`K:src/activation.cpp:294-302`); a getter without that check does not prove OS input foreground coverage. Return unknown rather than synthesize a fullscreen subject. | Fits independent core-unmanaged capture; override-redirect coverage remains unproven. |
| NativeObservation/Occupancy; `nativeWs`; `actionForeground,actionCached,actionOutputs,cursor,cursorHit` | Global desktop list/UUID ids and per-output cursors exist (`K:src/scripting/workspace_wrapper.cpp:69-107`, `K:src/virtualdesktops.h:50-63`). Current occupancy deliberately enumerates all native occupants, sticky on every desktop, unreadable=>null (`world-observe.ts:1343-1411`). Action context remains observed context, not output selection. Cursor read/windowAt native hit test exist (`K:src/scripting/workspace_wrapper.h:149-153,483`; `.cpp:369-395`). | Fits; selector coupling K2 is separate from count equality. |
| `closed`, no WindowId reuse; `intentRecords` | Window constructor creates a UUID; script constant and effect internalId share it (`K:src/window.cpp:57-63`, `K:src/window.h:383-387`, `K:src/effect/effectwindow.cpp:296`). Normalization strips braces consistently (`kwin/src/native-id.ts:68-85`). Snapshot duplicate rejection is not a historical no-reuse guard (`kde_raw.rs:308-312`); retain retired-id detection/reminting or refuse a collision before rebind, including native renderer mapping. Storage binds verified UUID within KWin owner namespace, not titles/processes. | Fits mechanism; no mathematical collision/no-reuse proof claimed. |
| `gesture,gestureIngress`; presentation `chordHeld,moveWindow,pagerOrder,pagerFrames,witness`; rendererInput | Native stacking/frame capture exists (`world-observe.ts:1547-1585,1599-1664`); chord/gesture facts are separate event sources. Retain inherited gesture cancellation uncertainty, not a guessed release (`docs/porting/kde-port-notes.md:234-245`). Witness and renderer input are audited below. | Fits carried raw lanes; native gesture acceptance remains separate. |

### 2. Reply application and G4

| Reply lane | Source-backed finding | Verdict |
| --- | --- | --- |
| `scopes,workspaces,outputs,windows,transfers`; frame/member/view/clear/create and attempts | Exact accepted targets resolve through native bindings, not local ordinals (`kwin/src/world-apply.ts:470-489,794-1030`). Frame setter/prewrite/readback and explicit float placement are separate (`:675-723,1120-1203`); maximize-clear uses setMaximize(false,false) (`kwin/src/world-native.ts:1777-1790`). Native member/view reads belong in consolidated native WorkspaceRef arms; frame DPI stays null in logical space. Asynchronous Wayland configure cannot be completed by setter success or later uncorrelated reads. Existing raw lanes suffice; retain pre/post stamps and core settlement. | Fits; remove existing adapter classification/automatic-focus policy rather than migrate it. |
| `state`; existing Maximize result | setMaximize, fullScreen, onAllDesktops are native mechanics (`world-native.ts:749-930`; `K:src/window.h:412,442,673`). Sticky-off goes to the current desktop (`K:src/window.cpp:771-781`); core retains prior tile/float intent and decides result. Apply core-resolved values and report bool pre/post, never call the existing local toggle/continuation policy as the v4 decision owner. | Fits API; stacking recovery is a prerequisite, K3. |
| Visibility targets, LastWrite, ArrivalBracket, FocusFeedback, SwitchEvent | KDE selects native desktops; it need not hide each member. With windowSuppression=false, visibility targets are null/no writes. Native visibility/home/frame/lifetime still support arrival reads and focus feedback; require a recovery tag only where project suppression actually exists. Do not implement hides by minimizing, app-hiding or effect-only paint suppression. Switch completion remains core-derived. | Needs draft clarification, applied without wire changes. |
| `effects` focus/raise/stack; `warpCursor` | Active-window setter delegates to KWin activation; raiseWindow is script-invokable (`K:src/scripting/workspace_wrapper.cpp:260-263,343-347`; `F:src/scripting/workspace_wrapper.h:290`). Existing adapter spends raise/stack without actuation (`world-apply.ts:2261-2288`), not an API limitation for raise. constrain/unconstrain take two windows, not the one-target Stack record (`K:src/scripting/workspace_wrapper.cpp:350-357`). Effect setElevatedWindow changes painting only (`K:src/effect/effecthandler.h:443-445`). No public script/effect warp setter in inspected wrapper/effecthandler at either pin; internal Cursor::setPos is not that API (`K:src/cursor.cpp:189`). | Focus/raise fit; K3 stacking vocabulary and K4 warp support gaps. |
| Presentation border/underlay/pager, result, gestureSettled, operationSettled, switchSettled, diagnostics, intentWrites | Results/terminals/diagnostics convey core outcomes, not new native permission. Named underlay/preview already consume core geometry; border currently uses absolute rects, must become native anchored descriptor. Effect has no pager paint/click implementation (`kwin/native-effect/activewindowborder.cpp:417-445,1550-1600,1756-1829`); tray publishes state, not the future renderer-input route (`kwin/src/tray-publisher.ts:110-157,214-263`). Future renderer must retain the drawn row plus common rev and send RenderTarget with its command; no local stale-click policy. Intent writes require opaque storage conversion below. | Fits renderer/input architecture; not implemented conformance. |

### 3. Recoverable preimages and host storage

- Available slot: session runtime file under `$XDG_RUNTIME_DIR/omnitiler/`,
  private anchored no-follow handles, atomic replace, namespace derived from
  session bus id plus current unique KWin owner (`float_intent_store.rs:11-34`,
  `planner_service.rs:222-231,295-309`, both under `crates/omnitiler/src/`).
  Live UUID intersection verifies subjects, not title/process restart matching
  (`float_intent_store.rs:37-45,222-269`). A new store instance reads the same
  content; namespace mismatch is empty (`:903-917,931-953`, tests inspected,
  not rerun here). This supports script/service restart while KWin lives;
  KWin restart intentionally invalidates ownership and window identities.
- Missing implementation: current file contains float/tile sets, not opaque
  v4 records/stickyPrior or recoverable native preimages (`:4-9,384-410`).
  Automatic keepAbove ownership and sticky prior above/below are in-memory
  sets/maps (`kwin/src/world-native.ts:227-232,769-774,1095-1174`), lost on script
  restart. Setting keepAbove can also clear keepBelow (`K:src/window.cpp:592-607`),
  so save both affected values before dispatch. R12 requires a separate
  adapter-owned, lifetime-verified preimage slot; do not put native recovery
  details into core's opaque intent string or claim the existing file saves them.
  No hide path is selected. Failed persistence disables that property only.
- Verdict: **fits storage mechanism; recovery gap remains**. Next-start recovery
  is required before writes resume; source inspection cannot establish crash
  durability, successful recovery or survival through KWin restart. The latter
  is not licensed by the current namespace and must never use stale UUIDs.

### 4. KDE capabilities

| Capability | KDE value at source-fit level | Evidence/qualification |
| --- | --- | --- |
| hostOwned / equalCount | true / true | Native global desktop list, repeated across outputs; observation failure is not a capability flip (`world-observe.ts:1301-1306,1334-1345`). |
| birthClear | true | Existing automatic-clear lane and explicit report value (`kwin/src/world-report.ts:1834`, `world-apply.ts:1222-1415`); unrelated to desktop creation. |
| windowSuppression | false | No selected recoverable per-window hide route; native desktop ownership does not imply suppression. |
| maximize / fullscreen / sticky | true / true / true | Native setters above; per-window rules/legacy refusal and R12 preimage availability still gate individual writes. Sticky remains owning-output membership, not moving to every output. |
| tiledFloatTransfer | true | sendClientToScreen plus desktop assignment and explicit frame_op can execute a core-decided tiled-to-float cross-output move (`world-native.ts:1486-1502`, `world-apply.ts:1120-1203`). No Windows-style unsupported remap veto identified; do not conflate native asynchronous arrival with missing hard support. Other hosts still determine final capability scope/polarity. |

### 5. Native retirement and creation correlation

- G2 KDE mechanism resolved: script removeDesktop calls the native manager;
  removal mutates the global list and refuses its last desktop
  (`K:src/scripting/workspace_wrapper.cpp:300-307`,
  `K:src/virtualdesktops.cpp:492-523`; floor declarations
  `F:src/scripting/workspace_wrapper.h:263-268`). Existing consumer checks
  fresh global occupants/visibility/pins/inventory/minimum before removal,
  deduplicating repeated native bindings (`world-apply.ts:1548-1643`). Removal
  affects every output: retirement must preserve any still-required backing
  across that shared domain, not treat one output row as physical ownership.
  No new removal-support flag is justified for KDE. Other hosts keep G2 OPEN.
- native-creation-correlation KDE source mechanism resolved: void wrapper calls
  manager inline; it inserts UUID into the list before desktopAdded/countChanged
  (`K:src/scripting/workspace_wrapper.cpp:295-298`,
  `K:src/virtualdesktops.cpp:448-481`). Spend before call, sample full ids before
  and immediately after in the same turn; unique delta can correlate, cap/no-op
  or ambiguous delta cannot. Existing consumer does exactly that; a failed
  immediate read is later returned uncorrelated (`world-apply.ts:1461-1539`).
  Notification alone is not causal creation proof; asynchronous window configure
  is not evidence that desktop insertion is asynchronous. Other-host and native
  execution gates remain OPEN.

### 6. Remaining gaps and decisions not taken

| Gap/question | Options; recommendation; consequence |
| --- | --- |
| K1: can core distinguish unavailable/irrelevant native safety inputs and classify KDE without adapter policy? | Existing raw capture has managed/wantsInput/normalWindow/transientFor, while v4 offers policy-shaped cannotControl/noActivate and an untagged type string. Null does not mean false or irrelevant. Options: specify sufficient neutral raw facts plus core applicability rules; use host-tagged native facts; hold all ambiguous cases. Recommend the smallest explicit raw fact/applicability vocabulary, including native type encoding, before freezing shape. Any added facts require fixture/catalog updates. Do not map !wantsInput to noActivate or raw KDE dialog directly to Windows owned-dialog policy. |
| K2: how does core know native selectors are linked, independently of equalCount? | At 6.6 only shared currentDesktop exists; current adapter substitutes shared mode (`kwin/src/workspace-native.ts:422-436,539-546`). At 6.8 per-output API exists, but native option=false makes selection apply across outputs (`K:src/options.h:140`, `K:src/virtualdesktops.cpp:577-611`). v4 handshake has no selector-coupling fact. Options: report native selector coupling/configuration for core planning; require newer KWin plus enabled setting; keep narrowly held unsupported operations. Recommend exposing the mechanism/configuration, preserving existing floor and user setting. Shape/support changes need approval; local mode substitution would violate G4. |
| K3: what exactly authorizes persistent float/sticky stacking and release? | Current adapter chooses keepAbove acquisition, fullscreen suspension, external-clear non-reacquisition and release (`world-native.ts:1177-1329`); v4 StateTarget has no stacking property, OneShotEffect::Stack only one subject and no on/off/preimage target. Options: explicit core-owned stacking facts/targets and release semantics; define Stack as a nonpersistent action and separately settle retained stacking; remove the behavior. Recommend explicit core authority while keeping preimage recovery mechanical under R12. Cannot infer persistent keepAbove from floating/sticky=true or substitute raise/effect elevation; draft/fixtures need an approved completion, not adapter policy. |
| K4: how is unsupported cursor warp conveyed to core? | No warp API at pin/floor, yet settings allow cursorWarp and reply carries warpCursor with no hard capability/result lane. Options: capability consumed by core; explicit unsupported/no-call evidence; documented silent unsupported no-op. Recommend capability so core cannot authorize unavailable work. Shape change remains unapproved; never pretend a warp ran. |
| K5: can the native witness join be established without changing wire shape? | Existing `{instance,counter}` -> JS capture -> broker coveredInstance/coveredInvalid -> native paint is real (`world_broker.rs:2018-2070,2257-2262`, `kwin/src/world-presentation.ts:482-534`, `presentation_consumer.h:21-42`, paths under their source directories). It is NOT report obs, and current frame events hide until new covering paint (`kwin/native-effect/presentation_stage5.cpp:59-109`). Options: retain an exact adapter-local capture-to-native-source join and split anchor-only motion from eligibility invalidations; extend FrameWitness if that cannot be proven. Recommend proving the local join first, not inventing wire fields from a source search. Activity/eligibility invalidation coverage and accepted-anchor motion remain unverified. KWin effect frame events/internalId exist at pin/floor (`K:src/effect/effectwindow.h:652,746`; `F:src/effect/effectwindow.h:663,757`); no missing native event API established. |

### OPEN-point disposition and draft reconciliation

- KDE source mechanisms resolved: logical scale=1; output geometry/scale/order
  notifications; metadata and DRM connector provenance; truthful absent primary
  with priority-order fallback; native removal; same-turn creation correlation;
  session-scoped intent-storage survival/ownership; tiledFloatTransfer=true.
  Unhomed native facts are also readable: output/desktops/frame reads do not
  require a retained core home (`kwin/src/world-observe.ts:851-874,949-966`);
  independent capture must not exclude such a lifetime by core classification.
  These are not live/codec acceptance and do not close global scenario gates.
- Still OPEN on KDE: K1-K5; exact native causal witness join and eligibility
  coverage; restart-safe preimage/opaque-record implementation; historical id
  collision handling; unrestricted override-redirect foreground proof. Windows
  and macOS portions of every shared evidence item remain for their reviews.
- Mechanical draft fixes: explicit KWin scale=1 vs DPR; priority order does not
  fabricate primary; native-owned selection needs no suppression; arrival tag
  checks apply only where a suppression/recovery tag exists; report obs and
  native renderer counters are distinct causal chains. No wire shape, fixture
  bytes, product decision or implementation changed.
- Mechanical verification: `git diff --check` clean; added tracked lines and all
  untracked files have zero non-ASCII bytes, no untracked whitespace/NUL issues.
  Repository link/anchor scan against the pre-review baseline reports exactly
  the same five known failures (four lifecycle anchors, one missing old KWin
  test), zero new unresolved. All 392 fixture JSON files parse with jq; only
  the two labelled duplicate-key negatives contain duplicates; all 375
  canonical message byte comparisons pass. Manifest covers all 378 message
  files and 14 scenarios; 160 step ids, 179 rule-step references and 60 contract
  references pass. Fixtures are unchanged. These checks do not execute a v4
  codec, native recovery, native settlement or native painting.

### K1-K5 technical resolution pending 0c approval

- These are technical contract details within the approved decisions, included
  in the shape submitted for 0c approval, not implemented authority. No new
  product behavior or adapter-local policy exception is authorized.

| Item | Technical resolution pending 0c approval | Evidence and remaining gate |
| --- | --- | --- |
| K1 | Closed neutral WindowType; native-type normalization is adapter mechanics. Replace shell/tool/ownedDialog/dialog/noActivate/cannotControl projections with raw managed, acceptsInput, activationInhibited, toolWindow and owned plus neutral type/class. Retain independent privileged/transient/modal/resizable/relationships. Include native activationPolicy/layer needed by existing classification. Remove PresentationIngress.shell; core derives foreground eligibility from neutral facts. A fixed, ordered, unique unavailableFacts declaration distinguishes unsupported inputs from a supported input unreadable now; contradictory non-null rows hold narrowly. Core applies native predicates only where the declared input exists, never treats null as false. | Windows source reads WS_EX_TOOLWINDOW/WS_EX_NOACTIVATE/GW_OWNER/class (`crates/tiler-windows/src/tiling_sys/observe.rs:1307-1330`); its dialog/shell gates are policy, not raw facts (`src/tiling/classification.rs:143-185,203-218`, `src/tiling_sys/shell.rs:5-9`, relative to that crate). KDE managed/wantsInput are separate, and its dialog/owner/modal map to transient membership (`crates/omnitiler/src/kde_raw.rs:313-335,425-437`). macOS AX role/subrole, activation policy and CG layer are needed before its core handoff (`crates/tiler-macos/src/native_facts.rs:40-66,129-149`, `src/classifier.rs:51-70,332-380`, relative to that crate). Windows/macOS reviews must confirm final source-fit declarations. |
| K2 | nativeSelectorScope is a fixed per-connection per-output/all-outputs capability; ignored without hostOwned. All-output selectors make core's effective mode shared, preserving existing behavior while retaining raw requested settings. Separate mode gets unsupported-mode diagnostic. Native coupling option changes require reconnect/new handshake; remove adapter-local mode substitution at cutover. | Existing fallback is `kwin/src/workspace-native.ts:539-546`; native option source and coupling are `K:src/options.h:140`, `K:src/virtualdesktops.cpp:577-611`. Restart-required/validation diagnostics take precedence when applicable; no two diagnostics per setting. |
| K3 | Add above capability, raw above/below observations and persistent StateTarget/StateAttempt above property. Core owns existing KDE automatic acquisition, fullscreen suspension, external-clear/below latch and release/reset policy, plus sticky stacking decisions. R12 preimage capture/restore remains mechanical and covers coupled native properties. OneShotEffect::Stack remains one-shot, not persistent write authority. Raise the state-target bound from three to four. | Existing policy and reset/lifetime behavior: `kwin/src/world-native.ts:1095-1329`. Windows/macOS reviews MUST report whether and which float windows they currently keep above, so one core policy is uniform. Preliminary source extraction found Windows intentional floats using HWND_TOPMOST (`crates/tiler-windows/src/tiling_sys/float_toggle.rs:138-155`); macOS app-float level writes were not found in the bounded source search. Neither is a substitute for the full reviews. |
| K4 | Add warp hard capability. With false, core never emits non-null warpCursor. A valid enabled cursorWarp setting gets unsupported-capability diagnostic without discarding the raw value or blocking startup/valid siblings. | No public KDE script/effect warp API established at pin/floor; internal Cursor::setPos is not such an API. Settings support diagnostics are recomputed for each new handshake. |
| K5 | OPEN: no wire change. Prove the adapter-local capture-to-native-source witness join and eligibility coverage during the phase-2 KDE port, including accepted-anchor frame motion without a round trip. Extending FrameWitness is a contingency only if the local join cannot be proven. | Keep native-witness-join-evidence OPEN. Existing effect instance/counter and report obs remain distinct; elapsed time, geometry coincidence or a constant event name cannot close the proof gate. |

- K1 vocabulary choices are minimal current-producer distinctions: an independent
  owner flag can be known when its WindowId is unbound; transientFor alone is
  not that flag. Native acceptsInput and activationInhibited are not complements.
  Utility type and a native tool-window flag are independent. Existing AX
  activation-policy/layer inputs cannot be replaced with an adapter eligibility
  verdict. Optional resizability/input preference data adds no new admission
  policy. Native type conversion never selects tile/float/leave-unmanaged.
- Contract/fixture reconciliation complete: all existing message/scenario
  capabilities, raw window facts and foreground ingress use the new fields and
  table order. Existing intentional negative shapes remain negative; the root
  duplicate handshake also carries the new capability shape without removing
  its duplicate v key. No implementation or dependency files changed.
- Fixture delta: 88 new messages (35 positive shape witnesses, 53 negative
  cases), for 466 files (465 JSON, one malformed raw text). New cases cover
  every neutral type, activation-policy arm, signed-layer boundary, raw native
  flag, null/missing/removed field, unavailable-fact declaration validation and
  contradiction, both selector scopes, above target/attempt on/off, four-target
  bound, unsupported above/warp authority and both support diagnostics. All
  65 catalogs/342 arms have typed actual-value witnesses; 70 records, 7 message
  families, 24 commands and 57 distinctions remain covered.
- Ordered fixture delta: three new scenarios, raw-fact-availability,
  linked-selectors-warp and persistent-above. Total 17 scenarios, 207 steps,
  164 complete wire inputs and 277 jq assertions. The added oracles cover
  narrow supported-null hold/recovery versus fixed unavailable facts, conflicting
  non-null facts, effective shared selection and reconnect, disabled warp,
  automatic above acquisition/fullscreen suspension/classification release,
  external-clear/below latch and preowned reset/lifetime discard, unsupported
  stacking, and persist-before-write/coupled above-below recovery with wrong-tag
  and storage-failure refusal. Storage steps test already-authorized mechanical
  writes, not automatic-float eligibility. All eight original OPEN/TBD gates
  remain pending; K5's gate now records the phase-2 KDE proof and contingency.
- Verification: jq parses all 483 JSON artifacts; strict parsing finds exactly
  the two labelled duplicate-key negatives (v/gap) and one labelled malformed
  raw message. All 463 canonical messages pass exact compact bytes and no-final-LF
  checks; 11931 numeric tokens and four numeric witnesses pass lexical checks.
  Manifest rosters/pointers, 250 record witnesses, 433 enum witnesses, closed
  inventory, recursive positive/scenario key order and all 277 assertion
  compilations pass, with zero unmapped objects. Independent new-slice value
  checks accept all 263 positive messages and identify the intended violations
  in all 53 new negatives; 40 new-scenario wire inputs match their specified
  valid/held-row disposition. Scenario maps cover 235 rule-step references and
  71 contract anchors. These are static contract/fixture checks, not executed
  v4 engine/codec/scenario/native acceptance.
- Final hygiene: git diff --check clean; added tracked lines and all untracked
  files have zero non-ASCII bytes, and untracked whitespace/NUL checks are clean.
  Authority paragraph/anchors remain exact. Repository links compared with the
  accepted KDE-review baseline retain exactly five known failures, zero new.
  Windows/macOS source-fit reviews, their float-above baseline reports, 0c
  approval and native/implementation gates remain outstanding.

## Phase-0b Windows platform-fit review

- Verdict: **needs draft fixes and W1-W4 resolution before freeze**. Win32
  geometry, suppression, maximize/fullscreen, topmost and cursor mechanisms fit;
  native-free arrival/sticky semantics and sufficient application identity do
  not yet fit the literal draft. This is source evidence, not v4 execution or
  native acceptance. KDE conclusions are not Windows evidence.
- Citation shorthand below: `T/` = `crates/tiler-windows/src/tiling_sys/`,
  `W/` = `crates/tiler-windows/src/`. Searches covered those sources and their
  referenced native wrappers, recovery and renderer paths. Negative findings
  mean no producer in that route, not OS-wide impossibility.

### 1. Report production

| Fields/records | Finding and evidence | Verdict |
| --- | --- | --- |
| Envelope/settings/capabilities, inventory/closed, gesture/attempt lanes and action context | Existing sequencer envelopes captures; Windows sends hostOwned/equalCount/birthClear=false, native=[] (`T/world_sequencer.rs:370-386`, `T/world_capture.rs:1681-1729`). Preserve fresh facts/counters and move settings/capabilities, not policy projections. Cursor, hit test, close and gesture sources remain separate from command selection. | Fits; cutover work. |
| Window `win,out,rect,hints,fullscreen,maximized,minimized,hostHidden` | Process identity, DWM extended frame, iconic/zoom/cloak, styles and captionless full-monitor coverage are read (`T/observe.rs:1104-1136,1270-1341`). Fullscreen is this documented Windows observation, not game identity. Hint queries use bounded WM_GETMINMAXINFO, not resizability inference (`T/minmax.rs:73-114`, `W/ffi/window.rs:335-365`); capture currently uses the hint cache (`T/world_capture.rs:1227-1232`). Failed/uncaptured hints must stay unknown, not fabricated unconstrained facts. | Fits sources; raw capture conversion required. |
| K1 `privileged,activationInhibited,toolWindow,owned,resizable,above,owner,parent` | Token integrity and WS_EX_NOACTIVATE/TOOLWINDOW, GW_OWNER, WS_THICKFRAME and WS_EX_TOPMOST supply neutral inputs; bind relationship IDs only to verified lifetimes (`T/observe.rs:1104-1136,1307-1340`, `W/ffi/window.rs:74-97,159-171,205-219`). Current identity path filters elevated processes before setting elevated=false: that false is not a measurement of every candidate. Checked style/exstyle reads must distinguish failure. GetParent can return an owner for WS_POPUP, not a distinct parent ([API](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getparent)). | Fits API; do not copy current projected/unchecked negatives. |
| K1 unavailableFacts, `role,appId,class,title,type,activationPolicy,layer` | Correct the prospective six-input list: selected Win32 route also lacks persistent below and role. HWND_BOTTOM is not a below flag. Ordered unavailable inputs are `managed,acceptsInput,transient,modal,below,role,activationPolicy,layer`; with the presently absent appId/title readers, insert appId/title in catalog order too. This is route support; UIA/other metadata routes are not proved impossible. GW_OWNER does not prove transient/modal; IsWindowEnabled would not prove keyboard preference. Class is read; #32770 mechanically maps to dialog, ordinary top-level to normal, with styles/owner flags independent (`T/observe.rs:1307-1330`, `W/tiling/classification.rs:143-218`). Application identity remains W4, not an eligibility-valued appId. | Needs draft fix, applied; W4 gap. |
| Screens `out,full,work,complete,scale`, primary and notifications | PMv2 uses physical pixels; monitor rects and primary bit are available, primary-first ordering is explicit (`T/monitors.rs:4-18,21-77`, `T/world_capture.rs:1011-1033`). Scale is effective output DPI/96. Existing GetDpiForWindow wrapper (`W/ffi/window.rs:281-283`) only proves the queried window's DPI; use a PMv2 window on each output, including empty outputs, not a foreign DPI-unaware window ([API](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow)). WM_DPICHANGED is per affected window, not an all-monitor broadcast ([API](https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged)). Existing dirty hooks cover that message, WM_DISPLAYCHANGE and WM_SETTINGCHANGE (`T/monitors.rs:30-49`), but do not prove empty-output scale-only coverage; the current windowless fallback is not a v4 event-coverage proof. | Fits units/API; per-output producer/notification coverage still required. |
| `identity.manufacturer,model,serial,connector` | Current MonitorArea has only device/full/work, no physical metadata (`T/monitors.rs:21-28`). QueryDisplayConfig/DisplayConfigGetDeviceInfo target data exposes validated EDID manufacturer/product IDs, monitor device path, technology and connectorInstance ([API](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_target_device_name)). It does NOT return EDID serial. Join the monitor device through SetupAPI and read EDID from its device key ([API](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdiopendevregkey)); unreadable/missing serial stays null. Connector instance is adapter-local and may be zero: never use it alone or DISPLAY ordinal as a durable connection ID. Preserve native adapter/target connection provenance; exact normalization/hotplug proof remains. | Fits nullable API route; no existing producer or universal EDID/connector guarantee. |
| Unmanaged foreground, `focused`, foregroundOut, fullscreen ownership/exitability | Presentation samples foreground independently of admission (`T/world_capture.rs:476-529`); action foreground includes unmanaged frames (`:1639-1655`), while focused currently requires an enumerated token (`:1686-1690`). v4 must include independently captured neutral foreground facts/output, not infer false on filtered/elevated/unreadable subjects. Verified fullscreen metadata can prove project ownership; missing/corrupt metadata does not prove a safely exitable app state (`T/fullscreen.rs:75-125,286-347`). | Fits known/nullable facts; R14 gate remains. |
| Window lifetime, intentRecords, visibility/lastWrite/suppressionAvailable | TokenMap key is HWND+process creation; member identity adds PID and a window property tag (`W/tiling/classification.rs:4-47`, `W/native_guards.rs:17-69`). Admission detects missing/mismatched tag, drops old state and reissues a token (`T/assignments.rs:59-137`). Same-process HWND reuse is NOT fenced by PID/creation alone. Untagged/unmanaged foreground identity still needs conservative retirement/reminting; event-delivery pid/tag reads explicitly do not prove the original lifetime (`T/observe.rs:477-501`). Storage/recovery facts are addressed below; never populate them from desired state. | Fits tagged members; untagged historical no-reuse proof remains a port gate. |

### 2. Reply application, G4 and recovery

| Lane | Finding and evidence | Verdict |
| --- | --- | --- |
| Frame/member/clear; scopes/output/workspace/transfer rows | Held identity, visible-to-outer conversion and post-call reads already exist (`T/world_native_frame.rs:15-81`, `T/world_native.rs:1882-1992`). Keep frame attempts, maximize-clear and membership mechanics separate. Current membership also chooses show/hide from local logical views: replace that with the explicit visibility authority, not a second visibility owner. Native-free view/create/removal attempts do not fabricate native desktop effects. | Fits mechanics; remove local policy. |
| Maximize/fullscreen/above state | Restore placement, project fullscreen enter/exit and SetWindowPos TOPMOST/NOTOPMOST are usable mechanics (`T/world_native.rs:1891-1903`, `T/fullscreen.rs:286-467`, `W/ffi/window.rs:142-171`). Apply core-decided booleans and raw immediate pre/post, not existing toggle/continuation verdicts. Project fullscreen restore currently verifies style and setter acceptance, not settled outer geometry. Sticky is not a native state on this route: W3. | Fits except W3; R12/R14 prerequisites. |
| Visibility, LastWrite, FocusFeedback, SwitchEvent | Durable hide commit precedes ShowWindowAsync; reveal uses stored show state and nonactivating commands (`W/product_hide/sys/mod.rs:1761-1793,1875-1884,1929-1944`). v4 needs a new executor retaining spent lifetime/visibility.rev/target across observations, batches shows, dispatches focus once, and imposes the same/earlier-revision cross-output hide barrier. Fallback permission comes only from core; native refusals/unknowns are facts, terminal verdicts remain core. Existing local view/membership receipts cannot satisfy the draft's native-home arrival bracket: W2 (`T/world_native.rs:1137-1185,1737-1755`, `T/world_native_receipts.rs:54-81,99-135`). | Fits native show/hide; W2 blocks literal arrival proof. |
| R12 host slot and watcher | Hide ledger stores identity plus minimized/maximized show preimage, not styles/topmost (`W/model.rs:64-115`). Watcher binds the exact owner process and restores the ledger after exit; later startup can reclaim it (`W/product_hide/sys/watcher.rs:123-263`). Fullscreen properties already store inert style/max/frame values before effects (`T/fullscreen.rs:8-45,286-347`), but watcher does NOT consume them. Float topmost preimage is runtime-only and lost on crash (`T/run_loop.rs:1481-1523`). Extend lifetime-bound recovery storage/consumers for chrome and topmost before authorizing those writes; next-start recovery and watcher chrome restoration are target requirements, not current features. Failed persistence disables that property only. | Fits storage mechanism; recovery implementation/proof gap. |
| warpCursor, results and intentWrites | SetCursorPos is wired with pending-hook/bounds gates (`T/world_native.rs:337-343,2483-2500`): warp=true, not guaranteed success or permission. Result/terminal/diagnostic rows convey core outcomes, never native write authority. Opaque storage conversion is below. | Fits. |
| Anchored border and renderer input | WinEvents include foreground/create/destroy/show/hide/location (`T/observe.rs:416-434`). Current border posts absolute core geometry with tag/PID/creation and renderer probes (`T/focus.rs:263-359`, `T/render_bridge.rs:139-205`, `W/render_pump/native/mod.rs:249-318`); underlay anchor resolution is not border anchor motion. FrameWitness capture-obs/native-event/render join is not implemented. Callback only bumps a visibility generation, not world obs (`T/observe.rs:546-570`, `T/world_owner.rs:231-232`); preserve a synchronous invalidation join and cover state/style/eligibility changes before accepting event-driven anchor motion. Pager content has reply req/obs/rev, but clicks locally validate current ring instead of conveying drawn rev+RenderTarget (`T/pager_host.rs:864-880,1148-1218`). Retain the actually drawn row/rev and let core judge staleness. | Fits event/input mechanisms; witness coverage remains OPEN, not paint acceptance. |

### 3. K3 float-above baseline

- Windows DOES keep some floats above. Explicit tiled-to-float (also sticky-on
  from tiled) reads current topmost; preowned topmost gets NOZORDER, otherwise
  placement acquires TOPMOST nonactivating. It retains the first prior band in
  `float_topmost_prev` after setter/PID success, then verifies frame/band and
  commits intent (`T/float_toggle.rs:138-245,269-302`). This is not every float.
- Automatic/capacity float adoption does not acquire topmost
  (`T/capacity_floats.rs:67-120`). Sticky-on from an existing float only installs
  its marker and preserves frame/classification (`T/dispatch.rs:970-1002`).
  A capacity re-float after attempted unfloat keeps an already acquired band;
  successful tile return restores it (`T/float_toggle.rs:624-690`).
- Release runs on successful unfloat/sticky-to-tile or graceful stop, under
  fresh identity gates; it lowers only `!prior && current`, never preowned
  topmost or an already externally cleared band. Failed restore retains the
  map during execution (`T/float_toggle.rs:319-408`, `W/tiling/float.rs:172-179`,
  `T/dispatch.rs:689-699,714-767`). Stop clears the runtime map regardless of
  failures. Crash/restart loses it. No corresponding Windows fullscreen
  suspension or KDE external-clear/reset latch was found in these band paths.
- K3's KDE automatic-only eligibility therefore does NOT preserve Windows
  explicit-float behavior. Proposed uniform policy and consequences are W1;
  neither floating=true nor a one-shot Stack authorizes persistent topmost.

### 4. Windows HostCapabilities

| Field | Source-fit value | Qualification |
| --- | --- | --- |
| hostOwned / equalCount / birthClear | false / false / false | Existing explicit report values above; birthClear=false preserves the Windows admission preamble, not absence of maximize actuation. G2 retirement and native-creation correlation are inapplicable to this route, not missing native API claims. |
| nativeSelectorScope | per-output, ignored | Required filler when hostOwned=false; no native selector scope is being claimed. |
| windowSuppression | true | Existing durable independent hide/reveal/watcher mechanism; per-window gates/readability can still refuse. |
| maximize / fullscreen / above / warp | true / true / true / true | Native mechanisms above; capability is not per-window authorization, safe app-owned fullscreen exit, or completed R12 recovery. |
| sticky | Unresolved under draft wording (W3) | Logical sticky commands are supported; native sticky actuation is not. A single unconditional true/false under the present native-state definition misstates one of those facts. |
| tiledFloatTransfer | false | Correct polarity for the selected no-remap route, NOT a Windows API impossibility or a ban on ordinary floats. |
| unavailableFacts | Ordered route list in section 1 | Include appId/title until actual metadata readers exist; W4 must settle sufficient identity. Do not declare supported-but-failed reads permanently unavailable. |

- Exact float-no-remap mechanism: candidate clone runs the report once. For
  Send/SendDirectional/MoveDirectional and legacy equivalent output-send/move,
  the guard requires a report-time tiled mover, cross-output Window transfer
  matching the command, and a floating staged row; it rejects without adopting
  the candidate (`T/world_sequencer.rs:868-946`). Same-output and already-float
  movers are exempt; it never branches on float_origin. Floats carry no desired
  frame and preserve physical geometry, so a new capacity float cannot establish
  destination affinity using this transfer path (`T/world_native.rs:1162-1185,
  1960-1991`). `tiledFloatTransfer` fits this scope; core must preserve source
  state and incorporate observations in an accepted rejected-command reply,
  not migrate the adapter's whole-report rejection or command-prefix parsing.

### 5. Intent storage and 6. R14 baseline

- Windows properties are HANDLE-sized values, not owned byte buffers
  ([API](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-properties),
  `W/ffi/window.rs:245-279`). Existing float/tile/sticky values are inert scalars;
  sticky encodes prior float intent. They belong to the live native window and
  survive tiler restart while that window survives, not destruction/HWND reuse.
  Restart re-adopts classification only under admission/identity/tag fences
  (`W/native_guards.rs:146-265`, `T/restart.rs:307-315,450-502`); no logical
  workspace/output map is restored.
- v4 must store the exact opaque <=1024-byte record in lifetime-bound storage,
  not a pointer into the exited process or reconstructed adapter classifications.
  A recoverable external slot keyed by the verified live window tag, or bounded
  inert property chunks with atomic publication, can provide the mechanism;
  neither is implemented for v4. Preserve original ownership evidence before
  admission rotates member tags; never match a record by title/PID alone.
  Windows host-storage-survival is resolved for existing scalar properties and
  exact-window ownership only; opaque record atomicity/recovery and native
  acceptance remain OPEN, as does the macOS portion of the shared gate.
- R14 needs an adoption-time native baseline separate from the project-owned
  marker, with R12 preimages and truthful exitability, and settled restore-frame
  proof stronger than today's style-only completion. Keep refusal for browser
  F11, player, borderless game and exclusive game until each baseline case is
  accepted; first-seen fullscreen has no baseline and remains refused. Absence
  of a marker does not create exit authority. No Alt+Enter synthesis or binding
  change (`docs/changes/windows-fullscreen-exit-baseline.md:5-15,19-56`).

### 7. Gaps requiring resolution, not decisions taken

| Gap/question | Options; recommendation; consequence |
| --- | --- |
| W1: which float origins acquire persistent above uniformly? | Options: KDE automatic-only; Windows explicit-only; union of qualified automatic and explicit float/sticky intent. Recommend the union, with one core-owned acquisition/release/fullscreen-suspension/external-clear latch and native-preowned preservation policy. Do not acquire merely from floating=true (workspace-floating/capacity fallback alone is not intent). This retains Windows explicit floats and adds automatic stacking there; it adds explicit stacking to the KDE baseline. Apply only when above is supported and durable preimages exist; sticky transitions retain/release the same ownership, not a second owner. macOS evidence still needed. Eligibility/scenario changes require 0c approval; current K3 fixtures prove only its automatic baseline. |
| W2: what proves native-free arrival home after G1 removes logical echoes? | Options: native-home requirement only for hostOwned, with core-retained home plus bracketed native output/frame/visibility/lifetime for Windows; a core-authorized host property binding with explicit non-native provenance; leave arrival-ready focus unsupported. Recommend the first, specifying an observed-output bracket arm and core comparison to the frozen target. The present home-required bracket cannot be truthfully filled from applied_homing. Shape/settlement changes need fixture updates; no invented receipt or restored ScreenView. |
| W3: how is sticky supported without a native sticky state? | Options: define sticky capability as semantic support with a core-only native-free route and no fabricated state target/attempt; split nativeSticky from command support; report unsupported Windows sticky. Recommend core-only native-free behavior, composing visibility from retained sticky intent and persisting it opaquely. Current capture uses state.sticky (`T/world_capture.rs:1072-1080,1159-1160`), not an OS fact. Literal native state/attempt semantics would copy projected intent back to core; false would regress existing commands. Clarify null applicability, capability and state-target rules before changing fixtures. |
| W4: can core retain executable-dependent safety/presentation without adapter verdicts? | Class alone is insufficient: shell-popup exclusion also checks executable basename, and own Settings identity checks executable equality (`T/shell.rs:12-46`, `W/tiling/classification.rs:154-185`, `T/world_capture.rs:480-488`). Options: specify sufficient neutral executable/application identity plus the adapter application's identity; give appId a precise native identity encoding that supplies both; weaken the existing exceptions. Recommend sufficient raw identity, not a boolean shell/own-settings verdict or an undocumented AppUserModelID/executable conflation. Missing self identity and foreign class reuse must not force classification back into the adapter. Fields/encoding/privacy and fixture consequences require review. |

- Minimal unresolved behavioral discriminators are recorded in
  [reference outcomes](../spec/reference-outcomes.md#v4-windows-platform-fit-discriminators);
  outcomes remain TBD, not approved by this source review.
- Windows source mechanisms resolved: physical scale units; explicit primary;
  EDID/connection API route with truthful nulls; independent unmanaged foreground
  capture; tagged-member reuse fencing; existing property survival; suppression
  recovery; warp=true; tiledFloatTransfer=false and its precise limitation.
  Native removal/creation gates do not apply with hostOwned=false.
- Remaining OPEN: W1-W4; opaque-record and chrome/topmost crash recovery; empty-
  output scale-only notifications; untagged lifetime fencing; exact border
  witness/invalidation join and event coverage; physical identity normalization;
  app-fullscreen baseline acceptance; all macOS evidence. None of the eight
  shared scenario gates is promoted to a pass.
- Mechanical draft fixes: Windows effective DPI/96; literal #32770 normalization;
  independent topmost/below and parent/owner semantics; corrected route-level
  unavailable facts; explicit W1-W3 blockers. No wire shape/fixture bytes,
  implementation, dependencies or product decision changed.
- Verification: `git diff --check` clean; added tracked lines and all untracked
  files contain zero non-ASCII bytes, with no untracked whitespace/NUL issues.
  Authority paragraph/anchors remain exact. Local Markdown links retain only
  the five known failures, zero new. Static fixture checks parse 483 JSON
  artifacts, preserve the two deliberate duplicate-key negatives and one raw
  malformed message, and validate 466 message/17 scenario inventories, 207
  steps, 235 rule-step references and 71 contract pointers. Canonical messages
  retain no final LF; fixture bytes are unchanged. These checks do not execute
  v4 codecs, native scenarios, recovery or paint. W1-W4 and the implementation/
  native gates above remain unresolved; this review does not approve cutover.

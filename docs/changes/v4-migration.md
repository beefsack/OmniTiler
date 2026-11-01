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
  carry-forward checklist approach remain accepted.
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
| `WindowFact`, WR:706-721; keys `win,out,rect,floating,fitExcluded,fullscreen,maximized,sticky,fixedAuto,fixedSuppress,minimized,hints,observedWs,nativeWs`, WJ:570-622 | C: replace adapter classification/intent projections (`floating,fitExcluded,fixedAuto,fixedSuppress,observedWs`) with normalized native classification facts and core-owned intent; unknown observations are optional. Retain `win` as lifetime id, not a new second `lifetime` namespace. Extend fullscreen, visibility and recoverable suppression facts. Tiler-owned completion uses retained core intent plus visibility facts; nativeWs remains native observation. | Facts, Intent, R13, R14, Visibility; [classification](../decisions/engine.md#classification-and-convergence-ownership); correction evidence below |
| `ScreenView {output,view}`, WR:729-732; keys `out,view`, WJ:645-658 | R: no logical view ingress; native selectors keep separate native evidence. See G1 technical reconciliation below. | Visibility:2793-2796; R19 reconciled under G1 |
| `NativeObservation {output,native_order,current_native,occupancy}`, WR:775-780; keys `out,order,current,occupancy`, WJ:660-731 | C only native id typing; otherwise U. Preserve `current:null` non-ordered-surface meaning and incompleteness holds. Do NOT resurrect `orderKnown/currentKnown`: canonical Windows decision explicitly supersedes native known-state flags. | R11; [accepted protocol fences](../decisions/windows.md#accepted-protocol-and-phase-one-fences), lines 39-43 |
| `NativeOccupancy {native,count}`, WR:738-741 | C native id type, U `{native,count}` and null-is-unknown/count coverage. | R11; baseline |
| `PresentationIngress`, WR:633-652; wire keys WJ:1127-1221 | C: settings leave ingress; remove `paused`; `cloaked` becomes host-hidden vocabulary; unknown foreground facts remain representable, including unmanaged foreground. Keep actual chord/move/order/live-frame facts; no adapter eligibility verdict. | Split, Facts, R5, R6, R10 |
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
| Native state targets/authority and raw state actuation facts | A only what's needed to apply core fullscreen/sticky decisions safely and report actual result; keep existing maximize result. Exact added fields are to be drafted, not generic `Applied` replacement. | R14, state-centric contract |
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
| `manifest.json` | One metadata file; all paths, directions, expectations, 7 message families, 24 verbs, 70 records, 59 closed catalogs/300 arms and 51 sentinel/presence assertions. |
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

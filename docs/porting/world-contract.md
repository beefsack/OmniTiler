# World contract (v4 target)

Status: normative target draft for phase 0c review, not implemented authority.
v4 is the implemented v3 wire plus the approved deltas below, nothing else.
Inline OPEN points await [phase-0b evidence](../changes/v4-migration.md#b-reconciliations-and-open-points).

## Contract authority rule

The v3 contract as implemented is authoritative now. Each approved change
becomes authoritative only at one coordinated implementation cutover across
core and all adapters. There are no dual message shapes: until that cutover,
the v3 shape governs; at cutover, the coordinated v4 shape replaces it.
The [approved v4 pointers](#approved-not-yet-implemented-v4) record target
mismatches, not interim authority.
See the [authority decision](../decisions/contract.md#contract-authority-and-v4-migration).

<a id="approved-not-yet-implemented-v4"></a>

## Approved v4 deltas

| Delta | Rule location | Authority |
| --- | --- | --- |
| Raw facts; core classification and intent | [Facts](#report-facts), [intent](#opaque-intent-storage) | [Facts](../decisions/contract.md#facts-versus-classification), [classification](../decisions/engine.md#classification-and-convergence-ownership), [intent](../decisions/contract.md#intent-ownership-and-persistence) |
| Capability handshake, lifetime and settings | [Handshake](#handshake-and-lifetime), [settings](#revisioned-settings) | [Split](../decisions/contract.md#host-capabilities-and-settings), [R12/R13 detail](../changes/core-interface-review.md#original-backlog-lines-103-122) |
| Primary output, scale, physical identity | [Screens](#screenreport) | [Primary](../decisions/contract.md#primary-output), [R10](../decisions/contract.md#size-units-and-scaling-r10), [R11](../decisions/contract.md#output-identity-r11) |
| Shared policy; unconditional fullscreen suspension | [Ownership](#ownership-and-g4-exceptions), [presentation](#presentation-and-renderer-input) | [R5](../changes/core-interface-review.md#original-backlog-lines-132-158), [suspension](../decisions/world-rendering.md#focused-fullscreen-presentation-suspension) |
| Typed vocabulary and reduced/promoted commands | [Types](#wire-conventions-and-bounds), [commands](#commands-and-results), [attempts](#raw-attempts-and-settlement) | [R6](../changes/core-interface-review.md#original-backlog-lines-159-186), [send source](../decisions/contract.md#send-source-fallback) |
| Rejected command in accepted reply; narrow diagnostics | [Results](#commandresult-and-diagnostics) | [Result](../decisions/contract.md#command-outcome-in-reply), [R13](../changes/core-interface-review.md#original-backlog-lines-103-122) |
| Recoverable native writes; independent suppression | [Recovery](#freshness-and-recovery), [visibility](#visibility-and-focus-ordering) | [R12](../decisions/contract.md#native-writes-difficult-to-undo-r12), [visibility](../decisions/windows.md#state-centric-v4-visibility) |
| Fullscreen/sticky world commands | [State](#window-state-policy) | [R14](../decisions/contract.md#window-state-commands-r14), [Windows baseline](../changes/windows-fullscreen-exit-baseline.md) |
| Native backing and consolidated references | [Native facts](#native-observation), [attempts](#raw-attempts-and-settlement) | [R19](../decisions/layout-and-admission.md#native-workspace-adoption-and-r19), [G1 reconciliation](../changes/v4-migration.md#b-reconciliations-and-open-points) |
| Named decorations, shared revision, anchored border | [Presentation](#presentation-and-renderer-input) | [Decorations](../decisions/contract.md#decorations), [R20](../decisions/contract.md#anchored-border-r20) |
| Exact conformance and shared scripting verbs | [Conformance](#conformance), [commands](#commands-and-results) | [Testing](../principles.md#testing), [R18](../decisions/contract.md#scripting-command-surface-r18) |

## Ownership and G4 exceptions

- One `WorldEngine` owns logical outputs, workspaces, order, views, membership,
  lifecycle, classification, user intent, focus/cursor policy and presentation.
  Adapters capture/normalize native facts, execute authorized native mechanics,
  store opaque records and render. Native workspace domains are observations,
  not mapping owners. No adapter target selection, classification verdict,
  settlement verdict or rejection-recovery replan crosses this boundary.
  [Boundary](../decisions/engine.md#core-owned-world-and-platform-boundary).
- The exhaustive G4 no-round-trip exception list is:
  1. Mechanical last-moment safety veto for vanished/replaced lifetime, stale
     identity/facts/handle, unreadable scope, native permission failure or live
     game/fullscreen safety. Veto the affected action only; report actual
     refusal/unknown. Do not substitute another target or outcome.
  2. Same-host-turn application of an already core-decided result when another
     round trip would cause jank, excessive cost or unsafe execution. This
     executes a decision; it never creates one.
  Every other adapter decision requires its own decision entry and backlog item
  moving it into core. Observation, coordinate conversion, recovery and native
  paint are mechanics, not additional policy exceptions.
  [G4](../decisions/engine.md#core-owned-world-and-platform-boundary).
- Automatic-focus guards are the [core rules](../decisions/focus.md#automatic-focus-guards)
  on every host. Automatic fixed-size float sends are allowed everywhere; host
  limits must not reintroduce platform policy opt-ins.
  [R5](../changes/core-interface-review.md#original-backlog-lines-132-158).

## Wire conventions and bounds

- Objects are closed: unknown keys/enum values and wrong JSON types refuse,
  except the per-field settings and identifiable fact-row validation specified
  below. Duplicate keys refuse. Required means present; nullable does not mean optional.
  Tables use `no`, `yes`, `omit`, or `empty` in the Null/absence column; `empty`
  means the existing empty-string sentinel, not JSON null. Keys grouped in one
  row each have that row's type. Message field order is table order; a grouped
  row uses the listed key order. Compact UTF-8 JSON has no insignificant
  whitespace or trailing newline; escape quote/backslash, encode C0 controls as
  lowercase `\u00xx`, pass other Unicode through. Decoders accept insignificant
  whitespace and object order differences. Exact duplicates compare decoded
  content, not whitespace. [Codec baseline](../../crates/tiler-protocol/src/world_protocol.rs).
- Canonical JSON numbers use finite IEEE-754 binary64 values. Choose the
  shortest decimal significand that round-trips to that value; among equally
  short choices choose the closest decimal, with an even last digit breaking
  an exact tie. Expand any exponent into ordinary decimal notation, strip
  redundant fractional trailing zeros and the decimal point if integral, and
  encode either signed zero as `0`. Never emit an exponent, leading plus or
  redundant leading zeros. Examples: `1.25`, `1.5`, `3` (not `3.0`), and
  `0.0000001` (not `1e-7`). Decoders accept any finite JSON number in range
  for number-valued fields, including fractional, exponent and signed-zero
  spellings; this does not relax integer-only fields or Counter strings.
- `Counter` is a canonical decimal `u64` string (`0` or a nonzero leading digit
  followed by digits), never a JSON number. Overflow refuses; counters never
  wrap. `Id` is 1..128 ASCII bytes from `[A-Za-z0-9-_.]`. `WindowId` already
  denotes a native lifetime and must not be reused within an adapter lifetime.
  `OutputId`, logical `WorkspaceId`, `NativeWorkspaceId`, operation/effect ids
  and node ids remain distinct types despite identical string encoding. A
  native id may be adapter-minted but must survive host reindexing in that
  lifetime. `gen` is 1..64 lowercase ASCII letters/digits/hyphens.
  [Baseline bounds](../../crates/tiler-core/src/bounds.rs), [identity](../../crates/tiler-core/src/ids.rs),
  [R6](../changes/core-interface-review.md#original-backlog-lines-159-186).
- `Rect` is `[x,y,w,h]` with integer coordinates, x/y in -16384..16384 and
  positive w/h <=16384, checked edges; `Point` is `[x,y]` under the existing
  point bound (-16384..16384 on each axis). Screens, work areas, frames, cursor
  and absolute presentation
  share one coherent adapter coordinate space. Native/client/frame conversions
  stay adapter-side; size hints are normalized with actual decoration extents,
  never another host's compensation. Exact arrival remains exact.
  [Baseline geometry](../../crates/tiler-core/src/bounds.rs),
  [coordinates](world-contract-v3.md#coordinates-and-safety).
- Requests and replies are independently <=4 MiB, validated before candidate
  mutation and checked against exact encoding before commit. No partial
  oversized state applies. Retain caps: screens 16; windows/closed 10000 each;
  frame/member/view/clear attempts 1024 each; creation attempts 256; native rows
  4096 with <=256 ids per output; occupancy <=10000 per desktop; pager order and
  frame sources 1024 each. New intent return/write arrays <=10000 each;
  state attempts/focus feedback <=1024 each; at most three state targets per
  window; arrival brackets <=10000 total per report; one renderer input at most.
  Failed-window lists are subsets of the frozen <=10000-window roster;
  diagnostics have at most one entry per held row, never unbounded queues.
  Metadata text below is UTF-8 <=4096 bytes each (display identity <=256);
  over-bound values are unreadable row facts, not silently truncated identity.
  [Bounds](../../crates/tiler-core/src/world_report.rs),
  [transport](../decisions/engine.md#world-transport-authority-and-boundedness).

### New typed references

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `AuthorityScope` | `kind` | `output` or `workspace` | no | Scope arm, replaces parsing composite text. |
| | `out` | OutputId | no | Owning live output. |
| | `ws` | WorkspaceId | omit for output arm | Exact logical workspace for workspace arm. |
| `WorkspaceRef` | `kind` | `logical` or `native` | no | Namespace, not inferred from string. |
| | `out` | OutputId | no | Output at sampling time. |
| | `ws` | WorkspaceId or NativeWorkspaceId | no | Id in the tagged namespace. |

- `ScopeAuthority` keeps `controlled/untouched/hold`; a hold reason is one of
  `unreadable,incomplete-inventory,gesture-barrier,refused-target,ambiguous-creation,occupied-removal,constraint-clamp,minimized`.
  Existing flattened auth/reason pairs remain flattened. Non-hold reason is `""`.
  [Types](../../crates/tiler-core/src/world_report.rs).

## Handshake and lifetime

- Only new handshake/settings families have `type`; existing report/reply/error
  discriminants and short keys remain. The connection authenticates one adapter
  lifetime, named by `adapter` in the handshake. Core mints `gen`; reports no
  longer carry `owner` or mint/rebind generations. A reconnect starts fresh core
  state and counters; it cancels correlations, flights, caches and ordering.
  No retained-incarnation/session restore or replay crosses lifetimes.
  [Lifetime split](../changes/core-interface-review.md#original-backlog-lines-103-122).

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `HandshakeRequest` | `type` | `handshake` | no | New routing tag. |
| | `v` | integer 4 | no | Exact contract version. |
| | `adapter` | Id | no | Sole adapter lifetime. |
| | `req` | Id | no | Handshake correlation. |
| | `capabilities` | HostCapabilities | no | Fixed hard host limits. |
| `HandshakeReply` | `type` | `handshake-reply` | no | New routing tag. |
| | `v` | integer 4 | no | Selected version, not negotiation of dual shapes. |
| | `adapter,req` | Id | no | Request correlation. |
| | `gen` | generation string | yes | Core-minted on accepted handshake; null on refusal. |
| | `ready` | bool | no | False: no world writes/effects authorized. |
| | `kind` | `""` or ProtocolKind | empty | Empty on acceptance; refusal kind otherwise. |
| `HostCapabilities` | `hostOwned,equalCount,birthClear` | bool | no | Existing native-domain/equal-count/automatic-birth-clear mechanisms. |
| | `windowSuppression` | bool | no | Independent recoverable per-window hide mechanism. |
| | `maximize,fullscreen,sticky` | bool | no | Native state actuation support, not permission. |
| | `tiledFloatTransfer` | bool | no | Tiled source can become a float while crossing outputs without unsupported remap. |

- `ready:false` is normal after an accepted handshake; readiness requires a
  complete usable report and initial settings. Hard support cannot flip per
  report. Unsupported versions use the bounded error shape below.
  [Capabilities](../decisions/contract.md#host-capabilities-and-settings).
- OPEN: `tiledFloatTransfer` is the draft spelling/scope of the existing
  `float-no-remap` limitation, not a ban on ordinary floating transfers. Phase
  0b must establish the exact host mechanism before this capability is frozen.
  No native-removal capability is added: see [G2](../changes/v4-migration.md#b-reconciliations-and-open-points).

## Revisioned settings

- Send one complete raw settings snapshot at startup and on change. Core
  validates each field, applies valid siblings, and keeps last-good/default for
  invalid fields. Missing known fields use defaults at startup, last-good later;
  unknown setting names receive per-field diagnostics, not startup refusal.
  Duplicate revision/content is idempotent; older revision returns `stale`
  without replacing state; equal revision with different content returns
  `revision-reused` without replacing state. Receipt order does not change a
  setting's catalog timing; `mode` (`workspace.mode`) is restart-only. A changed
  valid later value returns `restart-required` and takes effect only in a new lifetime.
  [Validation](../changes/core-interface-review.md#original-backlog-lines-103-122),
  [catalog/timing](../../crates/tiler-core/src/settings_catalog.rs).

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `Settings` | `type` | `settings` | no | New message family. |
| | `v,adapter,gen,req` | 4, Id, generation, Id | no | Current lifetime and correlation. |
| | `rev` | Counter | no | Independent settings revision. |
| | `values` | object of catalog values | no | Raw values, no adapter clamp/default. |
| `SettingsReply` | `type` | `settings-reply` | no | Settings acknowledgement only; no world authority. |
| | `v,adapter,gen,req,rev` | same envelope types | no | Exact settings input correlation. |
| | `status` | `accepted/stale/revision-reused` | no | Revision disposition. |
| | `diagnostics` | SettingsDiagnostic[] | no | At most one per supplied/missing catalog key. |
| `SettingsDiagnostic` | `key` | catalog name or bounded text | no | Affected field, not raw supplied value. |
| | `kind` | `missing/unknown-field/wrong-type/out-of-range/unknown-value/restart-required` | no | Closed typed diagnostic. |

### Settings values

| Key | Type/range or tokens | Default | Application timing |
| --- | --- | --- | --- |
| `gap`, `outer` | integer 0..64 logical px | 8 each | Next plan (`gaps.inner/outer`). |
| `defaultTiled` | bool | true | Subsequent new workspaces (`workspace.default-tiled`). |
| `mode` | `per-output-local/global-unique/shared` | per-output-local | Restart (`workspace.mode`). |
| `sameAxis` | `group-with-neighbor/swap-with-neighbor` | group-with-neighbor | Subsequent move (`move.same-axis`). |
| `bornMaximized` | `tile-on-launch/keep-maximized-overlay` | tile-on-launch | Subsequent admission (`admission.born-maximized`). |
| `predicate` | `both-axes-fixed/either-axis-fixed` | both-axes-fixed | Subsequent admissions only; retained classifications unchanged (`admission.fixed-size`). |
| `anchorFallback` | `largest-splittable-window/closest-splittable-relative/no-search` | largest-splittable-window | Subsequent admission (`admission.anchor-fallback`). |
| `noSpaceResult` | `float/allow-overlap` | float | Subsequent admission/actual capacity repair, not reflow on apply (`admission.no-space`). |
| `refill` | `last-remaining-workspace/most-recently-used-workspace` | last-remaining-workspace | Subsequent migration (`migration.refill`). |
| `switchTarget` | `cursor-output/focused-output` | cursor-output | Subsequent selection (`output.switch-target`). |
| `freshPlacement` | `cursor-output-last-focused/cursor-output-cursor-closest/focused-output-last-focused` | cursor-output-last-focused | Subsequent admission (`output.fresh-placement`). |
| `focusFollowsCursor`, `cursorWarp` | bool | false each | Live; next applicable input/plan (`output.focus-follows-cursor/cursor-warp`). |
| `borderEnabled`, `underlayEnabled` | bool | true each | Next presentation (`border.enabled/underlay.enabled`). |
| `borderWidth` | finite number 0..32 logical px | 3.0 | Next presentation (`border.width`). |
| `borderGap` | finite number 0..64 logical px | 0.0 | Next presentation (`border.gap`). |
| `borderRadius` | finite number 0..64 logical px | 6.0 | Next presentation (`border.radius`). |
| `underlayExtension` | finite number -1..32 logical px | -1.0 | Next presentation; negative sentinel follows border width (`underlay.extension`). |

- Table keys preserve existing short wire names; catalog ids appear in the
  timing column (`sameAxis` and `borderRadius` are newly conveyed settings).
  Values/defaults/ranges are from [settings_catalog.rs](../../crates/tiler-core/src/settings_catalog.rs).
  Native colors/theme/artwork are not computational wire settings. Style values
  remain fractional numbers, not integerized ingress echoes. Per-output gaps
  are computed from these logical values, no longer supplied on screen rows.

### Scale and rounding

- `scale` is a finite positive JSON number: adapter units per logical pixel on
  every screen report. In KWin's logical coordinate space it is `1`, not the
  output's physical/logical device-pixel ratio. Core retains no scale;
  scale/resolution changes invalidate the report and replan. Multiply each
  logical setting/constant once by the current target output scale, then round
  to nearest integer, exact halves toward positive infinity (`floor(x+0.5)`),
  using checked arithmetic. Adapters normalize native fractional coordinates
  consistently using their existing topology rule; native details are in the
  [KDE](kde-port-notes.md#v4-native-coordinate-normalization) and
  [macOS](macos-port-notes.md#v4-native-coordinate-normalization) port notes.
  Resolve the negative underlay sentinel to border width before
  scaling. No silent saturation or tolerance: a nonfinite/unrepresentable
  result holds the affected output as
  unreadable. Apply existing geometry/style bounds after scaling, not the
  logical settings range as a physical-pixel clamp.
  [R10](../decisions/contract.md#size-units-and-scaling-r10),
  [quantization](../changes/core-interface-review.md#original-backlog-lines-187-193).
- Boundary examples: 8*1.25 = 10; 3*1.5 = 4.5 -> 5; 0.49 -> 0;
  0.5 -> 1; 0 remains 0.
  A scaled style of 1024 is valid, 1024.5 rounds to 1025 and holds; a rectangle
  whose rounded width is 0 or 16385 is unreadable, not inflated/clamped. Scale applies to
  gaps, border/underlay, drop zones, resize minima and keyboard steps, not
  already-normalized native coordinates a second time.

## Report facts

### WorldReport

- Every report is a full factual snapshot, not a patch. `null` means unknown
  where declared, never false; neither absent/held rows nor unreadability imply
  closure. Explicit `closed` always closes that lifetime; implicit missing =
  closed only with complete inventory. A bad identifiable row holds its
  narrowest affected scope as unreadable and emits a diagnostic; invalid
  identity/envelope/structural bounds refuse the whole report before mutation.
  [Facts](../decisions/contract.md#facts-versus-classification),
  [row resilience](../changes/core-interface-review.md#original-backlog-lines-103-122).

| Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `v` | integer 4 | no | Version. |
| `adapter,gen` | Id, generation | no | Handshake lifetime. |
| `obs,reqSeq` | Counter | no | Observation watermark; ordered request sequence. |
| `req` | Id | no | Request correlation. |
| `cmdSeq` | Counter | yes | Paired with command. |
| `command` | WorldCommand | yes | Both it and cmdSeq null, or both non-null. |
| `screens` | ScreenReport[] | no | Stable output order also supplies primary fallback. |
| `windows` | WindowFact[] | no | Managed candidates and retained/native facts, not classified roster. |
| `closed` | WindowId[] | no | Positive lifetime closures. |
| `native` | NativeObservation[] | no | Empty without hostOwned. |
| `focused` | WindowId | empty | Adapter-observed foreground lifetime across outputs; empty when no lifetime is observed/bound, never a membership or focus-success verdict. |
| `complete` | bool | no | Inventory completeness, not a hard capability. |
| `gesture` | bool | no | Existing gesture write barrier. |
| `cursor` | Point | yes | Native cursor in common coordinate space. |
| `actionForeground,actionCached` | OutputId | yes | Observed frozen action context, not chosen source. |
| `actionOutputs` | OutputId[] | no | Ordered known-output registry. |
| `presentation` | PresentationIngress | no | Native foreground/render facts. |
| `cursorHit` | WindowId | omit/null/empty | Native hit-test, never rectangle guess. |
| `monotonicMs` | Counter | omit -> 0 | Arbitrary-epoch milliseconds, not wall-clock. |
| `frameAttempts` | FrameAttempt[] | omit -> [] | Existing geometry lane. |
| `memberAttempts` | MemberAttempt[] | omit -> [] | Consolidated namespace evidence. |
| `viewAttempts` | ViewAttempt[] | omit -> [] | Native selectors only. |
| `clearAttempts` | ClearAttempt[] | omit -> [] | Existing maximize-clear lane. |
| `creationAttempts` | NativeCreationAttempt[] | omit -> [] | Existing native-create lane. |
| `gestureIngress` | GestureIngress | omit/null | Existing native gesture lifecycle. |
| `primary` | OutputId | omit | Host-designated primary, unknown if absent. |
| `intentRecords` | IntentRecord[] | no | Opaque host storage return. |
| `stateAttempts` | StateAttempt[] | no | Raw fullscreen/maximize/sticky writes. |
| `focusFeedback` | FocusFeedback[] | no | Correlated one-shot native focus evidence. |
| `rendererInput` | RendererInput | yes | One exact core-drawn target per command. |

- Removed report keys: `owner,mode,views,hostOwned,equalCount,birthClear,predicate,refill,defaultTiled,switchTarget,freshPlacement,focusFollowsCursor,cursorWarp,anchorFallback,noSpaceResult,bornMaximized`.
  Capability/settings keys move, not alias. `ScreenView` is removed. Logical
  view echo cannot prove native-free completion; visibility/focus facts do.
  [Visibility removal](../decisions/windows.md#state-centric-v4-visibility),
  [G1 reconciliation](../changes/v4-migration.md#b-reconciliations-and-open-points).

### ScreenReport

| Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `out` | OutputId | no | Live output handle, distinct from reconnect identity. |
| `full,work` | Rect | yes | FULL output and work area; both valid for complete row. |
| `complete` | bool | no | Truthful topology/native-read completeness. |
| `scale` | finite positive number | no | Units per logical pixel. |
| `identity` | OutputIdentity | no | Native metadata, no adapter identity policy. |

| OutputIdentity key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `manufacturer,model,serial,connector` | text <=256 bytes each | yes | Null unknown; connector is native connection identity. |

- `gap,outer,primary` leave screen rows. Core derives physical reconnect identity
  from manufacturer/model/serial, using connector if unreadable or duplicated;
  no persistent platform-id map. Host primary is the report's one optional
  `primary`; core falls back to first stable screen order, never active screen.
  A readable host priority order can supply screen order without proving a
  separate primary designation; omit `primary` when that designation is unknown.
  OPEN: phase 0b confirms API readability/notifications and truthful connector
  fallback on each host. [R11/primary evidence](../changes/v4-migration.md#b-reconciliations-and-open-points).

### WindowFact

| Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `win` | WindowId | no | Native lifetime, no second lifetime-id namespace. |
| `out` | OutputId | yes | Observed output affinity, unknown never current-output guess. |
| `rect` | Rect | yes | Actual normalized native frame. |
| `fullscreen,maximized,sticky,minimized` | bool | yes | Native state, unknown preserved. |
| `hints` | WindowSizeHints | yes | Existing sanitized min/max frame constraints. |
| `nativeWs` | NativeWorkspaceId | omit/null/empty | Actual native membership under hostOwned only. |
| `hostHidden,privileged,shell,tool,ownedDialog,dialog,noActivate,transient,modal,resizable,cannotControl` | bool | yes | Neutral native safety/classification inputs; not membership verdicts. |
| `owner,parent` | WindowId | yes | Known live relationship, null if unavailable/unmanaged binding unknown. |
| `role,type,appId,class,title` | bounded text | yes | Native role/type/app-rule inputs; no policy-normalized role verdict. |
| `fullscreenOwner` | `tiler/app` | yes | Unknown does not mean app-owned or exitable. |
| `fullscreenExitable` | bool | yes | Whether neutral core-authorized exit can be performed. |
| `suppressionAvailable` | bool | yes | Per-window recoverable hide support/readability. |
| `visibility` | VisibilityFact | no | Current native visibility and recovery ownership. |
| `lastWrite` | LastWrite | yes | Latest visibility write evidence, no work id. |

- Remove `floating,fitExcluded,fixedAuto,fixedSuppress,observedWs`; core derives these and
  owns explicit overrides, born latches and last-known frames. Native role/type,
  safety booleans, ownership and hints must be grounded before normalization;
  do not infer Windows owned-dialog/no-activate merely from a parent or missing
  input preference. Unknown necessary facts hold narrowly.
  [Classification](../decisions/engine.md#classification-and-convergence-ownership).
- Tiler-owned completion truth is retained core intent plus raw native
  visibility/focus/frame facts, not an adapter's logical membership echo.
  Native-owned domains still report nativeWs for core translation. Restart
  intent retains the existing explicit float/tile/sticky classifications;
  it does not restore a logical workspace/output map. Source checks and the
  distinction from native adoption are recorded in the
  [correction evidence](../changes/v4-migration.md#milestone-3-source-checked-corrections).
  [Intent ownership](../decisions/contract.md#intent-ownership-and-persistence).
- Minimized/host-hidden membership and eligibility remain distinct; no implicit
  unminimize/app-unhide. Existing workspace migration/output displacement and
  pre-hide transfers may relocate membership without geometry or activation.
  OPEN: phase 0b verifies raw native fact coverage and lifetime reuse, including
  unmanaged fullscreen foreground capture, rather than inventing negative facts.
  [Evidence gaps](../changes/v4-migration.md#b-reconciliations-and-open-points).

### Native observation

- `NativeObservation {out,order,current,occupancy}` and `NativeOccupancy
  {native,count}` keep their existing wire shapes with typed native ids.
  `current:null` preserves a non-ordered surface; empty/missing current is not
  that state. Count null is unknown; occupancy includes all native occupants,
  not just managed members. There are no `orderKnown/currentKnown` flags.
  Native facts without hostOwned refuse. Complete order needs both screen and
  inventory completeness; partial/missing rows never delete backing. Core
  translates bindings; adapters retain native lookups, not a logical map.
  [Native baseline](world-contract-v3.md#host-owned-adoption-amendment-tentative-2026-10-25),
  [superseded flags](../decisions/windows.md#accepted-protocol-and-phase-one-fences).
- Fresh core adopts observed native order/membership without launch workspace
  moves; outside changes remain facts. Linked views share compatible order;
  contradictory order holds, not adapter merging. Stable logical ids survive
  migration; only positive arrival/close/newer off-output proof releases source
  binding. Existing native creation/ambiguity/released-source rules remain.
  `exist:false` is eligibility only, with fresh occupancy/pin/minimum vetoes;
  OPEN G2: no new removal-support gate until phase-0b establishes the mechanism.
  [R19](../decisions/layout-and-admission.md#native-workspace-adoption-and-r19),
  [G2](../changes/v4-migration.md#b-reconciliations-and-open-points).

## Commands and results

- At most one user command per report. Payloads carry only user choice or
  input-carried identity/coordinates; core resolves policy at execution. The
  experimental same-user scripting surface uses these same verbs on every
  platform, not a second engine route. [Payloads](../decisions/contract.md#command-payloads),
  [R18](../decisions/contract.md#scripting-command-surface-r18).

### WorldCommand payloads

| `op` | Other keys and types | Null/absence | Meaning/source |
| --- | --- | --- | --- |
| `focus` | `win`: WindowId | no | By-id focus; ignored direction removed (R6). |
| `send` | `win`: WindowId; `out`: OutputId; `ws`: WorkspaceId; `follow`: bool | no | Explicit destination, unchanged. |
| `select-workspace` | `out`: OutputId; `ws`: WorkspaceId | no | Explicit logical selection, unchanged. |
| `select-numbered` | `out`: OutputId; `index`: integer 1..9 | out nullable | Core resolves implicit output. |
| `select-relative` | `out`: OutputId; `delta`: integer -1 or 1 | out nullable | Relative ring selection. |
| `select-previous`, `select-trailing` | `out`: OutputId | out nullable | Existing planner selection. |
| `send-relative` | `win`: WindowId; `source`: SendSource; `delta`: -1 or 1; `follow`: bool | no | Core resolves retained source. |
| `send-numbered` | `win`: WindowId; `source`: SendSource; `index`: integer 0..9; `follow`: bool | no | 0 retains trailing-send meaning. |
| `set-workspace-tiling` | `out`: OutputId; `ws`: WorkspaceId; `tiled`: bool | no | Explicit requested state. |
| `toggle-workspace-tiling` | `out`: OutputId; `ws`: WorkspaceId | no | Toggle retained core state. |
| `migrate-workspace` | `from,to`: OutputId; `ws`: WorkspaceId | no | Distinct outputs, stable logical id. |
| `focus-directional` | `win`: WindowId; `direction`: Direction | win may be empty, never null/missing | Empty means genuinely no focused subject. |
| `move-directional` | `win`: WindowId; `direction`: Direction; `sameAxis`: SameAxisMove | no | Shared core route including macOS. |
| `send-directional` | `win`: WindowId; `direction`: Direction; `follow`: bool | no | Core resolves destination. |
| `migrate-directional` | `direction`: Direction | no | Action source, no retained-window-home source. |
| `toggle-maximize` | `win`: WindowId | no | Existing command and result kept. |
| `toggle-float` | `win`: WindowId; `rect`: Rect | rect nullable | Promoted live operation; rect is genuine input placement only. |
| `toggle-orientation` | `win`: WindowId | no | Promoted live operation. |
| `resize` | `win`: WindowId; `direction`: Direction; `mode`: resize token; `press`: integer | no | Existing promoted payload, no resize-policy change. |
| `set-fullscreen`, `set-sticky` | `win`: WindowId; `value`: bool | no | New neutral on/off state intent. |
| `toggle-fullscreen`, `toggle-sticky` | `win`: WindowId | no | Core resolves toggle. |

- `Direction = left/right/up/down`; `SameAxisMove = group-with-neighbor/swap-with-neighbor`;
  resize tokens remain `inwards/outwards/horizontal/vertical`, with the existing
  integer `press` validation. [Command codec baseline](../../crates/tiler-protocol/src/world_protocol.rs),
  [R6](../changes/core-interface-review.md#original-backlog-lines-159-186).
- `SendSource` keeps its existing object encoding below, exactly one arm.
  Remove `home-then-cached-live`. Implicit source resolves
  frozen pre-observation retained home first, then action foreground, pointer,
  cache, known-registry order; migration uses action context only. Stale chosen
  source refuses, never retargets. [Send source](../decisions/contract.md#send-source-fallback).
- Remove `switch-view` and `operation`. Do not promote direct nested
  drag-preview/drag-drop/pointer-resize or maintenance/duplicate move/focus/send
  routes. Native gesture lifecycle and `DragPreview` replies remain unchanged.
  [R6](../changes/core-interface-review.md#original-backlog-lines-159-186).

| SendSource key (exclusive arms) | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `explicit` | OutputId | no; implicit absent | Genuinely semantic source output. |
| `implicit` | `home-then-action` | no; explicit absent | Sole retained fallback chain. |

### Window-state policy

- Maximize/fullscreen/sticky go through core on every host. Sticky is float-only
  across managed workspaces of the owning output, never all outputs. Sticky-off
  preserves previous float/tile intent; float-on while sticky always tiles; both
  land on current workspace. Neutral fullscreen exit requires known exitability,
  not only tiler ownership. Unknown fullscreen state stays untouched. Windows
  app-owned exits remain refused until each [baseline case](../changes/windows-fullscreen-exit-baseline.md)
  is accepted. Bindings do not change; Alt+Enter stays app-owned. macOS sticky
  remains unsupported until an all-Spaces route is proven.
  [R14](../decisions/contract.md#window-state-commands-r14).

### CommandResult and diagnostics

- An accepted report always incorporates its trustworthy observations, even if
  its command is rejected. `result` keeps the existing `DragPreview,Float,
  Migration,Directional,Maximize` variants and null behavior: null is permitted
  for successful snapshot-only commands and observation-only replies, not a
  universal no-op label. Add only `Rejected {kind}`; no Applied/NoOp wrapper.
  [Outcome decision](../decisions/contract.md#command-outcome-in-reply).

| Changed/new record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `CommandResult::Rejected` | `op` | `rejected` | no | Accepted-report command rejection. |
| | `kind` | CommandRejectKind | no | Closed reason, no adapter fallback. |
| `CommandResult::Directional` | `op,outcome` | `directional`, DirectionalOutcome | no | Existing variant/tokens. |
| | `win,rect,cause` | WindowId, Rect, DirectionalCause | yes | Existing nullable fields; cause now closed. |
| `OperationEvent` | `op,win,outcome` | Id, WindowId, OperationOutcome | no | Existing clear/float terminal event. |
| | `cause` | OperationCause | empty | Existing empty sentinel, closed nonempty values. |
| `RowDiagnostic` | `scope` | AuthorityScope | no | Narrow held scope. |
| | `row` | `screen/window/native/attempt/presentation` | no | Affected lane. |
| | `id` | Id | yes | Identifiable row; invalid identity refuses envelope. |
| | `kind` | `unreadable/invalid-fact/conflicting-fact` | no | Typed diagnostic, not a policy verdict supplied by adapter. |

- `CommandRejectKind` retains command failures:
  `focus-unknown,send-target-unknown,send-window-unknown,send-refused,view-output-unknown,view-target-unknown,transfer-in-flight,direction-invalid,scope-held,selection-unknown,migration-refused,operation-refused,tiling-target-unknown`;
  R14/input adds `state-unsupported,state-unknown,fullscreen-not-exitable,stale-render-target`.
  Envelope failures are not command outcomes.
  [Existing kinds](../../crates/tiler-core/src/world_engine.rs), [R13](../changes/core-interface-review.md#original-backlog-lines-103-122).
- `DirectionalCause` is `no-target,unknown-window,focus-present,no-source,no-adjacent-output`.
  `OperationCause` is `CommandRejectKind` plus
  `baseline-non-subject,closed,constraints,early-verdict,external,fixed,floating,homing-loss,invalid-direction,invalid-geometry,non-subject-focus,refused-with-unmaximized,sticky,tiling-changed,topology-unreadable,unknown-output,unknown-window`.
  These are typed diagnostics, never behavior inferred by parsing ids.
  [Directional producers](../../crates/tiler-core/src/world_operations.rs),
  [clear](../../crates/tiler-core/src/world_clear.rs), [born](../../crates/tiler-core/src/world_born.rs),
  [float](../../crates/tiler-core/src/world_settle.rs).
- The new typed effect-cause spelling `focus-follows-cursor` names the existing
  ffc producer, not a new focus route. `focus-unknown` is the existing by-id
  rejection; `focus-present/no-source/no-adjacent-output` are existing no-focus
  directional result causes. [Source checks](../changes/v4-migration.md#milestone-3-source-checked-corrections).

## Desired world and intent

### WorldReply

| Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `v,adapter,gen,req` | 4, Id, generation, Id | no | Exact issuing request/lifetime. |
| `obs,rev` | Counter | no | Based-on observation; globally monotonic desired revision. |
| `ready,consumed` | bool | no | World readiness; repeated command incorporated, not re-executed. |
| `scopes` | ScopeStatus[] | no | Every known scope's authority. |
| `workspaces,outputs,windows` | WorkspaceDesire[], OutputDesire[], WindowDesire[] | no | Full desired snapshot, not a diff. |
| `transfers,effects` | TransferTarget[], OneShotEffect[] | no | Retained correlations and one-shot work. |
| `result` | CommandResult | yes | Issuing-reply-only result. |
| `gestureSettled` | GestureSettled | yes | Core mutation acknowledgement, not native completion. |
| `operationSettled` | OperationEvent[] | no | Existing clear/float terminals. |
| `nativeCreations` | NativeCreationDesire[] | no | Only authorization for native desktop creation. |
| `warpCursor` | Point | yes | Core-decided cursor target. |
| `presentation` | WorldPresentation | no | Full accepted rendering publication. |
| `intentWrites` | IntentWrite[] | no | Core save/clear decisions. |
| `switchSettled` | SwitchEvent[] | no | Terminal event on existing transfer/view parent. |
| `diagnostics` | RowDiagnostic[] | no | Narrow held-row errors. |

- Only controlled property authority authorizes that property's writes. Held,
  untouched, missing rows and retained geometry authorize nothing. Floating
  rows never imply frame writes: ordinary floats retain native frames; explicit
  `frame_op` alone names one-shot float placement. `float_origin:capacity` is
  diagnostic only. Existing clear/create/member/frame lanes stay distinct.
  [Desired-state baseline](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes),
  [state-centric decision](../decisions/contract.md#state-centric-contract).

### Changed desired rows

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `ScopeStatus` | `scope` | AuthorityScope | no | Typed authority target. |
| | `auth,reason` | ScopeAuthority token, HoldReason | reason empty outside hold | Existing flattened encoding. |
| `WorkspaceDesire` | `id,out` | WorkspaceId, OutputId | no | Logical identity and owner. |
| | `bind` | NativeWorkspaceId | empty | Existing unbound sentinel. |
| | `scope` | AuthorityScope | no | Typed owning scope. |
| | `order,mode,exist,hold` | integer, TilingMode, bool, HoldReason | hold empty if none | Existing ring order/mode/removal eligibility. |
| | `trailing` | true | omit unless true | Existing explicit spare designation. |
| `WindowDesire` | `win,out,ws` | WindowId, OutputId, WorkspaceId | no | Exact lifetime and core home. |
| | `alloc,frame` | Rect | yes | Existing allocation/effective frame. |
| | `frame_auth,frame_hold,member_auth,member_hold` | authority/reason pairs | hold strings empty if none | Existing independent geometry/membership authority. |
| | `frame_op` | Id | empty | One-shot explicit float placement. |
| | `floating,float_origin,clear_op` | bool, FloatOrigin, Id | origin/op empty if none | Existing classification diagnostic and one-shot clear. |
| | `cmdSeq` | Counter | yes | Explicit command correlation for frame/clear target; commandless birth clear has null. |
| | `visibility` | VisibilityTarget | yes | Composed visibility target, not row-presence permission. |
| | `visibility_auth,visibility_hold` | authority/reason pair | reason empty if none | Independent visibility lane. |
| | `state` | StateTarget[] | no | Independent native-state targets. |
| `VisibilityTarget` | `shown` | bool | no | Desired shown state; never native unminimize. |
| | `rev` | Counter | no | Existing world revision at target authorization; retained across observation-only replies. |
| | `fallback` | bool | no | Core permission for hide-only fallback after failed/unproven focus, false for shows. |
| `StateTarget` | `property` | `fullscreen/maximized/sticky` | no | Native property. |
| | `value` | bool | no | Core-resolved target. |
| | `auth,reason` | authority/reason pair | reason empty if none | This property only. |
| | `cmdSeq` | Counter | yes | Command correlation, not parsed operation text. |
| `TransferTarget` | `op,kind,win,out,ws,follow,held` | Id, TransferKind, WindowId, OutputId, WorkspaceId, bool, bool | win empty except window kind | Existing window/view/migration target shape. |
| | `cmdSeq` | Counter | yes | Original command sequence, null for commandless work. |
| `OneShotEffect` | `id,kind,target,rev` | Id, EffectKind, WindowId, Counter | no | Existing focus/raise/stack and issuing revision. |
| | `cause` | `explicit-command/automatic-admission/focus-follows-cursor` | no | Replaces fx-/af-/ffc- interpretation. |
| | `cmdSeq` | Counter | yes | Non-null for explicit-command; null otherwise. |
| | `awaitShown` | bool | no | Only explicit selection may grant arrival-ready focus. |

### Opaque intent storage

- Core defines/validates and decides save/restore. Adapters store/return `data`
  unchanged in lifetime-bound native storage; they never interpret it as policy.
  The core record is a compact JSON string <=1024 UTF-8 bytes with internal
  fields tabulated below.
  No geometry, layout tree, workspace ring or MRU/history is persisted. Invalid
  version/shape or unverified native storage ownership returns no intent and no
  write authority. The outer win binds the verified live storage subject, not
  an old adapter id embedded in the record. A returned
  record is not proof of native state. Cross-login persistence is not promised.
  OPEN: phase 0b verifies storage survival and exact lifetime ownership, not
  restart matching by title/process heuristic.
  [Intent](../decisions/contract.md#intent-ownership-and-persistence),
  [evidence](../changes/v4-migration.md#b-reconciliations-and-open-points).

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `IntentRecord` | `win` | WindowId | no | Storage's verified live subject. |
| | `data` | opaque bounded string | no | Exact core record, no adapter edits. |
| `IntentWrite` | `win` | WindowId | no | Exact subject. |
| | `data` | opaque bounded string | yes | Save value; null explicitly clears native slot. |
| Core intent data (inside string) | `v` | integer 1 | no | Storage format version, independent of wire v. |
| | `float` | bool | yes | Explicit float/tile override; null means no override. |
| | `stickyPrior` | bool | yes | Prior float/tile intent for sticky-off. |

## Raw attempts and settlement

- SetterOutcome is the renamed FrameSetter, with unchanged
  `accepted/refused/threw` tokens. Acceptance alone never proves completion;
  explicit refusal needs readable unchanged preimage. Anything possibly executed
  remains ambiguous/spent, never blind retry. Carry attempts into a freshly
  captured report whose observations do not predate immediate readbacks; later
  asynchronous observation is not itself a completion signal. Lost acknowledgements
  redeliver the original stamps idempotently. [Settlement baseline](world-contract-v3.md#5-raw-attempts-and-settlement),
  [rename/consolidation](../changes/core-interface-review.md#original-backlog-lines-159-186).

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `MemberAttempt` | `op,win,out,ws,setter` | Id, WindowId, OutputId, WorkspaceId, SetterOutcome | no | Existing operation/subject/desired target/call disposition. |
| | `pre,post` | WorkspaceRef | yes | Single logical-or-native reference set, live before/after. |
| | `attemptMs` | Counter | no | Original monotonic write stamp. |
| `ViewAttempt` | `op,out,ws,setter` | Id, OutputId, WorkspaceId, SetterOutcome | no | Native selector evidence for existing parent/phase. |
| | `pre,post,base` | WorkspaceRef with kind native | yes | Native before/after/observation baseline only. |
| | `attemptMs` | Counter | no | Original stamp. |
| `StateAttempt` | `win,property,target,rev` | WindowId, state property, bool, Counter | no | Exact core native-state target. |
| | `setter` | SetterOutcome | no | Actual native call disposition. |
| | `pre,post` | bool | yes | Observed property immediately around call. |
| | `attemptMs` | Counter | no | Original stamp. |

- Member evidence uses the namespace allowed by hard capability; core alone
  translates native bindings. Remove parallel `nativePre/nativePost/nativeBase`.
  ViewAttempt permits no logical arm; native-free visibility/focus completes
  selection. This is the [G1 technical reconciliation](../changes/v4-migration.md#b-reconciliations-and-open-points),
  pending 0c approval, not a new product decision.
- FrameAttempt, ClearAttempt, NativeCreationAttempt and NativeCreationDesire
  retain exact baseline keys/nullability and separate lanes, including frame
  target/readback/DPI/prewrite, clear pre/post-maximized and creation
  before/after/correlated-created id. Do not replace them with LastWrite.
  Creation needs explicit `nativeCreations` authorization, spent before call;
  an unbound spare is not permission. Unknown/ambiguous created identity holds
  without recreation. [Creation rules](world-contract-v3.md#host-owned-adoption-amendment-tentative-2026-10-25).

## Visibility and focus ordering

### Raw visibility evidence

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `VisibilityFact` | `visible,suppressionOwned` | bool | yes | Native shown state and project-owned suppression. |
| `VisibilitySample` | `obs` | Counter | no | Invalidation watermark at the read; subject comes from enclosing window/bracket. |
| | `lifetimeMatched` | bool | yes | Full native lifetime and any applicable stored suppression/recovery tag match, not handle equality. |
| | `visibility` | VisibilityFact | no | Native state at this read. |
| | `minimized,hostHidden` | bool | yes | Separate native ineligibility facts. |
| `LastWrite` | `property` | `visibility` | no | Property tag reserved for later separate lane migrations. |
| | `target,revision` | bool, Counter | no | Existing desired-state target and revision; no visibility id. |
| | `path` | `staged/fallback/independent` | no | Actual execution route. |
| | `setter` | SetterOutcome or `not-called` | no | No-call is not a successful native call. |
| | `reason` | VisibilityReason | empty on ordinary call | Bounded typed mechanical reason. |
| | `pre,post` | VisibilitySample | yes | Identity-bound reads immediately around action/no-call. |
| `ArrivalBracket` | `win,obs` | WindowId, Counter | no | Required arrival subject and common source watermark. |
| | `before` | VisibilitySample | no | First lifetime/visibility read. |
| | `home,frame` | WorkspaceRef, Rect | yes | Native reads BETWEEN before/after; null cannot prove readiness. |
| | `after` | VisibilitySample | no | Final lifetime/visibility read with no intervening invalidation. |
| `FocusFeedback` | `id,target,rev` | EffectId, WindowId, Counter | no | Existing focus grant correlation. |
| | `setter` | SetterOutcome or `not-called` | no | At-most-once call/no-call. |
| | `reason` | VisibilityReason | empty on ordinary call | Mechanical blocked/refusal fact. |
| | `pre,post` | WindowId | yes | Live foreground before/after; null means unknown. |
| | `foregroundProven` | bool | no | Actual foreground equality proof, never setter success. |
| | `arrival` | ArrivalBracket[] | no | Readiness brackets; blocked subjects use their LastWrite no-call evidence, not a different array arm. |
| | `attemptMs` | Counter | no | Original dispatch/no-call stamp. |
| `SwitchEvent` | `op` | Id | no | Existing parent transfer/view operation. |
| | `outcome` | `complete/completed-with-failures` | no | Core terminal accounting, not native per-entry verdict ingress. |
| | `failed` | WindowId[] | no | Frozen switch target failures. |
| | `focus` | `proven/refused/unproven` | no | Core-derived foreground outcome. |
| | `fallback` | bool | no | Whether separately authorized fallback hides ran. |

- `VisibilityReason` is `""` or
  `identity-stale,scope-held,unreadable,unsupported,preimage-unavailable,foreground-changed,fullscreen,game,show-blocked,focus-unproven,superseded,native-refused`.
  Samples are facts, never a copied target. Arrival requires the same live
  subject, matched lifetime/applicable tag and watermark, visible/non-minimized/non-hidden
  before/after, and coherent observed home/frame inside that bracket. Home must
  match the frozen core target; frame must equal its target when non-null. A
  null desired frame waives equality, never the requirement for a readable live
  frame. Unknown never proves readiness. Suppression is unavailable without
  both handshake support and
  per-window recoverable preimage support; never use native minimize/app-hide
  as a suppression substitute. [Evidence design](../changes/windows-workspace-return-focus.md#state-centric-visibility-direction-2026-10-30).
- Native-owned selection/membership uses native desktop targets and attempts;
  it does not require independent per-window hides. A host without suppression
  reports `windowSuppression:false`, per-window `suppressionAvailable:false`,
  null visibility targets and no visibility writes. Readable native visibility,
  lifetime, home and frame can still prove arrival for focus; no suppression
  recovery tag is required for a window that has never been project-suppressed.

### Normative apply rule

1. Latest desired state wins; drop superseded unissued targets.
2. Apply shows first, batched and nonactivating.
3. An `awaitShown` focus runs once only after every controlled desired-shown
   window on its output in the issuing revision's snapshot has a fresh arrival
   bracket or reported no-call reason.
   Any failed/blocked required show withholds focus. Spend before native dispatch.
4. Across outputs, hides wait for every outstanding `awaitShown` effect from
   the same or an earlier revision. After proven foreground they run staged.
   After refused/unproven focus, only separately approved fallback hides run
   under fresh identity/desktop/foreground/game/fullscreen/visibility/property
   gates and `visibility.fallback:true`, reported as fallback; otherwise report
   not-called and keep visible.
5. At most one write per lifetime/revision/target. A disagreeing fact never
   rewrites that same target at that revision; only new intent grants a new
   authorization stamped with its existing world revision in `visibility.rev`.
   Ordinary observation revisions do not renew it; LastWrite.revision names
   that authorization revision, not the later report's revision.
6. Issued writes stay spent across supersession/reset; report late LastWrite
   evidence without letting an old target override the new desired target.

- Core freezes switch target sets internally, derives each member's
  confirmed/refused/ambiguous/blocked state, and emits SwitchEvent only when
  every member has a verdict. Later admissions enter the next target revision,
  not the old switch silently. There are no visibility work/group ids,
  visibilityResults/selectionProofs/visibilitySettled rows, timers or polling.
  [Ordering and accounting](../changes/windows-workspace-return-focus.md#normative-apply-rule-contract-text),
  [cross-output barrier](../changes/windows-workspace-return-focus.md#phase-0-questions).

## Freshness and recovery

- Advance `obs` synchronously on every invalidating native event, even while a
  request is in flight. `reqSeq` is monotonic per lifetime/generation; exact
  latest duplicate input returns cached identical reply/revision with no new
  effect, changed reused input refuses, older requests are superseded.
  Apply only a reply paired to the exact lifetime/generation/request and current
  observation watermark. Repeated/consumed `cmdSeq` still incorporates fresh
  observations and sets `consumed`, never re-executes. Commandless reports do
  not move command watermark. Transport timeout is not an engine verdict or
  permission to mint a new duplicate command. [Sequencing](world-contract-v3.md#3-identity-sequencing-and-dedup).
- Effects are spent before dispatch, never replayed by subscribers or reconnect;
  retained transfers remain until observed, externally superseded or proven
  refused. Issuing-only result/terminal delivery is not a snapshot cache.
  [Desired-state baseline](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes).
- Before hide, stripped fullscreen chrome, project keep-above or sticky stacking
  writes, persist original native state in a host slot outliving the tiler.
  Restore leftovers at next startup; Windows watcher also restores stripped
  chrome. Unreadable/failed preimage disables only that window/property. Never
  mutate untouched state, nor infer restoration authority from a stale lifetime.
  OPEN: phase 0b verifies each host slot/recovery mechanism.
  [R12](../decisions/contract.md#native-writes-difficult-to-undo-r12),
  [evidence](../changes/v4-migration.md#b-reconciliations-and-open-points).
- Fail open narrowly: show project-hidden windows, release intercepted input,
  undo recoverable mutations, stop uncertain writes. Fail closed only when
  release causes harm, breaks system function, leaves worse state or the host
  requires it; state the reason at the implementation point. Freshness is causal,
  never an elapsed-time guess; unsolicited outside changes remain observations,
  not discarded as application side effects. [Resilience](../principles.md#resilience),
  [causality](../decisions/contract.md#transient-side-effects).

### Bounded errors

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `WorldProtocolError` | `v` | integer 4 | no | Existing error envelope version. |
| | `outcome` | `rejected` | no | No applicable world reply. |
| | `kind` | ProtocolKind | no | Closed report/transport rejection. |
| | `message` | fixed bounded token/text | no | Non-echoing diagnostic, never payload bytes. |
| | `gen` | generation | empty if unavailable | Existing generation sentinel. |

- `ProtocolKind` retains `malformed,oversized,snapshot-invalid,unknown-field,unknown-value,unsupported-version`
  and core envelope/fencing kinds `owner-invalid,correlation-invalid,generation-mismatch,observation-stale,request-superseded,request-reused,reply-oversize,revision-exhausted,creation-exhausted`.
  `owner-invalid` describes unauthorized connection ownership, not a revived
  owner field. Envelope/size/identity failures mutate nothing. Identifiable row
  invalidity uses RowDiagnostic instead of a whole-report snapshot error.
  [Existing errors](../../crates/tiler-protocol/src/world_protocol.rs),
  [core kinds](../../crates/tiler-core/src/world_engine.rs).

## Presentation and renderer input

### PresentationIngress

| Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `foreground` | WindowId | empty | Observed foreground token, including unmanaged subject. |
| `minimized,maximized,fullscreen,hostHidden,hiddenWorkspace,shell` | bool | yes | Actual foreground facts, not renderer eligibility. |
| `frame` | Rect | yes | Live foreground frame. |
| `chordHeld` | bool | no | Actual chord level, not underlay verdict. |
| `moveWindow` | WindowId | empty | Native move subject, not adapter classification. |
| `pagerOrder` | WindowId[] | no | Native top-to-bottom snapshot. |
| `pagerFrames` | PresentationPagerFrame[] | no | Fresh frame sources only. |
| `witness` | FrameWitness | yes | Causal identity/frame/foreground observation, unknown disables border. |
| `foregroundOut` | OutputId | yes | Observed output even for unmanaged fullscreen foreground. |

| PresentationPagerFrame key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `win` | WindowId | no | Live source subject. |
| `live` | Rect | yes | Keep live source; remove retained float/tile. |

- Remove `borderEnabled,underlayEnabled,borderGap,borderWidth,underlayExtension,paused`;
  settings are unscaled and independent. Rename cloaked to hostHidden.
  Core retains last-known pager frame; unknown live source never fabricates
  geometry or membership. Focused-fullscreen presentation suspension is
  unconditional, including truthful unmanaged fullscreen foregrounds; no
  adapter opt-in/paused verdict. `suspendedOutput` hides that output's
  border/underlay/pager/drop-preview only, preserving other outputs and gesture
  lifecycle/results. [R5](../changes/core-interface-review.md#original-backlog-lines-132-158),
  [fullscreen suspension](../decisions/world-rendering.md#focused-fullscreen-presentation-suspension).

### WorldPresentation and anchored border

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `WorldPresentation` | `foreground` | WindowId | empty | Existing foreground token. |
| | `rev` | Counter | no | One common rendered presentation revision. |
| | `suspendedOutput` | OutputId | omit | Existing inactive omission. |
| | `border,underlay,pager` | BorderPresentation, PresentationUnderlay, PagerScreen[] | no | Existing named typed fields. |
| `BorderPresentation` | `visible` | bool | no | Core eligibility verdict. |
| | `reason` | BorderHideReason | empty if visible | Existing tokens, host-hidden replaces cloaked. |
| | `subject` | WindowId | yes | Anchored lifetime, not absolute inner/outer rectangles. |
| | `outsets` | [left,top,right,bottom] integers | yes | Core-computed adapter-unit outsets. |
| | `width,radius` | nonnegative integer | yes | Core-scaled style, null without descriptor. |
| | `witness` | FrameWitness | yes | Exact causal observation on which eligibility was based. |
| `FrameWitness` | `win` | WindowId | no | Verified same native lifetime. |
| | `obs` | Counter | no | Captured invalidation watermark. |
| | `frame` | Rect | no | Native frame from that capture/event. |
| | `foreground` | WindowId | no | Verified foreground at same causal capture. |
| `RendererInput` | `rev` | Counter | no | Revision actually drawn, not latest cached reply. |
| | `target` | RenderTarget | no | Exact core row selected by native hit mapping. |
| `RenderTarget` | `kind` | `workspace/window/group` | no | Typed decoration target. |
| | `out` | OutputId | no | Owning output. |
| | `ws` | WorkspaceId | no | Logical row. |
| | `win` | WindowId | only window arm | Exact lifetime. |
| | `group` | NodeId | only group arm | Exact core group. |

- Renderer input must accompany its corresponding world command. Core compares
  the exact target row as rendered to its current row; a changed target refuses,
  an unrelated revision change does not. Adapter only maps hit position to target;
  no local click-freshness policy. Named border/underlay/pager share this rule.
  [Decorations](../decisions/contract.md#decorations).
- Apply an accepted border descriptor to the live native frame through existing
  native frame events, not stale absolute rectangles. A witness must be joined
  to the subject and native capture/invalidation chain; later frame events may
  move that same accepted anchor only while foreground/lifetime/eligibility
  facts remain causally valid. A constant event-name string, elapsed delay or
  guessed frame is not a witness. Unknown/stale witness suppresses paint
  narrowly, never polls or adds a settling marker. `obs` identifies the capture;
  it is not interchangeable with a native renderer's source-instance/counter.
  The adapter must preserve the exact join between those chains. OPEN: phase 0b must verify
  native witness joins/events on each port; macOS smoothness evidence is not a
  shape gate. Underlay remains group-defined absolute; its baseRev is projection
  provenance, not another input freshness rule.
  [R20](../decisions/contract.md#anchored-border-r20),
  [evidence](../changes/v4-migration.md#b-reconciliations-and-open-points).

## Unchanged vocabulary and carry-forward

- The records/enums below retain implemented v3 names, keys, nullability,
  omission and semantics except specifically replaced fields above. They are
  explicitly part of v4, not optional unspecified extensions. Rust is a shape
  reference only through the handwritten codec; do not derive JSON from serde.
  Before deleting the v3 guide at cutover, carry its remaining rules using the
  [exact checklist](../changes/v4-migration.md#cutover-carry-forward-checklist).

| Unchanged record/enum by name | Shape and behavior source |
| --- | --- |
| `OutputDesire`, `TilingMode`, `FloatOrigin`, `TransferKind`, `EffectKind`, `ScopeAuthority`, `HoldReason` | [Desired rows](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes), [world_report.rs](../../crates/tiler-core/src/world_report.rs) |
| `NativeObservation`, `NativeOccupancy`, `NativeCreationDesire`, `NativeCreationAttempt` | [Adoption](world-contract-v3.md#host-owned-adoption-amendment-tentative-2026-10-25), [world_report.rs](../../crates/tiler-core/src/world_report.rs) |
| `FrameAttempt`, `ClearAttempt`, `OperationOutcome` | [Settlement](world-contract-v3.md#5-raw-attempts-and-settlement), [world_report.rs](../../crates/tiler-core/src/world_report.rs) |
| `WindowSizeHints`, `FixedSizePredicate` | [size_hints.rs](../../crates/tiler-core/src/size_hints.rs) |
| `WorkspaceMode`, `MigrationSourceRefill`, `AnchorFallback`, `NoSpaceResult`, `SwitchTarget`, `FreshPlacement`, `BornMaximized` | [Settings baseline](world-contract-v3.md#current-message-shapes-v3), [catalog](../../crates/tiler-core/src/settings_catalog.rs) |
| `CommandResult::DragPreview`, `CommandResult::Float`, `CommandResult::Migration`, `CommandResult::Maximize`, `DirectionalOutcome` | [Results](world-contract-v3.md#current-message-shapes-v3), [world_sizing.rs](../../crates/tiler-core/src/world_sizing.rs), [world_directional.rs](../../crates/tiler-core/src/world_directional.rs) |
| `GestureIngress`, `GestureSettled`, `DragHoverPrior`, `PriorGroupEdge`, `DragSide`, `Direction`, `SameAxisMove` | [Wire baseline](world-contract-v3.md#current-message-shapes-v3), [gesture.rs](../../crates/tiler-core/src/gesture.rs), [world_protocol.rs](../../crates/tiler-protocol/src/world_protocol.rs) |
| `PresentationUnderlay`, `PresentationUnderlayFound`, `GroupHideReason`, `ActiveGroupMember`, `PagerScreen`, `PagerWorkspace`, `PagerWindow` | [Presentation](world-contract-v3.md#8-border-underlay-drop-preview-and-pager), [world_presentation.rs](../../crates/tiler-core/src/world_presentation.rs), [world_sizing.rs](../../crates/tiler-core/src/world_sizing.rs) |
| `BorderHideReason` except HostHidden spelling | [visual.rs](../../crates/tiler-core/src/visual.rs) |
| `DomainKey`, `OutputId`, `WorkspaceId`, `NativeWorkspaceId` string representation, `WindowId`, `NodeId`, `Rect` | [Baseline codec](../../crates/tiler-protocol/src/world_protocol.rs), [session/world.rs](../../crates/tiler-core/src/session/world.rs) |

## Conformance

- Exact v4 per-message fixtures are in the
  [manifest](../../test-fixtures/world-wire/v4/manifest.json), including ordered
  [scenario containers](../../test-fixtures/world-wire/v4/scenarios/) with fresh
  initialization and explicit OPEN/TBD gates. Fixture ids/assertions/metadata are not wire fields.
  Cover every retained/promoted/new command, message, changed record and closed
  enum arm; preserve empty/null/omission distinctions and unchanged result
  payloads. Cover causal races, rejection with observation incorporation,
  scaling boundaries, row holds, native backing, visibility failures, opaque
  restart intent and renderer target-row freshness.
  [Fixture layout and scenario plan](../changes/v4-migration.md#fixture-layout-and-ordered-scenario-plan).
- Literal bytes and catalog coverage are established for the target draft, not
  v4 decoder conformance. JSON parsing alone is not the oracle; later exact encoder/generator
  comparison, generated types, parser arm coverage and shared adapter acceptance
  must agree before merge. Native settlement/recovery/paint evidence is separate
  from codec acceptance. [Testing](../principles.md#testing),
  [migration gates](../decisions/contract.md#contract-authority-and-v4-migration).

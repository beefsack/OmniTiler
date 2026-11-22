# World contract (v4)

Status: normative supported report/command v4 contract (amended in place across `tiler-core`, `tiler-protocol`, `omnitiler`, and KDE offline); deferred macOS/Windows ports and remaining KDE rebuild items are tracked in the [migration decision](../decisions/contract.md#contract-authority-and-v4-migration).
v4 is the implemented v3 wire plus the approved deltas below, nothing else.

## Contract authority rule

Current amended v4 is authoritative now for supported consumers (`tiler-core`, `tiler-protocol`, `omnitiler`, and KDE offline). There are no dual message shapes, compatibility shims, or old-v4 fallbacks: unsupported `v:3` messages and frozen `v:4` reports missing required fields such as `propertyAttempts` refuse on supported routes. Frozen `test-fixtures/world-wire/v4`, `test-fixtures/stage5`, `test-fixtures/stage5-v4`, and `test-fixtures/kde-v4` (and [world-contract-v3.md](world-contract-v3.md)) are historical only.
See the [authority decision](../decisions/contract.md#contract-authority-and-v4-migration).

## Per-host applicability

Rules link [handshake](#handshake-and-lifetime), [report fields](#worldreport), [results](#commands-and-results), and [apply/recovery](#freshness-and-recovery).

| Route / host | Producer | Actuator | Results and retry | Capability and evidence |
| --- | --- | --- | --- | --- |
| Shared v4 baseline | Bound handshake with capabilities (`sharedNativeRoster` required on wire), revisioned settings, fresh correlated observations; screen/window inventory and completeness, `closed` as positive evidence, `foregroundKnown`, unified `propertyAttempts` (empty when none), separate focus feedback and creation proof | Apply only latest eligible desired state within authority, subject/lifetime, generation and freshness fences; report exact attempts or real no-calls and independent readbacks; never choose substitute targets | Queue ACK is filing only; disposition is separate from world. Retry by the route's identity; capture incorporation does not imply native apply or command execution | Capabilities, not port names, choose selector/roster, above, native sticky, warp and unavailable-fact behavior; no unsupported setters |
| KDE / broker | V4 envelope `v,adapter,gen,source,sourceSeq,nativeInvalid,report`; outer envelope alone carries `v`, `report` holds unversioned capture facts (no nested `v` or broker-owned stamps), and broker constructs the v4 standalone report with correlation, clock and sequences | Apply only eligible bound capture; renderer projection confers no setter authority; prove visibility feedback production in rebuild | Exact remembered source bytes replay before drain; requester-only Take, Wait is wakeup; retry unremembered rejection with a fresh source | Implemented offline; earlier native state-target slice only (`../decisions/contract.md`), not proof of this interface. Shared native roster, capability-gated selector; no KDE warp |
| Typed shared route | Same facts and bound connection using typed values; caller-owned ordered commands; no wire stamps manufactured for JSON | Prepared typed publication and explicit incorporation; same outbound semantics, independent latest-only apply fence | Typed report-only replay with no new command execution; returned per-sequence outcomes, caller owns issuer delivery | Shared implementation proved offline; not a native port |
| macOS | Deferred; build the typed route and prove native observations/receipts on port entry | Same obligations; prove setters and recovery locally | Preserve per-issuer outcomes and in-flight evidence | Deferred; no shim or build accommodation |
| Windows | Deferred; build the typed route and prove capability declarations on port entry | Same obligations; retained warp before advertising it | Preserve per-issuer outcomes and in-flight evidence | Deferred; no shim or build accommodation |

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
| One retained focus request | [Foreground](#worldreport), [focus request](#one-retained-focus-request) | [Approved design](../changes/retained-focus-requests.md) |

- The pre-release v4 close-focus amendment is an in-place breaking change:
  `foregroundKnown` and required nullable `focusRequest` are present, with
  no version bump or dual-shape fallback. Core, codecs, fixtures, and KDE's
  strict reply reader implement the amended shape offline; deferred macOS and
  Windows ports adopt it on their native rebuilds. The amended shape carries no
  `closeFocus` and no `Focus` effects. A matching component set is required for
  execution; offline conformance is not native acceptance.
- The pre-release shared-roster amendment also keeps version 4 and requires
  `sharedNativeRoster` on every handshake, without a missing-key default or
  dual-shape fallback. Current connection conformance uses
  `test-fixtures/world-wire/v4-active`; unchanged vectors reference the frozen
  `v4` family. Historical handshakes remain unchanged and explicitly reject the
  amended strict shape. Genuine current production-route recordings use
  `test-fixtures/stage5-v4-active` and `test-fixtures/kde-v4-active`; existing
  `stage5` and `stage5-v4` recordings remain historical.
- The pre-release retained `dragPreview` and command-result amendment
  ([Q1=B](../changes/retained-command-result.md#user-decisions-2026-11-15))
  also keeps version 4 with no alias or dual shape: `WorldReplyV4` carries the
  required nullable retained identity-bound `dragPreview` (`DragPreviewV4`)
  lane and removes `result` (and `CommandResult::DragPreview` is removed from
  `CommandResult`). Per-command outcomes ride only the issuing
  `TransitionResult.dispositions` (`Vec<BatchDisposition>`, covering the
  normalized inline entry and queued commands in order) and the
  broker's requester-only `TakeCommandResult` lane (`MAX_SETTLED_RESULTS = 1024`).
  Offline conformance is not native acceptance.

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
  limits must not reintroduce platform policy opt-ins. No send-admission flag,
  replacement capability or setting exists. Maximized-origin directional focus
  skips same-output candidates and crosses to an eligible adjacent output in
  the requested direction, otherwise stays, on native and core-composed routes;
  no maximized-focus fence or replacement capability exists. The
  [composed-float interpretation](../decisions/window-state.md#directional-maximize-and-fullscreen-operations)
  is explicitly assumed; ordinary float and fullscreen precedence are unchanged.
  [R5](../changes/core-interface-review.md#original-backlog-lines-132-158).

## Wire conventions and bounds

- Objects are closed: unknown keys/enum values and wrong JSON types refuse,
  except the per-field settings and identifiable fact-row validation specified
  below. Duplicate object keys at any depth refuse as `malformed` on
  capture, discovery, queue, take, wait and presentation-read. For the last
  four endpoints, size and authorization precede JSON decoding, then the
  envelope's keys, version and field values are checked in that order.
  Where required, serving generation is checked before command semantics
  or state access. Required means present; nullable does not mean optional.
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
  lifetime. `gen` is 1..64 lowercase ASCII letters/digits/hyphens. Core-derived
  tokens (generation-local gesture IDs, independent leaf mints, bounded rebind
  generations) stay within their `Id`/`gen` bounds without truncating or
  narrowing valid 128-byte window IDs or 64-byte generations; correlations stay
  in typed fields rather than parsed token spellings.
  [Baseline bounds](../../crates/tiler-core/src/bounds.rs), [identity](../../crates/tiler-core/src/ids.rs),
  [R6](../changes/core-interface-review.md#original-backlog-lines-159-186).
- Adapters mint `win` only while exact native-subject continuity is proved.
  macOS retains remote AX element identity, compares with CFEqual and fences the
  process; PID/CG window id/title alone never rebind an old token. Stale callbacks,
  replaced AX subjects and restart reacquisition cannot inherit the old binding.
  Hold uncertain subjects with incomplete inventory; positive close or proved
  subject replacement retires the old token through `closed`, then a fresh subject
  receives a fresh token. AXWindows omission/off-Space absence is not positive
  close evidence. Reconnect already discards the old connection's identities.
- `Rect` is `[x,y,w,h]` with integer coordinates in one shared half-open
  coordinate space: inclusive endpoint bounds `-32768..32768` (`x >= -32768`,
  `y >= -32768`, `x + w <= 32768`, `y + h <= 32768`) and positive extents
  `1..16384`, checked without clamping; `Point` is `[x,y]` under the same
  `-32768..32768` axis bounds. Screens, work areas, frames, cursor
  and absolute presentation share one coherent adapter coordinate space.
  Native/client/frame conversions stay adapter-side; size hints are normalized
  with actual decoration extents, never another host's compensation. Exact
  arrival remains exact.
  [Baseline geometry](../../crates/tiler-core/src/bounds.rs),
  [coordinates](world-contract-v3.md#coordinates-and-safety).
- Requests and replies are independently <=4 MiB, validated before candidate
  mutation and checked against exact encoding before commit. No partial
  oversized state applies. Retain caps: screens and `actionOutputs` 16 each; windows/closed 10000 each;
  unified `propertyAttempts` 1024 per kind (`frame`, `member`, `view`, `state`)
  and 10000 for `visibility` (14096 total); creation attempts 256;
  native rows 4096 with <=256 ids per output; occupancy <=10000 per desktop;
  pager order and frame sources 1024 each; `intentRecords` and `intentDesired`
  <=10000 each; `focusFeedback` <=1024; at most four state targets per window;
  arrival brackets <=10000 total per report; one renderer input at most.
  Failed-window lists are subsets of the frozen <=10000-window roster;
  diagnostics have at most one entry per held row, never unbounded queues.
  Metadata text below is UTF-8 <=4096 bytes each (display identity <=256).
  error messages and diagnostic text inherit this bound; producers emit fixed, non-echoing error messages.
  Over-bound values are unreadable row facts, not silently truncated identity.
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
| | `applicationIdentity` | ApplicationIdentity | no | Neutral adapter application identity, fixed for this connection. |
| | `capabilities` | HostCapabilities | no | Fixed hard host limits. |
| `ApplicationIdentity` | `appId` | bounded application id | yes | Own native bundle/application id; null when absent or unreadable, never a fallback verdict. |
| | `executable` | executable identity | yes | Own normalized executable; null only when executable is unavailable. |
| | `processId` | Id | yes | Own native process identity in the window processId namespace; null only when processId is unavailable. |
| `HandshakeReply` | `type` | `handshake-reply` | no | New routing tag. |
| | `v` | integer 4 | no | Selected version, not negotiation of dual shapes. |
| | `adapter,req` | Id | no | Request correlation. |
| | `gen` | generation string | yes | Core-minted on accepted handshake; null on refusal. |
| | `ready` | bool | no | False: no world writes/effects authorized. |
| | `kind` | `""` or ProtocolKind | empty | Empty on acceptance; refusal kind otherwise. |
| `HostCapabilities` | `hostOwned,equalCount,birthClear` | bool | no | Existing native-domain/equal-count/automatic-birth-clear mechanisms. `equalCount` ordinarily applies to logical lengths; on the slot route it applies to pager projection lengths. |
| | `sharedNativeRoster` | bool | no | One native desktop identity roster across outputs; ignored when hostOwned=false. Independent of `equalCount` and view-selector coupling. Does not alone enable the slot route. |
| | `nativeSelectorScope` | `per-output/all-outputs` | no | Effective native view selector coupling, not just API availability; ignored when hostOwned=false. |
| | `windowSuppression` | bool | no | Independent recoverable per-window hide mechanism. |
| | `maximize,fullscreen,nativeSticky,above,warp` | bool | no | Native state/stacking/cursor actuation support, not semantic command support or permission. |
| | `tiledFloatTransfer` | bool | no | Tiled source can become a float while crossing outputs without unsupported remap. |
| | `unavailableFacts` | FactName[] | no | Fixed unsupported classification inputs, unique and in FactName catalog order, <=21 entries. |

- `maximize` selects native actuation, not command availability: true uses the
  existing native overlay; false permits core-composed maximize through available
  frame writes. No extra composition/frame-support capability is added; existing
  per-window frame authority and safety gates determine whether writes are usable.
  Adapters never implement their own maximize policy. See [window-state policy](#window-state-policy).
- `ready:false` is normal after an accepted handshake; readiness requires a
  complete usable report and initial settings. Hard support cannot flip per
  report. Unsupported versions use the bounded error shape below.
  Readiness latches for the connection lifetime after accepted initial settings (field diagnostics do not block) and a complete usable report; later incomplete/unreadable reports retain `ready:true` with narrow scope holds, and only a new connection/handshake resets readiness.
  A valid report before initial settings returns an empty, correlated `ready:false` reply with no desired state or write/effect authority; it is not retained as authority, and readiness requires a complete usable report processed after initial settings are accepted.
  Before readiness latches, an incomplete report after settings returns the same empty, correlated `ready:false` reply; nothing is minted or retained as authority, and a later complete usable report initializes and latches readiness.
  [Capabilities](../decisions/contract.md#host-capabilities-and-settings).
- `nativeSelectorScope:all-outputs` makes core's effective workspace mode
  `shared`, preserving the existing global-selector behavior. Retain the raw
  requested mode and diagnose an incompatible separate-output mode; adapters
  never substitute settings or select policy. A native coupling option change
  requires reconnect/new handshake before further world writes.
  KWin advertises `per-output` only when `currentDesktopForScreen` exists
  and `options.perOutputVirtualDesktops` is strictly `true`. Otherwise it
  advertises `all-outputs`, including on KWin 6.6. KWin option notifications
  trigger a new handshake; a one-output write also rechecks the live option
  just before the setter and never falls back to a global switch.
  On newer KWin, shared actuation also requires a readable `false`; an
  unreadable option does not prove a global switch. The 6.6 shared setter
  remains available without the option.
- `above:false` forbids persistent project stacking targets; `warp:false`
  forbids non-null `warpCursor`, even when cursorWarp is requested. Read-only
  stacking observations are independent of actuation support. K1-K4 are
  [technical resolutions approved 2026-11-02](../changes/v4-migration.md#k1-k5-technical-resolution-pending-0c-approval).
- `tiledFloatTransfer` is exactly the existing `float-no-remap` limitation: a
  report-time tiled source becoming floating during a cross-output transfer,
  not a ban on same-output or already-floating transfers. KDE's mechanism is
  established as true; Windows's no-remap route is false. macOS provisionally
  declares false until cross-output frame plus native Space membership is proved.
  Its broader cross-output move limit is a macOS port gate, not another capability:
  implement frame writes onto the target display, settled by W2 native arrival
  observation. Until proved, such attempts settle as failed with native observation;
  no remap, guessed destination membership or adapter-local replan is allowed.
  No native-removal capability is added: see [G2](../changes/v4-migration.md#b-reconciliations-and-open-points).

## Revisioned settings

- Each adapter chooses which settings to expose on its own surfaces, using the
  shared catalog for field semantics. Exposure does not enter the handshake or
  settings messages: the handshake declares hard runtime capabilities, and the
  settings request carries raw values, not a UI field list.

- Send one complete raw settings snapshot at startup and on change. Core
  validates each field, applies valid siblings, and keeps last-good/default for
  invalid fields. Missing known fields use defaults at startup, last-good later;
  unknown setting names receive per-field diagnostics, not startup refusal.
  Direct `values` keys (raw setting names) are UTF-8 <=4096 bytes inclusive,
  including `""`; longer direct names refuse decoding before field processing.
  Nested JSON object keys inside setting values are opaque data subject only to
  the overall request byte limit; accepted unknown direct names are echoed
  verbatim in `SettingsDiagnostic.key`.
  Duplicate revision/content is idempotent; older revision returns `stale`
  without replacing state; equal revision with different content returns
  `revision-reused` without replacing state. Receipt order does not change a
  setting's catalog timing; `mode` (`workspace.mode`) is restart-only. A changed
  valid later value returns `restart-required` and takes effect only in a new lifetime.
  [Validation](../changes/core-interface-review.md#original-backlog-lines-103-122),
  [catalog/timing](../../crates/tiler-core/src/settings_catalog.rs).
- A checked settings revision installs its requested and effective settings,
  initial-settings readiness and layout settings only after preparing its
  exact reply within the reply byte bound. If the reply is oversized, refuse
  the whole revision with the existing transport size failure; a fitting
  snapshot can still use that revision. Diagnostics are not truncated.

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
| | `kind` | `missing/unknown-field/wrong-type/out-of-range/unknown-value/restart-required/unsupported-mode/unsupported-capability` | no | Closed typed diagnostic. |

- A valid requested separate-output `mode` with native all-output selectors
  receives `unsupported-mode`; core uses effective shared behavior without
  replacing the raw setting. `cursorWarp:true` with `warp:false` receives
  `unsupported-capability`; retain the setting but authorize no warp. These
  diagnostics do not refuse startup or valid siblings. Existing validation and
  restart-required diagnostics take precedence for the same field, so there
  is still at most one diagnostic per key. Reevaluate support at new handshake.

### Settings values

| Key | Type/range or tokens | Default | Application timing |
| --- | --- | --- | --- |
| `gap`, `outer` | integer 0..64 logical px | 8 each | Next plan (`gaps.inner/outer`). |
| `defaultTiled` | bool | true | Subsequent new workspaces (`workspace.default-tiled`). |
| `mode` | `per-output-local/global-unique/shared` | per-output-local | Restart (`workspace.mode`). |
| `sameAxis` | `group-with-neighbor/swap-with-neighbor` | group-with-neighbor | Subsequent move (`move.same-axis`). |
| `bornMaximized` | `tile-on-launch/keep-maximized-overlay` | tile-on-launch | Post-startup first appearance only; startup native maximize is adopted as an overlay (`admission.born-maximized`). |
| `predicate` | `both-axes-fixed/either-axis-fixed` | both-axes-fixed | Subsequent admissions only; retained classifications unchanged (`admission.fixed-size`). |
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
- User 2026-11-12 removed `anchorFallback` from Settings: ordinary anchor first,
  then fixed largest-splittable fallback under [D2](../decisions/layout-and-admission.md#best-effort-admission-fallback).
  A supplied removed key receives `unknown-field`; its value is not adopted and
  valid siblings still apply. No replacement setting or compatibility alias.
- User 2026-11-13 additionally removed `noSpaceResult`: geometric capacity
  exhaustion always floats under [D3/D7](../decisions/layout-and-admission.md#best-effort-d3-d4-no-space-result-and-focus).
  Supplied retired values receive `unknown-field` without adoption or loss of
  valid siblings. There is no compatibility alias or capacity-overlap selection
  or retained-overlap authority. Ordinary anchoring, fixed-largest fallback and
  B6 minimum handling remain; Apply alone never reflows, retries or evicts.
  Subsequent admissions and actual changed-bounds/gaps repairs use fixed float.
  The `noSpace` result provenance remains distinct from the removed setting.

### Scale and rounding

- `scale` is a finite positive JSON number: adapter units per logical pixel on
  every screen report. In KWin's logical coordinate space it is `1`, not the
  output's physical/logical device-pixel ratio. In Windows PMv2 physical-pixel
  space it is the target output's effective DPI divided by 96, not raw EDID DPI
  or the DPI of an arbitrary foreign window. In macOS CG/AX logical-point space
  it is `1`, not NSScreen.backingScaleFactor. Core retains no scale;
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
- Core retains at most 10,000 lifetime membership records, independently of
  the window-row limit on a single report. The prepared transition checks
  the final count after applying existing authoritative retirements and
  staged admissions, including commands. An over-cap candidate refuses as
  `snapshot-invalid` / `membership-over-cap`: no membership is evicted to
  make room, no staged world or command results are adopted, and the
  broker preserves its ordered FIFO. A fresh fitting capture can succeed
  in the same generation.

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
| `foregroundKnown` | bool | no | True with nonempty focused proves bound foreground; true with empty focused proves no client; false requires empty focused and means unknown/unreadable/unbound/foreign. |
| `focusChanges` | FocusChanges | required nullable | Ordered foreground and native selector changes not positively proved own, or snapshot-only. |
| `complete` | bool | no | Inventory completeness, not a hard capability. |
| `gesture` | bool | no | Existing gesture write barrier. |
| `cursor` | Point | yes | Native cursor in common coordinate space. |
| `actionForeground,actionCached` | OutputId | yes | Observed frozen action context, not chosen source. |
| `actionOutputs` | OutputId[] | no | Ordered known-output registry (<=16; refused as `snapshot-invalid` / `section-over-cap` above cap). |
| `presentation` | PresentationIngress | no | Native foreground/render facts. |
| `cursorHit` | WindowId | omit/null/empty | Native hit-test, never rectangle guess; empty when the hit is unreported/foreign/unbound. |
| `monotonicMs` | Counter | omit -> 0 | Arbitrary-epoch milliseconds, not wall-clock. |
| `creationAttempts` | NativeCreationAttempt[] | omit -> [] | Existing native-create lane. |
| `gestureIngress` | GestureIngress | omit/null | Existing native gesture lifecycle. |
| `primary` | OutputId | omit | Host-designated primary, unknown if absent. |
| `intentRecords` | IntentRecord[] | no | Opaque host storage return. |
| `propertyAttempts` | PropertyAttempt[] | no | Required even when empty. Closed tagged union: `frame`, `member`, `view`, `state`, `visibility`. Caps count all carried rows, including held rows: 1,024 per kind except 10,000 for `visibility`, 14,096 total. |
| `focusFeedback` | FocusFeedback[] | no | Correlated native evidence for the retained focus request. |
| `rendererInput` | RendererInput | yes | One exact core-drawn target per command. |
| `rosterFeedback` | SharedRosterFeedback | omit, never null | Exact native setter evidence for the retained shared-roster plan; absent on all other routes. |

- Removed report keys: `owner,mode,views,hostOwned,equalCount,birthClear,predicate,refill,defaultTiled,switchTarget,freshPlacement,focusFollowsCursor,cursorWarp,anchorFallback,noSpaceResult,bornMaximized`.
  Capability/settings keys move, not alias. `ScreenView` is removed. Logical
  view echo cannot prove native-free completion; visibility/focus facts do.
  [Visibility removal](../decisions/windows.md#state-centric-v4-visibility),
  [G1 reconciliation](../changes/v4-migration.md#b-reconciliations-and-open-points).

- The pre-release v4 attempt cutover has no dual spelling: the old five
  attempt arrays and `WindowFact.lastWrite` are not accepted.

- Report-codec hard refusal precedence (applies to the `WorldReportV4` codec
  only; it does not alter pre-decode endpoint checks or post-decode engine
  checks, and decoding order does not reorder wire replay): after the
  preceding envelope, inventory, and `monotonicMs` checks, `propertyAttempts`
  is validated as one section -- array shape, total cap, and row `kind` tags
  in producer order first, then per-lane caps and rows in
  `frame -> member -> view -> state -> visibility` order (producer order
  within each lane). Any hard `propertyAttempts` error beats
  `creationAttempts` and later sections; a hard `creationAttempts` error beats
  `focusFeedback` and later report decode. Identifiable held rows do not
  themselves refuse.

- `foregroundKnown:false` with nonempty `focused` refuses as
  `snapshot-invalid` / `focused-unknown-mismatch`. Inventory completeness or
  absence of a managed focus match cannot prove no-client foreground. Preserve
  fresh native read outcomes: foreign/unbound focus, failure, unsupported or
  stale reads remain unknown. On macOS only coherent frontmost/process evidence
  with genuine AX NoValue on both focused and main legs proves windowless;
  main-window fallback is not no-client evidence. Do not fabricate foreground
  from desired focus, setter success or retained membership absence.

- `focusChanges:null` is snapshot-only, for hosts without continuous
  capture. Otherwise use the closed object `{after,events}`. `after` is
  the quoted-decimal `rev` of the previous fully incorporated reply in
  this generation, or null if none. `events` is an array of at most 64
  records in platform capture order, or null for a gap. An empty array
  asserts continuous coverage and no unproved change; a gap asserts
  neither. Overflow or interrupted tracking produces a gap, not
  snapshot-only. The cap bounds memory and bytes, not host transitions.
- Event objects are closed: `{kind:"foreground",win}` uses a bound
  WindowId, `""` for positively known no-client, or null for an observed
  unbound/unreadable activation. `{kind:"view",out,native}` uses an
  OutputId and native desktop Id. All keys are required; recurring
  activations or selectors are allowed. Cover changes after the anchor
  through the snapshot capture cut. Omit a transition only if native
  evidence establishes that our call applying a core decision caused
  it; setter return, matching readback alone and timing are not proof.
  If the snapshot exposes a change the interval cannot account for, send
  a gap. A later proved-own change does not erase earlier unproved events.
  An empty array does not permit merging cuts across proved-own transitions.
- Freeze the interval with the capture. Only its exact bound
  incorporation acknowledgement retires that prefix, even when apply is
  skipped; a rejection or bootstrap reply does not. Events received
  during flight remain. Exact replay uses identical bytes and never
  consumes the prefix twice. A mismatched predecessor is adjudicated as
  a gap; the next acknowledged incorporation re-anchors capture. Reset
  on generation change. Malformed shapes and over-cap arrays are codec
  errors; a producer that overflows sends `events:null`.

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
  macOS vendor/model/serial unknown or disconnected sentinels normalize to null;
  a readable value is neutral metadata, not a stability guarantee. Connector stays
  null unless its physical-connection normalization is proved: never substitute
  CG display id, UUID or ordinal; a unit number alone does not prove connector
  identity. Ambiguous reconnect holds rather than picking a candidate or inventing
  a connector.
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
| `hostHidden,privileged,managed,acceptsInput,activationInhibited,toolWindow,owned,transient,modal,resizable,above,below` | bool | yes | Raw native facts; applicability is specified below, never adapter membership verdicts. |
| `owner,parent` | WindowId | yes | Known live relationship, null if unavailable/unmanaged binding unknown. |
| `role,appId` | bounded text | yes | Native role/application values without policy verdicts. |
| `executable` | executable identity | yes | Neutral native executable identity, not an appId alias or eligibility verdict. |
| `processId` | Id | yes | Native owning process identity, comparable only to this connection's applicationIdentity.processId. |
| `class,title` | bounded text | yes | Native class/title values without policy verdicts. |
| `type` | WindowType | yes | Mechanically normalized native kind, not admission or eligibility. |
| `activationPolicy` | `regular/accessory/prohibited` | yes | Native application activation policy, not inferred from acceptsInput. |
| `layer` | signed 32-bit integer | yes | Native window layer, not a tiling or above verdict. |
| `fullscreenOwner` | `tiler/app` | yes | Unknown does not mean app-owned or exitable. |
| `fullscreenExitable` | bool | yes | Whether neutral core-authorized exit can be performed. |
| `suppressionAvailable` | bool | yes | Per-window recoverable hide support/readability. |
| `visibility` | VisibilityFact | no | Current native visibility and recovery ownership. |

- Remove `floating,fitExcluded,fixedAuto,fixedSuppress,observedWs,shell,tool,ownedDialog,dialog,noActivate,cannotControl`; core derives these and
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
- Core adopts startup native maximize without a clear: the first accepted usable
  complete inventory after settings/screen readiness defines startup presence,
  including held identities. Earlier incomplete/unready reports do not finish
  startup. Delayed held/minimized/unreadable/unhomed admission retains that
  exemption until lifetime end; no maximized fact is invented. Ordinary tiled
  bases reserve slots; imported float/sticky and fixed-size classification keep
  their existing precedence. Only later first appearances use `bornMaximized`.
  Re-handshake resets the model and adopts its initial roster; transport recovery
  without reset preserves known-window state. No wire field or adapter policy is
  added. Fullscreen and the composed `maximize:false` route are unchanged.
  [Born-maximized decision](../decisions/window-state.md#born-maximized-policy-and-tile-overlays).
- Minimized/host-hidden membership and eligibility remain distinct; no implicit
  unminimize/app-unhide. Existing workspace migration/output displacement and
  pre-hide transfers may relocate membership without geometry or activation.
  OPEN: phase 0b verifies raw native fact coverage and lifetime reuse, including
  unmanaged fullscreen foreground capture, rather than inventing negative facts.
  [Evidence gaps](../changes/v4-migration.md#b-reconciliations-and-open-points).

- macOS declares maximize=false and reports native maximized=null in WindowFact
  and PresentationIngress; core-composed maximize is the selected 0.1 route.
  The core-owned overlay bit is not a native fact. A non-null maximized fact on
  this route is a conflicting-fact row hold. FactName/unavailableFacts covers
  classification inputs, not maximized, so no fixed-unavailable enum arm is added.
  Do not substitute a false constant or engine-placement/border-geometry zoom
  heuristic. User-invoked Window-menu Zoom has no readable maximize state and
  is ordinary frame drift, not a native maximize/restore command for this route.
  [M4 amendment](../decisions/contract.md#window-state-commands-r14).

### Neutral classification vocabulary and applicability

- `WindowType` is closed: `normal,dialog,sheet,utility,desktop,dock,menu,tooltip,notification,splash,other`.
  Native-kind conversion is adapter-owned mechanics: NETWM normal/dialog/utility
  (including toolbar)/desktop/dock/menu variants/tooltip/notification variants/
  splash map to those arms; remaining known kinds map to other. AXSheet maps to
  sheet; AXWindow with AXStandardWindow/AXDialog or AXSystemDialog/AXFloatingWindow
  maps to normal/dialog/utility; other known AX kinds map to other. Win32
  generic dialog class `#32770` maps to dialog, other ordinary top-level windows
  to normal; WS_POPUP/WS_EX_TOOLWINDOW/ownership alone do not manufacture a
  dialog/utility verdict. Native text stays in role/class; shell class checks, exceptions and
  type-based admission remain core policy. Unreadable kind is null, not other.
- `managed` is the native host's management flag (KWin isClient), not whether
  OmniTiler controls the window. `acceptsInput` is a native keyboard-input
  preference/capability (KWin wantsInput); it does not imply activation safety.
  `activationInhibited` is an independently observed native nonactivation flag
  (Windows WS_EX_NOACTIVATE); never derive it from !acceptsInput.
  `toolWindow` is an independent native tool-window flag (WS_EX_TOOLWINDOW),
  not the utility type. `owned` is an independent native owner-window flag
  (GW_OWNER non-null), even when the owner cannot be bound to a WindowId;
  transientFor alone does not produce it. `owner` may bind transientFor,
  and `transient`/`modal` remain independent native facts. `above,below` are
  native stacking flags/bands, never inferred from float/sticky intent.
  Windows WS_EX_TOPMOST supplies above, not below=false: HWND_BOTTOM is a
  placement, not a persistent below flag. GetParent can return a popup's owner;
  do not publish that as a parent relationship without distinguishing the arms.
- `FactName` is closed, in order:
  `privileged,managed,acceptsInput,activationInhibited,toolWindow,owned,transient,modal,resizable,above,below,owner,parent,role,appId,executable,processId,class,title,activationPolicy,layer`.
  A listed unavailable fact must be null in every window row; a non-null
  contradiction holds that row as conflicting-fact. Unknown names, duplicates,
  wrong ordering/types or missing unavailableFacts refuse the handshake.
  Unavailable means the connection cannot supply that native input, not an
  eligible default. Never add a fact to the list because one read failed.
  An unlisted required input that is null is unknown now and holds only the
  classification/action that needs it, not unrelated windows or properties.
- Utility (including normalized toolbar) and splash are ordinary float origins,
  like dialog/sheet; independent safety gates still win. PiP has no universal
  kind or title-derived policy; use ordinary reported facts. See
  [admission decision](../decisions/window-state.md#splash-utility-and-picture-in-picture-admission).
- Unreadable `hints` do not erase independently proven dialog/sheet/utility/splash,
  transient/modal or retained explicit/automatic-fixed float membership.
  Preserve unknown hints; ordinary hint-dependent admission and explicit tile
  override remain held. Refuse float-to-tile convergence and constrained float
  placement while hints are unreadable, before authorizing a prerequisite
  maximize-clear. Withhold outstanding dependent frame operations and
  float-route maximize-false targets across a hint-read gap without cancelling
  their bindings or rearming spent attempts; readable recovery resumes the
  same operation identity.
- Core applicability: evaluate only predicates whose native inputs are supplied
  by the connection; skip unsupported predicates, never turn null into false.
  Available managed=false excludes host-unmanaged candidates. Available native
  privileged/toolWindow/owned/activationInhibited facts feed their existing
  safety gates; owned is not inferred from a KDE transient relationship.
  Core derives shell/generic-Win32-dialog safety from native class/type/executable, with
  existing product exceptions; KDE dialogs and AX dialogs/sheets remain float
  origins, not Windows owned-dialog exclusions. Existing transient/modal and
  fixed-size predicates remain core-owned; resizable is carried without a new
  admission rule. acceptsInput is carried separately, not a replacement for the
  no-activate safety gate. Existing application activation-policy/layer gates
  use activationPolicy/layer where provided, with recognized float exceptions
  preceding the nonordinary-layer gate. No native policy verdict is needed.
- Initial source-fit declarations: KDE lacks independent privileged,
  activationInhibited/toolWindow/owned flags and activationPolicy/layer; the
  reviewed Windows Win32 route lacks managed/acceptsInput/transient/modal/below/
  role/activationPolicy/layer inputs. Its appId/title readers are also absent;
  API-readable metadata needs producers or an unavailable declaration, not
  fabricated negatives. This is route support, not proof of OS impossibility;
  macOS lacks privileged/managed/acceptsInput/activationInhibited/toolWindow/owned/
  transient/above/below/owner/parent producers. These are source-fit inputs for
  platform review, not fabricated negatives or live acceptance. The
  [Windows review](../changes/v4-migration.md#phase-0b-windows-platform-fit-review)
  records the W2-W4 technical resolutions approved 2026-11-02.
  Metadata availability follows actual capture support; all other unreadable
  observations stay nullable. Core rules requiring unavailable optional metadata
  do not apply; required identity/geometry/type are never waived by this list.

### Executable identity

- `executable` is required nullable; handshake `applicationIdentity` is a required
  object with required nullable `appId,executable,processId`, in that order. Its non-null
  appId is nonempty text <=4096 UTF-8 bytes without NUL, in the same native encoding
  as WindowFact.appId; absence is null, not an invented own-bundle value.
  A non-null executable is nonempty text <=4096 UTF-8 bytes, with no NUL. It is a
  native executable identity in the host's normalized encoding, never a title,
  display name, shell/own-settings boolean or substituted application id. Hosts
  without this input list executable in unavailableFacts, use null for window
  executable and applicationIdentity.executable and skip only predicates requiring
  it. Supported-but-unreadable window identity stays null and holds only the
  dependent predicate. Supplying self executable while declaring executable
  unavailable, or omitting readable self executable when declaring support,
  refuses the handshake.
- Windows uses the full executable path: replace `/` with `\`, then ASCII
  lowercase, exactly the existing executable comparison. Do not canonicalize
  dot segments, resolve relative paths, consult the filesystem or Unicode-fold.
  Executable equality is not an own-process or Settings classification rule.
  The existing generic-dialog membership gate still refuses unowned `#32770`,
  not arbitrary dialog types or reuse of the Settings class.
- Core preserves the border-only generic-XAML exception: exact classes
  `Windows.UI.Core.CoreWindow` or `XamlExplorerHostIslandWindow` require basename
  `startmenuexperiencehost.exe,shellexperiencehost.exe,searchhost.exe,searchui.exe,widgets.exe,textinputhost.exe`.
  Split executable on either slash; normalized Windows values already have
  lowercase basenames. Exact unconditional shell classes remain unchanged;
  ordinary explorer file browsers and foreign generic-XAML windows are not
  excluded by executable/class resemblance. This is not a membership verdict.
- KDE/macOS may supply native executable identity when available; otherwise
  declare it unavailable. Identity stays on the local wire only: do not add it
  to logs, telemetry or persisted intent. Fixtures use synthetic identities.
  W4 is a technical resolution approved 2026-11-02, not native acceptance.

### Process identity and own-process exclusion

- `WindowFact.processId` is required nullable immediately after `executable`.
  It is the native owning process identity, using the existing opaque Id grammar,
  not a JSON number, executable, bundle id, class, window id or policy verdict.
  Host values share one process namespace with `applicationIdentity.processId`;
  compare exactly within the handshake lifetime only. Native PID encodings may
  use decimal Id text; fixtures use synthetic identities. Do not infer process
  identity from metadata or persist it across handshakes.
- Declaring `processId` unavailable requires null own and window processIds;
  support requires a non-null own processId at handshake. A supported-but-unreadable
  window processId remains null and holds only dependent classification/actions.
  A non-null window value contradicting unavailableFacts holds that row as
  conflicting-fact. Executable and processId availability are independent.
- Core excludes only windows whose known processId equals the connection's known
  own processId: internal overlays, borders, pager, carriers and proof/control UI.
  A separate process of the same executable or bundle is not excluded by identity
  alone. Settings is ordinary on every platform: resizable ordinary windows tile;
  fixed-size/dialog windows follow the same admission rules as any application.
  There is no Settings-class exception or appId/executable self-exclusion.
  Generic-dialog and generic-XAML shell rules above are unchanged.
  [User decision 2026-11-02](../decisions/contract.md#own-process-ui-and-ordinary-settings)
  supersedes the Windows Settings-class exemption and amends M1/W4. Windows
  Settings resizability and macOS Settings process separation or a neutral marker
  are separate product follow-ups, not phase-1 adapter work.

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
  Core is the sole native-removal authority: `exist:false` under the
  [existing retirement rules](#native-only-retirement-with-per-output-rings),
  explicit removals on the [slot route](#shared-native-roster-slot-plans).
  Unknown occupancy is not zero; incomplete evidence holds. Adapters never
  delete autonomously, from omitted desires, or from local creation ownership.
  Fresh occupancy, visibility, pin, minimum and completeness vetoes apply on
  both routes. No native-removal capability is added.
   [R19](../decisions/layout-and-admission.md#native-workspace-adoption-and-r19),
   [G2](../changes/v4-migration.md#b-reconciliations-and-open-points).

#### Native-only retirement with per-output rings

Outside the slot route below, keep existing backing and `exist:false`
retirement authority, including for linked `shared` on a native shared
roster and effective `shared` with an all-output-only selector.
For independent host-owned rosters, logical ring removal and native deletion
are separate operations. A native desktop can remain on the host after its
empty, unshown logical workspace leaves the ring. `equalCount` keeps equal
logical ring lengths as an ordinary-empty pruning exception; eligible padding
prunes when the longest ring shrinks. This says nothing about shared native
identity or the adapter's native minimum-count floor.

KDE declares true in both native selector modes. Output-local macOS Spaces and
the tiler-owned Windows workspace route declare false. This required connection
shape must be adopted by every adapter; old strict readers reject an amended
handshake until updated. These declaration updates do not establish native port
acceptance or adopt the separate close-focus amendment.

- Core continues publishing an existing workspace row with `exist:false` and
  `bind` naming that native desktop for each locally retired output/native pair.
  The removed logical `id` identifies the absence record, not a live binding or
  ring member. These records follow live ring rows, are never trailing spares,
  and do not contribute to logical ring count or occupancy. No new wire key,
  format, version or platform selector is introduced.
- Core retains the absence record until complete native deletion evidence or
  positive evidence on that output clears it: native current selection or an
  observed window on that output/native pair. Ordinary adoption then supplies a
  fresh logical identity at its position among the output's live native-backed
  ring rows. Global occupancy counts alone are not output-local arrival evidence.
  Existing settled migration source-release adoption remains operation-scoped:
  only the migrated workspace's vacated source bindings clear before fresh adoption.
- Retirement records retain existing scope, freshness, readability and hold
  fences. Core does not retire displaced or held units. A native still backing a
  live workspace on another output is protected from deletion; a retirement
  record does not authorize destruction of that live backing.
- The adapter rechecks all native occupants, any-output visibility, protection,
  completeness and its minimum-count veto before deleting. Native floor padding
  is adapter-owned: bootstrap may create native desktops up to its minimum,
  which core adopts from ordinary native observations. This is distinct from
  ordinary spare backing, which still requires explicit core creation desires.
- Surviving native IDs, not host ordinal numbers, identify backing. Native
  renumbering never authorizes relocating occupants to compact gaps. Existing
  migration-release adoption and displaced-workspace handling are unchanged.

[User decision 2026-11-10](../decisions/contract.md#native-only-retirement-intent).

#### Shared native roster slot plans

Applies only with `hostOwned:true`, `sharedNativeRoster:true`, an independently
writable per-output selector and effective `per-output-local` or
`global-unique`. Linked `shared` and all-output selectors do not run this plan;
see [scope and portability](../decisions/workspaces.md#workspace-scope-vocabulary).

- Core retains a settled epoch with the ordered native IDs and each output's
  real-row or tail-padding projections. A desired plan is separate until proved.
  Padding is not a `WorkspaceDesire` logical row, spare or occupancy owner.
  Core computes the plan before publication using accepted projected membership
  and views for lifecycle; unproved native facts remain preconditions, not
  predicted arrivals. The [slot decision](../decisions/workspaces.md#dynamic-workspace-lifecycle-and-kde-pager-backing)
  governs the count, append-only growth and order-preserving shrink.
- A retained plan binds a stable operation and stable step identities to its
  settled epoch and issuing generation/revision. It names the exact baseline
  roster/order and selector currents, desired projections, explicit append slots
  and removal IDs, exact window lifetimes and native membership/view preimages
  and targets, and the dependencies between steps. A dependent target in a
  newly appended slot resolves only from exact correlated creation proof,
  never a guessed ID by number or name. No per-output tombstones or spare
  fan-out authorize native mutations on this route.
- Preflight requires complete, exact raw occupancy evidence on affected
  source, target and removal slots, including unmanaged occupants. Counts
  alone do not identify the windows. Require a complete native occupant
  witness or readable, deduplicated memberships that reconcile exactly with
  the occupancy totals; residual or unknown occupancy holds.
  Apply the [occupancy fences](../decisions/workspaces.md#dynamic-workspace-lifecycle-and-kde-pager-backing)
  to affected steps, not to a proven unchanged sibling occupant. Recheck
  generation, freshness, lifetime, writable scopes and native preconditions
  before setters; remove only above the floor with live global emptiness,
  no current selector showing that desktop and no protection.
- One publication and one apply is the clean synchronous target: append any
  required slots with exact creation proof, perform the authorized final member
  changes and verify arrival, switch only planned selectors and verify current
  before authorized focus, then remove proved-safe surplus and verify absence.
  Frame writes keep their independent authority and arrival gate. Separate
  setters are not a compositor transaction; final visibility and focus are
  the guarantee, not uninterrupted visibility. No dual-membership bridge.
  Arm correlated feedback and spend each step before native dispatch.
  Expected echoes do not waive freshness or external-input fences.
- Async arrival, refusal, ambiguity or external input stops dependent work.
  The adapter reports the actual prefix and readbacks, without substitution,
  rollback or autonomous recovery. Core matches each receipt's readbacks to
  the exact pending step, including kind, preimage, target and dependencies,
  before retaining it. It independently requires fresh native order,
  occupancy, same-lifetime membership and view proof before settling the plan.
  A refusal ends that operation's write authority. Replan only from fresh
  observation establishing the actual state, including any applied prefix;
  refusal is not evidence that the original roster is intact. When the
  refusal arrives with sufficient recovery evidence, do not immediately
  issue equivalent native work against the same state. An unproved step
  does not release a migration source or reinterpret a window through the
  desired mapping. A settled replay emits no mutations; unchanged publications
  never renew spent or possibly executed steps.

The optional v4 `rosterPlan` and `rosterFeedback` envelope keys are absent
when unused. A present value is a strict object, not `null`. All objects
below have exactly the indicated keys; counters are canonical quoted u64
decimals. `op` and `step` identify correlated authority; other IDs use
the usual v4 scalar validators.

| DTO | Exact shape |
| --- | --- |
| Plan | `{op,gen,rev,obs,baseline,currents,occupants,projections,slots,members,views,removals,focus}` |
| Baseline | `{epoch,order,projections}` |
| Projection | `{out,rows}`; `rows` contains logical IDs in a real-row prefix. |
| Current / home | `{out,native}` |
| Occupants | `{native,windows:[{win,out}]}` |
| Slot | `{kind:"existing",native}` or `{kind:"append",step}` |
| Member step | `{step,win,before:{out,native},out,target:Slot,deps:[step]}` |
| View step | `{step,out,before:native,target:Slot,deps:[step]}` |
| Removal step | `{step,native,deps:[step]}` |
| Focus | The existing `FocusRequest` object or `null`. |
| Feedback | `{op,gen,rev,steps:[{step,status,before,after}]}` |
| Readback (`before` / `after`) | `null`, `{kind:"order",order:[native]}`, `{kind:"home",out,native}`, or `{kind:"current",native}` |

`currents`, `occupants`, `projections`, `slots`, `members`, `views` and
`removals` are arrays. Native orders and slots cap at 25; output arrays
cap at 16, member steps and total occupant lifetimes at 10,000, and step
ID arrays at 10,066. Existing request/reply byte bounds also apply.
Readbacks preserve the setter's actual pre/post snapshots. `appended`
requires two orders whose only change is one fresh tail ID. `applied`
requires matching non-null readback kinds and either a changed home, a
changed current, or an order with exactly one deletion and preserved
survivor order. `refused` may carry `null` for an unavailable readback;
it ends the correlated plan, but its readbacks are not success receipts.
Core receives the typed status and both readbacks. It checks success
readbacks against the exact plan step, deriving an appended ID only from
that step's ordered proof and requiring the exact planned ID for removal.

## Commands and results

- At most one user command per report. Payloads carry only user choice or
  input-carried identity/coordinates; core resolves policy at execution. The
  experimental same-user scripting surface uses these same verbs on every
  platform, not a second engine route. [Payloads](../decisions/contract.md#command-payloads),
  [R18](../decisions/contract.md#scripting-command-surface-r18).

### Ordered command ingress

- Every platform uses one mandatory prepared core transition
  (`WorldEngine::handle_report_with_gate_and_prepare`), taking a fresh report
  plus an ordered list of `(command sequence, command)` pairs, empty when there
  are no queued commands. Wire inline `command` + `cmdSeq` normalizes to a
  proof-bearing first entry followed by the caller FIFO (128-entry total cap
  including inline).
- Structural invalidity on any batch entry (v4 vocabulary, subject/ID, or
  renderer-proof rules) refuses the whole batch before incorporation as
  `snapshot-invalid` / `command-invalid`. Duplicate or descending sequence
  numbers in a fresh ready batch refuse before incorporation as
  `snapshot-invalid` / `command-order-invalid` without sorting. Both
  whole-batch refusals emit no per-sequence dispositions and advance no
  high-water mark.
- Only the first submitted entry (normalized inline entry when present,
  otherwise the first queued command) may use the frozen pre-observation
  send/migration source; any later command resolves against post-incorporation
  state after prior commands, even if the first entry is consumed or refused.
- Report incorporation is reported explicitly via `ReportIncorporation`:
  `Incorporated` (fresh ready report adopted), `BootstrapIntentOnly` (valid
  report before readiness with no ready world authority), `Replay` (cached
  publication for an exact report retry, not a refusal, returning the cached
  reply and `NotRun` for any resubmitted batch), `NotAdmitted` (whole-capture
  pre-inspection refusal before queue inspection --
  owner/correlation/adapter/generation/capability/arrival/size/report-structure
  and stale/reused request fences -- returning empty dispositions without
  enumerating per-sequence receipts), and `Rejected` (inspected whole-report
  or batch refusal with no adoption or execution). Invalid or disordered
  inspected batches emit empty dispositions (`snapshot-invalid` with
  `command-invalid` / `command-order-invalid`); valid inspected post-inspection
  refusals (observation stale, preflight, row/operation caps, revision
  exhaustion, or later staged/bootstrap outbound gates) retain `NotRun`
  dispositions because queue validation already established the supplied
  identities.
  A stale inline entry reports `AlreadyConsumed` without suppressing fresh
  queued commands; a per-command refusal returns `Refused(kind)`, consumes its
  sequence, and rolls back only that command's partial changes.
- Mandatory outbound semantic and host-capability validation runs at final
  prepare on both ready and bootstrap paths before candidate adoption. Late
  semantic failures return unremembered `InvalidReply` (`reply-invalid`), and
  actual bound overruns return unremembered `Unavailable`; both leave authority,
  watermark, and parked FIFO untouched.
- User commands do not wait for batching. A ready command preempts any armed
  native-event debounce; the 120 ms debounce applies only to native-event
  bursts. An outstanding capture or filing acknowledgement still fences the
  next command capture to preserve ordering.

- The queued transport's command envelope remains
  `{"v":4,"gen":G,"client":C,"clientCmd":"N","command":{...}}`.
  The subject distinction belongs to the command's `win`, not a second queue
  envelope field. Existing explicit command bytes remain unchanged.
- Queueing parks each command in FIFO order and fires the existing wake. For
  KDE shortcut ingress, KWin takes a native cut as it handles the shortcut,
  before filing. Commands may share an unsent cut only with continuous
  subject-relevant native state. Foreground or selector transitions, including
  proved-own ones, and gaps split the cut. Commands do not wait for batching.
- From the first filing of a shortcut segment until ready incorporation, the
  actuator sends only that segment's frozen cut or its exact replay, and
  files no later segment. Exact replay acknowledges the original cut without
  draining again. A coherent cut with a gap uses the conservative focus rule;
  an unprovable cut never falls back to later native focus. An irrecoverable
  filed cut requires generation reset, which also drops parked external work.
- The broker still drains the whole parked FIFO on fresh incorporation;
  external senders have no physical-key ordering promise. Commands arriving
  during capture park for the next batch. There is no queue-time execution
  or restamping of old evidence, and no wait for native application.
- KDE keys issuer command high-water marks and results by the authenticated
  unique D-Bus sender and client in the serving generation. At most 1,024
  live sender/client namespaces are retained; result eviction does not
  lower a live issuer's sequence floor. Only proved loss of that exact
  unique name on the serving bus retires its namespaces and results.
  Already-filed commands stay in FIFO order and execute without publishing
  results to the departed issuer. Authorization in flight for a departing
  sender cannot recreate its retired namespace; unrelated issuers are
  unaffected.
- For every window-targeting command below, `win:{"currentFocus":true}` selects
  core's current focus at execution, after preceding actions. An explicit id
  keeps its existing subject meaning. Empty `win` is allowed only on
  directional focus and still means genuinely no focused subject; missing or
  null `win` remains invalid. The object has exactly one key with boolean
  `true`; extra keys, false and other types refuse. Implicit maximize uses core-resolved focus instead of
  a settled-native-focus equality check; explicit maximize keeps its gate.
- The capture returns one final full actuator publication for the batch, while
  `handle_report_with_gate_and_prepare` returns `TransitionResult` (incorporation,
  deliverable `WorldReplyV4`, and sequence-keyed `Vec<BatchDisposition>`), and
  the broker retains per-command results on the requester-only
  `TakeCommandResult` lane (`MAX_SETTLED_RESULTS = 1024`). Native
  application keeps only the newest pending publication and follows the
  [normative apply rule](#normative-apply-rule).
  Superseding native work never discards incoming `closed` or attempt evidence.
  [Rapid-input decision](../decisions/commands.md#kde-rapid-input).

### WorldCommand payloads

| `op` | Other keys and types | Null/absence | Meaning/source |
| --- | --- | --- | --- |
| `focus` | `win`: WindowId or current-focus object | no | Explicit-id or execution-time focus; ignored direction removed (R6). |
| `send` | `win`: WindowId or current-focus object; `out`: OutputId; `ws`: WorkspaceId; `follow`: bool | no | Explicit destination on the window's own output; a different-output `ws` is refused (`operation-refused`). Cross-output moves use `send-directional`. |
| `select-workspace` | `out`: OutputId; `ws`: WorkspaceId | no | Explicit logical selection, unchanged. |
| `select-numbered` | `out`: OutputId; `index`: integer 1..9 | out nullable | Core resolves implicit output. |
| `select-relative` | `out`: OutputId; `delta`: integer -1 or 1 | out nullable | Relative ring selection. |
| `select-previous`, `select-trailing` | `out`: OutputId | out nullable | Existing planner selection. |
| `send-relative` | `win`: WindowId or current-focus object; `source`: SendSource; `delta`: -1 or 1; `follow`: bool | no | Core resolves retained source. |
| `send-numbered` | `win`: WindowId or current-focus object; `source`: SendSource; `index`: integer 0..9; `follow`: bool | no | 0 retains trailing-send meaning. |
| `set-workspace-tiling` | `out`: OutputId; `ws`: WorkspaceId; `tiled`: bool | no | Explicit requested state. |
| `toggle-workspace-tiling` | `out`: OutputId; `ws`: WorkspaceId | no | Toggle retained core state. |
| `migrate-workspace` | `from,to`: OutputId; `ws`: WorkspaceId | no | Distinct outputs, stable logical id. |
| `focus-directional` | `win`: WindowId or current-focus object; `direction`: Direction | win may be empty, never null/missing | Empty means genuinely no focused subject; object selects core focus. |
| `move-directional` | `win`: WindowId or current-focus object; `direction`: Direction; `sameAxis`: SameAxisMove | no | Shared core route including macOS. |
| `send-directional` | `win`: WindowId or current-focus object; `direction`: Direction; `follow`: bool | no | Core resolves destination. |
| `migrate-directional` | `direction`: Direction | no | Action source, no retained-window-home source. |
| `toggle-maximize` | `win`: WindowId or current-focus object | no | Existing command and result kept. |
| `toggle-float` | `win`: WindowId or current-focus object; `rect`: Rect | rect nullable | Explicit placement stays input-carried. KDE sealed implicit toggles carry `rect:null`; ordinary unfloat can save only the resolved lifetime's eligible cut frame (already settled floating, known nonfullscreen, and unchanged by earlier batch commands), otherwise retaining the saved placement. |
| `toggle-orientation` | `win`: WindowId or current-focus object | no | Promoted live operation. |
| `resize` | `win`: WindowId or current-focus object; `direction`: Direction; `mode`: resize token; `press`: integer | no | Existing promoted payload, no resize-policy change. |
| `set-fullscreen`, `set-sticky` | `win`: WindowId or current-focus object; `value`: bool | no | New neutral on/off state intent. |
| `toggle-fullscreen`, `toggle-sticky` | `win`: WindowId or current-focus object | no | Core resolves toggle. |

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
  routes. Native gesture lifecycle remains unchanged; live drag preview rides
  the retained nullable wire `dragPreview` slot (Rust `WorldReplyV4.drag_preview`) rather than
  `CommandResult`.
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
  native-Spaces behavior remains unsupported until its route is proven.
  [R14](../decisions/contract.md#window-state-commands-r14).
- Fullscreen-on from maximized enters immediately: maximize and fullscreen are
  independent overlays. Ordinary fullscreen exit returns to retained maximize.
  For settled maximize, the native route issues no unmaximize on entry or extra
  maximize on exit; host flag retention remains a native acceptance gate. A pending,
  unattempted native maximize-on/off target is retained but held while a newer
  fullscreen-on is pending or active, even before raw facts change, then applied
  after exit. The latest maximize command's requested state is the exit destination;
  no unordered same-reply Fullscreen=true plus Maximized write authority is allowed.
  The composed route keeps its owned maximize bit and recomputes the work-area
  frame on exit. Explicit R-MAX-11 clear-before-maximize replacement is separate.
- Sticky command support is semantic: nativeSticky permits native sticky
  StateTarget/StateAttempt actuation. Without nativeSticky, windowSuppression
  permits core to compose visibility across its managed workspaces from retained
  sticky intent, persisting stickyPrior in the opaque record. No native sticky
  target, attempt or projected fact is fabricated: WindowFact.sticky must be
  null when nativeSticky=false (non-null is a conflicting-fact row hold).
  Core emits no sticky StateTarget on that route (a reply containing one is
  invalid); unsolicited sticky attempts
  are conflicting-fact attempt rows, not completion. Commands reject with
  state-unsupported only when both routes are absent; per-window suppression
  unreadability/recovery gates still hold the affected visibility lane. This
  retains Windows sticky behavior without native actuation. W3 is a technical
  resolution approved 2026-11-02, not an all-Spaces or implementation claim.
- Maximize has the same native/core-composed ownership split: maximize=true
  retains the existing native overlay route unchanged (KDE/Windows); maximize=false
  with frame writes available selects core composition (macOS). Core owns a
  lifetime-local overlay bit, retains slot/share/allocation without sibling reflow,
  and publishes frame=output work area under frame authority. Clear restores the
  retained tile allocation or retained float placement; the overlay frame never
  becomes the saved float placement. Core emits no maximized StateTarget on this
  route (a reply containing one is invalid); unsolicited native maximize attempts
  are conflicting-fact attempt rows, not completion. WindowFact and
  PresentationIngress maximized remain null; a non-null value holds the conflicting
  row. No adapter-owned maximize path is permitted on any host.
- A composed maximize request with unavailable frame authority is refused using
  the existing applicable refusal gate; overlay intent stays unchanged. The
  request is not deferred or automatically composed when authority returns: a
  fresh press is required. This applies existing refusal precedent, with no new kind.
- External user move/resize clears composed maximize; do not infer a user action
  merely from delayed/clamped frame readback. macOS Window-menu Zoom is ordinary
  frame drift, a host limit rather than a synthetic native maximize fact.
  Born-maximized is undetectable on the composed route, so admission is ordinary.
  Orderly restart drops the bit and returns the window to base; it is not persisted
  in opaque intent and adds no M3 storage/recovery dependency.
- Composed maximize follows the same overlay directional-focus/move policy:
  directional focus skips same-output siblings, and directional move clears the
  core bit locally before ordinary move policy, without native clear/readback.
  For a composed-maximized float directional move when the screen or work area
  is missing, or half-snap geometry is unavailable: retain the bit, emit the
  existing directional refusal, mint no placement token and perform no deferred
  clear on recovery; a fresh move may clear. Once geometry resolves, the local
  clear precedes ordinary constraints; a constraint refusal restores the
  retained base with one authorized token. Exact native/reference timing
  remains TBD. B9 intentional unfloat clears the bit locally before fresh
  admission, with no observe-clear wait. R-MAX-11 first exits native fullscreen under existing
  exitability/observation rules, then composes; unconfirmed exit grants no frame
  write. Fullscreen precedence and all safety/arrival gates remain unchanged.
  Sends/output transfers preserve the bit and recompute the destination work-area
  frame under destination authority; work-area changes recompute it in place.
  [Overlay decision](../decisions/window-state.md#fullscreen-and-maximize-overlays),
  approved 2026-11-03. Reassess after the full contract rearchitecture and platform
  ports if jank/issues appear; this specifies intent, not implementation/native proof.

### CommandResult and diagnostics

- An accepted report always incorporates its trustworthy observations, even if
  its command is rejected. `WorldReplyV4` carries no `result` field;
  per-command outcomes are returned only on the issuing
  `TransitionResult.dispositions` (`Vec<BatchDisposition>`) and retained per
  requester on the broker `TakeCommandResult` lane (`MAX_SETTLED_RESULTS = 1024`).
  `CommandResult` keeps `Float`, `Migration`, `Directional`, `Maximize`, and
  `Rejected {kind}` (`CommandResult::DragPreview` is removed; no Applied/NoOp
  wrapper). `null` in a settlement/take payload is permitted for successful
  snapshot-only commands, not a universal no-op label.
  [Outcome decision](../decisions/contract.md#command-outcome-in-reply).

| Changed/new record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `CommandResult::Rejected` | `op` | `rejected` | no | Accepted-report command rejection. |
| | `kind` | CommandRejectKind | no | Closed reason, no adapter fallback. |
| `CommandResult::Directional` | `op,outcome` | `directional`, DirectionalOutcome | no | Existing variant/tokens. |
| | `win,rect,cause` | WindowId, Rect, DirectionalCause | yes | Existing nullable fields; cause now closed. |
| `OperationEvent` | `win,kind,op,outcome` | WindowId, `clear/float`, Id, OperationOutcome | no | Latest completed phase, unique by `(win,kind)`; kind must agree with outcome. |
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
| `transfers` | TransferTarget[] | no | Retained transfer correlations. |
| `focusRequest` | FocusRequest | yes | Required nullable retained request for every cause. |
| `dragPreview` | DragPreviewV4 | yes | Required nullable retained identity-bound drag-preview overlay state (`{id,rev,domainOut,domainWs,sourceOut,sourceWs,rect,hover,win}` on the wire; Rust `WorldReplyV4.drag_preview`). |
| `gestureSettled` | GestureSettled | yes | Retained latest core mutation outcome, not native completion. |
| `operationSettled` | OperationEvent[] | no | Complete retained latest clear/float outcomes keyed by `(win,kind)`. |
| `nativeCreations` | NativeCreationDesire[] | no | Explicit native creation authority on the existing route; the approved slot route authorizes appends through its retained plan. |
| `warpCursor` | Point | yes | Core-decided cursor target. |
| `presentation` | WorldPresentation | no | Full accepted rendering publication. |
| `intentDesired` | IntentDesire[] | no | Complete retained per-window storage targets, including explicit clears. |
| `switchSettled` | SwitchEvent[] | no | Complete retained latest transfer/view parent outcomes keyed by `out`. |
| `diagnostics` | RowDiagnostic[] | no | Narrow held-row errors. |
| `rosterPlan` | SharedRosterPlan | omit, never null | Retained authority to mutate the shared native roster, only on the slot route. |

- Within each projected tiled `(out, ws)` domain, rows are in bytewise `win`
  order. Domain traversal and the interleaving of other lanes are unchanged.
  Consumers must match rows by `win`, never by index; deterministic ordering
  keeps the KWin application prefix the same across routes when freshness
  changes stop application.

- Only controlled property authority authorizes that property's writes. Held,
  untouched, missing rows and retained geometry authorize nothing. Floating
  rows never imply frame writes: ordinary floats retain native frames; explicit
  `frame_op` names one-shot float placement. Core-composed maximize and its base
  restore instead use explicit `frame/frame_auth` authority, including for a
  floating subject; no native-state target or extra wire bit is needed. The
  floating diagnostic never substitutes for or vetoes that explicit authority.
  `float_origin:capacity` is diagnostic only. Create, member, frame and state
  authority stay distinct.
  [Desired-state baseline](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes),
  [state-centric decision](../decisions/contract.md#state-centric-contract).

- The approved [slot-plan authority](#shared-native-roster-slot-plans) is
  retained with the desired publication, not derived from workspace rows or
  a reply omission. This section specifies its semantics, not a new wire key
  or an implemented codec shape.

- Occupants of a floating-mode workspace, including minimized occupants,
  publish `floating:true` with empty `float_origin`. This describes the scope's
  classification, not per-window float intent or frame/membership write
  authority; minimized holds remain. Retiling must not feed this classification
  back as retained explicit float intent. See the
  [workspace-mode decision](../decisions/workspaces.md#per-workspace-tiling-floating-and-bindings).

### Changed desired rows

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `ScopeStatus` | `scope` | AuthorityScope | no | Typed authority target. |
| | `auth,reason` | ScopeAuthority token, HoldReason | reason empty outside hold | Existing flattened encoding. |
| `WorkspaceDesire` | `id,out` | WorkspaceId, OutputId | no | Logical identity and owner. |
| | `bind` | NativeWorkspaceId | empty | Existing unbound sentinel. On the slot route, a published target binding is descriptive only, not settled evidence or setter authority. |
| | `scope` | AuthorityScope | no | Typed owning scope. |
| | `order,mode,exist,hold` | integer, TilingMode, bool, HoldReason | hold empty if none | Existing ring order/mode/removal eligibility. |
| | `trailing` | true | omit unless true | Existing explicit spare designation. |
| `WindowDesire` | `win,out,ws` | WindowId, OutputId, WorkspaceId | no | Exact lifetime and core home. |
| | `alloc,frame` | Rect | yes | Existing allocation/effective frame. |
| | `frame_auth,frame_hold,member_auth,member_hold` | authority/reason pairs | hold strings empty if none | Existing independent geometry/membership authority. |
| | `frame_op` | Id | empty | One-shot explicit float placement. |
| | `floating,float_origin` | bool, FloatOrigin | origin empty if none | Existing classification diagnostic. |
| | `cmdSeq` | Counter | yes | Explicit command correlation for the frame target. |
| | `visibility` | VisibilityTarget | yes | Composed visibility target, not row-presence permission. |
| | `visibility_auth,visibility_hold` | authority/reason pair | reason empty if none | Independent visibility lane. |
| | `state` | StateTarget[] | no | Independent native-state targets. |
| `VisibilityTarget` | `shown` | bool | no | Desired shown state; never native unminimize. |
| | `rev` | Counter | no | Existing world revision at target authorization; retained across observation-only replies. |
| | `fallback` | bool | no | Core permission for hide-only fallback after failed/unproven focus, false for shows. |
| `StateTarget` | `property` | `fullscreen/maximized/sticky/above` | no | Independent native property; above is persistent project stacking. |
| | `value` | bool | no | Core-resolved target. |
| | `auth,reason` | authority/reason pair | reason empty if none | This property only. |
| | `cmdSeq` | Counter | yes | Command correlation, not parsed operation text. |
| | `automaticClear` | literal `true` | omit otherwise | Allowed only on `maximized` with `value:false`. Selects the directional/born clear actuation guards; no operation ID. |
| `TransferTarget` | `op,kind,win,out,ws,follow,held` | Id, TransferKind, WindowId, OutputId, WorkspaceId, bool, bool | win empty except window kind | Existing window/view/migration target shape. |
| | `cmdSeq` | Counter | yes | Original command sequence, null for commandless work. |
| | `parent` | Id | yes | Explicit parent operation id for a paired migration source-refill view; null everywhere else. Both `op` and `parent` are bounded opaque ids with no spelling guarantee; never parse or infer the relation from `op`. |

- V4 `WorkspaceDesire` row uniqueness is keyed by `(out,id)` in both
  reply codecs. Shared mode may repeat one logical id on different outputs;
  repeating the same pair refuses with `duplicate-workspace`. This preserves
  the shared ring without relabel/dedup; reference/native external row-identity
  counterparts remain TBD. [Decision](../decisions/workspaces.md#workspace-scope-vocabulary).

- On the slot route, native member and view steps require explicit plan/step
  correlation and exact native preimages and targets. Existing transfer
  correlations still identify the logical command, when there is one; a
  membership step relocating an existing occupant for backing alone is
  commandless, not a fabricated logical send or `cmdSeq`. A selector step can
  change native target even when its logical view is unchanged.

- StateTarget is native actuation only: maximized requires maximize=true, and
  sticky requires nativeSticky=true. Core-composed maximize uses the existing
  controlled frame lane, never a maximized target, attempt or projected native
  fact. `CommandResult::Maximize` remains the existing shape and describes the
  core-resolved toggle, not native completion or independent write permission.

- Directional and born maximize clears publish `state` targets with
  `property:"maximized"`, `value:false`, and `automaticClear:true`. The
  target's `cmdSeq` is the originating requester sequence for directional
  clear and null for commandless born clear. KDE uses the synchronous
  automatic-clear setter with fresh safety, homing and frame checks and no
  preimage write; ordinary explicit maximize targets omit the marker and
  keep the durable preimage executor. Native feedback does not echo the
  marker. Renderer projections omit the automatic state target entirely.
  Core retains the continuation and requester provenance; the
  `operationSettled` kind and operation identity are unchanged.

- Scope authority (`ScopeStatus`) and row authority (`WindowDesire`) are
  independent: scope authority gates scope-level actuation (including
  migration view/refill follow), while per-row `frame_auth,frame_hold` and
  `member_auth,member_hold` govern geometry and membership writes separately.
  A carried float or minimized member under an active retained
  transfer with `kind: "migration"` targeting its published `(out,ws)` carries
  `member_auth: controlled` (unless narrowly held) while preserving
  `frame_auth: untouched` (or `hold/minimized`). Actuators must require
  row-level `member_auth: controlled` with empty `member_hold` before either
  output or membership writes; already-homed live readback still proves arrival
  without a setter, and an unhomed unauthorized row must neither disappear
  from membership proof nor release follow/refill view switches or premature
  operation dispatch.

### Retained settlement outcomes

- User 2026-11-14: the required `gestureSettled`, `operationSettled` and
  `switchSettled` fields project complete current outcome state without draining.
  Bootstrap is null/empty. Gesture is one nullable `{id,committed}` slot per
  `(adapter,gen)`; only a newly incorporated terminal replaces it. Consumed/stale
  replay cannot regress identity or change its original verdict; preserve genuine
  first unbound terminals and existing active-gesture cancellation predicates.
- Operation rows are `{win,kind,op,outcome,cause}`, unique by `(win,kind)`,
  with closed `kind:"clear"|"float"` agreeing with the existing outcome vocabulary.
  Clear and resumed float can share an `op` and retain both phase outcomes.
  Switch rows are `{out,op,outcome,failed,focus,fallback}`, unique by the existing
  terminal stager's owning output, not a claim that only one output is affected.
  No id parsing, new operation identity or synthetic superseded-success outcome.
- Q1 A: starting pending successor work leaves the preceding completion readable;
  only the successor's terminal replaces it. Match `id/op`, never infer completion
  of pending work from an older outcome. Different keys cannot erase one another.
  Repeated/consumed commands and commandless publications retain outcomes in stable
  key order; latest-wins is not a guarantee of every historical terminal.
- Q2 A: retain through holds, minimize and incomplete omission; remove operation
  outcomes with proven window lifetime end and switch outcomes with output departure.
  Close/departure terminals may never be visible: that part of the previous
  terminal-delivery contract is explicitly retired. Core emits trace-level
  diagnostics carrying final dropped outcomes without eager disabled-trace
  formatting. No departed summaries. Absence proves no verdict and grants no
  authority over replacement lifetimes. Gesture survives subject close within its
  generation. Fresh handshake/new generation clears all outcomes.
- The existing 1,024 operation-row and 1,024 switch-row section caps and independent
  4 MiB reply bound cover whole retained projections, failed lists and pending
  reservations. Both operation phases share the cap. Reserve before admission,
  refuse overflow transactionally, never truncate or evict unrelated outcomes.
  Reservations budget accepted obligations, not another delivery owner.
- Outcomes describe completed core accounting only. They never authorize setters,
  unspend dispatches, release arrival barriers, replace raw attempt receipts or
  strengthen existing native-completion predicates.
- KDE broker accepted envelopes require nullable
  `gestureSettledAssoc:{instance,serial,coreId}`, distinct from current-ingress
  `gestureAssoc`; rejected/connection-error envelopes require it null. Core payloads
  contain no native identity. Broker pins the verified active gesture association
  and latest terminal association independently of normalization-cache eviction,
  reconciles only accepted transitions and preserves them on transport reconnect.
  Fresh handshake resets both. Missing originating identity stays null, never
  recovered from id prefixes or unrelated ingress.
- Every KDE settlement consumer matches retained terminal/association against the
  currently pending native FIFO head, not the issuing flight. Keep exact current
  adapter/generation/request/source/nativeInvalid binding. The acknowledgement-only
  stale-snapshot path may retire that exact head but applies no desires or paint.
  Redelivery and late callbacks are exact-identity/idempotent; idle capture cannot
  clear an unrelated unresolved terminal-capture retry. Existing native FIFO
  acknowledgement remains, with no publication-delivery acknowledgement or sequence.
- This is a coordinated in-place pre-release v4 amendment across core, protocol,
  broker and KDE, without alias, fallback or version bump. Other ports must adopt
  retained identity matching before claiming compatibility; offline KDE proofs
  establish no native acceptance.

### Retained dragPreview

- The wire `dragPreview` field (`DragPreviewV4 | null`, Rust
  `WorldReplyV4.drag_preview: Option<DragPreviewV4>`) is a required nullable
  slot per `(adapter, gen)`. On the wire it encodes the flattened keys
  `{id, rev, domainOut, domainWs, sourceOut, sourceWs, rect, hover, win}`
  (`id` and `rev` as bounded quoted decimals), corresponding to the conceptual
  `DragPreviewV4` members `{id, revision, domain, source, rect, hover, target}`.
  `id` (`u64` from `retained.id`) binds the preview to the active gesture so a
  stale preview cannot repaint a replacement gesture and redelivery is inert.
- Omitted/commandless ingress while a gesture is active preserves the latest
  `dragPreview` across omitted ingress and skipped replies, unless
  `prune_drag_preview` invalidates it against current-session safety, scope,
  domain, or neighbor fences (`gesture_continue_fences`, `gesture_game_paused`,
  `is_owned_composed_float`, non-`Move`, unreadable/held `domain`, or
  departed/closed/held/hidden/floating/sticky/overlay `target` neighbor).
- Genuine current-session active `Held` no-preview outcomes (bind-only
  `Started`, safety/pause/composed-float/non-`Move` gates, missing cursor, zero
  movement, `gesture_pair_preview`/`gesture_local_event` `None`, and engine
  refusals) clear `dragPreview` to `null`. Mismatched ignored newcomers cannot
  relabel an old `id` or authorize a newcomer preview. Explicit `Released`,
  `Cancelled`, proven owner departure (`staged.closes`), or fresh
  handshake/new generation also clears `dragPreview` to `null`.
- Broker `CarriedPreview` keeps the output-level suspension gate
  (`domain.output == presentation.suspended_output`) and matches `dragPreview.id`
  against the existing `kde_gesture::active_assoc` pin via the read-only
  exact-id `active_assoc_for` accessor (reconciled before projection). Missing
  or mismatched association stays unproven (`producer: null`), never guessed.

<a id="retained-positive-close-focus"></a>

### One retained focus request

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `FocusRequest` | `id,target,rev` | Id, WindowId, Counter | no | Opaque request identity, frozen target lifetime and immutable issuing revision. |
| | `cause` | `explicit-command/automatic-admission/focus-follows-cursor/positive-close` | no | Closed focus-request vocabulary. |
| | `out,ws` | OutputId, WorkspaceId | no | Frozen activation scope; close retains its frozen source domain. |
| | `cmdSeq` | Counter | yes | Originating explicit command only; null for every other cause. |
| | `afterShown` | AfterShown | yes | Frozen issuing selection dependencies, defined below. |

| Record | Required keys | Types / nullability |
| --- | --- | --- |
| `AfterShown` | `op,obs,members,views,selected,departing` | Parent operation Id; issuing observation Counter; four non-null arrays. |
| `members[]` | `win,out,ws,frame` | WindowId lifetime, OutputId, WorkspaceId, required nullable Rect. |
| `views[]`, `selected[]`, `departing[]` | `out,ws` | OutputId, WorkspaceId; non-null. |

- All keys are required and objects are closed; reject duplicate keys, duplicate
  member lifetimes, duplicate scope pairs and duplicate native-view outputs.
  View targets must occur in `selected`; the view target output is its own `out`.
  Arrays may be empty. Existing Id, quoted-decimal Counter and Rect grammars apply.
  Only `afterShown` itself and member `frame` may be null; null frame waives frame
  equality, never readable live geometry. A non-null dependency requires
  `explicit-command`, and `cmdSeq` is non-null exactly for that cause.
- `members` has at most 10,000 rows (the existing window-inventory cap);
  `views`, `selected` and `departing` each have at most 16 rows (the output cap).
  Exact and conservative upper-bound sizing include every frozen dependency
  within the unchanged independent 4 MiB reply limit. Overflow refuses before
  mutation; never truncate. Parent transfer presence and later window-row presence
  are not validity requirements. Operation-id prefixes grant no authority.

- Zero or one global request for every cause. This lane is approved
  2026-11-11 ([design and implementation status](../changes/retained-focus-requests.md)).
  Core and protocol implement this shape; coordinated adapter adoption and
  verification remain incomplete. `closeFocus` and any `effects` key
  (empty or non-empty) are rejected as unknown, not alternate accepted shapes.
  Explicit-command
  requests originate from direct/directional focus, remembered selection,
  send-follow and operation focus producers under existing execution-time
  subject gates. Automatic-admission requests originate from the
  complete-inventory newcomer selection, keeping the last non-withheld newcomer
  per batch and spending withheld automatic-fixed newcomers, with no repair,
  eviction or alternate-anchor focus. Focus-follows-cursor requests originate
  from a genuine observed cursor hit/movement or hover transition while the
  setting is on. Positive-close requests freeze the pre-close source-domain
  tiled MRU survivor before ingesting native fallback, excluding the whole
  close cohort; ordinary float close uses the same tiled survivor policy, with
  no float/sticky/fullscreen candidate expansion. Bare absence, incomplete
  inventory and off-current omission never originate a request. Survivor reflow
  is independent of activation or observed focus.
- Same-report candidates resolve explicit-command, then automatic-admission,
  then positive-close, then focus-follows-cursor: a positive close selecting
  MRU survivor C plus an ordinary newcomer N in one report retains N only.
- For a new positive-close request, native fallback already at the target
  satisfies immediately with no setter. A different eligible same-domain tiled survivor in
  the close report is attributed to close fallback and permits one correction;
  positively known no-client also permits it. Unknown/foreign foreground does
  not. That same-report close correction determines the origin of a
  request, not attribution of a setter. Once issued, close requests use
  the shared later supersession and pre-setter rules.
- Only evidence after the issuing report can supersede. Before running
  segment commands, adjudicate the cut's ordered interval against the older
  request and core-retained state. Resolve each implicit subject after prior
  commands from retained execution state, using eligible native foreground
  at the cut when there is no applicable retained choice. A later cut cannot
  retarget an earlier command. Proved-own transitions do not supersede or
  resurrect retired requests. An eligible unproved activation supersedes;
  an ineligible activation on a departing workspace cannot. Unproved
  no-client or unbound activation conservatively supersedes without
  inventing a target. A positive unproved native selector event
  supersedes if its destination hides the target, even if foreground
  is unchanged. Same-desired-view confirmation and unrelated-output
  drift do not supersede. In snapshot-only mode, an observed foreground
  change is unknown-cause intent: judge a bound client against retained
  eligibility and use positive native observations for relevant selector
  changes. A gap conservatively retires the older unspent request
  without inventing a replacement.
- A retained selection's own view write may activate a destination
  fallback before `afterShown` is ready. This is not new intent when a
  scoped native call and matching readback prove the activation on that
  operation's output and destination. A simultaneous view readback and
  changed foreground snapshot alone are not proof. Keep the original
  request and its dependencies; newer independent foreground evidence
  still takes precedence.
- A newer accepted focus decision can replace an older one; executing
  or refusing a non-focus command does not itself supersede. Superseded
  requests cannot resurrect or grant a compensating setter. Target
  equality satisfies without a setter but cannot erase earlier eligible
  intent. Preserve terminal refusal, target loss and cause-specific
  safety and arrival gates: close and ordinary automatic focus get no
  alternate target, unhide, unminimize, Space switch or retry. Explicit
  focus and send-follow keep their existing exemptions and view
  authority, subject to native identity, session and game safety.
- Every publication retains the identical id/target/issuing revision/cause
  until satisfied, superseded, ineligible or refused. Core ingests reports even
  when adapters skip publications. Adapters need only apply the latest
  publication, never intermediate replies: an issuing reply may be skipped and
  its request acted on from a later retained snapshot. No fabricated command,
  awaitShown, transfer or raise/stack authority is attached; only an explicit
  command carries its originating cmdSeq, never the currently applying ticket.
- After nonactivating geometry, revalidate the latest eligible bound
  reply, target lifetime, frozen home and shown native view, visibility,
  foreground and existing safety fences. Compare live evidence to the
  incorporated capture that produced the reply. A foreground difference
  may pass only with causal proof of our own effect and no outstanding
  unproved evidence. Equality means zero setters, not adjudication of
  newer intent. After the last potentially reentrant read, recheck the
  reply, native invalidation and evidence cut; no reentrant work may
  intervene before spending and calling the setter.
- Fresh unproved evidence needing core adjudication defers unspent in
  both ordinary apply and stopped-mid-apply. Obtain a later eligible
  publication by observation, never polling. Spend exactly once per
  `(adapter,gen,id)` on a native attempt or terminal safety no-call,
  even if feedback is lost; never retry a spend. Null or newer requests
  cancel unstarted work; stale envelopes and late callbacks cannot
  reactivate it. Keep bounded current and retired-revision accounting.
  Transport loss retaining context retains accounting; losing accounting
  requires a fresh handshake, which cannot replay old intent.
- Core keeps an execution-time logical focus cursor separate from observed
  foreground and per-workspace history. Same-capture admission enters the
  ordering before commands. Within a batch each accepted focus choice advances
  the cursor immediately in FIFO order, even though only the final winner
  publishes; the final winner follows same-report cause precedence and owns
  the cursor too. A losing candidate cannot overwrite the published winner:
  close choosing C while hover enters D retains C as the implicit subject.
  A new
  implicit command resolves its subject from the cursor before the older
  request is retired for a newer focus decision. A still-live eligible
  request owns the cursor through own-caused or ineligible changes,
  keeping its id, target, issuing revision and dependencies. The cursor
  may differ from eligible native foreground only while core has its
  own unsettled focus choice, including an earlier accepted choice in
  the current batch. After retirement use eligible observed foreground;
  never invent an execution subject from an ineligible observation.
- User decision 2026-11-12, amended 2026-11-13: when the execution cursor is empty and no
  pending retained request owns the subject, an implicit command uses the
  host-reported foreground if core knows it as a live, bound window of the
  current generation. Resolve this in the same execution-subject owner without
  writing canonical focus memory or changing session eligibility, and without
  minting a focus request. Unknown or unbound foreground remains `focus-unknown`;
  stale-generation ingress rejects before command execution and cannot supply a
  subject. The sole pending-request exception: when its target is unavailable,
  a newer accepted report still showing another live, bound, known window focused
  permits that observed subject without retiring or changing the request. The
  request may complete later; an already executed command stays on its resolved
  window. Same-batch focus decisions cannot use this exception without a newer
  report. Core compares accepted report order with the request's issuing revision,
  not elapsed time or an assumed host apply order. Otherwise the rule never
  overrides or outlives a logical focus decision or a pending retained request.
  A nonempty logical cursor from an unsettled focus decision takes
  precedence over a different observed foreground. The separate macOS
  suppressed-admission outcome is not amended; automatic guard withholding
  stays spent, not retried.
- The winning request is frozen once per accepted publication with a
  generation-scoped revision-derived opaque id. Re-emission never changes
  id/target/issuing revision/cause/command correlation/frozen dependencies.
  Each fresh qualifying hover entry after leaving the live target and returning
  mints a new id; unchanged reports or pointer movement over the already-pending
  target keep the id and never retry. This replaces lifetime `ffc-<token>` spent
  identity.
- `afterShown:null` replaces ordinary `awaitShown:false`. Explicit selections
  and eligible follow/migration requests carry the existing parent operation
  and frozen application dependencies instead of `awaitShown:true`: issuing
  observation, required members with lifetime/home/frame, native-view targets
  and affected selected/departing scopes. This is a projection of existing
  selection state, not a new work id or selection policy; a later snapshot
  carries these dependencies even if the parent transfer has settled. Freeze the
  projection above at the existing post-layout selection snapshot, including its
  issuing departing scopes; keep terminal/evidence bookkeeping separate. Never
  recover dependencies from an
  old ticket or reconstruct them from later window rows.
- Reuse `focusFeedback` by exact opaque `(id,target,rev)` equality, with rev the
  immutable issuing revision, not the later applied publication revision. Latch
  the first correlated raw row/stamp only, in the single pending request;
  no parallel historical-grant settlement ledger runs underneath it.
  Proven refusal or informative safety
  no-call retires; accepted dispatch without target equality is unproven and
  retains an inert request, never authorizing another call. Current observed
  equality satisfies independently of setter evidence. Feedback for a retired
  request grants nothing. No adapter verdict ingress is added. Freeze the issuing
  report's truthful foreground baseline for later comparison; the issuing
  baseline itself is never a newer focus event.
- Sole close keeps the source output's shown empty workspace and no focused
  member, with `focusRequest:null`; no other-domain activation or native clear.
  Observed native desktop/app foreground remains truthful. Same-report
  attribution can override a click before the first close report once; entirely
  between-capture A->B->A changes remain invisible. No causal input guard or new
  interception is implied.
- All required keys participate in strict validation, canonical codecs and
  exact/upper-bound byte sizing; frozen dependencies must fit the existing
  whole-message bounds, never silently truncate. The foreground bit
  participates in report digest/replay equality. Bootstrap/not-ready replies
  carry `focusRequest:null`. Bounds remain 4 MiB independently per
  report/reply and at most one focus row.

### Persistent stacking

- `above` targets use independent StateTarget authority and StateAttempt
  pre/post evidence, gated by the above capability. No one-shot raise/stack
  action exists; floating/sticky=true is not write authority.
- Core acquires persistent above for every float: qualified automatic floats,
  explicit float/sticky intent and capacity-fallback floats use one policy.
  Workspace-floating-mode floats use the same rule even with no tiles present;
  there is no origin-specific branch. Require supported above and durable
  preimages, readable live facts, no suppression/minimize/native overlay safety
  veto, no preexisting above/below and no external-clear latch. Pre-existing
  below gets zero stacking writes; pre-existing above stays untouched.
  Suspend project-owned above for focused fullscreen on its scope;
  fullscreen-game safety wins. Release when the window is no longer floating,
  and discard ownership when the native lifetime ends.
  External below or external above-clear relinquishes ownership and prevents
  reacquisition for every float origin until lifetime end or the existing reset
  when native preowned-above is observed. Sticky transitions retain this same
  ownership/latch; sticky-off releases only if core returns the window to tiled.
  No second sticky stacking owner or adapter toggle is introduced.
- Core emits above=true for acquisition and above=false for release/suspension
  of its owned change. Adapter persist-before-write preimages cover every coupled
  native property (KWin keepAbove can clear keepBelow); release restores only
  the owned preimage, with raw resulting facts returned. Failed preimage storage
  disables that window/property; no adapter reacquisition/release policy runs.
  Native overlay refusal holds the affected target, not a local replan.
  [W1 union and capacity follow-up](../decisions/contract.md#persistent-above-float-policy)
  were approved 2026-11-02. Support is a capability, not a host policy opt-in;
  macOS above=false authorizes no persistent stacking writes.

### Opaque intent storage

- Core defines/validates and decides save/restore. Adapters store/return `data`
  unchanged in a lifetime-bound native per-window slot or adapter-owned atomic
  external slot; they never interpret it as policy. Both storage routes have
  identical obligations: <=1024 opaque UTF-8 bytes, atomic publication before
  dependent writes, and a restart-verifiable exact native-window lifetime binding.
  Independent native property preimages must also be persisted atomically before
  dependent property writes; a failed preimage disables only that property.
  No new storage capability is introduced.
  The core record is a compact JSON string <=1024 UTF-8 bytes with internal
  fields tabulated below.
  No geometry, layout tree, workspace ring or MRU/history is persisted. Invalid
  version/shape or unverified storage lifetime ownership returns no intent and no
  write authority. The outer win binds the verified live storage subject, not
  an old adapter id embedded in the record. A returned
  record is not proof of native state. Cross-login persistence is not promised.
  Discard only genuinely unverifiable bindings, never recover by title/PID/CG id
  guesses. macOS must retain intentional-float intent across orderly tiler restart
  within a session. Prove within-session re-identification, exploring Apple's
  session-unique CGWindowID plus process/launch-date fencing and a KDE-style
  atomic private store; this is not proof by id alone or reuse of old wire tokens.
  macOS re-identification and host-storage-survival remain OPEN port work, not
  permission to drop restart intent. An external file/property key alone is not
  ownership proof. [M3 amendment](../decisions/persistence.md#orderly-restart-adoption).
  OPEN: phase 0b verifies storage survival and exact lifetime ownership, not
  restart matching by title/process heuristic.
  [Intent](../decisions/contract.md#intent-ownership-and-persistence),
  [evidence](../changes/v4-migration.md#b-reconciliations-and-open-points).

- Import closes at the window's first successful admission. Core retains pending
  records through held/unreadable observations; after admission it ignores late
  records and never overwrites newer runtime intent. An adapter must return a
  stored record in or before the report where its window is first observed
  admissible. A later return is ignored (diagnostic only, no write authority).
  This is an ordering obligation, not a new storage capability: per-window native
  slots and pre-loaded atomic stores both satisfy it. Native delivery of that
  ordering remains a port acceptance check, not proof from offline import tests.

- `intentDesired` is required and unique by `win`, replacing the one-time storage
  lane in place in v4. No alias, version bump or compatibility path exists.
  Core retains one typed target per `(adapter,gen,win)` lifetime, independent of
  visible `WindowDesire` rows. Publications project the complete retained map,
  never drain it, including observation-only and consumed-command replies.
  Accepted mutations replace the target in command order; rejected mutations
  leave it unchanged. Proven native sticky refusal restores the prior record or
  explicit null clear under the existing rollback rule; storage failure is not
  native refusal. Null remains retained after a successful clear; a missing row
  carries no storage authority and never means deletion.
- Accepted valid imports initialize desired storage before admission. Late imports
  never overwrite runtime intent. Protocol encoding is canonical, so an accepted
  noncanonical record may be rewritten once per window; no byte-preserving import
  special case exists. Retain targets through holds, minimize and incomplete
  omission; forget on proven lifetime end or complete departure. Old-generation
  replies and old-window identifiers never authorize writes to replacements.
- Durability is adapter-local, with no new report field. Reconcile latest accepted
  desire against verified storage or the last successfully persisted snapshot,
  not pending data. Equal targets cause zero disk writes. Unknown/different values
  require atomic save/clear before that reply's dependent native application;
  preserve the whole-reply hold on failure. Retry on every later accepted reply
  while unequal, with no timer or recovery signal. Serialize storage calls and
  correlate their actual success; completion records the captured persisted
  snapshot, compares it with the latest desire and never resumes an obsolete
  issuing-reply continuation or proves a newer target durable.
- Reconnect preserves verified storage knowledge; lost knowledge requires storage
  and lifetime verification. Restart preloads lifetime-bound imports before first
  admission. The entire retained map, including clears, must fit the 10,000-row,
  1024-byte-per-value and 4 MiB whole-reply bounds: refuse overbound candidates
  transactionally, never truncate. No sequence, acknowledgement or attempt lane
  is added for repeatable storage replacement.

- Composed maximize is runtime-only: orderly restart drops it and restores base,
  with no maximize field or base-geometry extension in this record. This adds no
  M3 persistence dependency; intentional-float/sticky restart obligations remain.

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `IntentRecord` | `win` | WindowId | no | Storage's verified live subject. |
| | `data` | opaque bounded string | no | Exact core record, no adapter edits. |
| `IntentDesire` | `win` | WindowId | no | Exact retained lifetime subject. |
| | `data` | opaque bounded string | yes | Save value; null explicitly clears the lifetime-bound slot. |
| Core intent data (inside string) | `v` | integer 1 | no | Storage format version, independent of wire v. |
| | `float` | bool | yes | Explicit float/tile override; null means no override. |
| | `stickyPrior` | bool | yes | Non-null means retained sticky intent and prior float/tile intent for sticky-off; null means not sticky, independent of native fact availability. |

- Sticky is float-only: sticky-on saves `float:true` and records the previous
  float/tile mode in `stickyPrior`. Sticky-off saves `float:<prior mode>` and
  `stickyPrior:null`. This is the core's current-mode encoding, not a new
  product behavior. All six sticky-bearing input combinations remain accepted:
  non-null `stickyPrior` restores sticky-float intent and determines sticky-off
  membership, even when the returned `float` lane is null or false.

## Raw attempts and settlement

- A native setter outcome is `accepted`, `refused`, or `threw`.
  Acceptance alone never proves completion;
  explicit refusal needs readable unchanged preimage. Anything possibly executed
  remains ambiguous/spent, never blind retry. Carry attempts into a freshly
  captured report whose observations do not predate immediate readbacks; later
  asynchronous observation is not itself a completion signal. Lost acknowledgements
  redeliver the original rows without reissuing the native call.
  [Settlement baseline](world-contract-v3.md#5-raw-attempts-and-settlement),
  [rename/consolidation](../changes/core-interface-review.md#original-backlog-lines-159-186).

- Active v4 uses exact tagged `disposition` and frame `purpose` objects
  with no legacy aliases. Frame, state and view accept only `called`;
  membership also accepts `already-at-target`; visibility and focus also
  accept `not-called` with lane-specific reasons. Nested members not
  specified for the selected arm are invalid. An invalid row never supplies
  typed evidence; existing identity-bound row holds and hard refusals still
  apply. Attempt disposition, old rows and matching property readback are not
  foreground attribution. Attribution does not prove property settlement or
  focus arrival.

| Former active row | Active v4 representation | Meaning retained |
| --- | --- | --- |
| `setter:"accepted"`, `"refused"` or `"threw"` (frame, member, view, state, visibility, focus) | `disposition:{kind:"called",setter}` with the same setter token | Native call outcome; any former visibility or focus `reason` stays at row level on this arm, with exactly its former vocabulary and meaning. |
| Member `setter:"already-at-target"` | `disposition:{kind:"already-at-target"}` | Exact operation consumed without a native call. Only membership has this arm. |
| Visibility `setter:"not-called"` with `reason` | `disposition:{kind:"not-called",reason}` with the same reason, no row-level `reason` | Visibility safety evidence, not a native call. All former reason values in the table below remain valid here. |
| Focus `setter:"not-called"` with former visibility-vocabulary `reason` | `disposition:{kind:"not-called",reason}` with the focus token in the table below, no row-level `reason` | Focus safety evidence, not a native call. Empty old reason was not itself equality; independent foreground evidence still proves equality. |
| Frame `op:"",pre:null` | `purpose:{kind:"layout"}` | Regular layout frame; no float operation or prewrite fact. |
| Frame nonempty opaque `op` with `pre` a rectangle or null | `purpose:{kind:"float-placement",op,pre}` | Exact float operation and original nullable prewrite fact. |

| Former row-level reason (visibility or focus) | Visibility reason (called or not called) and focus called reason | Focus not-called reason |
| --- | --- | --- |
| `""` | `""` | `focus-no-reason` |
| `identity-stale` | `identity-stale` | `focus-identity-stale` |
| `scope-held` | `scope-held` | `focus-scope-held` |
| `unreadable` | `unreadable` | `focus-unreadable` |
| `unsupported` | `unsupported` | `focus-unsupported` |
| `preimage-unavailable` | `preimage-unavailable` | `focus-preimage-unavailable` |
| `foreground-changed` | `foreground-changed` | `focus-foreground-changed` |
| `fullscreen` | `fullscreen` | `focus-fullscreen` |
| `game` | `game` | `focus-game` |
| `show-blocked` | `show-blocked` | `focus-show-blocked` |
| `focus-unproven` | `focus-unproven` | `focus-foreground-unproven` |
| `superseded` | `superseded` | `focus-superseded` |
| `native-refused` | `native-refused` | `focus-native-refused` |

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `FrameAttempt` | `kind,win,target,disposition` | `"frame"`, WindowId, Rect, called disposition | no | Exact window and attempted frame target. |
| | `readback` | Rect | yes | Immediate frame readback. |
| | `dpiBefore,dpiAfter` | integer | yes | Native DPI observations around the call. |
| | `attemptId` | Counter | no | Producer-assigned frame attempt identity, independent of clock. |
| | `attemptMs` | Counter | yes | Original monotonic call stamp, or null when unavailable. |
| | `purpose` | layout or float-placement purpose | no | Layout has only `kind`; float-placement also requires opaque nonempty `op` and nullable Rect `pre`. |
| `MemberAttempt` | `kind,op,win,out,ws,disposition` | `"member"`, Id, WindowId, OutputId, WorkspaceId, called or already-at-target disposition | no | Operation, subject, target and call or no-call disposition. |
| | `pre,post` | WorkspaceRef | yes | Single logical-or-native reference set, live before/after. |
| | `attemptMs` | Counter | no | Original monotonic call or no-call consumption stamp. |
| `ViewAttempt` | `kind,op,out,ws,disposition` | `"view"`, Id, OutputId, WorkspaceId, called disposition | no | Output-scoped native selector evidence for existing parent/phase. |
| | `pre,post,base` | WorkspaceRef with kind native | yes | Native before/after/observation baseline only. |
| | `attemptMs` | Counter | no | Original stamp. |
| `StateAttempt` | `kind,win,property,target,rev` | `"state"`, WindowId, state property, bool, Counter | no | Exact core native-state target. |
| | `disposition` | called disposition | no | Actual native call outcome. |
| | `pre,post` | bool | yes | Observed property immediately around call. |
| | `attemptMs` | Counter | no | Original stamp. |

- Frame rows require both `attemptId` and `attemptMs`; only the stamp may
  be null. Allocate frame attempt IDs in strictly increasing order within
  the bound producer generation. Assign one ID to each new native frame
  attempt and retain it unchanged, with all original evidence, on retries
  and resends. A new attempt gets a new ID even if its stamp and readbacks
  match. Frame replay compares IDs at the consumed watermark for the exact
  window lifetime; equal and older IDs are inert, even with a null stamp.
  A new producer generation starts a new identity scope. Timing is not an
  identity and other attempt lanes do not carry `attemptId`.

- Duplicate keys by kind: frame by `win`, membership by `(op,win)`, view by
  `op`, native state by `(win,property)`, visibility by
  `win`. Identifiable held rows count toward the same uniqueness checks.
  Ingress uniqueness applies even if the window is absent or close-only.
  A visibility duplicate refuses as `snapshot-invalid` with
  `duplicate-visibility-attempt`. After structural checks, ignore visibility
  rows without a live or held window, with no authority or diagnostic.
  Identifiable malformed visibility evidence for a live or held window
  holds only that scope. Native state malformed or duplicate rows refuse
  rather than holding.
  A frame with valid call evidence but malformed purpose is held; a frame
  without valid called disposition refuses. Malformed focus feedback refuses.
  Neither the retired flat attempt fields nor a held frame can settle a write.
- Decode `propertyAttempts` in this order: frame, membership, view, native
  state, visibility. Decode the separate creation section next and focus
  feedback after it. Use producer order within each kind; the first hard
  error wins. The input ordinal is for wire reconstruction, not settlement.
  The retired `kind:"clear"` attempt refuses.
- On the bound broker capture path, compare the original `attemptMs` on
  frame, membership, view, native state (including automatic maximize
  clears), and creation evidence with the authoritative dispatch clock.
  Stamps at or below the clock are eligible for settlement, subject to
  each lane's other rules. A null frame stamp is skipped; frame `attemptId`
  is not a clock, and visibility has no stamp. Ignore identifiable typed
  evidence stamped above the clock: it cannot settle a write, but valid
  siblings still apply. An already-held codec row stays held. Hard shape
  and identity errors refuse the capture before clock filtering. The
  broker does not adjust stamps or the clock, or change exact capture
  replay identity. The separate focus-feedback lane is unchanged.

- StateAttempt `rev` is the acted-on WorldReply publication's `rev`, not the later
  report's `obs` or `reqSeq`, nor the write's `attemptMs`. The adapter acts only
  on that reply's Controlled property row. Settlement correlates raw evidence of
  an already-performed write: match exact window, property, target and capability,
  with `binding.target.revision <= attempt.revision <= latest published revision`.
  Capture the first matching evidence as spent even under Hold; this creates no
  authority or publication, and observation-only replies never reauthorize a
  spent write. Preserve the original evidence and stamp on redelivery. The
  immutable `window-state` scenario pairs
  `native-sticky-target` at revision 2 with `native-sticky-observed` at observation
  3 carrying attempt revision 2. See the
  [correlation discriminator](../spec/reference-outcomes.md#native-state-attempt-correlation).
- Member evidence uses the namespace allowed by hard capability; core alone
  translates native bindings. Remove parallel `nativePre/nativePost/nativeBase`.
  ViewAttempt permits no logical arm; native-free visibility/focus completes
  selection. This is the [G1 technical reconciliation](../changes/v4-migration.md#b-reconciliations-and-open-points),
  approved 2026-11-02, not a new product decision.
- Membership disposition carries `called` for native calls and
  `already-at-target` for consumption of a transfer operation without a call
  (User 2026-11-17). The latter reports the exact operation, window and desired
  target with live `pre`/`post` membership readbacks; it is not setter success.
  Spend that operation before the no-call, preserve the original evidence on
  redelivery, and never renew a native call from unchanged reemission.
  Core derives settlement using this evidence and current facts, preserving
  lifetime, visibility and output fences and at-most-once ambiguous effects.
  A target fact alone does not prove that an operation was consumed.
  This is not a world-applied or delivery acknowledgement and adds no sequence
  lane, retries or adapter policy; retained settlement outcomes are unchanged.
  KDE emits the row on its already-at-target transfer branch; macOS and Windows
  adopt the same row later.
- Frame and maximized-false state rows retain their kind-specific readback
  facts (frame target, readback, DPI and float prewrite; state target and
  pre/post-maximized) in `propertyAttempts`. Native creation remains separate,
  retaining its before/after and correlated-created identity fields.
  Outside slot plans, ordinary creation needs explicit `nativeCreations`
  authorization, spent before call; an unbound spare is not permission.
  Unknown/ambiguous created identity holds without recreation. Without causal
  proof, macOS `created` stays null and binding holds even if same-call or
  later native-list deltas contain one new id. Preserve the original spent
  attempt; no later-delta attribution or retry.
  [Creation rules](world-contract-v3.md#host-owned-adoption-amendment-tentative-2026-10-25).

- Slot-plan feedback identifies the plan operation, epoch and exact step.
  Member/view attempts report raw native before/after facts and call or
  no-call disposition, tied to the exact lifetime or selector; append/remove
  feedback carries correlated native roster readback. Attempt reports do not
  commit desired bindings or infer success for the rest of a partial apply.
  Preserve original evidence on redelivery without reissuing writes.

## Visibility and focus ordering

### Raw visibility evidence

| Record | Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- | --- |
| `VisibilityFact` | `visible,suppressionOwned` | bool | yes | Native shown state and project-owned suppression. |
| `VisibilitySample` | `obs` | Counter | no | Invalidation watermark at the read; subject comes from the enclosing attempt row or bracket. |
| | `lifetimeMatched` | bool | yes | Full native lifetime and any applicable stored suppression/recovery tag match, not handle equality. |
| | `visibility` | VisibilityFact | no | Native state at this read. |
| | `minimized,hostHidden` | bool | yes | Separate native ineligibility facts. |
| `VisibilityAttempt` | `kind,win` | `"visibility"`, WindowId | no | Closed `propertyAttempts` row for visibility writes; one row per window lifetime. |
| | `target,revision` | bool, Counter | no | Existing desired-state target and revision; no visibility id. |
| | `path` | `staged/fallback/independent` | no | Actual execution route. |
| | `disposition` | called or visibility not-called disposition | no | Actual call or bounded visibility no-call reason. |
| | `reason` | visibility reason | required on called (may be empty); omitted on not-called | Bounded mechanical fact about the call. |
| | `pre,post` | VisibilitySample | yes | Identity-bound reads immediately around action/no-call. |
| `ArrivalBracket` | `win,obs` | WindowId, Counter | no | Required arrival subject and common source watermark. |
| | `before` | VisibilitySample | no | First lifetime/visibility read. |
| | `kind` | `native/output` | no | Native-home arm only with hostOwned; observed-output arm only without hostOwned. |
| | `home` | WorkspaceRef | null; omit in output arm | Native membership read BETWEEN before/after; non-null must have kind=native. |
| | `out` | OutputId | null; omit in native arm | Observed native output affinity BETWEEN before/after, never copied from desired home. |
| | `frame` | Rect | yes | Native frame read BETWEEN before/after; null cannot prove readiness. |
| | `after` | VisibilitySample | no | Final lifetime/visibility read with no intervening invalidation. |
| `FocusFeedback` | `id,target,rev` | Id, WindowId, Counter | no | Exact retained-request correlation by opaque id, target and issuing revision. |
| | `disposition` | called or focus not-called disposition | no | At-most-once call or bounded focus no-call reason. |
| | `reason` | focus called reason | required on called (may be empty); omitted on not-called | Bounded mechanical fact about the call. |
| | `pre,post` | WindowId | yes | Live foreground before/after; null means unknown. |
| | `foregroundProven` | bool | no | Actual foreground equality proof, never setter success. |
| | `arrival` | ArrivalBracket[] | no | Readiness brackets; blocked subjects use their visibility attempt no-call evidence, not a different array arm. |
| | `attemptMs` | Counter | no | Original dispatch/no-call stamp. |
| `SwitchEvent` | `out,op` | OutputId, Id | no | Retained owner key and existing parent transfer/view operation. |
| | `outcome` | `complete/completed-with-failures` | no | Core terminal accounting, not native per-entry verdict ingress. |
| | `failed` | WindowId[] | no | Frozen switch target failures. |
| | `focus` | `proven/refused/unproven` | no | Core-derived foreground outcome. |
| | `fallback` | bool | no | Whether separately authorized fallback hides ran. |

- A selection with no required focus target uses `focus:"unproven"` and
  `outcome:"complete"` once all visibility obligations are satisfied, with
  `failed:[]` when nothing failed. This includes empty-to-empty selections with
  zero visibility members and native-owned focusless selections. An explicit
  selection whose resolved focus target fails route eligibility freezes a
  binding with no required focus target; visibility completes through this same
   path and focus reports `unproven`. Independent focus authority uses only the
   global retained request, never a parallel direct effect; eligibility gates
   are unchanged. Focus-bearing
  switches still require proven focus for complete. Empty/minimized-only
  workspaces retain no focused client. This is the same completion rule, not a
  new token, wire shape, fake focus, fallback, retry or second completion path.
  [Focusless completion](../decisions/workspaces.md#focusless-switch-completion).
- Commandless disconnect/return policy views bind as focusless selections under
  their existing `op-dpv-*` parent, without an invented `cmdSeq` or focus effect,
  and retire through the same completion. Native focus stays untouched. Internal
  parent/correlation carriers do not add a wire identity or receipt predicate.
  [Disconnect](../decisions/workspaces.md#kde-common-model-output-disconnection),
  [return](../decisions/workspaces.md#kde-common-model-output-reconnection).
- Cross-output follow-send and same-output follow-send take the same frozen
  selection binding as an ordinary explicit selection. Window membership
  settles the transfer independently; the binding owns destination visibility
  and focus through the existing selection completion, with no follow-specific
  predicate or second completion path. The `{op}-follow` child is removed;
  retained follow focus waits until the destination is shown, carrying the
  frozen issuing dependencies. Existing
  refusal/safety fences are unchanged.
  [Follow settlement](../decisions/workspaces.md#workspace-send-follow-settlement).
- Native-free workspace migration takes one frozen selection binding covering
  both outputs. Member arrival settles membership; the binding owns target and
  source-refill visibility and focus through the existing selection completion,
  and the refill child is removed. Empty migration uses the zero-member
  completion rule. Native-owned migration keeps its native proof.
  [Migration settlement](../decisions/workspaces.md#workspace-migration-d6-focus-settlement).
- Superseding the global focus slot cancels its unstarted focus authorization
  only: frozen parent/member visibility and terminal obligations persist under
  their own rules, and a fulfilled slot never fabricates missing arrival
  evidence or a SwitchEvent. A later snapshot carries the frozen original
  dependency set even when the parent transfer has settled.

- `VisibilityReason` is `""` or
  `identity-stale,scope-held,unreadable,unsupported,preimage-unavailable,foreground-changed,fullscreen,game,show-blocked,focus-unproven,superseded,native-refused`.
  Samples are facts, never a copied target. Arrival requires the same live
  subject, matched lifetime/applicable tag and watermark, visible/non-minimized/non-hidden
  before/after, and coherent native-home or observed-output/frame inside that
  bracket. With hostOwned, translate the observed native home and compare it
  to the frozen core target. Without hostOwned, core compares its retained logical
  home and the bracket's observed output to that frozen target; no logical home
  is returned by the adapter. A mismatched retained home/output, tag/lifetime,
  watermark or frame cannot prove readiness. Frame must equal its target when non-null. A
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
- ArrivalBracket is an exclusive union: missing, mixed or wrong-host arms and
  logical home in the native arm refuse the wire message. Readable-shape null
  home/output/frame is valid evidence but cannot prove readiness. Core retains
  the frozen target set and compares facts; no applied-homing receipt, logical
  workspace echo or ScreenView is restored. W2 is a technical resolution approved
  2026-11-02; static fixtures do not establish native arrival or focus success.

### Normative apply rule

1. Latest desired state wins; drop superseded unissued targets.
2. Apply shows first, batched and nonactivating.
3. A retained request with frozen selection dependencies runs once only after
   every controlled desired-shown window on its output in the issuing
   revision's snapshot has a fresh arrival bracket or reported no-call reason.
   Any failed/blocked required show withholds focus. Spend before native dispatch.
4. Across outputs, hides wait for every outstanding retained request with
   frozen dependencies from the same or an earlier revision. After proven
   foreground they run staged.
   After refused/unproven focus, only separately approved fallback hides run
   under fresh identity/desktop/foreground/game/fullscreen/visibility/property
   gates and `visibility.fallback:true`, reported as fallback; otherwise report
   not-called and keep visible.
5. At most one write per lifetime/revision/target. A disagreeing fact never
   rewrites that same target at that revision; only new intent grants a new
   authorization stamped with its existing world revision in `visibility.rev`.
   Ordinary observation revisions do not renew it; `VisibilityAttempt.revision`
   names that authorization revision, not the later report's revision.
6. Issued writes stay spent across supersession/reset; report late visibility
   attempt evidence without letting an old target override the new desired target.

- Core freezes switch target sets internally, derives each member's
  confirmed/refused/ambiguous/blocked state, and emits SwitchEvent only when
  every member has a correlated verdict. Arrival and proven focus never replace
  a missing show attempt; the member stays pending, with no SwitchEvent.
  [Per-member decision](../decisions/workspaces.md#selection-visibility-accounting).
  Later admissions enter the next target revision,
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
- `WorldReplyV4` omits the retired `effects` key (any `effects` key is rejected
  as unknown); retained transfers remain until observed,
  externally superseded or proven refused. `WorldReplyV4` carries no `result`
  field (command outcomes ride only the issuing
  `TransitionResult.dispositions` (`Vec<BatchDisposition>`) and the broker's
  requester-only `TakeCommandResult`
  lane within `MAX_SETTLED_RESULTS = 1024`);
  [settlement outcomes](#retained-settlement-outcomes) are latest-completed state,
  not issuing-only delivery or native-write authority.
  The separate retained `focusRequest` lane follows its
  [latest-only, once-per-id rules](#one-retained-focus-request), not one-shot
  delivery: skipping its issuing publication cannot drop or renew the request.
  [Desired-state baseline](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes).
- Before hide, stripped fullscreen chrome, project keep-above or sticky stacking
  writes, persist original native state atomically in a lifetime-bound native or
  adapter-owned external slot outliving the tiler, under the storage obligations
  above. Restore only verified leftovers at next startup; Windows watcher also
  restores stripped chrome. Unreadable/failed preimage disables only that
  window/property. Never mutate untouched state, nor infer restoration authority
  from a stale lifetime.
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

- In-band connection setup failures on `DiscoverWorld` (handshake and
  settings, including codec and authorization rejections) use exactly
  these five keys. They carry no capture source, watermark, clock or
  gesture associations. Transport failures without an in-band reply are
  unchanged.
  Authorization failures retain their transport reasons (`unauthorized`,
  `namespace-unavailable`, `not-kwin-owner`).
  A `generation-mismatch` settings rejection causes KDE to start a fresh
  handshake and then send settings for the newly granted generation, not
  retry the rejected generation's settings. Capture-path rejections keep
  their broker binding; the raw error envelope is unchanged.

- `ProtocolKind` retains `malformed,oversized,snapshot-invalid,unknown-field,unknown-value,unsupported-version`
  and core envelope/fencing kinds `owner-invalid,correlation-invalid,generation-mismatch,observation-stale,request-superseded,request-reused,reply-oversize,revision-exhausted,creation-exhausted`.
  `owner-invalid` describes unauthorized connection ownership, not a revived
  owner field. Envelope/size/identity failures mutate nothing. Identifiable row
  invalidity uses RowDiagnostic instead of a whole-report snapshot error.
  [Existing errors](../../crates/tiler-protocol/src/world_protocol.rs),
  [core kinds](../../crates/tiler-core/src/world_engine.rs).

- The capture envelope rejects duplicate object keys as `malformed`,
  including keys nested in its report. The protocol checks raw request
  length before decoding and owns the 4 MiB byte policy; core owns structural
  limits. Retain the conservative early reply bound. At staged admission,
  exact canonical inner bytes plus future terminal headroom can cause a
  remembered `reply-oversize`. Independently, actual broker-bound bytes
  above 4 MiB give the unremembered transport error
  `reply exceeds size bound` before installing candidate state, publishing
  coverage or settling queued commands. The next fitting capture requires
  a fresh broker source.
- KDE retains at most 1,024 exact capture request/reply pairs and at most
  32 MiB of their combined payload bytes per generation. Eviction is oldest
  first, not refreshed on replay. Exact remembered bytes replay before any
  drain and changed bytes for a remembered source refuse as `source-reused`.
  Eviction does not lower the sequence floor: a previously dispatched
  sequence that has fallen out of retention refuses as `stale-source`.

## Presentation and renderer input

### PresentationIngress

| Key | Type | Null/absence | Meaning |
| --- | --- | --- | --- |
| `foreground` | WindowId | empty | Observed foreground token, including unmanaged subject. |
| `minimized,maximized,fullscreen,hostHidden,hiddenWorkspace` | bool | yes | Actual foreground facts, not renderer eligibility. |
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

- Remove `borderEnabled,underlayEnabled,borderGap,borderWidth,underlayExtension,paused,shell`;
  settings are unscaled and independent. Rename cloaked to hostHidden.
  Foreground shell/eligibility comes from its neutral WindowFact/type/class,
  even if it is not a managed member; missing classification facts stay unknown.
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
  the target's identity and home with current core state: workspace on `out`,
  exact window lifetime in `ws` on `out`, or exact group in `ws` on `out`.
  A changed identity/home refuses the command with `stale-render-target`, not a
  publication hold. Ordinal, trailing, native binding, label, occupied and group
  members are not compared; no rendered-row history is retained. Group ids draw
  from one generation-scoped monotonic counter, including trial and rolled-back
  placements; a removed group's id is never minted again within that generation.
  A rendered revision above the latest published presentation revision also
  refuses with `stale-render-target`; every published revision, however old,
  proceeds to the identity/home check. Selected commands retain their normal
  gates. Adapter only maps hit position to target; no local click-freshness
  policy. Named border/underlay/pager share this rule.
  [Decorations](../decisions/contract.md#decorations).
- Apply an accepted border descriptor to the live native frame through existing
  native frame events, not stale absolute rectangles. Scale `width` and
  `radius` once by the foreground output scale with the existing nearest,
  halves-upward rule; `outsets` is `[g+w, g+w, g+w, g+w]` from the scaled
  border gap plus width. An unknown or absent witness, or a witness whose
  subject or foreground does not join the observed foreground, hides the
  border with empty reason and null subject, outsets, width, radius and
  witness under the existing carried hidden geometry-veto spelling; no hide
  token is added. A witness must be joined
  to the subject and native capture/invalidation chain; later frame events may
  move that same accepted anchor only while foreground/lifetime/eligibility
  facts remain causally valid. A constant event-name string, elapsed delay or
  guessed frame is not a witness. Unknown/stale witness suppresses paint
  narrowly, never polls or adds a settling marker. `obs` identifies the capture;
  it is not interchangeable with a native renderer's source-instance/counter.
  The adapter must preserve the exact join between those chains. OPEN: phase 0b must verify
  native witness joins/events on each port; macOS smoothness evidence is not a
  shape gate. As for K5, the local causal join is the phase-2 port proof gate;
  native-witness-join-evidence remains OPEN. macOS suppresses paint for unwatched
  subjects beyond its watch cap and for stale/unknown eligibility, until causal
  coverage is established; a reread or timestamp is not a substitute.
  A native renderer may keep an **already visible, causally current** border
  across only these unrelated native events: another window's frame change,
  desktop roster addition/removal, or another window's desktop membership
  change. The native effect must prove at the event that the same foreground
  lifetime remains eligible and visible, its native desktop membership and
  output's current desktop are unchanged, and its output identity and scale
  still match the accepted anchor. It must compare with retained proven
  anchor facts and advance only across the exact next native invalidation.
  Core still owns eligibility and the descriptor; the effect neither
  reclassifies a window nor arms, retargets, or replaces a descriptor here.
  A foreground/lifetime, eligibility, view, home, output or scale change,
  a fullscreen/maximize transition of any window on the anchor's output
  (even without a focus change), or unknown, divergent or missed evidence
  breaks continuity and requires a fresh covering core projection. Retention
  lasts only until the next covering core projection; it remains authoritative
  and hides the border if it says `bvis=0`. An unrelated event cannot restore
  a hidden border. This exception is border-only; new paint acceptance and
  underlay/preview freshness retain their existing rules.
  Underlay remains group-defined absolute; its baseRev is projection
  provenance, not another input freshness rule. When the computed underlay
  `outer` rect overflows or fails `bounds::valid_carried_rect` (such as an
  expanded underlay on an output at `-32768`), underlay publication hides with
  `status: "hidden"`, `reason: "unrepresentable"`, and retained `baseRev`
  rather than clamping or failing reply validation.
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
| `OutputDesire`, `TilingMode`, `FloatOrigin`, `TransferKind`, `ScopeAuthority`, `HoldReason` | [Desired rows](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes), [world_report.rs](../../crates/tiler-core/src/world_report.rs) |
| `NativeObservation`, `NativeOccupancy`, `NativeCreationDesire`, `NativeCreationAttempt` | [Adoption](world-contract-v3.md#host-owned-adoption-amendment-tentative-2026-10-25), [world_report.rs](../../crates/tiler-core/src/world_report.rs) |
| `FrameAttempt`, `OperationOutcome` | [Settlement](world-contract-v3.md#5-raw-attempts-and-settlement), [world_report.rs](../../crates/tiler-core/src/world_report.rs) |
| `WindowSizeHints`, `FixedSizePredicate` | [size_hints.rs](../../crates/tiler-core/src/size_hints.rs) |
| `WorkspaceMode`, `MigrationSourceRefill`, `SwitchTarget`, `FreshPlacement`, `BornMaximized` | [Settings baseline](world-contract-v3.md#current-message-shapes-v3), [catalog](../../crates/tiler-core/src/settings_catalog.rs) |
| `CommandResult::Float`, `CommandResult::Migration`, `CommandResult::Maximize`, `DirectionalOutcome` | [Results](world-contract-v3.md#current-message-shapes-v3), [world_report.rs](../../crates/tiler-core/src/world_report.rs), [world_directional.rs](../../crates/tiler-core/src/world_directional.rs) |
| `GestureIngress`, `GestureSettled`, `DragHoverPrior`, `PriorGroupEdge`, `DragSide`, `Direction`, `SameAxisMove` | [Wire baseline](world-contract-v3.md#current-message-shapes-v3), [gesture.rs](../../crates/tiler-core/src/gesture.rs), [world_protocol.rs](../../crates/tiler-protocol/src/world_protocol.rs) |
| `PresentationUnderlay`, `PresentationUnderlayFound`, `GroupHideReason` (adds `Unrepresentable` / `unrepresentable`), `ActiveGroupMember`, `PagerScreen`, `PagerWorkspace`, `PagerWindow` | [Presentation](world-contract-v3.md#8-border-underlay-drop-preview-and-pager), [world_presentation.rs](../../crates/tiler-core/src/world_presentation.rs) |
| `BorderHideReason` except HostHidden spelling | [visual.rs](../../crates/tiler-core/src/visual.rs) |
| `DomainKey`, `OutputId`, `WorkspaceId`, `NativeWorkspaceId` string representation, `WindowId`, `NodeId`, `Rect` | [Baseline codec](../../crates/tiler-protocol/src/world_protocol.rs), [session/world.rs](../../crates/tiler-core/src/session/world.rs) |

## Equivalent typed and bound-broker examples

Both routes evaluate the same noncommuting sequence (`focus win-b`, then
`toggle-maximize currentFocus`, then refusal control `focus win-missing`) from
an identical initial world (`win-a` focused on `DP-1`) and keep host evidence,
logical world state, native actuation, and per-command settlement separate.

### Typed prepared route

```rust
// 1. Bind lifetime and settings, then incorporate initial host evidence:
let mut engine = WorldEngine::new("gen-seq-1").expect("valid gen");
engine.handshake(&handshake_req);
engine.handle_settings(&settings_req);
handle_report_checked(&mut engine, &initial_report_focused_on_a, &[]);

// 2. Evaluate ordered batch through the checked prepared transition:
//    report carries host evidence only (WorldReportV4), no wire JSON stamps.
let batch = [
    (1, WorldCommand::Focus { window: WindowSubject::Window(WindowId("win-b".into())), direction: None }),
    (2, WorldCommand::ToggleMaximize { window: WindowSubject::CurrentFocus }),
    (3, WorldCommand::Focus { window: WindowSubject::Window(WindowId("win-missing".into())), direction: None }),
];
let result = handle_report_checked(&mut engine, &followup_report, &batch);

// 3. Separate outputs:
//    - result.incorporation == ReportIncorporation::Incorporated
//    - result.outcome == WorldOutcome::Accepted(reply): logical world + native actuation
//      (`reply.focus_request` targets "win-b"; "win-b" has Maximized=true, "win-a" unchanged)
//    - result.dispositions: caller-owned per-sequence settlement
//      [
//        1 => Executed(None),
//        2 => Executed(Some(CommandResult::Maximize { window: WindowId("win-b".into()), maximized: true, clear_fullscreen: false, was_maximized: false })),
//        3 => Refused(CommandRejectKind::FocusUnknown),
//      ]
```

### Bound-broker route

```jsonc
// Abbreviated wire snippets ({...} stands for full required fields; "gen-seq-1" is an illustrative bound generation).
// 1. Handshake + settings on DiscoverWorld, then initial capture on EvaluateCapture:
{"type":"handshake","v":4,"adapter":"kwin-actuator-1","req":"req-0","applicationIdentity":{...},"capabilities":{...}}
{"type":"settings","v":4,"adapter":"kwin-actuator-1","gen":"gen-seq-1","req":"settings-0","rev":"1","values":{...}}

// 2. Park ordered commands on QueueWorldCommand (ACK is filing only, outcome: "queued"):
{"v":4,"gen":"gen-seq-1","client":"seq-client","clientCmd":"1","command":{"op":"focus","win":"win-b"}}
{"v":4,"gen":"gen-seq-1","client":"seq-client","clientCmd":"2","command":{"op":"toggle-maximize","win":{"currentFocus":true}}}
{"v":4,"gen":"gen-seq-1","client":"seq-client","clientCmd":"3","command":{"op":"focus","win":"win-missing"}}

// 3. EvaluateCapture carries capture facts only (outer envelope alone carries v; report omits v and broker-owned stamps such as cmdSeq/command, while carrying required propertyAttempts):
{"v":4,"adapter":"kwin-actuator-1","gen":"gen-seq-1","source":"src-2","sourceSeq":"2","nativeInvalid":"2","report":{"screens":[...],"windows":[...],...,"propertyAttempts":[]}}
// Broker sets the standalone report's v from the outer discriminant, stamps correlation, clock and sequences, and dispatches.
// Reply authorizes native actuation (focusRequest -> "win-b", win-b state maximized=true, win-a unchanged);
// exact source redelivery returns the remembered reply without re-executing commands.

// 4. Separate requester-only settlement via TakeCommandResult:
//    clientCmd "1" -> {"outcome":"ready","result":null,...}
//    clientCmd "2" -> {"outcome":"ready","result":{"op":"maximize","win":"win-b","maximized":true,"clearFullscreen":false,"wasMaximized":false},...}
//    clientCmd "3" -> {"outcome":"failed","kind":"focus-unknown",...}
```

## Conformance

- Active connection, protocol, and production-route conformance uses
  `test-fixtures/world-wire/v4-active`, `test-fixtures/stage5-v4-active`, and
  `test-fixtures/kde-v4-active`. Frozen `test-fixtures/world-wire/v4`,
  `test-fixtures/stage5`, `test-fixtures/stage5-v4`, and `test-fixtures/kde-v4`
  are historical only and are explicitly refused by the active codec and broker.
  Fixture ids/assertions/metadata are not wire fields.
- Offline conformance covers both typed prepared (`handle_report_checked`) and
  real checked broker (`WorldService`) routes, including sequence equivalence
  across combined/split/skipped-apply deliveries and legacy `v3` / missing-`propertyAttempts`
  refusals. Native desktop settlement, visibility-attempt emission, and live
  paint evidence remain separate from offline conformance.
  [Testing](../principles.md#testing),
  [migration gates](../decisions/contract.md#contract-authority-and-v4-migration).

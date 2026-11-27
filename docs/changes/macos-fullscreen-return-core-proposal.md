# Proposal: return to a remembered native fullscreen member on macOS

**Status:** host single-window round-trip evidence collected; Split View and product switch-away remain unprobed; pending Pass 3 re-review

No wire, core, or return-route change is enabled by this proposal.

## Problem

Selecting a logical workspace should return to its remembered fullscreen member when that member still owns a separate native fullscreen Space. macOS exposes two physical destinations: the user Desktop that anchors the logical workspace, and the disposable fullscreen Space that presently covers it. OmniTiler must switch to the latter for this explicit command, prove exact arrival without a focus setter, and leave focus, frame, and fullscreen state to the host. A fullscreen Space must not become a workspace, ring entry, or write-authorizing domain.

## Present representation and gap

- [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs#L439-L509) resolves focus from logical homing and last-active/layout history. [`retain_view`](../../crates/tiler-core/src/world_tiling_views.rs#L218-L427) can publish the exact lifetime as `FocusRequest.target`, with `cmd_seq` and `after_shown.op` identifying its parent select. [`host_owned_focus_bearing_fullscreen_focus_freezes_frameless_and_proves`](../../crates/tiler-core/src/world_selection_await_tests.rs#L10478) shows this works for a fullscreen member reported on the **same native binding** as its logical home. It does not prove return to a separate unbound fullscreen Space.
- On macOS [`build_report`](../../crates/tiler-macos/src/world_builder.rs#L398) emits a grounded fullscreen window with `fullscreen: true` and `NativeWorkspaceFact::Omitted` when it cannot home the window to a user Space. Physical membership is read by the adapter, not published as user membership. The exact focus target is a useful selection subject, not a physical destination or an arrival receipt.
- [`prove_selection_arrival`](../../crates/tiler-core/src/world_selection.rs#L2388-L2461) is only consulted through [`settle_selection_feedback`](../../crates/tiler-core/src/world_selection.rs#L1628-L1669) when an effect exists, and arrival members freeze only under that effect gate ([`freeze_one_selection`](../../crates/tiler-core/src/world_selection.rs#L1331-L1333)). [`switch_members`](../../crates/tiler-core/src/world_selection.rs#L1417-L1441) is a separate host-owned-empty lane. [`selection_terminal`](../../crates/tiler-core/src/world_selection.rs#L2283-L2327) reports `SwitchFocus::Unproven` without an effect once native proof passes. A focusless return therefore cannot use arrival brackets, and today's native proof only matches the Desktop binding.
- [`retain_view`](../../crates/tiler-core/src/world_tiling_views.rs#L284-L327) mints one retained view flight for the initiating output, then fans the logical view and one shared history entry to all live outputs. [`selection_native_proof`](../../crates/tiler-core/src/world_selection.rs#L1879-L1940) requires one native echo per obligated output from that identical logical/native binding. An exact post-read of the fullscreen SID cannot prove the selection through today's v4 fields.
- [`held_scopes`](../../crates/tiler-core/src/world_run_v4.rs#L381-L423) fences a whole logical workspace when a staged window already has a staged home but no readable logical receipt, so a detached fullscreen member also fences ordinary Desktop siblings. The loop does not fence newcomers with no staged home; Pass 2's "every missing receipt" framing was too broad. [`logical_homing`](../../crates/tiler-core/src/world_observe.rs#L875-L887) is the `receipt.is_none() && native_authoritative()` branch: it retains an output-matched or legitimately remapped existing home and returns `None` for a born-fullscreen newcomer, whose own writes are fenced. It is not a bare host-owned fallback. [`execution_subject`](../../crates/tiler-core/src/world_close_focus.rs#L171-L190) resolves an implicit command from the old cursor or observed foreground, and [`seed_focus_cursor`](../../crates/tiler-core/src/world_close_focus.rs#L212-L219) only updates it on a later report.
- v4 currently shares [`WorkspaceRef`](../../crates/tiler-core/src/world_report.rs#L68-L77) between member and view evidence, stores only a bare native string in [`FocusChange::View`](../../crates/tiler-core/src/world_report.rs#L1539-L1541), and keys view-attempt duplicates by operation alone ([`world_property_attempts.rs`](../../crates/tiler-protocol/src/world_property_attempts.rs#L494-L558)). `NativeObservation.current` is nullable with an absent key held ([`world_protocol.rs`](../../crates/tiler-protocol/src/world_protocol.rs#L2996-L3039)), so read gaps and known absence are indistinguishable.

## Pass 3 decisions

### 1. Focusless return settlement

An explicit return to a detached fullscreen member **never mints a FocusRequest, ArrivalBracket, focus feedback, or any focus/visibility setter**. A detached override forces the focusless binding even when `remembered_view_focus` resolves a candidate, so the direct and awaitShown focus effects are not taken. At grant, each admitted View flight freezes its own typed destination and, for a detached target, its own intended `WindowId`. Store these together in the output-keyed `RetainedSelection.view_targets` as `{home: DomainKey, destination: ViewRef, intended_member: Option<WindowId>}`; `intended_member` is present exactly when `destination` is detached. With no effect, arrival members and host-owned `switch_members` stay empty; the common terminal still reaches `SwitchOutcome::Complete` once all remaining obligations and native proof pass, with `SwitchFocus::Unproven`, never `Proven`. The intended member is a membership subject only; it is not a focus decision.

[`selection_native_proof`](../../crates/tiler-core/src/world_selection.rs#L1879-L1940) and [`settle_views_pre`](../../crates/tiler-core/src/world_settle.rs#L550-L653) compare each output's frozen `destination`; a detached override skips the Desktop-binding echo and proves only through the branches below. Logical `workspace.views` keeps the selected logical id until a newer known coherent departure supersedes it under the existing rule; physical `last_views` records the typed destination. A settled logical view never implies the covered Desktop is current or receives a write.

The detached target is proven by either branch, from coherent readable facts on the frozen output:

| Branch | Exact proof | Rejection |
|---|---|---|
| A. Fresh physical current | `currentKnown: true`, `detachedCurrent` exactly the frozen `DetachedRef`, and the frozen lifetime grounded on that same surface (`detachedSurface` exact, `fullscreen: true`, same output) in the same coherent report. | Missing/gapped current, Desktop current, another surface, missing/mismatched member, or incomplete inventory holds. |
| B. Correlated accepted post | A correlated `ViewAttempt` with `disposition.setter: "accepted"`, `post` exactly the frozen `DetachedRef`, exact `(op,out,ws)` tuple, and `postMember` equal to the frozen intended lifetime and grounded at that read (live exact lifetime with a readable fact on the frozen surface); the adapter may send `postMember` only when it read that membership between two exact physical samples of the same target. | Refused/threw, unreadable/non-target post, wrong output/workspace, non-null `postMember` on a non-detached post, a different member, or an unattested/bad row holds and is never retried. |

`postMember` is a membership observation, never focus evidence. Branch B is historical: once latched it is not re-proven, and a newer **known** physical departure (Desktop, another detached surface, or positive member loss) is adopted as newer external input without replaying the command, re-sending a View, or borrowing foreground as focus. A report gap is not a departure. This preserves the existing proven-then-abandoned rule ([`world_settle.rs`](../../crates/tiler-core/src/world_settle.rs#L439-L456)) for the detached scene.

### 2. Homing, holds, and born-fullscreen

| Case | Rule |
|---|---|
| Detached fullscreen with a pre-existing same-lifetime, same-output home | Exempt **only** from the membership-derived whole-scope hold in [`world_run_v4.rs`](../../crates/tiler-core/src/world_run_v4.rs#L383-L423) when `detachedSurface` is exact, `fullscreen == Some(true)`, and that lifetime's pre-existing `core_homing` names the same output. Keep the retained home. |
| Any other hold | Still applies: held/excluded windows, incomplete inventory, gesture barrier, native order conflict, conflicting facts, and window-local holds. The exemption never clears them. |
| Unknown/unreadable detached membership with a staged home | Remains fenced; no positive detached fact means no exemption. |
| Born fullscreen with no prior home | Remains unhomed (`logical_homing` returns `None`), its own writes are fenced by a window-local hold, and no arbitrary logical scope is fenced. Do not invent a home or Desktop currency. |
| Ordinary Desktop siblings | Retain tiling/focus eligibility on their own readable proof; the detached member does not poison the scope once the narrow exemption applies. |
| Detached member writes | No frame, membership, show-state, focus, raise, or fullscreen setter is authorized for the detached lifetime. Its covered Desktop remains in the ordinary user Desktop order but is not published as physically current or visible on the strength of the detached fact. |

The focus fallback in [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs#L439-L509) may consider only already-homed candidates. A born-fullscreen lifetime with no retained home never enters that fallback.

### 3. Shared per-output flights

For a capability-enabled host-owned explicit select that changes the shared view, the parent operation remains `op-<cmdSeq>` and the batch mints **one View flight per affected live output**, each with its own frozen target workspace and nullable detached destination. The shared logical view and its single history entry are recorded once. Non-shared mode keeps the single-output flight.

| Rule | Requirement |
|---|---|
| Frozen plan | Compute the per-output plan atomically from the pre-command reports: logical home, intended membership subject, exact current/physical refs, capability, and liveness. Later reports never add or retarget an override. |
| Binding/evidence tuple | Bind and correlate every flight and attempt by exact `(op,out,ws)`. A sibling output's attempt can never prove another sibling. |
| Duplicate identity | The wire codec and adapter spent/pending keys use `(op,out)`, not `op` alone; the current op-only view duplicate and the adapter's op-only `pending_views` key ([`world_runtime.rs`](../../crates/tiler-macos/src/world_runtime.rs#L831-L866)) are replaced. A duplicate `(op,out)` rejects the lane row. |
| Termination | The existing single binding remains keyed by the issuing/focus output. Its terminal waits until every sibling flight admitted by the frozen plan has a latched verdict; no sibling completes the shared switch alone. |
| Supersession | A newer command supersedes only the affected output's flight. It never re-mints a flight for an output already exact-current detached for the same logical home, nor for an obsolete/lost output. |
| Redundant select | Already current on the exact detached surface for the same logical home: no View flight and no focus. A same-workspace selection with a null/ambiguous physical reading is not transformed into a Desktop write. |

**Canonical shared reconciliation.** If physical siblings disagree, retain the completed shared logical selection; adopt a newer workspace only when the live siblings coherently report a common logical workspace. Keep each output's divergent physical snapshot and fence scoped to that output. Never elect an arbitrary sibling, reassert a shared selector, or overwrite one output from another.

### 4. Cursor and focus supersession during a detached return

On an admitted detached return, immediately clear [`logical_focus_cursor`](../../crates/tiler-core/src/world_close_focus.rs#L212-L219) and install a retained guard scoped to the affected outputs. For the remainder of the batch and across later reports, [`execution_subject`](../../crates/tiler-core/src/world_close_focus.rs#L171-L219) returns `None` for every implicit subject, including the observed-foreground fallback. Queued implicit commands fail closed as focus-unknown rather than acting on the departing window.

The guard is released only by a newer coherent exact physical surface for an affected output **and** a trustworthy host foreground observation. A positive physical departure or explicit refusal may release it only with that fresh coherent foreground; read gaps never release it. Seeding uses the actual grounded member on that surface; a Split View peer is admissible only when live, eligible, and homed. Never guess among members, and never restore the cursor from an accepted post alone. Pending-focus supersession is scoped to the affected output, not the whole engine.

`last_views` becomes a physical, typed per-output snapshot distinct from logical history: only a coherent Desktop or detached ref is stored, and a gap removes it. [`selector_hides_target`](../../crates/tiler-core/src/world_close_focus.rs#L441-L452) compares typed expected visibility (the frozen detached target, else the Desktop binding, else the target's own grounded detached surface), so a gap is never a change. Positive detached external adoption in [`adopt_idle_native_echoes`](../../crates/tiler-core/src/world_native.rs#L420-L462) is allowed only when exactly one retained logical home owns the grounded surface members and no view flight owns the output; ambiguous/Split View homes are not adopted. Adoption never claims the covered Desktop is current or visible.

### 5. Closed v4 wire delta

`HostCapabilities.detachedFullscreenView` is a **required boolean**; `true` only with `hostOwned: true`, no default or omission. KDE and Windows declare `false` with no behavior change. Existing capability keys are unchanged: `hostOwned`, `equalCount`, `sharedNativeRoster`, `birthClear`, `nativeSelectorScope`, `windowSuppression`, `maximize`, `fullscreen`, `nativeSticky`, `above`, `warp`, `tiledFloatTransfer`, `unavailableFacts`. Update codecs and active fixtures together; historical fixtures keep their exact bytes and are not a current-parse compatibility shape.

Every new key is required-nullable where declared nullable. On an identifiable observation or attempt row, a missing required-nullable key holds that row (never a guessed value); structural identity and native order/occupancy violations still reject. On a grant (`TransferTargetV4`), a missing `detachedDestination` is a malformed grant and rejects with no actuation.

**Shared reference types**

| Type | Exact wire shape |
|---|---|
| `DetachedRef` | `{"kind":"detached","out":OutputId,"surface":NativeId}` exactly three required keys. `surface` is a bounded opaque native surface id in its own namespace (nonempty, same length/grammar bounds as `NativeWorkspaceId`, never compared with one). No extra keys. |
| `ViewRef` | Either the existing native shape `{"kind":"native","out":OutputId,"ws":NativeWorkspaceId}` or `DetachedRef`. No logical arm. |

`ViewRef` is used only by `ViewAttempt.{pre,post,base}` and `FocusChange::View.native`. `MemberAttempt.WorkspaceRef` is unchanged (`logical`/`native`) and cannot accept `detached`; the existing decoder already holds identifiable invalid member rows and grants them no authority ([`world_property_attempts.rs`](../../crates/tiler-protocol/src/world_property_attempts.rs#L367-L461), [`opt_workspace_ref_or_null`](../../crates/tiler-protocol/src/world_protocol.rs#L1122-L1162)).

**Observation tri-state**

`NativeObservation` keeps `out`, `order`, `occupancy`; `current` is required `ID|null`; new required keys are `currentKnown: bool` and `detachedCurrent: DetachedRef|null`.

| `currentKnown` | `current` | `detachedCurrent` | Meaning |
|---|---|---|---|
| `false` | `null` | `null` | Read gap; retain the logical view, store no physical snapshot, and prove nothing. |
| `true` | `null` | `null` | Known none of these surfaces. |
| `true` | `ID` | `null` | User Desktop; `ID` names an `order` member, with a non-member or contradictory row handled by the existing conflict/duplicate lane. |
| `true` | `null` | ref | Detached surface; `ref.out` equals the row output and the id never enters `order`/`occupancy`. |

`false` with any non-null ref, both `current` and `detachedCurrent`, a detached id in `order`/`occupancy`, or a mismatched `ref.out` is invalid.

`WindowFact` keeps its existing keys exactly (`win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`, `minimized`, `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`, `acceptsInput`, `activationInhibited`, `toolWindow`, `owned`, `transient`, `modal`, `resizable`, `above`, `below`, `owner`, `parent`, `role`, `appId`, `executable`, `processId`, `class`, `title`, `type`, `activationPolicy`, `layer`, `fullscreenOwner`, `fullscreenExitable`, `suppressionAvailable`, `visibility`) and adds required nullable `detachedSurface: DetachedRef|null`.

| `nativeWs` | `detachedSurface` | Meaning |
|---|---|---|
| omitted, or `""` (legacy empty sentinel), or `detachedSurface` omitted | any | Gap; existing unknown semantics preserved. |
| explicit `null` | explicit `null` | Known neither. |
| `ID` | explicit `null` | User Desktop membership. |
| explicit `null` | ref | Detached membership; `ref.out` equals the fact output and `fullscreen` must be `Some(true)`. |

Both arms non-null, a detached ref with missing/`false` fullscreen, a mismatched output, or capability-false with a non-null detached value is invalid. With capability false, `nativeWs` keeps its exact existing encoding and state semantics (including `""`), and `detachedSurface` must be `null`. More than one live lifetime may share one detached surface on the same output; membership proves neither a logical home nor which member has focus.

**Grant and attempt deltas**

| Record | Keys and rules |
|---|---|
| `TransferTargetV4` | Existing `op`, `kind`, `win`, `out`, `ws`, `follow`, `held`, `cmdSeq`, `parent` unchanged (canonical decimal string or null for `cmdSeq`, opaque id or null for `parent`). Add required nullable `detachedDestination`, exactly `{"surface":DetachedRef,"member":WindowId}`. Only a standalone explicit keyboard `kind:"view"` flight with `follow:false`, `parent:null`, `cmdSeq` non-null, and `surface.out == out` may carry it; every other retained transfer carries null. |
| `ViewAttempt` | Existing `kind`, `op`, `out`, `ws`, `disposition`, `pre`, `post`, `base`, `attemptMs` unchanged; `pre`/`post`/`base` are `ViewRef|null`; `attemptMs` is the canonical decimal string. Add required nullable `postMember: WindowId`, non-null only on an accepted disposition with an exact detached `post` and a grounded adapter-bracketed membership read; it must match the frozen intended lifetime to prove. `disposition` keeps `{"kind":"called","setter":"accepted"|"refused"|"threw"}`. |
| `FocusChange::View` | Exact keys `kind`, `out`, `native`; `native` changes from an ID string to a non-null `ViewRef`, and `native.out` must equal `out`. A malformed ordered focus event rejects; a malformed grant rejects with no actuation. |
| `MemberAttempt` | Unchanged keys `kind`, `op`, `win`, `out`, `ws`, `disposition`, `pre`, `post`, `attemptMs`; `WorkspaceRef` only, never `DetachedRef`. |

**Hold versus reject**

| Condition | Rule |
|---|---|
| Identifiable observation row with bad/conflicting detached fields | Hold the row, clear the affected physical/membership proof, and grant no fallback authority. Existing structural duplicate/output/order/occupancy refusals stay refusals. |
| Capability-false non-null detached on a report lane | Hold when a stable row identity exists, otherwise reject. |
| Detached id in `order`, occupancy, a logical ring, or a workspace binding | Structural contradiction: reject the row/reply and never bind it. |
| Detached coexisting with native current/membership, non-`true` fullscreen, or mismatched output | Conflicting observation: hold; attempt/grant: hold or reject by lane identity. |
| Malformed ordered focus event or malformed grant | Reject; no actuation. |
| Bad/unknown matching attempt | Hold, never retry. |
| Explicit refusal | Proves only with an unchanged readable preimage; otherwise holds as ambiguous. |
| Duplicate view attempt `(op,out)` | Reject the duplicate lane row. |

## Assumptions

- **ASSUMPTION:** the host may rekey a fullscreen surface for the same lifetime, and may reorder Space/display/foreground notifications arbitrarily. Changed surfaces are always supersession; only same-coherent-report or adapter-bracketed observations prove. Two round trips do not establish a general lifetime or ordering guarantee.
- **ASSUMPTION:** Split View may place several logical homes on one detached surface and may focus a peer rather than the intended member. Membership and physical surface never prove which member has focus; the cursor guard uses only a fresh actual foreground observation.
- **ASSUMPTION:** the adapter truthfully withholds `postMember` unless it read that membership between two exact target samples. The wire carries the ID and timestamp, not the two samples, so the core cannot re-derive that bracket from the bare ID; an unattested or mismatched value holds.
- **ASSUMPTION:** `DetachedRef.surface` and `NativeWorkspaceId` are disjoint namespaces even when their opaque strings collide.
- **ASSUMPTION:** a host foreground reading is trustworthy only in a coherent report with readable physical current; unknown/gapped reads never ground a release. An accepted post alone is not foreground proof.

Evidence gaps, not assumptions: same-SID access while away and after return is proved only on the tested single-window layout; lifetime rekey, notification ordering, Split View membership, unassisted Desktop focus, and the product redundant-refusal diagnostic remain unprobed.

## Prospective tests

| Area | Narrow assertions |
|---|---|
| Wire | Missing `detachedFullscreenView` rejects; non-host `true` rejects; capability-false non-null detached holds/rejects; tri-state and WindowFact tables round-trip; both arms or detached-in-order reject; `(op,out)` duplicate rejects; `MemberAttempt` with detached holds and grants no authority. |
| Settlement A | Fresh exact detached current plus grounded member completes focusless with `Unproven`, no FocusRequest/arrival/setter; gap, Desktop, wrong surface, and wrong member hold. |
| Settlement B | Accepted exact post plus bracketed `postMember` proves; refused, non-detached post, wrong member, unattested row hold; no retry. |
| Proven then abandoned | Accepted post proves, then a known Desktop/other-surface departure adopts once with no replay and focus still `Unproven`; gap does not supersede. |
| Holds | Detached retained home is not fenced and ordinary siblings tile; unknown membership still fences; born fullscreen stays unhomed with own writes fenced and no scope hold; fallback never invents a home. |
| Shared flights | One shared op/history, per-output destinations, sibling proof isolation, terminal waits all planned siblings, latest-per-output supersession, lost output retires only its flight, common-newer adoption versus divergent-sibling retention. |
| Redundant/no-op | Exact detached current same home sends no View/focus; ambiguous same-workspace never becomes a Desktop write. |
| Cursor/guard | Cursor clears immediately; guard blocks same-batch and later implicit subjects; gaps do not release; departure/refusal plus fresh foreground releases; accepted post alone does not restore; Split View peer only when eligible/homed. |
| Focus supersession | Typed `last_views` ignores gaps; different typed ref hides only with readable expectation; same expected does not; detached adoption only with unambiguous home and no owning flight. |
| Authority | Detached refs never enter user Desktop current/order/occupancy/ring/binding; a detached current never implies the covered Desktop is current or visible. No write is authorized to the detached lifetime or the covered Desktop by the override. |

## Host evidence

| Decisive host fact | Result |
|---|---|
| Exact fixture lifetime survives both round trips; same detached SID present while away and after each return | Proved on the tested layout. |
| Four legs reach exact SLS current without bounce; other display current preserved | Proved. |
| No fullscreen-member writes during switching; only entry and teardown state requests | Proved. |
| Teardown restores original currents/order and leaves no owned fixture lifetime or Space | Proved on the tested layout. |
| Unassisted Desktop focus after pinned Mission Control | Not proved; the harness focused the owned Desktop sibling only after measuring away. |
| General Space-notification order, lifetime rekey, Split View | Unprobed. |
| Product redundant-refusal diagnostic and product switch-away | Not observed; no product route promoted. |

Operational detail, timing samples, caveats, and recovery: [macOS probe record](macos-fullscreen-probe.md).

## Alternatives

- **Reuse `focus_request.target` with adapter-only switching:** finds an exact candidate but cannot settle a separate native scene without inventing focus evidence or an arrival receipt.
- **Report the user Desktop as current or map the fullscreen SID onto the logical home:** rejected; invents physical currency, conflates transient and workspace identity, and risks granting writes to the covered Desktop.
- **Treat fullscreen as an ordinary user Space:** rejected; changes ordinal/history/creation semantics and gives a host scene a logical identity it does not own.
- **Only add a fullscreen SID to `after_shown`:** insufficient; a reply hint cannot prove operation-specific physical arrival or reconcile outside switches.
- **A separate singleton destination or second selection terminal:** rejected by Pass 2; it breaks Shared/multi-output and duplicates the existing proof engine.

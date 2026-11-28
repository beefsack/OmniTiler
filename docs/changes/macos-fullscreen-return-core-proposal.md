# Proposal: return to a remembered native fullscreen member on macOS

**Status:** Pass 4 approved. The [amended v4 contract](../porting/world-contract.md#native-observation) defines the coordinated cutover. Single-window host round-trip evidence was collected; signed product acceptance (redundant refusal → armed away → exact return) and Split View remain open.

Neither macOS product route advertises detached fullscreen support before signed acceptance.

## Problem

Selecting a logical workspace should return to its remembered fullscreen member when that member still owns a separate native fullscreen Space. macOS exposes two physical destinations: the user Desktop that anchors the logical workspace, and the disposable fullscreen Space that presently covers it. OmniTiler must switch to the latter for this explicit command, prove exact arrival without a focus setter, and leave focus, frame, and fullscreen state to the host. A fullscreen Space must not become a workspace, ring entry, or write-authorizing domain.

## Present representation and gap

- [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs#L439-L509) resolves focus from logical homing and last-active/layout history. [`retain_view`](../../crates/tiler-core/src/world_tiling_views.rs#L218-L427) can publish the exact lifetime as `FocusRequest.target`, with `cmd_seq` and `after_shown.op` identifying its parent select. [`host_owned_focus_bearing_fullscreen_focus_freezes_frameless_and_proves`](../../crates/tiler-core/src/world_selection_await_tests.rs#L10478) shows this works for a fullscreen member reported on the **same native binding** as its logical home. It does not prove return to a separate unbound fullscreen Space.
- On macOS [`build_report`](../../crates/tiler-macos/src/world_builder.rs#L398) emits a grounded fullscreen window with `fullscreen: true` and `NativeWorkspaceFact::Omitted` when it cannot home the window to a user Space. Physical membership is read by the adapter, not published as user membership. The exact focus target is a useful selection subject, not a physical destination or an arrival receipt.
- [`prove_selection_arrival`](../../crates/tiler-core/src/world_selection.rs#L2388-L2461) is only consulted through [`settle_selection_feedback`](../../crates/tiler-core/src/world_selection.rs#L1628-L1669) when an effect exists, and arrival members freeze only under that effect gate ([`freeze_one_selection`](../../crates/tiler-core/src/world_selection.rs#L1331-L1333)). [`switch_members`](../../crates/tiler-core/src/world_selection.rs#L1417-L1441) is a separate host-owned-empty lane. [`selection_terminal`](../../crates/tiler-core/src/world_selection.rs#L2283-L2327) reports `SwitchFocus::Unproven` without an effect once native proof passes. A focusless return therefore cannot use arrival brackets, and today's native proof only matches the Desktop binding.
- [`retain_view`](../../crates/tiler-core/src/world_tiling_views.rs#L284-L327) mints one retained view flight for the initiating output, then fans the logical view and one shared history entry to all live outputs. [`selection_native_proof`](../../crates/tiler-core/src/world_selection.rs#L1879-L1940) requires one native echo per obligated output from that identical logical/native binding. An exact post-read of the fullscreen SID cannot prove the selection through today's v4 fields.
- [`held_scopes`](../../crates/tiler-core/src/world_run_v4.rs#L381-L423) fences a whole logical workspace when a staged window already has a staged home but no readable logical receipt, so a detached fullscreen member also fences ordinary Desktop siblings. The loop does not fence newcomers with no staged home; Pass 2's "every missing receipt" framing was too broad. [`logical_homing`](../../crates/tiler-core/src/world_observe.rs#L875-L887) is the `receipt.is_none() && native_authoritative()` branch: it retains an output-matched or legitimately remapped existing home and returns `None` for a born-fullscreen newcomer, whose own writes are fenced. It is not a bare host-owned fallback. [`execution_subject`](../../crates/tiler-core/src/world_close_focus.rs#L171-L190) resolves an implicit command from the old cursor or observed foreground, and [`seed_focus_cursor`](../../crates/tiler-core/src/world_close_focus.rs#L212-L219) only updates it on a later report.
- v4 currently shares [`WorkspaceRef`](../../crates/tiler-core/src/world_report.rs#L68-L77) between member and view evidence, stores only a bare native string in [`FocusChange::View`](../../crates/tiler-core/src/world_report.rs#L1539-L1544), and keys view-attempt duplicates by operation alone ([`world_property_attempts.rs`](../../crates/tiler-protocol/src/world_property_attempts.rs#L494-L558)). `NativeObservation.current` is nullable, but a missing key holds the row ([`world_protocol.rs`](../../crates/tiler-protocol/src/world_protocol.rs#L3025-L3039)). A valid `null` row does not distinguish an unreadable current from a current known not to name a Desktop: today's macOS producer uses `None` for both ([`world_builder.rs`](../../crates/tiler-macos/src/world_builder.rs#L492-L500)).

## Pass 3 decisions

### 1. Focusless return settlement

An explicit return to a detached fullscreen member **never mints a FocusRequest, ArrivalBracket, focus feedback, or any focus/visibility setter**. A detached override forces the focusless binding even when `remembered_view_focus` resolves a candidate, so the direct and awaitShown focus effects are not taken. At grant, each admitted View flight freezes its own typed destination and, for a detached target, its own intended `WindowId`. Store these together in the output-keyed `RetainedSelection.view_targets` as `{home: DomainKey, destination: ViewRef, intended_member: Option<WindowId>}`; `intended_member` is present exactly when `destination` is detached. With no effect, arrival members and host-owned `switch_members` stay empty; the common terminal still reaches `SwitchOutcome::Complete` once all remaining obligations and native proof pass, with `SwitchFocus::Unproven`, never `Proven`. The intended member is a membership subject only; it is not a focus decision.

[`selection_native_proof`](../../crates/tiler-core/src/world_selection.rs#L1879-L1940) and [`settle_views_pre`](../../crates/tiler-core/src/world_settle.rs#L550-L653) compare each output's frozen `destination`; a detached override skips the Desktop-binding echo and proves only through the branches below. Logical `workspace.views` keeps the selected logical id until a newer known coherent departure supersedes it under the existing rule; physical `last_views` records the typed destination. A settled logical view never implies the covered Desktop is current or receives a write.

**Latched per-output verdicts.** `RetainedSelection` stores one latched typed verdict per frozen output — `pending`, `settled`, or the exact refused `ViewRef` — not one shared acceptance for the whole binding. Today [`reconcile_selection_scopes`](../../crates/tiler-core/src/world_selection.rs#L643-L721) skips multi-output coverage whenever `binding.effect.is_none()` (L644-646 and L700-702) and then removes by key only (L712-721), so a focusless host-owned binding loses the whole `RetainedSelection` when the key output settles or refuses while a sibling is still pending. [`selection_refusal_pends`](../../crates/tiler-core/src/world_selection.rs#L730-L767) likewise compares only the Desktop logical `effective_view_pre` ([`world_settle.rs`](../../crates/tiler-core/src/world_settle.rs#L113-L127)), which returns `None` for a detached preimage. Both functions must cover multi-output focusless bindings with typed `ViewRef` equality: a settled or refused output retires only its own obligation, the binding survives while any planned sibling verdict is pending, and a refusal proves only when `pre == post` equals the exact frozen preimage `ViewRef` and differs from the frozen `destination`. Read gaps never settle an output.

The detached target is proven by either branch, from coherent readable facts on the frozen output:

| Branch | Exact proof | Rejection |
|---|---|---|
| A. Fresh physical current | `currentKnown: true`, `detachedCurrent` exactly the frozen `DetachedRef`, and the frozen lifetime grounded on that same surface (`detachedSurface` exact, `fullscreen: true`, same output) in the same coherent report. | Missing/gapped current, Desktop current, another surface, missing/mismatched member, or incomplete inventory holds. |
| B. Correlated accepted post | A correlated `ViewAttempt` with `disposition.setter: "accepted"`, `post` exactly the frozen `DetachedRef`, exact `(op,out,ws)` tuple, and `postMember == Some(intended_member)` for the frozen lifetime; the adapter sends `postMember` only when it read that membership between two exact physical samples of the same target, and sends `null` when it withheld the read. | Refused/threw, unreadable/non-target post, wrong output/workspace, non-null `postMember` on a non-detached post, a different member, or a missing/malformed row holds and is never retried. `postMember:null` withholds proof; there is no attestation field. |

`postMember` is a membership observation, never focus evidence, and `null` is the withholding rather than a failed attestation. Branch B is historical: once `postMember == Some(intended_member)` latches, core never rechecks it against current membership and never requires the later snapshot to still place the member on the frozen surface; a newer **known** physical departure (Desktop, another detached surface, or positive member loss) is adopted as newer external input without replaying the command, re-sending a View, or borrowing foreground as focus. A report gap is not a departure. This preserves the existing proven-then-abandoned rule ([`world_settle.rs`](../../crates/tiler-core/src/world_settle.rs#L439-L456)) for the detached scene.

### 2. Homing, holds, and born-fullscreen

| Case | Rule |
|---|---|
| Detached fullscreen with a pre-existing same-lifetime, same-output home | Exempt **only** from the membership-derived whole-scope hold in [`world_run_v4.rs`](../../crates/tiler-core/src/world_run_v4.rs#L383-L423) when `detachedSurface` is exact, `fullscreen == Some(true)`, and that lifetime's pre-existing `core_homing` names the same output. Keep the retained home. |
| Any other hold | Still applies: held/excluded windows, incomplete inventory, gesture barrier, native order conflict, conflicting facts, and window-local holds. The exemption never clears them. |
| Unknown/unreadable detached membership with a staged home | Remains fenced; no positive detached fact means no exemption. |
| Born fullscreen with no prior home | Remains unhomed (`logical_homing` returns `None`), its own writes are fenced by a window-local hold, and no arbitrary logical scope is fenced. Do not invent a home or Desktop currency. |
| Ordinary Desktop siblings | Retain tiling/focus eligibility on their own readable proof; the detached member does not poison the scope once the narrow exemption applies. |
| Detached member writes | Neither the override nor an automatic action authorizes a frame, membership, show-state, focus, raise, or fullscreen setter for the detached lifetime. Its covered Desktop remains in the ordinary user Desktop order but is not published as physically current or visible on the strength of the detached fact. |

The focus fallback in [`remembered_view_focus`](../../crates/tiler-core/src/world_tiling_views.rs#L439-L509) may consider only already-homed candidates. A born-fullscreen lifetime with no retained home never enters that fallback.

**Explicit R14 stays R14.** The no-write rule narrows the detached override and automatic actions only. Explicit maximize/fullscreen/sticky commands still follow [R14](../decisions/contract.md#window-state-commands-r14) on the adapter's normal route, including `ToggleFullscreen` when `fullscreenExitable == Some(true)`. Tiled siblings on the covered Desktop keep ordinary layout authority on their own readable facts, subject to the adapter's existing off-Space fences.

### 3. Shared per-output flights

For a capability-enabled host-owned explicit select that changes the shared view, the parent operation remains `op-<cmdSeq>` and the batch mints **one View flight per affected live output**, each with its own frozen target workspace and nullable detached destination. The shared logical view and its single history entry are recorded once. Non-shared mode keeps the single-output flight.

| Rule | Requirement |
|---|---|
| Frozen plan | Compute the per-output plan atomically from the pre-command reports: logical home, intended membership subject, exact current/physical refs, capability, and liveness. Later reports never add or retarget an override. |
| Binding/evidence tuple | Bind and correlate every flight and attempt by exact `(op,out,ws)`. A sibling output's attempt can never prove another sibling. |
| Duplicate identity | The wire codec and adapter spent/pending keys use `(op,out)`, not `op` alone: the op-only view duplicate ([`world_property_attempts.rs`](../../crates/tiler-protocol/src/world_property_attempts.rs#L494-L558)), the macOS op-keyed `pending_views` and its op-only retained-target lookup ([`world_runtime.rs`](../../crates/tiler-macos/src/world_runtime.rs#L831-L866)), and KWin's op-keyed `viewAttempts`/`appliedViewTarget` ([`world-state.ts`](../../kwin/src/world-state.ts#L197-L220)) are replaced together. A duplicate `(op,out)` rejects the lane row. |
| Termination | The existing single binding remains keyed by the issuing/focus output. Its terminal waits until every sibling flight admitted by the frozen plan has a latched verdict; no sibling completes the shared switch alone. |
| Supersession | A newer command supersedes only the affected output's flight. It never re-mints a flight for an output already exact-current detached for the same logical home, nor for an obsolete/lost output. |
| Redundant select | Already exact-current on the detached surface for the same logical home: no View flight and no focus; proof is the fresh current, not a dispatch. This narrows only the detached case; host-owned Desktop redundant selects keep today's staged flight. A same-workspace selection with a null/ambiguous physical reading is not transformed into a Desktop write. |

**Canonical shared reconciliation.** If physical siblings disagree, retain the completed shared logical selection; adopt a newer workspace only when the live siblings coherently report a common logical workspace. Keep each output's divergent physical snapshot and fence scoped to that output. Never elect an arbitrary sibling, reassert a shared selector, or overwrite one output from another.

### 4. Cursor and focus supersession during a detached return

On an admitted detached return, immediately clear [`logical_focus_cursor`](../../crates/tiler-core/src/world_close_focus.rs#L212-L219) and install one retained guard entry per affected output, not a global suppression flag. For the remainder of the batch and across later reports, [`execution_subject`](../../crates/tiler-core/src/world_close_focus.rs#L171-L190) resolves the guard for the output implicated by the command: on a guarded output it refuses the retained cursor and the observed-foreground fallback, so queued implicit commands fail closed as focus-unknown rather than acting on the departing window. A newer core focus decision (`logical_focus_cursor = Some`) is never blocked, and a trustworthy foreground on an unguarded output is not blocked by another output's entry.

Each entry is released on the first of four conditions, independently per output: (a) a superseding selection or focus decision on that output; (b) that output's removal; (c) a coherent exact physical surface for that output plus a trustworthy host foreground observation; or (d) a proven refusal onto an empty workspace (`foregroundKnown: true` and known-empty focused), which returns that output to ordinary empty-subject semantics. A positive physical departure or explicit refusal releases only through (c) or (d); read gaps never release. Seeding uses the actual grounded member on that surface; a Split View peer is admissible only when live, eligible, and homed. Never guess among members, and never restore the cursor from an accepted post alone. Pending-focus supersession and guard state are scoped to the affected output, not the whole engine; an unaffected output keeps its own foreground and implicit subject.

`last_views` becomes a physical, typed per-output snapshot distinct from logical history: only a coherent Desktop or detached ref is stored, and a gap removes it. [`selector_hides_target`](../../crates/tiler-core/src/world_close_focus.rs#L441-L452) compares typed expected visibility (the frozen detached target, else the Desktop binding, else the target's own grounded detached surface), so a gap is never a change. Positive detached external adoption in [`adopt_idle_native_echoes`](../../crates/tiler-core/src/world_native.rs#L420-L462) is allowed only when exactly one retained logical home owns the grounded surface members and no view flight owns the output; ambiguous/Split View homes are not adopted. Adoption never claims the covered Desktop is current or visible.

### 5. Closed v4 wire delta

`HostCapabilities.detachedFullscreenView` is a **required boolean**; `true` requires both `hostOwned: true` and `nativeSelectorScope: per-output` ([`connection.rs`](../../crates/tiler-core/src/connection.rs#L85-L96)), with no default or omission. KDE and Windows declare `false` with no behavior change. Existing capability keys are unchanged: `hostOwned`, `equalCount`, `sharedNativeRoster`, `birthClear`, `nativeSelectorScope`, `windowSuppression`, `maximize`, `fullscreen`, `nativeSticky`, `above`, `warp`, `tiledFloatTransfer`, `unavailableFacts`. Update codecs and active fixtures together; historical fixtures keep their exact bytes and are not a current-parse compatibility shape.

This is an in-place v4 amendment: core, protocol, KWin (including `viewAttempts` at [`world-state.ts`](../../kwin/src/world-state.ts#L220) and `appliedViewTarget` at [L198](../../kwin/src/world-state.ts#L198) if `(op,out)` keying is adopted uniformly), `tiler-windows`, `tiler-macos`, [`docs/porting/world-contract.md`](../porting/world-contract.md) and active v4 fixtures update together, per the contract's in-place rule ([`contract.md`](../decisions/contract.md#contract-authority-and-v4-migration), lines 18-27); frozen historical fixture bytes stay unchanged.

Every new key is required-nullable where declared nullable. On an identifiable observation or attempt row, a missing required-nullable key holds that row (never a guessed value); structural identity and native order/occupancy violations still reject. On a grant (`TransferTargetV4`), a missing `detachedDestination` is a malformed grant and rejects with no actuation.

**Shared reference types**

| Type | Exact wire shape |
|---|---|
| `DetachedRef` | `{"kind":"detached","out":OutputId,"surface":NativeId}` exactly three required keys. `surface` is a bounded opaque native surface id in its own namespace (nonempty, same length/grammar bounds as `NativeWorkspaceId`, never compared with one). No extra keys. |
| `ViewRef` | Either the existing native shape `{"kind":"native","out":OutputId,"ws":NativeWorkspaceId}` or `DetachedRef`. No logical arm. |

`ViewRef` is used only by `ViewAttempt.{pre,post,base}` and `FocusChange::View.native`. `MemberAttempt.WorkspaceRef` is unchanged (`logical`/`native`) and cannot accept `detached`; the existing decoder already holds identifiable invalid member rows and grants them no authority ([`world_property_attempts.rs`](../../crates/tiler-protocol/src/world_property_attempts.rs#L367-L461), [`opt_workspace_ref_or_null`](../../crates/tiler-protocol/src/world_protocol.rs#L1122-L1162)).

**Typed positions, never spelling.** A `DetachedRef` object is accepted only in a slot declared `DetachedRef` or `ViewRef`: `WindowFact.detachedSurface`, `NativeObservation.detachedCurrent`, `TransferTargetV4.detachedDestination.surface`, `ViewAttempt.{pre,post,base}`, and `FocusChange::View.native`. Every Desktop/logical identity position stays a bare id (`NativeObservation.current`, `order[]`, `occupancy[].native`, logical rings and mapping/view bindings, `AfterShownScope.ws`, `MemberAttempt.WorkspaceRef.native`): a `DetachedRef` there is a shape violation — rejected where the position is structural (`order`/`occupancy`, [`world_protocol.rs`](../../crates/tiler-protocol/src/world_protocol.rs#L3018-L3024), [L3045-L3056](../../crates/tiler-protocol/src/world_protocol.rs#L3045-L3056)) and held without authority where the row already holds malformed values (`current` at [L3026-L3037](../../crates/tiler-protocol/src/world_protocol.rs#L3026-L3037), identifiable `MemberAttempt` rows at [`world_property_attempts.rs`](../../crates/tiler-protocol/src/world_property_attempts.rs#L438-L456)). A bare string in a Desktop/logical slot is that Desktop/logical id even when its bytes equal a `DetachedRef.surface`; core never infers detachment from spelling. Producers never write a detached surface id into `order`, occupancy, a logical ring, binding, or `afterShown`, so core never learns membership or currency from a bare id.

**Observation tri-state**

`NativeObservation` keeps `out`, `order`, `occupancy`; `current` is required `ID|null`; new required keys are `currentKnown: bool` and `detachedCurrent: DetachedRef|null`.

| `currentKnown` | `current` | `detachedCurrent` | Meaning |
|---|---|---|---|
| `false` | `null` | `null` | Read gap; retain the logical view, store no physical snapshot, and prove nothing. |
| `true` | `null` | `null` | Known none of these surfaces. |
| `true` | `ID` | `null` | User Desktop; `ID` names an `order` member, with a non-member or contradictory row handled by the existing conflict/duplicate lane. |
| `true` | `null` | ref | Detached surface; `ref.out` equals the row output and the id never enters `order`/`occupancy`. |

`false` with any non-null ref, both `current` and `detachedCurrent`, or a mismatched `ref.out` holds the observation row without authority; a `DetachedRef` object in `order`/`occupancy` takes the structural rejection above.

`WindowFact` keeps its existing keys exactly (`win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`, `minimized`, `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`, `acceptsInput`, `activationInhibited`, `toolWindow`, `owned`, `transient`, `modal`, `resizable`, `above`, `below`, `owner`, `parent`, `role`, `appId`, `executable`, `processId`, `class`, `title`, `type`, `activationPolicy`, `layer`, `fullscreenOwner`, `fullscreenExitable`, `suppressionAvailable`, `visibility`) and adds required nullable `detachedSurface: DetachedRef|null`.

`detachedSurface` is required-nullable on every v4 `WindowFact`, including capability-false hosts (explicit `null`). A missing key holds the identifiable row as a `HeldWindowRow` ([`world_protocol.rs`](../../crates/tiler-protocol/src/world_protocol.rs#L2871-L2880)); it is never a benign gap. With capability false, `nativeWs` keeps its exact existing encoding and state semantics (including `""`), and `detachedSurface` must be `null`.

| `nativeWs` | `detachedSurface` | Meaning |
|---|---|---|
| any | missing | Hold the row (required-nullable violated); no window authority. |
| omitted, or `""` (legacy empty sentinel) | explicit `null` | Read gap; existing unknown semantics preserved (no Desktop receipt). |
| explicit `null` | explicit `null` | Known neither: no Desktop receipt. Non-sticky fences its scope; sticky all-desktop keeps its existing exemption ([`world_run_v4.rs`](../../crates/tiler-core/src/world_run_v4.rs#L402-L407)). |
| `ID` | explicit `null` | User Desktop membership. |
| explicit `null` | ref | Detached membership; `ref.out` equals the fact output and `fullscreen` must be `Some(true)`. |
| omitted, `""`, or `ID` | ref | Hold as invalid/conflicting; a detached arm never accompanies a Desktop or unknown arm. |

Both arms non-null, a detached ref with missing/`false` fullscreen, a mismatched output, or capability-false with a non-null detached value holds the fact without authority as a value violation; only a detached id in `order`/`occupancy` rejects structurally. More than one live lifetime may share one detached surface on the same output; membership proves neither a logical home nor which member has focus. A bare Desktop `ID` that happens to spell a `DetachedRef.surface` stays a Desktop id: the typed position, never string equality, decides.

**Grant and attempt deltas**

| Record | Keys and rules |
|---|---|
| `TransferTargetV4` | Existing `op`, `kind`, `win`, `out`, `ws`, `follow`, `held`, `cmdSeq`, `parent` unchanged (canonical decimal string or null for `cmdSeq`, opaque id or null for `parent`). Add required nullable `detachedDestination`, exactly `{"surface":DetachedRef,"member":WindowId}`. Only a standalone explicit selection may carry it, defined on core concepts with no input-device provenance: a `kind:"view"` flight minted by ordinary `SelectWorkspace` or `SelectNumbered`/`SelectRelative`/`SelectPrevious`/`SelectTrailing` ([`world_commands.rs`](../../crates/tiler-core/src/world_commands.rs#L543-L690)) with `follow:false`, `parent:null`, `cmdSeq` non-null, and `surface.out == out`; every other retained transfer carries `null`. Exclude only `SelectWorkspace` in the `plan_global_swap` branch ([L221-L541](../../crates/tiler-core/src/world_commands.rs#L221-L541)), including its empty-swap path, not ordinary GlobalUnique selections. `SwitchView` is not admissible on v4 ([`valid_command_v4`](../../crates/tiler-core/src/world_report_v4_checks.rs#L29-L35)). |
| `ViewAttempt` | Existing `kind`, `op`, `out`, `ws`, `disposition`, `pre`, `post`, `base`, `attemptMs` unchanged; `pre`/`post`/`base` are `ViewRef|null`; `attemptMs` is the canonical decimal string. Add required nullable `postMember: WindowId`, non-null only on an accepted disposition with an exact detached `post` and a grounded adapter-bracketed membership read; `null` withholds proof, and proof requires `postMember == Some(frozen intended_member)` with no later recheck. `disposition` keeps `{"kind":"called","setter":"accepted"|"refused"|"threw"}`. |
| `FocusChange::View` | Exact keys `kind`, `out`, `native`; `native` changes from an ID string to a non-null `ViewRef`, and `native.out` must equal `out`. A malformed ordered focus event rejects; a malformed grant rejects with no actuation. |
| `MemberAttempt` | Unchanged keys `kind`, `op`, `win`, `out`, `ws`, `disposition`, `pre`, `post`, `attemptMs`; `WorkspaceRef` only, never `DetachedRef`. |

The adapter validates only the wire shape. A non-view kind, `follow:true`, non-null `parent`, null `cmdSeq`, or `surface.out != out` carrying a non-null `detachedDestination` is a malformed grant and rejects with no actuation. Swap exclusion is a core emission invariant, not an adapter inference: the adapter never reconstructs command provenance from the row and never treats a swap-shaped flight as eligible by shape alone.

**Hold versus reject**

| Condition | Rule |
|---|---|
| Identifiable observation row with bad/conflicting detached fields | Hold the row, clear the affected physical/membership proof, and grant no fallback authority. Existing structural duplicate/output/order/occupancy refusals stay refusals. |
| Capability-false non-null detached on a report lane | Hold when a stable row identity exists, otherwise reject. |
| `DetachedRef` object in a structural Desktop/logical position (`order`, occupancy, logical ring/binding) | Structural contradiction: reject the row/reply and never bind it. A bare id there stays Desktop/logical by position, even on equal spelling. |
| `DetachedRef` object in a tolerant value position (`current`, a `WindowFact` value, an identifiable attempt row) | Hold the row; never reinterpret it as detached and never bind it. Unidentifiable rows still reject. |
| Detached coexisting with native current/membership, non-`true` fullscreen, or mismatched output | Conflicting observation: hold; attempt/grant: hold or reject by lane identity. |
| Malformed ordered focus event or malformed grant | Reject; no actuation. |
| Bad/unknown matching attempt | Hold, never retry. |
| Explicit refusal | Proves only with an unchanged readable preimage; otherwise holds as ambiguous. |
| Duplicate view attempt `(op,out)` | Reject the duplicate lane row. |

## Assumptions

- **ASSUMPTION:** the host may rekey a fullscreen surface for the same lifetime, and may reorder Space/display/foreground notifications arbitrarily. Changed surfaces are always supersession; only same-coherent-report or adapter-bracketed observations prove. Two round trips do not establish a general lifetime or ordering guarantee.
- **ASSUMPTION:** Split View may place several logical homes on one detached surface and may focus a peer rather than the intended member. Membership and physical surface never prove which member has focus; the cursor guard uses only a fresh actual foreground observation.
- **ASSUMPTION:** the adapter truthfully emits `postMember` only when it read that membership between two exact target samples. The wire carries the ID and timestamp, not the two samples, so the core cannot re-derive that bracket from the bare ID; `postMember:null` is a withheld proof, and a mismatched or malformed value holds. There is no attestation field.
- **ASSUMPTION:** `DetachedRef.surface` and `NativeWorkspaceId` are disjoint namespaces even when their opaque strings collide; typed positions, never byte equality, decide membership and currency.
- **ASSUMPTION:** a host foreground reading is trustworthy only in a coherent report with readable physical current; unknown/gapped reads never ground a release. An accepted post alone is not foreground proof.

Evidence gaps, not assumptions: same-SID access while away and after return is proved only on the tested single-window layout; lifetime rekey, notification ordering, Split View membership, unassisted Desktop focus, and the product redundant/away legs remain unprobed.

## Prospective tests

| Area | Narrow assertions |
|---|---|
| Wire | Missing `detachedFullscreenView` rejects; non-host `true` rejects; `true` with a non-per-output selector scope rejects; missing `detachedSurface` holds; capability-false non-null detached holds/rejects; tri-state and WindowFact tables round-trip; conflicting non-null arms on an identifiable observation hold; a `DetachedRef` in structural `order`/`occupancy` rejects; a `DetachedRef` in a tolerant Desktop/logical value holds without authority; equal-spelling bare ids stay Desktop; `(op,out)` duplicate rejects; `MemberAttempt` with detached holds and grants no authority. |
| Grant eligibility | Ordinary `SelectWorkspace` can emit a detached destination, including the non-swapping GlobalUnique fallthrough; neither the empty nor the migration-bearing GlobalUnique swap branch emits one. |
| Settlement A | Fresh exact detached current plus grounded member completes focusless with `Unproven`, no FocusRequest/arrival/setter; gap, Desktop, wrong surface, and wrong member hold. |
| Settlement B | Accepted exact post plus `postMember == Some(intended_member)` proves; refused, non-detached post, wrong member, `postMember:null`, or malformed row hold; no retry; a latched proof survives a later non-target membership read without recheck. |
| Settlement scopes | A key-output settled/refused verdict retires only that output's obligation; a sibling stays pending; a typed refusal compares the exact preimage `ViewRef`; a Desktop logical echo never settles a detached output. |
| Proven then abandoned | Accepted post proves, then a known Desktop/other-surface departure adopts once with no replay and focus still `Unproven`; gap does not supersede. |
| Holds | Detached retained home is not fenced and ordinary siblings tile; unknown membership still fences; born fullscreen stays unhomed with own writes fenced and no scope hold; fallback never invents a home. |
| Shared flights | One shared op/history, per-output destinations, sibling proof isolation, terminal waits all planned siblings, latest-per-output supersession, lost output retires only its flight, common-newer adoption versus divergent-sibling retention. |
| Redundant/no-op | Exact detached current same home sends no View/focus; ambiguous same-workspace never becomes a Desktop write. |
| Cursor/guard | Cursor clears immediately; each guarded output blocks its own implicit subjects; a newer focus decision or an unguarded output's foreground is not blocked; gaps do not release; each of the four releases fires independently; accepted post alone does not restore; Split View peer only when eligible/homed. |
| Focus supersession | Typed `last_views` ignores gaps; different typed ref hides only with readable expectation; same expected does not; detached adoption only with unambiguous home and no owning flight. |
| Authority | Detached refs never enter user Desktop current/order/occupancy/ring/binding; a detached current never implies the covered Desktop is current or visible. No write is authorized to the detached lifetime or the covered Desktop by the override or an automatic action; explicit R14 commands keep R14. |

## Host evidence

| Decisive host fact | Result |
|---|---|
| Exact fixture lifetime survives both round trips; same detached SID present while away and after each return | Proved on the tested layout. |
| Four legs reach exact SLS current without bounce; other display current preserved | Proved. |
| No fullscreen-member writes during switching; only entry and teardown state requests | Proved. |
| Teardown restores original currents/order and leaves no owned fixture lifetime or Space | Proved on the tested layout. |
| Unassisted Desktop focus after pinned Mission Control | Not proved; the harness focused the owned Desktop sibling only after measuring away. |
| General Space-notification order, lifetime rekey, Split View | Unprobed. |
| Product redundant and away legs | Signed rerun observed the redundant leg's pre-dispatch refusal (`fresh row unprovable`), not the redundant-target refusal; the same guard covers an away target, but the away chord was never posted; no product route promoted. |

### Signed product evidence and live acceptance sequence

The signed SIP-on rerun makes the product blocker explicit. The product's user-Space row drops non-User Spaces but keeps the raw current SID ([`workspace_select.rs`](../../crates/tiler-macos/src/workspace_select.rs#L82-L99)); validation requires `current ∈ spaces` ([L21-L36](../../crates/tiler-macos/src/workspace_select.rs#L21-L36)); and the ordinary owner refuses before any native dispatch decision ([L930-L934](../../crates/tiler-macos/src/workspace_select.rs#L930-L934)). The guard refuses any select while a detached current persists, so it covers both a redundant target and an away target; the rerun observed the redundant instance as `fresh row unprovable`, and the away chord was never posted. Product switch-away requires an explicit detached current in the product's snapshot/owner — a product change, not a harness or oracle fix. The generic refusal does not identify which prerequisite failed and says nothing about how an unposted away chord would behave.

Once the product models explicit detached current, live acceptance is an ordered three-leg sequence, and only then is the route promoted:

1. **Redundant refusal.** The redundant select is refused *as redundant* with no native re-entry, and the exact detached current is read unchanged. The generic `fresh row unprovable` line does not satisfy this leg.
2. **Armed away.** An armed away select to the covered Desktop is accepted, with exact Desktop current read.
3. **Return.** The return select restores the exact detached current and the remembered member.

Core emits no View for a redundant detached select, so leg 1 must be decided by the adapter's pre-dispatch admission: there is no core flight to refuse. It is adapter acceptance evidence, not a core flight. Split View remains unprobed, and the fixture has no native Split View affordance; the probe's conservative contract (read gaps vs known absence, bracketed readback, a changed SID is always supersession) still holds.

Operational detail, timing samples, caveats, and recovery: [macOS probe record](macos-fullscreen-probe.md).

## Pass 4 review response

Pass 4 approved 2026-11-28, with two implementation notes: exclude only planned GlobalUnique swaps, and hold conflicting observation arms while rejecting detached refs in structural order/occupancy.

All four must-fix items and four clarifications are applied above. The code claims were checked against the pre-amendment implementation; the corrections and qualifications are below.

- **Swap range.** The review cites `world_commands.rs ~450-541`. The `plan_global_swap` branch also mints an identically shaped view flight on the empty-swap path at [`world_commands.rs:272-305`](../../crates/tiler-core/src/world_commands.rs#L272-L305), outside that range. Exclude the full planned-swap branch at [L221-L541](../../crates/tiler-core/src/world_commands.rs#L221-L541), not the ordinary `SelectWorkspace` fallthrough at [L543](../../crates/tiler-core/src/world_commands.rs#L543).
- **"Pager clicks (SelectWorkspace with renderer_input)".** `renderer_input` is report-level drawn-revision/target evidence, not command provenance ([world_report.rs:1519-1523](../../crates/tiler-core/src/world_report.rs#L1519-L1523), [world_protocol.rs:4125-4133](../../crates/tiler-protocol/src/world_protocol.rs#L4125-L4133)); `WorldCommand::Select*` carries no input field ([world_report.rs:2160-2201](../../crates/tiler-core/src/world_report.rs#L2160-L2201)). The provenance-free predicate is right, and `renderer_input` is not and must not become a grant condition.
- **"A detached id in a workspace binding rejects".** Identifiable `MemberAttempt` rows with an invalid `WorkspaceRef` **hold**, not reject ([world_property_attempts.rs:438-456](../../crates/tiler-protocol/src/world_property_attempts.rs#L438-L456); [`opt_workspace_ref_or_null`](../../crates/tiler-protocol/src/world_protocol.rs#L1122-L1162)), and a bad-shaped `NativeObservation.current` also holds ([world_protocol.rs:3026-3037](../../crates/tiler-protocol/src/world_protocol.rs#L3026-L3037)). Only structural `order`/occupancy shapes reject ([L3019](../../crates/tiler-protocol/src/world_protocol.rs#L3019), [L3055](../../crates/tiler-protocol/src/world_protocol.rs#L3055)). The typed-position rule above is split between reject and hold to match.
- **Consequence the review leaves implicit.** Because a missing required-nullable key holds ([world_protocol.rs:2871-2880](../../crates/tiler-protocol/src/world_protocol.rs#L2871-L2880)), every v4 `WindowFact` producer — including capability-false KDE and Windows — must emit `detachedSurface:null`, and every active fixture must update. Their "no behavior change" is behavioral, not wire-shape.
- **Existing current ambiguity.** A missing `current` holds the row ([world_protocol.rs:3025-3039](../../crates/tiler-protocol/src/world_protocol.rs#L3025-L3039)); it is not a valid encoding for a read gap. The ambiguity is in the valid explicit `null`, used by macOS for both non-Desktop and unreadable current ([world_builder.rs:492-500](../../crates/tiler-macos/src/world_builder.rs#L492-L500)).

## Alternatives

- **Reuse `focus_request.target` with adapter-only switching:** finds an exact candidate but cannot settle a separate native scene without inventing focus evidence or an arrival receipt.
- **Report the user Desktop as current or map the fullscreen SID onto the logical home:** rejected; invents physical currency, conflates transient and workspace identity, and risks granting writes to the covered Desktop.
- **Treat fullscreen as an ordinary user Space:** rejected; changes ordinal/history/creation semantics and gives a host scene a logical identity it does not own.
- **Only add a fullscreen SID to `after_shown`:** insufficient; a reply hint cannot prove operation-specific physical arrival or reconcile outside switches.
- **A separate singleton destination or second selection terminal:** rejected by Pass 2; it breaks Shared/multi-output and duplicates the existing proof engine.

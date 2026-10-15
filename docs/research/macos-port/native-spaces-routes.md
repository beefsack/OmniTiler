# Native Spaces: non-SA routes and bounded probe plan

Source survey, 2026-10-14. Policy: native Spaces first, yabai primary reference,
tier 2, SIP enabled, no scripting addition (SA) or Dock injection; macOS 15
floor. **Native window moves are not established impossible:** yabai 7.1.25
claims SIP-on recovery, with a reported bridged-operation success on Tahoe.
Enumeration/membership and bounded mutations now have
[owned-fixture evidence][phase0]. The [live verdict below](#live-verdict-2026-10-16)
establishes selection and lifecycle routes, but **no ordinary-window transfer
route is yet accepted**. Source routing and observed effects remain separate.

FACT = inspected pinned source; CLAIM = upstream report, not our experiment;
EVIDENCE = our observed readback; UNKNOWN = not established. Source was read
from existing checkouts; only the public issue [#2788][move-issue] was fetched.
Pins match the [availability snapshot][pins], including its newer Amethyst,
rift, glide and paneru pins; historical plan citations are not advanced.

## Route classification

“No” in the SA column describes the particular route, not every yabai branch.
Public AX UI automation and synthetic system input are tier-2 candidates;
visible transitions, scope and unproven recovery constrain their probes.
AppleScript/System Events is an AX-automation wrapper, not another native API.

| Operation | Candidate route / yabai SA routing (FACT) | SIP-on status, version caveat and evidence |
| --- | --- | --- |
| Window → Space, same display | Bridged `SLSBridgedMoveWindowsToManagedSpaceOperation.initWithWindows:spaceID:` + `SLSPerformAsynchronousBridgedWindowManagementOperation`; **no SA**, yabai first branch [Y1] | CLAIM: [#2788][move-issue] reports 26.4/26.4.1 success without disabling SIP; 7.1.25 changelog says moves work again [Y3]. Not proof for 15 or every newer family. EVIDENCE: dispatcher absent, class present; no dispatch. |
| Window → Space, same display | `SLSMoveWindowsToManagedSpace`; **no SA** on old direct branch [Y1] | FACT: yabai bypasses this branch on 12.7+, 13.6+, 14.5+, and every major ≥15 [Y2]; Hammerspoon independently changes route at 14.5 [H3]. Diagnostic direct call is not the source-recommended floor-15 route; it returns void. EVIDENCE: one dispatch, membership unchanged through delayed readback; no rejection status. |
| Window → Space, same display | Compat ID + `SLSSetWindowListWorkspace`; yabai **tries SA first**, then non-SA compat fallback [Y1]; Hammerspoon uses compat directly [H1] | FACT: set destination compat tag → set window workspace → reset tag to zero, without exact tag-preimage restoration or effect readback. These sources establish a workaround, not universal efficacy. The 7.1.25 recovery issue specifically identifies bridged operation; do not transfer that runtime claim to compat. Our setters remain disabled. |
| Window → Space, other display | Same three move helpers; yabai send path passes destination SID [Y4] | FACT: no separate cross-display authority API in this send path. UNKNOWN: membership, geometry/display adoption and focus side effects. Same-display success is insufficient evidence. Requires later distinct owned-display leg. |
| Window → Space, same/other display | SA client [Y15] → Dock payload `SLSMoveWindowsToManagedSpace` [Y12] | Excluded. Calling from Dock's connection is not evidence that an ordinary-process call is entitled to the same effect. |
| Window → Space, same/other display | Silica/Amethyst titlebar mouse-down + posted system Space-switch chord + delayed mouse-up [A1] | FACT: non-SA synthetic drag, not direct AX Space setter; visible and timing-sensitive. Silica **enables a disabled shortcut** before posting [A2]; that settings write must be omitted. Disabled/missing target chord blocks this route. Cross-display Amethyst additionally adjusts the frame [A3]. |
| Window → Space | `SLSProcessAssignToSpace` [Y6]; AX position/size, raise, app activation | Process assignment is process-wide, not a proven per-window move/inverse; do not substitute it. Plain AX frame/raise has no Space-assignment attribute in surveyed code. Mission Control window-thumbnail drag is a UI/manual candidate, not a surveyed low-level setter. |
| Switch/focus Space | `SLSShowSpaces`/`SLSHideSpaces` + `SLSManagedDisplaySetCurrentSpace` and Dock state update, **inside SA** [Y5]; yabai focus tries SA first [Y7] | Excluded direct SA route. These calls alone outside Dock are not a proven activation route. `space_manager_switch_space` is a different operation: same-display SA focus, cross-display window-set swap [Y7], not an ordinary desktop selection. |
| Switch/focus Space | Non-SA synthesized Dock-swipe; yabai fallback [Y7], rift directional implementation [R1] | CLAIM: yabai 7.1.19/20 SIP-on focus [Y3]. EVIDENCE: same-display pair switched both ways; no warp/click. Cross-display route expands scope. Rift has a separate ≥27 payload branch: source version coupling, not evidence of 27 acceptance. |
| Switch/focus Space | Hammerspoon Mission Control + AX `gotoSpace`/`AXPress`; System Events equivalent [H2] | FACT: public AX, no SA, visible UI; index must be freshly joined to target SID. EVIDENCE: exact same-display current-SID round trips through public-app toggle + Dock AX, without System Events. Visual animation/jank unobserved. |
| Switch/focus Space | Synthetic enabled Ctrl+Left/Right or Desktop-N chord; Silica resolves symbolic ID `118+N−1` [A2] | No SA. Arrow route is relative; digit route is index-bound. EVIDENCE: corroborated enabled Ctrl+Right candidate had no effect after an exact-helper pair. Desktop-N unresolved, not posted. Silica's enable-if-disabled behavior is not adopted. No physical delivery/collision evidence implied. |
| Create Space | Yabai `scripting_addition_create_space`: Dock `ManagedSpace` object + internal add function [Y5], [Y8], [Y11] | SA-only yabai command; excluded. No proven ordinary-process SkyLight equivalent in survey. |
| Create Space | Hammerspoon Mission Control `mc.spaces.add` AXPress [H2]; System Events or physical + button | FACT: non-SA visible UI route. EVIDENCE: one temporary user Space created by AX, exact ordered before/after difference, never a guessed desktop number. |
| Destroy Space | Yabai SA internal remove function [Y5], [Y8], [Y11]; Hammerspoon AX `AXRemoveDesktop` [H2]; physical thumbnail close button | Yabai command SA-only; AX alternative non-SA. EVIDENCE: uniquely created inactive, user-window-empty SID removed once and exact original ordered topology/current restored. System backing chrome exclusions were identity-checked. |
| Move Space between displays | Yabai SA internal move function + desktop-picture-manager bookkeeping [Y5], [Y9], [Y11] | SA-only in yabai; no surveyed non-SA implementation. Mission Control thumbnail drag is an unverified manual/UI candidate, not part of the initial probe. Keyboard/AX window focus does not move a Space. |
| Focus window on another Space | Yabai default SLPS front-process/make-key + AXRaise [Y10], plus optional `SLSSpaceSetFrontPSN` and gesture event handling [Y13]; send-path interaction [Y4] | Default window focus **not SA** (SA focus in disabled branch). CLAIM: animation suppression 7.1.21–23 [Y3]. Cross-Space transfer/animation differs from same-Space focus; issue #2788 separately warns about send-and-follow interaction. Require Space-current + AX focused WID/PID + CG owner + NSWorkspace agreement, not status 0. |
| Focus window on another Space | AXRaise + app activation, optionally followed by explicit UI/chord switch | No SA; OS/app settings can switch Spaces or bring other same-app windows forward. No general exact cross-Space result proved by surveyed source. Our latest SLPS fixture has incomplete full-agreement evidence [phase0]. |
| Window-to-Space membership | `SLSCopySpacesForWindows(cid,0x7,[wid])`, joined to owned CG PID/WID [H1], [R2]; private ABI [Y14] | No SA. EVIDENCE: works through Child-held helper [phase0]. Sticky windows may have multiple memberships; fullscreen/system transitions need type checks. Readback must be exact `[target]`, not merely contain target. |

### What other non-SA tools do

| Tool / source pin | Actual alternative (FACT unless marked CLAIM) |
| --- | --- |
| AeroSpace `74a1bf17e82d70e0a21945ba04bb4f590bf19f83` | AX corner parking for virtual workspaces, not native Space moves: [hide/restore](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/tree/MacWindow.swift#L123-L183). |
| Amethyst `5cfe2fe12b1a933139010da5760aaa7e44033e9b`, Silica `ebead82a28486e7719b7ed69633ecfed8a4fe2d2` | Native Spaces via drag+resolved shortcut [A1], [A2], [A3]; Amethyst's direct `move(toSpace: CGSSpaceID)` is [empty](https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Model/Window.swift#L392-L397), unlike its indexed mover. |
| rift `8114ccc534b5c9a7e6a77ce27568d8739be9216f` | Queries native membership [R2], directional gesture selection [R1]; no native window-move binding found. Its virtual workspace machinery is not evidence of native mutation authority. |
| glide `ef3b30f88445fe4b517eb4c60023011f6d3725dd` | Per-native-Space integration and [SLPS focus](https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/window_server.rs#L215-L284); no native window-move implementation found. |
| paneru `8a213313538a352537b26e25131cfe75f1e00724` | Virtual strips/slivers and [SLPS focus](https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/manager/windows.rs#L906-L960); [SkyLight queries](https://github.com/karinushka/paneru/blob/8a213313538a352537b26e25131cfe75f1e00724/src/manager/skylight.rs#L144-L256), not native move. |
| FlashSpace `968015308d8d284a98831f90f365538a63456ee9` | App-assignment workspaces using [hide/show/activate](https://github.com/wojciech-kulik/FlashSpace/blob/968015308d8d284a98831f90f365538a63456ee9/FlashSpace/Features/Workspaces/WorkspaceManager.swift#L180-L230), not native per-window Spaces. |
| komorebi-for-mac `bdf10452f10ebf15ec1978cb05d0aac79ef5611f` | [Corner parking with saved geometry](https://github.com/LGUG2Z/komorebi-for-mac/blob/bdf10452f10ebf15ec1978cb05d0aac79ef5611f/komorebi/src/window.rs#L506-L577). |
| Hammerspoon `23e387e2805a9890066366e0ac96c71b27f0cfd5` | Private membership/move dispatch [H1] plus animated AX Mission Control create/goto/remove [H2]; dispatch returns success without move-effect readback. |

The 14.5+/15 workaround is not a blanket private-API/SIP failure. Nor does
upstream Tahoe success certify the 15 floor. Recheck each supported release
family; keep floor and newer-family results separate. The 2026-10-15 decision
retains native Spaces and tier 2: an unresolved transfer route blocks the MVP;
it does not select parking or a SIP-policy change.

## Live verdict — 2026-10-16

Physical native graphical-session evidence on a release newer than the macOS 15
floor, SIP enabled, persistent signed bundle, observed Accessibility trust. One
temporary user Space was created, identified by exact ordered per-display SID
difference, used throughout the comparisons, and removed. All windows affected
by move/focus/input experiments were disposable helpers. This is neither
ordinary-app acceptance nor minimum-supported-release acceptance.

**Phase 0 Spaces verdict: selection and create/remove are viable; general window
transfer remains unresolved, so the workspace MVP is blocked.** The direct move
had observed no effect; bridge dispatch was unavailable; other move candidates
were ineligible or lacked exact restoration. This does **not** establish that
all SIP-enabled native window moves are impossible, or that SIP caused any result.

| Route | Observed result | Latency and rejection evidence |
| --- | --- | --- |
| MC AX create | Works: exactly one new user SID, preexisting order/current unchanged | 448 ms; AX action accepted and SID effect observed |
| MC AX goto / return | Works: exact current-SID readbacks both ways; owned membership retained | Approximately 628–660 ms per leg, including MC traversal and settled readback; no explicit rejection |
| MC AX remove | Works: created SID inactive, user-window-empty, then absent; original ordered layout/current restored | 515 ms; one remove dispatch, effect verified |
| Public AppKit helper self-join | Works **only as an independent recovery fixture**: owned window `[O] → [T] → [O]`, exact frame and collection-behavior restoration | 112–317 ms self-action reports, followed by membership readbacks; no general foreign-window authority implied |
| Direct `SLSMoveWindowsToManagedSpace` | **Observed no effect**: positively dispatched once; immediate and delayed membership stayed exactly `[O]`, current stayed O, frame unchanged | Mover child 180 ms; delayed observation 3,139 ms over a 3-second poll window. Void API, no rejection status; SIP cause UNKNOWN. No no-op inverse dispatched |
| Bridged asynchronous move | Unavailable: operation class present, dispatcher function absent; no dispatch | N/A; symbol availability, **not rejection** |
| Compat ID + workspace setter | Blocked: exact compat-tag preimage/readback/restore remains unproven; no setters called | N/A; safety prerequisite, **not rejection** |
| Silica drag + Desktop-N chord | Ineligible: outbound and return Desktop-N bindings unresolved; private resolver header absent from surveyed source | N/A; no mouse-down, no shortcut enabling, **not rejection** |
| Enabled Ctrl+Right switch candidate | **Observed no effect**: effective Carbon table corroborated the enabled chord; exact-helper down/up delivered, current stayed O; no inverse/retry | 164 ms **focus + posting only**, then bounded 3-second current poll; no authority error. Chord-to-action attribution remains unproved; not physical-input evidence |
| Same-display yabai Dock-swipe | Works: began/ended pairs switched O→T and T→O, memberships unchanged, no warp/click | 15/131 ms **focus + posting only**; exact current effects within the 3-second polling window. End-to-end switch latency was not captured |
| Desktop-N switch chord | Ineligible: no resolved enabled outbound/return binding; Carbon entries have no symbolic IDs | N/A; no post, **not rejection** |
| Cross-display window/Space moves; cross-Space ordinary-app focus | Not tested by this single-display fixture | TBD; no non-SA Space-to-display route established |

**Visual jank is unobserved for every route.** MC UI is necessarily exposed by
the AX route; neither posted-event timing nor current-SID readback proves whether
the desktop transition animated or looked smooth. Gesture payloads remain
version-coupled; the comparison refuses the newer augmented-payload family.

### Recovery, restoration and limitations

- Independent public helper recovery passed before direct SLS dispatch, on the
  same helper. The primary direct mover ran as a separate short-lived process
  and exited while the exact helper Child remained held. Off-current title
  withholding is handled by the documented Child/pipe-nonce/executable/CG join;
  separate controllers without that Child still require current nonce proof.
- The first integrated `mc-lifecycle` run incorrectly checked a Space switch
  immediately. A later snapshot showed the asynchronous effect. Corrected
  bounded **readback polling**, without redispatch, proved return; the corrected
  integrated create/goto/remove command has not itself had a fresh full pass.
- AX error decoding had omitted two SDK error constants, misnaming `NoValue`.
  Maintained-binding regression coverage now pins the numeric/name mapping.
- Removal initially parked on system backing windows. Fresh user-window
  emptiness now records narrowly verified compositor and zero-area, offscreen,
  all-user-Space accessibility chrome exclusions. A versioned WindowServer
  executable alias is canonicalized to the same system file; unreadable or
  different identity still blocks. Ordinary sticky windows and Ghostty block.
- The switch comparison did not naturally restore foreground focus. A single
  identity-verified public app activation restored the captured Ghostty target,
  with full selected-oracle agreement and unchanged frame/membership; no
  keystrokes, SLPS events, AXRaise or window setters targeted it. One offscreen,
  non-Space Ghostty chrome surface self-resized during setup. Ordinary document
  frames/memberships were unchanged; the chrome was not written back.
- Final B2 equals B0 for ordered per-display SIDs/current, display topology,
  separate-Spaces state and shortcut overrides. Temporary SID absent and with
  no members; helpers/probes/taps/display-sleep assertions released and reaped;
  Ghostty frontmost by the full focus oracle. No native preferences changed.

### Exact continuation

Find and review a **safe ordinary-window transfer candidate with exact inverse**:
first investigate an authoritative compat-tag getter/preimage, or a release with
the bridged dispatcher actually present. The drag alternative still needs
resolved already-enabled Desktop-N bindings and independently recoverable input.
Do not repeat the direct no-effect call without a changed hypothesis. Native
per-display mapping, numbered/relative select and send/follow/stay, trailing
creation and background-frame actuation remain pending that transfer gate.

## Read-only Mission Control binding map

Read overrides with `defaults read com.apple.symbolichotkeys AppleSymbolicHotKeys`.
Standard parameters are character/keycode/CG modifier mask (Ctrl `0x40000`,
Shift `0x20000`, Option `0x80000`, Command `0x100000`). **Missing `value` or ID
does not mean disabled**: preferences are incomplete overrides, not the
effective hotkey table. Keep actual personal bindings outside repository records.

| Conventional symbolic IDs to inspect | Meaning / expected default chord | How to establish the actual binding |
| --- | --- | --- |
| 32 / 34 | Mission Control / slow variant; Ctrl+Up / Ctrl+Shift+Up | If absent, inspect effective system chords and read the System Settings label. This conventional attribution is a lookup hypothesis, not verified ID metadata from the surveyed clones. |
| 33 / 35 | Application windows / slow variant; Ctrl+Down / Ctrl+Shift+Down | Same; do not equate absence with inactivity. |
| 79 / 80 | Previous Space / slow variant; Ctrl+Left / Ctrl+Shift+Left | Enabled-without-value needs effective-chord corroboration. |
| 81 / 82 | Next Space / slow variant; Ctrl+Right / Ctrl+Shift+Right | Same. |
| 118–133 | Switch to Desktop 1–16 | Silica explicitly derives these IDs [A2]. Read actual key/modifiers, not assumed Ctrl+digit; reject disabled or unresolved targets. |
| 60 / 61 | Previous input source / next source; commonly Ctrl+Space / Ctrl+Option+Space | Decode explicit values; relevant neighbouring Ctrl collisions, not Space switching. |

Public Carbon `CopySymbolicHotKeys` gives effective **chord/enabled** dictionaries,
but no symbolic IDs. Hammerspoon [matches chords and strips the automatic Fn
bit](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/hotkey/libhotkey.m#L208-L241).
Its modifiers are **Carbon**, not CG: Ctrl `0x1000`, Shift `0x200`, Option
`0x800`, Command `0x100`; `0x20000` here is the automatic Fn bit. Array index
is not symbolic ID. A read-only public-table query can corroborate collision
chords, but cannot certify that a chord activates Desktop N.

Silica reads IDs through `CGSGetSymbolicHotKeyValue`/
`CGSIsSymbolicHotKeyEnabled`; its private-header submodule is unavailable in
the surveyed checkout. No guessed ABI resolver was added. Effective chord
observation and ID attribution must stay separate; unresolved Desktop-N
bindings block the digit fallback, rather than triggering an enable/write.
Settings reads are not delivery, interception, suppression or physical evidence.

## Minimal live sequence (initial readiness plan)

The verdict above supersedes unimplemented/physical-only setup statements in
this initial plan. Reviewed MC AX automation and public owned-helper self-join
now provide the measured setup and independent restore paths. Preserve its
identity, exact-readback and restoration requirements for subsequent experiments.

Follow [live-macos-testing.md][live]. Use one identified disposable window and
one newly created **user** Space on the same display; do not touch Ghostty or
any other controlling terminal. No display rearrangement, fullscreen Spaces,
SA, shortcut changes, or unrelated windows. UI actions below may initially be
user-performed physical Mission Control actions; the equivalent AX implementation
is not present in the probe. A plan must not be reported as an existing recovery
command. Keep run-specific identities/preimages in the ephemeral test record.

1. **Baseline/preflight.** Rebuild/sign/verify the stable bundle; record source,
   executable hashes, bundle/requirement and arguments per the guide. Require
   unlocked ordinary session, LaunchServices `ax_trusted:true`, Secure Input
   inactive and posting consent for any synthetic leg. Record SIP read-only.
   Establish the disposable helper through
   `mise exec -- just launch -- probe-spawn --seconds 120` and its fresh identity
   joins; the user physically gives it focus. Finish one route pair and restore
   before its finite lifetime expires; insufficient remaining time blocks a
   new action. Record pre-fixture focus separately
   for user-owned physical restoration, never an agent write to the terminal.
   Run `mise exec -- just preflight-app` and
   `mise exec -- just launch -- spaces-diagnose`. Snapshot B0: ordered user-SID
   list and current SID **per display**, separate-Spaces mode, topology,
   frontmost/AX focus and relevant window identity/geometry/visibility. Preserve
   display associations; flattened sorted IDs cannot prove ordered restoration.
   Capture hotkey overrides plus effective chords. Never change settings to fit
   the fixture. The user puts focus on the owned fixture before any synthetic
   leg; all synthetic down/up pairs target only that verified fixture.
2. **Create exactly one temporary Space.** Open Mission Control physically;
   on the chosen display press its + once (future AX: `mc.spaces.add` AXPress).
   Close Mission Control. Read SLS again: B1 must differ from B0 by exactly one
   new user SID T on that display, preserving all previous order/memberships
   and current SID O. Stop on any other difference. Never identify T solely
   as “Desktop 2”, a thumbnail coordinate, or the largest SID.
3. **Independent recovery first.** Revalidate the existing owned-helper
   nonce/CG/AX join flow; capture fresh PID/start/executable/nonce, WID,
   membership `[O]`, frame, visibility and owned focus. Prove graceful stop and
   exact-identity stop. The user can physically drag **only the owned window**
   thumbnail O→T→O in Mission Control, with membership and frame readbacks at
   each boundary. Repeat return through an independent control path after the
   primary mover process exits, keeping the fixture owner alive. This manual
   restore is the initial independent fallback; do not claim the untested SLS
   inverse proves recovery. If this proof is unavailable, no API move follows.
   Space removal is not a substitute for verified window restoration.
4. **Move/back route order.** Fresh symbols/type/session/identity checks first:
   (a) bridged operation only if available **and** a reviewed bounded dispatch,
   asynchronous readback and independent recovery implementation exists;
   otherwise record unavailable/unimplemented, no dispatch. (b) One direct-SLS
   diagnostic round trip, despite upstream's ≥15 bypass, after step 3:

   ```sh
   mise exec -- just launch -- spaces-move --pid "$PID" --nonce "$NONCE" --wid "$WID" --space "$T" --allow-untested
   mise exec -- just launch -- spaces-diagnose --pid "$PID" --nonce "$NONCE" --wid "$WID"
   # Only if fresh identity/membership permit restoration:
   mise exec -- just launch -- spaces-restore --pid "$PID" --nonce "$NONCE" --wid "$WID" --space "$O" --allow-untested
   ```

   Inspect JSON, not wrapper exit alone. At each boundary poll read-only, at
   most 3 seconds, for exact `[T]`/`[O]`, current SID, CG/AX identity and frame;
   record dispatch separately. Existing command reads immediately, so preserve
   its report and collect delayed diagnosis separately. On no effect `[O]`,
   do not run a no-op inverse or retry. On ambiguity use the independent path,
   verify restoration, then stop. (c) **Compat remains blocked** until exact
   compat-tag preimage/readback/independent restoration exists; never reset an
   unknown tag to zero. (d) Silica-style drag+digit is a later eligible comparison
   only when both destination and return chords are already enabled, have SID
   mappings and a reviewed owned-window input guard; otherwise skip. No blind
   mouse-down or shortcut enabling. Cross-display moves need a later distinct
   recovery leg; one temporary same-display Space does not cover them.
5. **Switch/back separately.** First establish physical Mission Control O→T→O
   current-SID readbacks as independent recovery. Compare only implemented and
   scoped routes, returning to O after each: AX Mission Control goto (fresh
   SID→display/index join); then enabled Desktop-N down/up chords; then relative
   Ctrl+arrow only if O and T are adjacent and the chord is enabled; finally a
   reviewed same-display Dock-swipe route. Stop after one failed/ambiguous pair.
   No cross-display cursor warp/click. Require current `[T]` then `[O]` within
   3 seconds; owned window membership must remain unchanged, and owned focus
   transfer/return must agree across NSWorkspace, AX and CG. No tap needed to
   test a posted chord. Carbon/tap collision legs are separate, not implied by
   this switch result. Current `spaces-switch` only reports unsupported.
6. **Remove only T and compare.** Restore `[O]`, exact geometry/visibility and
   baseline focus first; confirm T inactive, still the uniquely created SID,
   and no unrelated windows assigned there. Re-open Mission Control, freshly
   map T to the display's thumbnail; use its close control (future AX:
   `AXRemoveDesktop`). Never remove active/last Space or guess a stale index.
   Stop/reap only the verified helper; check process/record/CG residue. Snapshot
   B2 must equal B0 in ordered per-display SIDs/current, topology, separate-Spaces
   mode and shortcut overrides. Compare the owned-window preimage before its
   exit; after exit require owned-window absence. User restores any original
   protected-terminal focus physically. Uncertain restoration blocks cleanup
   expansion: preserve residue/evidence and report exact remaining state.

### Implementation readiness and verdict oracle

`spaces-session` remains read/lifecycle-only even with `--allow-untested`.
`spaces-move`/`spaces-restore` require nonce/record and fresh CG ownership,
one-Space preimage, known target, **user source/target types**, readable
non-loginwindow frontmost context, **unambiguous same-display mapping** and
explicit opt-in. Cross-display move/restore are refused pending their distinct
recovery leg. Session observation is
not an atomic lock detector; recheck at each action. No new write route is
enabled by this preparation. Bridged dispatch, AX MC and input writers still
need bounded recovery implementation before automation; the physical setup and
independent restore must be demonstrated first. Do not launch the pending focus
fixture with a protected terminal as a writable restore target.

An omitted/disabled symbol, consent shortfall, refused topology, unsupported
CLI, or no dispatch is **not SIP rejection**. A dispatched void API with stable
unchanged membership is **observed no effect**, not a causal SIP finding.
Preserve exact OS errors if a route returns them. “SA unavailable” can describe
missing excluded infrastructure; “SIP caused rejection” needs an explicit
authority diagnostic identifying that cause, not a generic failure. Without
such evidence, causal attribution remains inferred/UNKNOWN; no reduced-SIP
comparison is run. Accept native move/select only after observed effects and
independent exact restoration, separately for the floor and newer families.

[phase0]: ../../changes/macos-phase-0.md
[pins]: prior-art.md#source-checkout-snapshot-2026-10-14
[live]: ../../live-macos-testing.md
[move-issue]: https://github.com/asmvik/yabai/issues/2788
[Y1]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
[Y2]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/workspace.m#L17-L26
[Y3]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/CHANGELOG.md#L12-L43
[Y4]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L2092-L2122
[Y5]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/osax/payload.m#L460-L593
[Y6]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1074-L1077
[Y7]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035
[Y8]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1037-L1072
[Y9]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L905-L923
[Y10]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1269-L1335
[Y11]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L442-L482
[Y12]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/osax/payload.m#L906-L930
[Y13]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/event_loop.c#L359-L375
[Y14]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h
[Y15]: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L604-L621
[H1]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L116-L199
[H2]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L689-L882
[H3]: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L16-L21
[A1]: https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SIWindow.m#L210-L262
[A2]: https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SISystemWideElement.m#L29-L65
[A3]: https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Managers/WindowManager.swift#L949-L974
[R1]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/space_switch.rs#L170-L223
[R2]: https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/window_server.rs#L426-L476

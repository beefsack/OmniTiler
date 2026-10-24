# Backlog archive

Historical bodies removed from the active backlog during the 2026-10-24
backlog consolidation (repo main `a294d62d`). Each section below is the
verbatim Open-work entry (or entry group) as it stood at archive time,
retained as delivery evidence and planning history.

- Active work lives in [backlog.md](backlog.md) (`## Open work`, `## Future`,
  `## Pending live checks`, `## Open user decisions`, `## Known issues and risks`).
- Status words like "pending" inside archived bodies are historical: they
  describe the state at archive time, not current work. Consult the linked
  change records and the active backlog for what remains.
- Decisions cited here remain in [decisions](decisions.md); nothing in this
  archive re-decides them.

## Windows dual-output run history (condensed 2026-10-24; user checks remain active)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Condensed; manual user checks and the (4) dogfood re-check remain active in the backlog.

- P0 | Windows dual-output live testing and fixes | User 2026-10-19 first
  two-monitor run: (1) cross-output window move right-to-left fails (tiling
  within each output works); (2) no workspace pager on the secondary output;
  (3) workspace switching with the secondary output focused changes the
  primary; (4) new windows open on the secondary output while the primary is
  focused. Status (agent-observed unless stated; physical chords, hotplug,
  DPI/resolution changes and sleep/wake are user checks):
  - (2)/(3) fixed; secondary pager bar-right anchoring user-accepted
    2026-10-20 (quick physical check).
  - (1) DPI placement rebasing and exact-owner directional control User
    2026-10-20 accepted; Paint/Firefox transfers pass both ways with exact arrival
    and follow focus. User dogfood 2026-10-20 reports cross-output window moves working well.
  - User physical 2026-10-20: Win+digit on each output passes except the
    empty-workspace focus item below; Win+Shift+arrows cross-output moves pass;
    Win+Ctrl+Alt+arrows explicit send-follow works as designed. After Explorer
    restart tiling survived and primary/secondary pager positions were correct;
    primary anchor churn looked fine once. Cross-output title-bar drag with
    drop overlay passed between outputs with different DPI. Task Manager stays
    untiled (elevated by default; expected unmanageable class, not a defect).
  - (4) cursor-first fresh placement delivered in core and Windows
    2026-10-22, fixture-verified on both outputs; user dogfood re-check pending.
  Checklist sweep: W2M-02 tiled follow/stay and admitted-float return pass
  (`send-output` control User 2026-10-20 accepted); W2M-13 mouse selection, occupied
  hide/show and Explorer-restart recovery pass with event-driven anchor
  revalidation (User 2026-10-20 accepted; game cost unmeasured); W2M-10 partial; owner
  exit-cause logging added.
  Open, now manual user checks: cross-output drag empty singleton and
  reverse-DPI start (not clearly covered by the user's passing drag check;
  refusal and START diagnostics added); W2M-04/07/08, remaining topology,
  display changes and sleep/wake. Primary anchor churn has one physical
  observation, not a repeated stress pass. The synthetic-input drag driver is
  disabled fail-closed; the input-ownership invariant is in
  [live testing](live-windows-testing.md).
  [Record](changes/windows-dual-output-live.md),
  [two-monitor checklist](changes/windows-physical-checklist.md#two-monitor-pc).

## Windows single-output dogfood history (condensed 2026-10-24; acceptance remains active)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Condensed; remaining user-run acceptance and the (4) recheck stay active in the backlog.

- P0 | Windows single-output dogfood 2026-10-19 | Delivered and pushed
  (`a372078e`, `6911b8d2`, CI green); remaining user-run acceptance (app
  hide/reshow, desktop/Start pager, accent change while running) is listed in
  the [record](changes/windows-single-output-dogfood.md). User reports: (1) a
  fullscreen game run as administrator (unmanageable) suspended all tiling;
  user-approved: other managed windows keep non-activating geometry
  reconciliation, game untouched, focus/overlays suppressed (user physically
  accepted); (2) maximized return focus, (3) pager z-order and pager styling
  user physically accepted, as are the centered number halo and the shared
  WinRT system accent for border theme mode and pager highlight. Open: (4)
  pager disappears when the desktop background is clicked or the Start menu
  opens (low-integrity Start surface was misclassified as elevated; fixed,
  native visibility/topmost diagnostics added; user recheck pending); (5) intermittent
  black accent and startup crash (cached WinRT factory across apartments)
  fixed and user physically accepted over 10 starts; (6) closing W2 in
  `H[W1 V[W2 W3]]` left W3 unexpanded with a gap: Steam hid its live
  window and the domain silently deferred. User-approved: a positively
  verified app-initiated hide is treated as a close (release and reflow,
  fresh admission on reshow), matching COSMIC/sway/Hyprland unmap; log any
  domain deferral; add the reference outcome and a functional-spec row.
  [Record](changes/windows-single-output-dogfood.md). Hidden windows'
  Alt+Tab/taskbar presence is the separate post-0.1 item below.

## Core/platform boundaries pre-consolidation body (superseded by the 2026-10-24 closeout)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Superseded planning detail; current status lives in the condensed backlog entry pointing at the rearchitecture closeout.

- P1 | Core/platform boundaries and shared acceptance baselines | User
  2026-10-18: discuss with the user before planning. The boundary is now set by
  [Layering](principles.md#layering) and the user-approved 2026-10-20
  [core/platform decisions](decisions.md#core-platform-boundary), extending the
  [core-owned world model direction](decisions.md#core-owned-world-model).
  First deliverable and discussion input: inventory everything currently
  platform-owned in each area and map it to its core destination: workspace
  mapping/scope/history/trailing lifecycle, admission/output selection,
  classification, verification/fences, drag/gesture, overlay geometry, pager
  content and cursor. Include Windows workspace owner/send paths (W3 world-route
  plan) and KDE's TypeScript born-maximized copy. Discuss the inventory and
  sequencing with the user. Status 2026-10-21: inventory and staged plan
  delivered (tentative sequencing, user review pending); stage 1 (core-owned
  ordered window eligibility, Windows duplicates removed) delivered with
  fixture checks on both outputs. 2026-10-22: stage 2 (core logical workspace
  authority, sole Windows world owner, cursor-first switch and fresh placement),
  3a (frame clamp/backoff) and 3b1 (learned minimum sizes) delivered with
  both-output fixture checks; remaining 3b completion/anti-fighting and
  frameless migration, stages 4-5 pending. 2026-10-23: stages 3 and 4
  (gestures, focus-follows-cursor/warp default off, border/underlay/pager
  presentation) delivered with both-output fixture checks; stage 5 audit in
  progress: directional/float policy moved to core but maximized-tiled clear
  continuation fails natively (`685dc0b9` checkpoint, not dogfood-ready);
  remaining migration/control context, dead APIs, common acceptance
  connector, uniform wire presentation, port notes, regression matrix and
  dogfood summary. KDE/macOS ports follow once the core interface settles
  ([record](changes/archive/core-world-model-rearchitecture.md)). Goal: a consistent test
  process over agreed cases, consistent with Tier 1 input tilers, for high
  quality and cross-platform consistency. A maximal, high-quality core also
  eases future targets (GNOME, Cinnamon) and lets window-manager developers
  embed the core tiling library. Acceptance scope to agree:
  - Baseline acceptance tests for the approved boundary that run against core alone.
  - One shared baseline acceptance case set for KDE, Windows and macOS, minus
    agreed, documented platform gaps.
  - Platform test helpers: stage an initial tiling state and modify tiling
    during a test; capture the platform's current window state into a data
    structure that fits the shared case format; a test window that can be
    created and changed dynamically without recreation (decorations on/off;
    windowed, fullscreen, borderless; resizable or fixed; size; minimum size;
    maximum size if hosts support it), with several open at once.
  - Ideally the shared cases and assertions are reused unchanged across
    platforms, with only a thin per-platform connector driving the host.
    Cases may also assert responsiveness, so they may be time-bound with
    deadlines (a stated reason for a limit under
    [Simplicity](principles.md#simplicity)).
## Windows full world-state route (superseded by the core world-model rearchitecture)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Superseded implementation route; stages 2a/2b of the rearchitecture replaced it. W2a diagnostics remain evidence only.

- P1 | Windows: adopt full world-state core replies | W2a gated offline
  diagnostics delivered 2026-10-18: 55 fixtures reconciled onto current main;
  native build/test/strict clippy/fmt/whitespace pass (2,852 passed, 1 ignored).
  Real W1 FULL reports with empty adjacency now produce direction-only core
  migration/refill, but target authority holds and no native attempt occurs.
  W2 acceptance remains blocked on shared migration authority and pre-commit
  T2 capacity-float crossing refusal; [proposals and exact next action](changes/archive/windows-full-world-state-replies.md#w2b-cross-stream-proposals).
  No Windows adjacency mapping, shared semantic edits or runtime cutover.
  Shared topology prerequisite delivered offline in
  [core/KDE](changes/archive/shared-core-owned-output-topology.md) (`f9bd2df1`).
  [World split-edge alignment](changes/archive/world-split-edge-selection.md)
  has also landed; integrated native gates and all 55 W2 fixtures pass without
  changing either migration-authority or capacity-send diagnostic outcomes.
  W2/native acceptance remains pending. Replace
  adapter-triggered trailing policy and per-op application with complete
  observed world reports and host diffs; move scope/history/mapping policy
  into the world state; preserve recovery ledger/window properties/native
  safety and D16/D23/overlay semantics. Adapt `TileLoop`,
  `tiling_sys/admission.rs`, `tiling_sys/scope_trailing.rs` and workspace
  owner/effect/send paths; remove direct session-state policy queries where
  the reply replaces them. Cover bootstrap/order, stable creation identities,
  lifetime/gesture fences, refused targets, partial transfer and follow proof
  in all workspace modes. Native acceptance remains separately required.
  The legacy domain API remains compiling, not behaviorally migrated.
  User accepted constructor prerequisite 2026-10-18 (T1):
  additive core configuration preserves existing defaults and Windows send/
  maximized policies without wire changes. [Slices and evidence](changes/archive/windows-full-world-state-replies.md);
  constructor prerequisite delivered in `0869f08c` and replacement W1 capture/
  sequencing in `c73e4cc7` (CI green). Historical W2 prototype remains on
  `wip/windows-world-diff`; its former native-gate failures no longer reproduce
  on current main. W2a is diagnostic evidence only, not positive migration or
  T2 parity acceptance; W3/W4 hidden/fullscreen/bootstrap/native gates remain.
## Migration authority and T2 refusal discussion (condensed; parked items remain)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Condensed; the parked session gate, the user 2026-10-19 core caveat and the parked KDE mismatch remain active in the backlog.

- P1 | Core + Windows: world migration authority and T2 crossing refusal |
  Assessed 2026-10-20 ([record](changes/archive/windows-full-world-state-replies.md)):
  (1) User 2026-10-20 did not approve the proposed default-off, migration-only
  authority capability in that form: whole-workspace migration is a
  [core-owned mapping change](decisions.md#core-owned-world-model) actuated by
  the adapter. Rework with the core/platform boundary inventory above.
  (2) adapter-only staged-clone refusal proven offline for
  directional output-send/R4 (User 2026-10-20 accepted direction; test-only,
  not wired, not runtime acceptance); explicit-target
  sends keep `cross-domain-mismatch`. One change across shared core and the Windows adapter, delivered on a
  two-output Windows host so the theory is validated and tested live during
  implementation. Unblocks W2 above (proposals 1 and 2 in
  [W2b cross-stream proposals](changes/archive/windows-full-world-state-replies.md#w2b-cross-stream-proposals)).
  **Caveat (user 2026-10-19): be extremely careful to avoid platform-specific
  logic in core.** For each blocker, first assess whether a small, simple,
  sensible alternative without core changes exists (adapter-side or
  platform-neutral). Only if none exists, design the core change to add
  minimal conditional logic or burden, expressed as a platform-neutral
  capability rather than a host check; precedent is the KDE opt-in for
  output-shared workspaces. Any core change needs explicit user approval
  before implementation; bring the options, trade-offs and recommendation to
  the user.
  (1) Migration authority. Root cause: after planning the migration, core retains
  the workspace under the target output while the adapter still reports it on
  the source output. The target scope is `Hold(IncompleteInventory)` with held
  migration rows; Windows correctly refuses uncontrolled scope, so members
  never move and `migration_confirmed` never settles. KDE
  `world-apply.ts` currently bypasses the scope gate via
  `command_result.members`. Next: use the boundary inventory as discussion input,
  then rework migration as core-owned workspace-to-output mapping plus adapter
  actuation, truthful observations and explicit final settlement; align KDE's
  bypass with that contract. The narrow capability proposal is superseded,
  not approved. Prove member -> target view -> source refill
  -> topology/binding confirmation through actual Windows W1 capture, feedback
  and re-observation, plus native two-output journeys. Coordinate the shared
  contract and KDE alignment with the core/KDE stream (and the parked KDE
  global-native-inventory migration-confirmation mismatch); macOS follows the
  shared contract.
  (2) T2 crossing refusal. User decision T2 (2026-10-18): Windows refuses
  cross-output capacity-float transfers until a remap rule is approved. Legacy
  Windows refuses `float-no-remap`, but the world route's
  `world_sends.rs::execute_output_send` accepts the send and commits the
  transfer before the adapter sees the reply, so a post-reply refusal would
  diverge from core state. Assess first whether the adapter can refuse before
  sending the intent (e.g. output-send of a capacity float) and what remains
  for whole-workspace migration/R4 paths. If a core change is still needed,
  the previously proposed option is an additive, default-off world
  configuration capability (as with T1) refusing unsupported capacity-float
  crossings before commit for output-send and R4 paths, with no protocol
  change and no `FloatOrigin` branching; subject to the caveat above.
  Parked until the two-output Windows host session; user 2026-10-19.
## Windows visibility diagnostics (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed delivery evidence.

- P3 | Windows visibility diagnostics | Delivered offline: bounded normal-level
  per-window hide/reveal outcomes and reasons, native visibility/cloak samples,
  exception lanes/refusal causes and monotonic timing. Omitted/coalesced history
  is explicit; no behavior change or Xbox recovery claim. Locked native gates:
  2,313 passed, 0 failed, 1 ignored
  ([record](changes/archive/windows-visibility-diagnostics.md),
  [incident diagnosis](changes/windows-gaming-coexistence.md)).
  Agent-live 2026-10-17 normal-level switches: final log 10 hide/10 reveal/15
  visibility transitions, no malformed hide/reveal rows; overflow/exception
  stress and Xbox recovery not claimed. [Evidence](changes/archive/windows-visibility-diagnostics.md#agent-live-follow-up-2026-10-17).

## Next-implementation-order note (completed deliveries)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed; retained as history.

- Next implementation order (user review 2026-10-13): D23 best-effort restore
  rework plus close cleanup delivered offline ([evidence](changes/archive/d23-rework.md));
  D16 ordinary/combo floating sends delivered offline ([evidence](changes/archive/d16-floating-transfer.md));
  Born-maximized core policy and KDE conformance delivered offline
  ([evidence](changes/archive/born-maximized-shared-policy.md));
  KDE underlay default delivered offline ([evidence](changes/archive/kde-dogfood-underlay-options.md#8-kde-default-delivery-2026-10-13));
  next code: unsafe planner explicit-config
  rework plus crate-wide forbid, third-party notices, then KWin-to-Rust
  migration research (delivered below). Native verification remains queued.
## DONE: Windows 0.1 gate exceptions (approved 2026-10-19)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed; native/physical acceptance stays under the P0 0.1 release item.

- DONE | Windows: propose 0.1 gate exceptions against the shared template |
  [57-check mapping, host exceptions and Windows extras](changes/windows-0.1-gate-proposal.md)
  approved exactly as written in D1-D4 (User 2026-10-19), under the
  [shared gate decision](decisions.md#scope-and-platform-goals).
  Native/physical acceptance remains tracked under the P0 0.1 release item.
## 0.1 triage verification queues (detail archived; native verification stays active)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Superseded detail; per-unit native journeys remain pending in the backlog and Pending live checks.

  - D16 verification queue: ordinary and approved floating+fullscreen/maximized
    sends delivered offline in shared core/KDE ([record](changes/archive/d16-floating-transfer.md)).
    Native R-WS-05 roundtrip (WS1 -> WS2 -> WS1), numbered/relative follow/stay,
    sole-window source/empty target, retained float/overlay flags and exact
    same-output frame remain pending. Verify zero geometry/overlay writes,
    unchanged source/destination tiles, restart float memory and later overlay
    exit returning to floating frame. Sticky/automatic-fixed refusals unchanged;
    no setting. Windows adapter delivered offline 2026-10-13
    ([record](changes/archive/windows-d16-floating-transfer.md)); native acceptance
    remains in item 22.
    Agent-live Windows 2026-10-17: float + maximized numbered/relative
    follow/stay and native destination unmaximize preserved the remembered
    float frame. Plain-float matrix remains partial; fullscreen skipped for
    Xbox prompt risk. [Evidence](changes/archive/windows-d16-floating-transfer.md#agent-live-follow-up-2026-10-17).
    Second agent-live sweep 2026-10-17: numbered ordinary-float roundtrip and
    fullscreen stay-carry passed; a small-frame destination-exit defect was
    [fixed](changes/archive/windows-floating-fullscreen-frame-restore.md).
    Direct/destination exits now restore the exact float frame; comprehensive
    relative/physical/game matrix remains pending.
  - D22 verification queue: native R-FLT-03 float-out reflow live check remains
    pending; surviving siblings must preserve their relative shares. Shared
    removal already matches; verification-only, no setting.
  - D23 verification queue (user 2026-10-13): best-effort rejoin and explicit
    close-slot cleanup delivered offline after matching COSMIC `3d55cba0`
    source verification ([evidence](changes/archive/d23-rework.md)). Native
    journeys remain pending: focus change, resize, new window, sibling close,
    swap, other minimize/restore, gaps/resolution, restart and close cleanup;
    verify same side/group/index where possible, comparable old share,
    session-only lifetime and no restore focus write. Minimize/reflow/MRU and
    sole-none are preserved. Windows adapter delivered offline 2026-10-13
    ([record](changes/archive/windows-d23-restore-close.md)); native journeys
    remain in item 22. Source-slot lifetime after send elsewhere is decided
    User 2026-10-17, option B (forget on confirmed rehome; shared-core
    implementation approved, not implemented); [proposal ready for cross-stream
    review](changes/shared-core-d23-rehome-and-13f-maximize-base.md). Shared
    behavior unchanged until implementation.
    Agent-live Windows 2026-10-17: minimize/reflow/restore recovered measured
    frames; minimized Paint exited and survivors reflowed. Internal slot
    retirement remains unproven; attempted sole leg invalid; minimized restart
    iconic retention observed only. [Evidence](changes/archive/windows-d23-restore-close.md#agent-live-follow-up-2026-10-17).
    Second agent-live sweep 2026-10-17: resize-skewed rejoin, minimized close
    and distinct fresh admission passed; combined native non-reuse plus the
    slot-absence regression supports internal cleanup. Sole iconic retention
    passed, but internal null focus, hidden bursts and broader variants remain
    open. [Evidence](changes/archive/windows-d23-restore-close.md#second-agent-live-sweep-2026-10-17).
  - D27 KDE verification queue: native valid-token switch convergence,
    missing/stale-token mark-only without interference; FromTool/allowed-X11
    native behavior. Must-pass real-game active play and idle/paused check,
    tiler-off/on comparison; rely on KWin freshness, no tiler guard. Reopen
    only on actual active-play switch-away. Included in reference-comparison
    check 1, counted once; native evidence TBD.
  - D25/D26 verification queue: native dogfood/live checks will assess orderly
    owner restart with apps alive and end-session restore. D25 keeps host set,
    observes membership without moves, restores only provable focus and
    intentional floats via existing store; re-adoption rebuilds layout. D26
    reloads persisted gaps/settings and observes host set/membership/maximize;
    cross-login intentional-float loss is the approved 0.1 limitation.
    Verification-only; no saved-layout restore or D26 code work.
  - D01/D28 verification queue: compare tiler-off/on native admission over
    maximized/fullscreen B, host-approved/denied activation and explicit
    switching; verify overlay-close cleanup. D28 must-pass native acceptance
    covers numbered/relative follow/stay and a real game, both retained and
    never-tiled born-fullscreen. The latter stays slotless with neither source
    nor target reflow until D5 exit admits on the destination; include sole
    held source and empty target. Record native flag/membership/view/focus,
    geometry and visible/input state; stop and return to the user if KWin
    drops/corrupts fullscreen.
  - Windows impact: shared core fixes serve Windows; Linux work owns KDE/core,
    Windows adapter/settings/native work goes to handoff item 22 below.
    D23 Windows retained-allocation divergence was replaced offline 2026-10-13
    by the approved release/reflow target. D28 KDE carry aligns with
    approved Windows carry (item 9). No Windows code changes here.
## Windows dogfood 2026-10-11 (delivered offline; live acceptance stays active)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivery evidence; remaining live/physical legs stay in the condensed backlog entry.

- P0 | Windows dogfood 2026-10-11 | User live run `tile --user-start --trace`
  at `7f407ca` (log `%LOCALAPPDATA%\omnitiler\session-1\run-01dd5920bbba8256.log`).
  Passed: TOEM 2 unimpeded across exclusive/borderless/maximized/windowed
  modes and hide/reveal across a workspace switch. Windows order (user):
  (a) stop blocked by an open first-run preset prompt: stop must cancel the
  pending prompt with no preset written; (b) DEFECT: returning to WS1 did not
  refocus the previously focused fullscreen game (Steam took focus; REQ-WS-09);
  (c) DEFECT: group underlay too short in `V[W1 H[W2 W3]]` with W2 focused
  (bottom of screen to ~60% of W2); (d) born-maximized setting, default tile
  ([decision](decisions.md#window-state-float-sticky-maximize-fullscreen)
  R-MAX-06 CHANGED 2026-10-11); (e) border radius default 6
  ([decision](decisions.md#visuals-border-underlay-and-grouping)); then
  handoff items 10+18, 11+15 after offline-delivered 13+17. Later: (f) Win+B
  default browser and
  Win+T Windows Terminal launchers ([decision](decisions.md#shortcuts-conflicts-and-presets));
  (g) underlay visibility: user used `#50909090`; candidate C `#662a82da`
  selected for now on both platforms, defaults delivered offline (item 24).
  Saved colors preserved; black/white/grey at 100%/150% visual check pending
  ([Windows record](changes/archive/windows-underlay-default.md)). Win+G/F11
  leakage observed again (known, parked under gaming coexistence).
  - User physical 2026-10-14 (scoped Notepad/Paint/Calculator, single output,
    `--group-underlay-color '#662a82da'`): underlay 40% blue with correct size
    (item 24; black/white/grey and 150% not reported); Win+Y toggles both ways,
    no held repeats (item 21); D16 float and float+maximized sends keep state
    and focus, unmaximize/remaximize on target and return work; D23 minimize
    reflows, restore rejoins, minimized close keeps tiling sound. One issue:
    fullscreen send arrives without focus (trace `focus-suppressed`), the
    then-current R-MAX-09 fullscreen-follow focus suppression. User 2026-10-14
    selected option A: a verified follow send focuses the fullscreen mover on
    arrival like other follow sends (stay unchanged); real-game check under
    gaming coexistence. Delivered offline 2026-10-14 for numbered/relative
    workspace and output follows, with mover-focus and unchanged stay
    regressions; user physical recheck passed 2026-10-14 (fullscreen Notepad
    kept focus across workspace follow sends) ([decision](decisions.md#window-state-float-sticky-maximize-fullscreen)).
  - Windows review follow-ups (User 2026-10-14):
    - 5b / correction C APPROVED user 2026-10-16, NOT YET IMPLEMENTED;
      folded User 2026-10-17 into the planned post-0.1 `Activation request`
      setting item (P2 0.1 triage settings follow-up), standalone entry
      removed:
      Windows Switch and focus honors OS-permitted activation even with a
      fullscreen/game foreground; no urgency-only downgrade. Default Mark urgent
      only and non-permitted requests unchanged. Windows itself permits this;
      an extra tiler game-protection override risks suppressing legitimate requests.
      Accepted consequence: workspace switch hides the game's workspace; later
      real-game evidence pending. First establish the request/permission seam:
      no dedicated classifier or activation setting is shipped. Keep Windows-only;
      stop if shared-core work would alter KDE. Tests must cover permitted
      Switch and focus over fullscreen/game, unchanged default and denied requests;
      include the new behavior in future setting help. [Decision](decisions.md#activation-requests).
    - Reconcile `tick` skipped count without identities accepted for now.
    - (6) D23 source slot after a minimized window is rehomed elsewhere:
      [Tier 1 research delivered 2026-10-17](research/d23-minimized-rehome.md).
      COSMIC removes the source entry and discards its restore anchor on a
      minimized-window move; Hyprland/i3/sway have no native counterpart and
      do not vote. User decision 2026-10-17, option B: forget the source slot
      on confirmed rehome; a later return is admitted fresh. Shared-core
      implementation (affects KDE) approved, not implemented;
      [proposal ready for cross-stream review](changes/shared-core-d23-rehome-and-13f-maximize-base.md).
    - (8) Linux KDE `just dev` streaming/teardown check remains pending;
      [entry-point record](changes/dev-entry-point.md) stays active until it passes.
    - (13f) [Tier 1 research delivered 2026-10-17](research/fixed-override-born-maximized.md).
      No exact durable-override/restart/maximize counterpart in Tier 1. User
      decision 2026-10-17, option A: preserve the durable tiled base beneath
      native maximize under either born-maximized setting, including hint
      loss; automatic fixed D4 unchanged. Approved shared policy, not
      implemented. All targets including KDE affected by the future change;
      [Windows interim unchanged](decisions.md#fixed-size-admission);
      [proposal ready for cross-stream review](changes/shared-core-d23-rehome-and-13f-maximize-base.md).
    - (13g) Research delivered 2026-10-16:
      [safe Win32 wrappers](research/windows-port/safe-win32-wrappers.md).
      User decision 2026-10-17, option A: one minimal safe FFI module on
      `windows-sys`; keep the module minimal. Route chosen, implementation
      queued, not started. Default-forbid lints,
      narrow module opt-in and the CI allowlist check remain pending under
      P1 Windows unsafe FFI consolidation / Rust unsafe policy and the
      [guardrail](decisions.md#rust-unsafe-guardrail).
  - User 2026-10-14: the Windows taskbar workspace indicator is now high
    priority and blocks Windows dogfooding (tiling otherwise close to dogfood
    ready, apart from the parked Win+G/Win+F11 Xbox prompts). Pull the
    indicator part of Windows workspace presentation research forward.
    Record in the item 21/24/D16/D23 change records with the next delivery.
  - (f) Offline-delivered 2026-10-12 (`b8edd14`, `8e56546`, CI green):
    Win+B opens the system-default browser via shell `open` of bare
    `https://`, Win+T launches `wt.exe` (missing Terminal logs and
    continues); [accepted implementation](decisions.md#shortcuts-conflicts-and-presets). Physical check pending:
    Authentic launches, Compatible inert, rebind/disable
    ([record](changes/archive/windows-launcher-shortcuts.md)).
  - (a) Offline-delivered: pending first-run prompt cancels on graceful stop,
    with no settings publication; native dismissal/teardown live check pending
    ([record](changes/archive/windows-first-run-stop.md)).
  - (b) Offline-delivered: verified departure focus now feeds remembered return
    focus, including fullscreen members; game focus/display-mode live check
    pending ([record](changes/archive/windows-workspace-fullscreen-focus.md)).
  - (c) Offline-delivered: group projection now honors the same minimum hints
    as tile placement; exact trace replay and nested-layout regression pass.
    Underlay appearance live check pending
    ([record](changes/archive/windows-group-underlay-geometry.md)).
  - (d) Offline-delivered: `core.born_maximized`, default `tile-on-launch`
    and working `keep-maximized-overlay`; live changes affect subsequent
    admissions only. Both-values maximized Paint launch user-accepted physically
    2026-10-12, scoped normal owner, single output, Notepad/Paint, base `e506350`
    plus combined directional/retile/B9 changes. Browser restore, detailed
    Settings, fixed-size, game/DPI/multi-output checks pending
    ([record](changes/archive/windows-born-maximized-setting.md)); R-MAX-03
    overlay parity is delivered offline with item 11; fixed admission is
    delivered with 13+17 below.
  - (e) Offline-delivered: active border radius defaults to 6 in Windows
    settings and CLI/proof owners; explicit radii preserved. Radius 6 appearance
    and live Settings Apply user-accepted physically 2026-10-12 under the same
    conditions as (d); Revert/game/DPI/multi-output pending
    ([record](changes/archive/windows-border-radius-default.md)).
  - Items 13+17 offline-delivered: bounded min/max observations, predicate
    schema/UI, current-hint fullscreen exit/enable classification and fixed-origin
    restart markers. Native hints/overlay/game no-touch acceptance pending;
    hydrated fixed-override maximize routing is tentative
    ([item 13](changes/archive/windows-fixed-size-admission.md),
    [item 17](changes/archive/windows-per-axis-zero-predicate.md)).
  - Items 10+18 offline-delivered 2026-10-12: ordinary/sticky float directional
    focus and stateless half-snaps; maximized focus fence and one observed-clear
    unmaximize-before-move. User-accepted physically 2026-10-12 under (d)'s
    conditions: float focus/half-snaps, maximized tiled focus fence, maximized
    float clear then all four halves, maximized tiled clear then move (sensible
    to user). Tie/sticky/clamp/preset variants, failure/race, fixed-size,
    game/DPI/multi-output pending
    ([item 10](changes/archive/windows-floating-directional-navigation.md),
    [item 18](changes/archive/windows-maximized-directional-navigation.md)).
  - Items 11+15 offline-delivered 2026-10-12: R-MAX-03 workspace retile retains
    maximize over a reserved slot under both born settings; ordinary maximized
    unfloat clears/observes before fresh admission with fixed D3/D7 overrides.
    Item 11 retile-preserves-maximize/restore-to-slot user-accepted physically
    2026-10-12 under (d)'s conditions; tray toggle does not restore previous
    window focus (observation only; may suit tray keyboard accessibility).
    Item 15 ordinary clear/admission/focus already has scoped native evidence;
    physical B9, hidden retile, session restore, fixed marker/restart,
    failure/race, game/DPI/multi-output pending
    ([item 11](changes/archive/windows-maximized-floating-retile-overlay.md),
    [item 15](changes/archive/windows-maximized-intentional-unfloat.md)).
## Windows owner threading (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed stage evidence.

- P0 | Windows owner threading | User direction 2026-10-12: rendering moves
  off the input hook thread so LL mouse/keyboard hook delivery never waits on
  owner work, done now while the owner is still small. Evidence: edge
  drag-resize feels ~30-60 Hz with the owner running versus smooth at the
  display refresh without it, after the border-raster fix; LL hooks share one
  pump with observation, Engine, actuation, overlay rendering and trace I/O.
  Recommended target C, staged: (1) dedicated input thread for LL hooks plus
  the synchronous consume/pass classifier over published snapshots; (2)
  dedicated render thread owning border/underlay/drop-preview windows,
  latest-wins desired state with hide/cancel epochs; (3) combined
  stress/recovery, then multi-output/DPI and gaming-cost checks. One
  observation/Engine/recovery owner remains. User selected target C
  2026-10-12: stage 1 (complete A) first, then stage 2 (B) early. Selected for
  latency only; parked Win+G/F11 containment is not
  claimed. Stage 1 offline-delivered 2026-10-12 (`ba2eb66`, CI green): both
  LL hooks and classifiers on a dedicated input pump. User physical
  2026-10-12 (dev, trace, Notepad scope): edge drag-resize much faster, still
  short of tiler-off; graceful stop clean. Win+arrow felt slightly slower; cause
  was a missing owner wake (up to 100 ms poll), fixed in `21a9e58` and
  user-confirmed physically
  ([record](changes/archive/windows-input-thread.md),
  [wake fix](changes/archive/windows-input-owner-wake.md)). Release and
  trace-off comparison, native drag/Esc and preheld-modifier checks pending;
  Win+drag-only checks retired by the 2026-10-18 removal decision.
  Stage 2 offline-delivered 2026-10-12 (`8fe0224`, CI green): border,
  underlay and drop preview on a dedicated render thread with latest-wins
  generations, safety epochs and acknowledged hide before drop actuation;
  agent live evidence on scoped Notepad/Calculator/Paint (carrier placement,
  preview hide-before-drop, clean stop). Tentative: 500 ms hide-proof timeout
  refuses that drop without writes. User physical check pending (pixels,
  follow, z-order, cross-slot drop, Esc, stale-show, edge-resize, Win+arrow)
  ([record](changes/archive/windows-render-thread.md)). Stage 3
  single-output hardening delivered 2026-10-12 (`baee279`, `2fce63f`, CI
  green): fixed repeated-stop panic, false-clean stop after thread panic,
  10 ms input/render idle polling, stranded final callback/retained input and
  renderer stop wake; agent live bursts and five clean start/stop runs; idle
  trace-on dev ~1% of one core with owner maintenance still periodic
  ([record](changes/archive/windows-owner-threading-acceptance.md)).
  Remaining: user physical single-output check, multi-output/mixed-DPI
  (after multi-output foundation) and gaming-cost legs. [Design](research/windows-port/owner-threading.md),
  [evidence](changes/archive/windows-resize-hook-latency.md).
## Rust unsafe policy guardrail (applied)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed guardrail evidence.

- P1 | Rust `unsafe` policy | User rule 2026-10-13: only tiny ABI/FFI
  boundaries in one dedicated FFI module per crate; every other module and
  crate uses `#![forbid(unsafe_code)]`. Audit at `2af85a7`:
  `tiler-core` and `tiler-protocol` have none; stage 1 thread handoff
  (`input_handoff.rs`) has none and uses no `unsafe impl Send/Sync`,
  `static mut`, `UnsafeCell` or cross-thread raw pointers; remaining
  `tiler-windows` and `tiler-kwin-effect-ffi` uses are Win32/C FFI calls and
  hook/window procedures, which Rust cannot express without `unsafe`.
  Guardrail applied 2026-10-12: `#![forbid(unsafe_code)]` on `tiler-core`, `tiler-protocol`
  and unsafe-free Windows modules (`25de29a`; workspace build/test, clippy
  `-D warnings` and fmt pass). Linux planner delivered 2026-10-13:
  five unsafe env set/remove call sites replaced by explicit trace inputs,
  preserving `OMNITILER_TRACE` semantics; all three planner target roots now
  forbid unsafe. Audit also closed 29 unsafe-free core/protocol test/example
  root gaps. Workspace tests: 1,644 passed, zero failed/ignored; all-target
  clippy `-D warnings` and fmt pass
  ([record](changes/archive/linux-planner-unsafe-guardrail.md)).
  Windows FFI consolidation into dedicated modules is required after
  its current release-gate work (timing tentative), in the Windows queue.
  KWin effect FFI stays a minimal boundary module.
## KDE follow-ups from 2026-10-11 Windows dogfood (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed delivery evidence; remaining live checks stay in the condensed entry.

- P1 | KDE follow-ups from 2026-10-11 Windows dogfood | Delivered
  offline: R-MAX-06 born-maximized setting (default tile), active border
  radius 6. Born-maximized shared-core policy delivered offline with one
  Rust/KWin conformance table ([record](changes/archive/born-maximized-shared-policy.md)).
  User 2026-10-13 option C amends immediate KDE rewiring: keep its pinned copy
  and defer rewiring to post-0.1 Push code down; behavior, labels and accepted
  `bornMaximized` key unchanged. Windows adapter-copy replacement delivered
  offline with handoff item 23; shared-table settings/admission conformance
  and native MSVC build/test pass (1,847 passed, 0 failed, 1 ignored), closing
  the lift's native compile gap. Behavior/schema/lifetime adoption and the
  hydrated fixed-override maximize route are preserved (User 2026-10-17
  option-A approved shared policy, not implemented)
  ([Windows record](changes/archive/windows-born-maximized-shared-policy.md)).
  Underlay candidate C `#662a82da` selected for now on both platforms; KDE
  kcfg/KCM default delivered offline 2026-10-13 (native build, CTest 32/32,
  KWin static-contract tests 15/15). Windows defaults/mirrors delivered offline
  with item 24 ([record](changes/archive/windows-underlay-default.md)); explicit
  colors preserved, schema v1, no migration.
  Settings UI, both-values
  launch, radius appearance and black/white/grey underlay live checks pending;
  underlay visual check before 0.1 ([options](changes/archive/kde-dogfood-underlay-options.md)).
## Windows unsafe FFI consolidation (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed delivery evidence.

- P1 | Windows unsafe FFI consolidation | Required under user rule 2026-10-13:
  one tiny dedicated FFI module per crate, every other module/crate forbids
  unsafe. Delivered offline 2026-10-17; safe input/render handoffs preserved.
  [Rule](decisions.md#rust-unsafe-guardrail).
  User 2026-10-17, item 13g option A: one minimal safe FFI module on
  `windows-sys`, implemented as responsibility-split `ffi/` with one unsafe
  opt-in root. Zero unsafe blocks/functions/impls outside it; workspace lint
  inheritance and the CI allowlist gate delivered (32/32 policy tests).
  Follow-up Windows gates pass: 2,596 passed, 0 failed, 1 ignored. Linux,
  Windows and macOS CI allowlist gates pass. Owned-window live lifecycle,
  settings transactions, tray/pager picks and renderer smoke passed; the
  Settings process-exit regression was fixed and retested. Physical keyboard
  and DPI/display/Explorer acceptance remain pending.
  [Research](research/windows-port/safe-win32-wrappers.md),
  [evidence](changes/archive/windows-unsafe-ffi-consolidation.md).
## Module size refactoring (Windows phases complete)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed Windows split evidence; shared/KDE remainder stays active.

- P1 | Module size refactoring | User 2026-10-14: split oversized source
  files toward the [file size](principles.md#file-size) soft caps.
  Behavior-preserving moves between feature deliveries, with two verified
  helper-extraction exceptions (keyboard match arms, run-loop wait/intake/tick). Windows first:
  `crates/tiler-windows/src/tiling_sys/`, then
  `workspace_owner.rs` (~12k) and `snapkey.rs` (~6.5k). Shared crates
  (`tiler-core` `engine.rs` and `session.rs`) and production KDE world files
  are split in coordination with their work streams. Legacy KDE adapters and
  `planner_protocol.rs` were removed rather than split.
  Windows phase 1: 14 pure-move extraction commits reduce `tiling_sys.rs`
  from 61,917 to 35,537 lines (-26,380, 42.6%). All 28 inline test modules
  now occupy separate child files totaling 26,288 lines; production unchanged.
  Windows `tiling_sys` phase 2 complete: rows 14-30 plus observation, D23
  `hidden` submodule, scope filename cleanup and both approved helper
  extractions (keyboard arms; run-loop wait/intake/tick) are done. Family: 103
  files including root, `mod.rs` 1,082 lines; max production `cross_drag.rs`
  1,455 and max test `tests_pending_drop.rs` 2,852, within caps. Scope tests
  keep all 46 qualified paths via `#[path]`; moves preserve bodies apart from
  necessary visibility/imports and rustfmt signature wraps. Gates green 2,106 /
  0 / 1.
  Windows owner/snapkey split complete 2026-10-15: tests-first row plus one
  production row per file move both families into role files, every file
  within caps (owner max production 467, max test 2,810; snapkey max
  production 1,262, max test 304). Sizes are physical lines including blanks
  (nonblank alongside in the record). Gates green 2,115 / 0 / 1.
  Windows remaining top-level split complete 2026-10-15: tests-first row
  plus one production row per family moves settings, tiling, input_pump
  and render_pump into role leaves, every file within caps (max
  production `tiling/cli.rs` 1,429, max test
  `input_pump/native/core_tests.rs` 1,568; 124 tests, paths unchanged).
  Sizes are physical lines including blanks (nonblank alongside in the
  record). Gates green 2,115 / 0 / 1.
  Windows tail split complete 2026-10-17: tests-first row plus four production
  rows reduce `workspace.rs` 2,918 -> 1,144 (outputs 624),
  `active_border_sys.rs` 1,986 -> 1,107 (carrier 654), `product_hide.rs`
  1,836 -> 135 (native 1,429 / watcher 254), `winarrow.rs` 1,774 -> 731
  (native 1,026), `render_handoff.rs` 1,515 -> 1,057 (tests-only split).
  Settings UI was already split (max production 1,184) and stays unchanged.
  All 70 extracted tests keep their qualified paths; gates green 2,313 /
  0 / 1. Sizes are physical lines including blanks; no oversized family
  skipped. No Windows test file exceeds 3,000 (max `tests_pending_drop.rs`
  2,852). Core
  engine.rs (8,491) and session.rs (7,239) still carry large inline test
  modules as the next test-extraction candidates. Shared crates and KDE files
  above still split with their work streams; no further plan.
  ([split map](research/windows-port/tiling-sys-split-map.md),
   [phase 1](changes/archive/windows-tiling-sys-split-phase1.md),
   [phase 2](changes/archive/windows-tiling-sys-split-phase2.md),
   [owner/snapkey record](changes/archive/windows-owner-snapkey-split.md),
   [remaining-split record](changes/archive/windows-remaining-module-split.md),
   [tail-split record](changes/archive/windows-module-size-tail.md)).
## Windows shortcut-dialog presentation review (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Completed delivery evidence.

- P2 | Windows shortcut-dialog presentation review | Handoff delivered and
  scripted-live verified 2026-10-17: owned conflict preview, Apply/Revert
  success/error dialogs, discard protection, and preview-time stale refusal.
  Settings bytes and system-input state restored; 2,458 / 0 / 1 locked gates.
  User 2026-10-17 keeps the shipped dialogs as shipped: advisory Apply/Cancel
  preview, no extra Apply confirmation, whole-draft discard. Accepted-for-now;
  a later UX review note is nonblocking. Physical chord delivery is not claimed.
  Windows Revert still reloads saved settings, not KDE defaults.
  [Delivery](changes/archive/windows-shortcut-settings-dialogs.md),
  [decisions](decisions.md#settings-tray-and-first-run).
## Windows fullscreen-exit acceptance (delivered offline)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivery evidence; remaining physical legs stay in the condensed entry.

- P1 | Windows fullscreen-exit acceptance | Startup/born exits, retained managed
  slot return, borderless exit and owned-exit metadata retirement pinned offline
  2026-10-12; directional post-clear owned-metadata veto corrected. Owned
  Notepad managed-slot return and fullscreen stop/relaunch/exit tiling
  physically user-accepted 2026-10-12, scoped single-output Notepad/Paint,
  base `e506350` plus pre-retirement Windows changes. User-selected automatic
  stale retirement delivered offline for positively restored nonzero chrome
  masks, with double live proof/lifetime/preimage fences and transition/
  re-fullscreen regressions. User follow-up 2026-10-12: originally captionless
  zero-bit metadata stays fail-closed without a restoration witness; Win+F11
  owned toggle exit is the release path. User watches for untiled windows
  during dogfood; no remaining policy decision for this case. App-native/born-after-start/
  borderless, retirement physical, fixed/game/DPI/multi-output pending.
  [Record](changes/archive/windows-fullscreen-exit-acceptance.md).
## Windows port history (foundation delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivered phase evidence; open decisions (package/update channel) stay active in the condensed entry.

- P0 | Windows port | KDE-first core extraction finished at K1 (user
  2026-09-30; K2/K3 revisit when macOS starts, see
  [extraction](research/cross-platform-core/extraction.md),
  [K2 audit](changes/archive/portable-core-k2-settings-actions.md)).
  User decisions (2026-09-30): Windows 11 x64 only; managed per-monitor
  workspaces required for feature-complete; Meta/Win default shortcuts matching
  KDE (2026-10-01), Windows Snap takeover with a visible off setting, and Win+L
  explicit opt-in; custom-drawing
  underlay experiment with outline fallback; most obvious distribution
  (Store plus signed installer plus winget under evaluation); develop
  natively on Windows 11; multi-output live work needs a multi-output Windows setup;
  elevated (administrator) windows unmanaged by default, a user-chosen
  run-elevated option possible later; logical workspace model refined in the
  spikes; shared settings/action intent (deferred K2) and difference
  classification (deferred K3) revisited when macOS starts. Open: final
  package/update channel. Status 2026-10-02: Phase 1 lifecycle/recovery plus
  WH_KEYBOARD_LL input done and accepted on a single-display setup; Phase 2
  automatic Engine-driven tiling-only dogfood preview verified (no shortcuts,
  input hooks or hiding; stop/crash leaves windows in place). Shortcut slice
  completed with retained-Engine focus/move, default-on keyboard takeover and
  session-only mouse prevention plus explicit CLI off switches. Exact native
  foreground and geometry proof passed the full owned-helper journey
  `20261002-040025-21256`; graceful/crash setting restore and ordinary-app
  default-on/both-off smoke passed. Foreground lock is handled by E8 prime plus
  temporary foreground-thread attachment; the synthetic Shift misrouting was
  a proof-arrow extended-key encoding defect, corrected without changing
  product filtering. Native gates and independent mutation review pass.
  Initial SPI target mix-up was recovered; final arranging is 1 and pen is 35.
  Windows settings UI Apply/Revert delivered; physical shortcuts and Snap
  Layouts/bar/Assist/shake coverage remain pending. Managed workspaces now pass
  current-artifact helper and scoped Notepad/Calculator/Paint journeys: existing
  ordinals, send/follow, trailing empty, independent layouts, identity-safe
  hiding and graceful/crash recovery. Terminal is an ordinary managed app by
  default. The physical send trace exposed a new-target outer-gap mismatch;
  fixed with both-domain regression coverage. Fresh independent review and
  native gates pass. Workspace latency fix 06f9d8d passes scoped helper and
  ordinary-app recovery/timing: 72-124 ms ordinary-app selects, 171-241 ms
  physical select/send trace; user accepts select/send/follow and drag-resize
  usability. [Latency evidence](changes/archive/windows-workspace-action-latency.md).
  Batch visibility follow-up 2026-10-17 (739a97c0/5f1bf4fa): user physically
  accepts "huge improvement"; slow one-by-one workspace reveal resolved.
  New trace: 20/20 workspace actions ok; five-window returns take 103-113 ms
  with 62-74 ms reveal phases (prior three-window samples: 493/737/1293 ms
  actions, 406/656/761 ms reveals; application mix differs).
  [Batch visibility acceptance](changes/archive/windows-workspace-reveal-latency.md).
  Review 2026-10-18: tentative per-phase 2,000 ms expiry remains offline-only;
  the owned helper has no delayed-visibility control. Native uncertain/partial
  recovery needs a bounded fixture design and independent recovery proof, not
  another healthy timing sample; no new fault injector selected.
  Follow-up 2026-10-12 edge-resize lag: pixel/checksum-identical border raster
  optimization passes native offline gates; user dev/trace-on reports much
  better but still laggy. LL hooks share the rendering/owner thread; unchanged
  hide traces suppressed and owner busy-span diagnostics delivered offline.
  Input/owner latency and routine render/drag-preview traces now use bounded
  per-second summaries and throttled timing outliers at the run-log source;
  safety/proof transitions and failure/decision/actuation evidence remain
  immediate ([trace-volume record](changes/archive/windows-trace-volume.md)).
  User dogfood 2026-10-14 after that change: console readable, mouse movement
  no longer floods. Small follow-ups delivered: reconcile `tick` records emit
  only a deduplicated skipped count, removing the repeated per-window list;
  Windows `just dev` reprints the run-log path in its final cleanup report
  after the start-of-session lines scroll out of the console buffer.
  Trace-off and release smoothness/release-settle comparison pending
  ([raster record](changes/archive/windows-edge-resize-latency.md),
  [input-thread evidence and commands](changes/archive/windows-resize-hook-latency.md)).
  Owner threading assessment 2026-10-12: separate rendering early; user selected
  target C (isolated input then render pumps, A first), with one observation/Engine/recovery owner; latency selection
  is separate from parked Win+G/F11 containment
  ([design](research/windows-port/owner-threading.md)).
  Minimum-size hints shipped in 49d4191 and were physically accepted, including
  send into a populated workspace (2026-10-02). Late directional dogfood moves
  match KDE/core R2c/R3; minimum-pinned geometry explains zero visible change.
  [Minimum-size evidence](changes/archive/windows-minimum-size-hints.md).
  [Directional diagnosis](changes/archive/windows-directional-dogfood-diagnosis.md).
  Send split-axis defect fixed in `c4a837b` (shared core): stale destination
  focus after send-away/return now falls back to valid destination focus
  history; minimum sizes did not force the axis. One user return split
  stacked as expected, but a later run (2026-10-03) returned Notepad side
  by side at the right edge, then into the middle; see dogfood defects.
  [Evidence](changes/archive/windows-send-split-axis.md).
  Next: Windows feature parity queue below.
  [Managed workspaces](changes/archive/windows-managed-workspaces.md).
  [Shortcut slice and handover](changes/archive/windows-shortcut-slice.md).
  Full owned-helper proof passed `20261001-222611-6068`: 8/8 gaps at 125%,
  minimize/restore, close reflow, graceful/emergency stop preserving frames,
  11 owned writes and zero foreign writes; clean recovery. Normal-mode smoke
  `20261001-223101-30052` passed Notepad/Calculator/Paint/Terminal admission,
  true new-window open/close, Calculator minimize/restore and unchanged stop
  frames. Paint held 617px against a 543px plan, causing 66px overlap; minimum-
  size neighbour replanning was an initial preview limitation, resolved by the
  accepted minimum-size hints. Standard native gates
  passed; multi-monitor/mixed-DPI, gestures and games remain unaccepted.
  See [Phase 2 record](changes/archive/windows-phase2-tiling.md).
  [plan](research/windows-port/plan.md)
  [decision](decisions.md#scope-and-platform-goals)
  Day-one setup and pending governance: [Windows development environment](windows-dev-environment.md).
## Windows feature parity queue (delivered offline)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Per-slice delivery evidence; physical/live acceptance stays active in the condensed entry.

- P0 | Windows feature parity queue | User order (2026-10-02), each matching
  KDE behavior and bindings: (1) active window border done (`b5374ce`, native
  gates/CI and scoped composed-pixel/live routes pass;
  [evidence](changes/archive/windows-active-border.md); temporary development
  default `#ffff00` with theme off, `3fcc953`); (2) group underlay stages A/B
  done (`cb790d9`; [evidence](changes/archive/windows-group-underlay.md)), C
  (unfocused dragged subject) parked for reassessment with item 7;
  (3) maximise done (`1be97a1`, Win+M; [evidence](changes/archive/windows-maximise.md);
  physical button/input/feel checks remain); (4) fullscreen shipped
  (`e84b1a7`, Win+F11; [evidence](changes/archive/windows-fullscreen.md));
  (5) float shipped (`d8329e3`, Win+G; [evidence](changes/archive/windows-float.md));
  (6) sticky float shipped (`292d8c1`, Win+Shift+G;
  [evidence](changes/archive/windows-sticky-float.md)). Live acceptance of
  4-6 is user-owned: agent runs were disturbed by Xbox mode (cloaked
  `ApplicationFrameWindow` foreground, DWM cloak 0->2;
  [record](changes/archive/windows-foreground-acceptance.md)).
  (7) mouse drag move and (8) drop preview delivered same-output
  (`1b9bf7b` title-bar drag, `7dc6aa3` Win+left drag, `b68d490` preview; CI
  green; [evidence](changes/archive/windows-mouse-drag.md)); current selection
  (2026-10-18): native title-bar drag and shared preview/drop remain; Win+drag
  is removed ([removal and verification](changes/archive/windows-win-drag-review.md)).
  Former modifier-drag press-focus delivered 2026-10-11 (`9bb2b6f`,
  [handoff item 7](changes/archive/windows-drag-press-focus.md)) is historical;
  inactive resize keeps focus. Physical native-drag feel checks
  user-owned; underlay C stays parked (needs extra machinery).
  (9) multi-output foundation delivered offline `2c05ee5` + D23 compile fix
  `f2169d6`: local move first, then Win+Shift+direction transfer all four ways
  with follow focus; horizontal Win+direction focus crosses outputs; native
  title-bar drops join destination layout at the pointer. Transfer
  commits last; failure retains source membership. Explicit output-send
  bindings, non-local modes, dependent items 5, 12, 14, 16, 19, hotplug and
  ordinary-float transfer remain outside this foundation. Pending: user
  physical two-output journey ([exact steps](changes/windows-multi-output-foundation.md#remaining-physical-check))
  and Linux/KWin gates; record stays active;
  (10) taskbar workspace indicator research pulled forward as a Windows
  dogfooding blocker (user 2026-10-14); pager-style target selected, surface
  research pending ([indicator options](research/windows-port/taskbar-workspace-indicator.md)).
  Hidden-workspace Alt+Tab/taskbar semantics remain in workspace-presentation
  research as a Windows release gate
  ([note](research/windows-port/alt-tab-hidden-workspaces.md));
  (11) settings core slice delivered (`df4edc5`, `1c97c52`, `999f2f3`,
  `81986ab`; CI green; `just settings`;
  [evidence](changes/archive/windows-settings.md)): JSON store under
  LocalAppData, plain Win32 Apply/Revert UI, live gaps/visuals/Snap
  takeover, conflict list and presets (compatible disables 35 conflicting
  chords, authentic stays the default), system accent border default with
  KDE `#2a82da` fallback (yellow removed). Ratified 2026-10-08. Tray
  (Settings, Stop, shortcut-conflict warning, TaskbarCreated re-add) and
  first-run Authentic/Compatible prompt delivered (`60fd7bb`, `533f3c9`;
  [evidence](changes/archive/windows-tray-first-run.md)); tray workspace
  mode toggle omitted at first, then delivered with per-workspace
  tiling/floating modes and `core.workspace.default_tiled` (`90ee5c2`,
  `0fa3d29`, `ace352c`, `618683b`;
  [record](changes/archive/windows-workspace-tiling.md)); maximized windows
  on floating workspaces now match KDE (R-MAX-03 resolved).
  Remaining: broader rebinding, physical checks incl. real Explorer
  restart.
  Original item 11 scope: settings
  (UI with Apply/Revert parity, including Snap takeover off; decide the final
  accent/configured border default and remove the temporary yellow default;
  per-binding OS-conflict list with compatible/authentic quick-set presets,
  first-run prompt ratified 2026-10-08; [decision](decisions.md#settings-tray-and-first-run)).
  Provisional choices 2-6 accepted by the user (2026-10-03).
## Windows gaming coexistence history (condensed; containment parked, M16 pending)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Condensed; parked containment, detection block and unmeasured game cost remain active in the backlog.

- P0 | Windows gaming coexistence | Windows release gate after multi-output,
  before any Windows release (user 2026-10-10); not now despite P0.
  User (2026-10-03): (a) authentic mode
  until settings exist: our bindings must not leak to OS shortcuts (Win+G
  opened Xbox Game Bar when the foreground was unmanaged, and Xbox mode was
  entered during agent tests; user dogfood 2026-10-03: Win+F11 entered Xbox
  mode, so Win+F11 collides with the OS full screen experience shortcut);
  (b) detect Xbox mode (full screen
  experience), pause tiling, effects and shortcuts, remember windows and
  workspaces, restore on exit; (c) research alternate Game Bar access and
  anti-cheat false-positive risk (LL keyboard hook, overlays, window
  moves). Status 2026-10-03: (a) authentic containment offline-verified,
  CI green (`962b0f3`, `5746fca`; the Win+F11 leak was our deliberate
  pass-through). Physical test after those commits FAILED: Win+F11 still
  opens the Xbox mode prompt and Win+G still opens Game Bar (traces
  `run-01dd53116aa74f37.log`, `run-01dd5311a7282421.log`). PARKED
  2026-10-03 (`2c9c11c`, `bbe5b0f`, `6c4ffad`): a provably current,
  consumed Win+F11 tap still produced the Xbox prompt, also after
  reinstalling our hook last; no official-API fix found; prior art has
  none. User 2026-10-08 keeps Authentic ownership with the limitation
  disclosed; Compatible/disable/rebind provide Game Bar access. Containment
  remains parked for later test and user review, grouped with this work;
  host-setting, registry/policy or dedicated-hook-thread alternatives remain
  unselected and require separate authorization. (b) user chose
  documented-signal-only detection (2026-10-03); blocked until one exists.
  (c) researched; alternate Game Bar access deferred (see Future).
  - User physical re-check 2026-10-12, scoped single-output Notepad/Paint,
    base `e506350` plus pre-retirement Windows changes: every Win+F11 owned
    fullscreen/exit press also raised the Xbox full screen experience prompt.
    Known parked containment issue confirmed; user wants a fix eventually.
    Remains parked in gaming coexistence, with no new containment work.
  - 2026-10-16 incident: consumed fullscreen, generic fullscreen suspension,
    later terminal workspace hide and floating/exception refusal confirmed;
    invisibility/exception origin unresolved in normal logs. No runtime fix
    selected. [Diagnosis and prevention options](changes/windows-gaming-coexistence.md#incident-diagnosis-and-prevention-options-2026-10-16):
    recommend user-chosen fullscreen/float rebinds for conflict avoidance;
    Xbox-mode-off targets F11 only, GameDVR capture policy is not a Win+G
    suppression guarantee. Exact-chord containment remains open; bounded
    visibility/reveal/exception diagnostics needed before another recovery repro.
  - M16 gaming-cost measurement: compare the event-driven owner's incremental
    CPU, frame-time and input cost against a without-tiler game baseline,
    including suspended work/effects. The unconditional 100 ms owner poll was
    removed 2026-10-17; non-game idle evidence showed 75-98% less CPU before
    the final log-notification correction. Post-correction eight-case idle
    matrix, bounded hidden-pager wait evidence and later auto-hide response
    measured 2026-10-17: hidden CPU was granularity-limited near-zero, not
    proven zero. Visible timeout attribution and fractional-deadline correction
    delivered 2026-10-18 (0.524/s -> 0.498/s; no demonstrated CPU reduction);
    native event alternatives researched. Conditional pre-0.1 event-coverage
    probe completed 2026-10-18: tested burial emitted desktop-parent reorder
    outside current subscriptions, but complete native coverage remains unproven.
    User 2026-10-19 option A amends 2026-10-18 decision 5: keep the
    visible-pager 2 s topmost-recovery timer for 0.1; remaining coverage is
    deferred to the P2 post-0.1 pager event-coverage item below.
    [Probe and limits](changes/archive/windows-pager-event-coverage.md).
    Chevron response and physical input/preview remain pending. Existing
    approved icon-cache refresh explains visible managed completion wakes;
    timer replacement requires proven complete native events.
    [Idle record](changes/archive/windows-owner-event-idle.md).
    Scoped follow-up 2026-10-18: chevron discovery again found no verified
    target; no click or response result. External gap-file edit/restore woke the
    owner and changed helper geometry; edited gaps reported two mismatches,
    so correct-convergence acceptance stays open. Matched two-helper
    8/8 -> 12/12 -> 8/8 at `b274797f` reproduced two stable default-helper
    physical position deltas, while PMv2 helpers matched exactly. Independent
    paired outer/DWM readbacks exclude a stale-observer-only artifact;
    awareness/creation visibility/hints remain confounded. Host-vs-adapter
    mechanism parked with an exact controlled fixture; no tolerance/fix
    ([evidence](changes/windows-best-effort-admission-live.md#native-gap-discriminators-2026-10-18)).
    First-run baseline/readback
    limits are explicit in the record; no independent exact-restoration claim.
    Game cost remains unmeasured; idle improvement, suspension and nominal
    wake frequency are not game acceptance evidence.
    Apply the M04 bar: aim for zero; demonstrate any unavoidable residual
    minimal and imperceivable. Physical game measurements are user-owned.
    [principle](principles.md#gaming-compatibility)
    Non-native measurement lessons:
    [event/pump boundary](research/non-native-tiler-host-interaction.md#layer-layout-authority-events-and-reconciliation-dim-1),
    [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
    [evidence](changes/archive/non-native-tiler-host-interaction.md).
  [record](changes/windows-gaming-coexistence.md)
  [decision](decisions.md#gaming-safety)
## Windows workspace presentation research (condensed; pager MVP delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Condensed; remaining anchors, acceptance legs and the cloak spike stay active in the backlog.

- P0 | Windows workspace presentation research | After multi-output, before
  Windows release (user 2026-10-10). Windows work combines hidden-workspace
  Alt+Tab/taskbar semantics and taskbar workspace indicator (parity 10),
  producing options plus a recommendation for the user. Existing Alt+Tab
  research (`580c766`, `8129b37`;
  [note](research/windows-port/alt-tab-hidden-workspaces.md)):
  keep SW_HIDE (no supported reinsertion); public off-screen parking is the
  only official candidate, with visibility/recovery costs; native desktops
  and private cloaking need separate API decisions. Activation behaviour
  is matrix row R-WS-07. Research deliverable complete 2026-10-17: per-option
  comparison, owned-helper live visibility baseline, and reference-outcome
  discriminators ([note](research/windows-port/alt-tab-hidden-workspaces.md)).
  Mechanism plus switch-versus-pull selection moves to post-0.1 research;
  shell visuals stay user-observed TBD. User decision 2026-10-17: keep
  `SW_HIDE` for 0.1 ([decision](decisions.md#workspaces)).
  [Delivery evidence](changes/archive/windows-workspace-presentation-research.md).
  Non-native inputs:
  [workspaces/Alt+Tab](research/non-native-tiler-host-interaction.md#workspaces-monitors-overview-and-alttab-dim-4),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).
  - Taskbar indicator slice pulled forward as a high-priority Windows dogfooding
    blocker (user 2026-10-14). [Indicator options and recommendation](research/windows-port/taskbar-workspace-indicator.md):
    User 2026-10-14 target: a KDE Plasma pager-style
    indicator in the taskbar of every output, showing only that output's
    workspaces: numbered workspace cells, empty cells a slightly darker
    shaded rectangle, each window a lighter rectangle with a light 1 px
    border and its app icon centred, the active workspace in an active
    colour. Pager is visualization plus click-to-switch; existing tray icon
    stays for settings/toggles. AppBar rejected (maximized-window interaction);
    Explorer injection/hooking (Windhawk-style, TaskbarWidgets-style XAML
    hooks) rejected, reconfirmed 2026-10-15 for shell stability, Windows
    update breakage, AV/anti-cheat flags and security posture. Win11 has no
    supported reservation among taskbar app icons; tray-icon placeholders
    rejected (position not guaranteed, icons hideable). Pager is always
    anchored left of the system tray: it may overlap app icons when full but
    never the tray, and never shrinks or hides due to crowding. Enabled by
    default with an opt-out setting; users avoiding overlap use the tray
    number plus menu pager. MVP delivered 2026-10-16 on the primary bottom
    taskbar: numbered active/empty cells, real scaled window miniatures,
    click-to-switch and live refresh. User-approved 2026-10-16 external
    nonactivating topmost tool-window hosting, anchored left of the tray,
    bypasses unresolved child-parenting failures;
    no injection, AppBar, placeholder icons or child manifest dependency.
    Default-on Settings opt-out, fresh tray-left geometry, DPI-scaled cells
    and fonts, retry/TaskbarCreated recovery, safe topmost reassert and
    fullscreen/elevation hiding are implemented. Owned-window native captures,
    two-way pager selection, create/send/close refresh, managed-fullscreen
    hide/return, synthetic TaskbarCreated and exact cleanup passed
    ([record](changes/archive/windows-taskbar-pager.md),
    [research](research/windows-port/taskbar-workspace-indicator.md#product-overlay-2026-10-16)).
    Native 2026-10-17: actual Explorer restart recovered the tray and pager from
    fresh geometry; auto-hide on/off and live shared-settings opt-out passed
    with original settings restored. User-confirmed 2026-10-17, as shipped:
    auto-hide hides the pager throughout auto-hide, retaining the tray
    number/menu. Resource counts
    plateaued over several minutes. Calculator paints a plain gray miniature
    without packaged artwork; User 2026-10-17 keeps it as implemented for now,
    user reviews dogfooding. An earlier missing-miniature report was an
    image-analysis error, not a source defect
    ([record](changes/archive/windows-taskbar-pager-lifecycle.md)).
    Remaining: secondary taskbar tray anchors and physical multi-output;
    physical mouse/game/anti-cheat acceptance; DPI reconfiguration;
    non-bottom taskbars; accessibility/theme/cost checks.
    Whole-strip overcapacity behavior confirmed 2026-10-16: no pager
    drawn, reason logged, tray number/menu remain. Physical proof pending.
    Miniature icons are now
    implemented through asynchronous window/class/executable lookup, with
    user-approved 2026-10-16 DPI-aware 16/8 logical px sizing and omission when
    missing or too small. Limits cleanup delivered offline: fixed five-second
    creation/spawn retries, uncapped deduplicated icon queues, latest click only.
    Native appearance remains pending; exact packaged/UWP
    artwork was deferred until the user saw Calculator physically (now seen);
    User 2026-10-17 keeps the plain-gray miniature as implemented for now, user
    reviews dogfooding
    ([active record](changes/windows-taskbar-pager-icons.md)). Urgency
    acquisition remains separate. Secondary/non-bottom/invalid
    geometry currently refuses safely and logs, never guesses a tray edge.
    Stopgap delivered 2026-10-15: tray icon number plus in-menu workspace pager
    with single-output native acceptance
    ([record](changes/archive/windows-tray-menu-pager.md)); no popup, no click
    rebinding. The visual pager now complements this stopgap.
  - D27 5a (user 2026-10-14): hidden-window urgency marks its workspace urgent
    in the indicator, cleared on visit. Native shell flash was heard for hidden
    owned helpers, but visible urgency was not established and no product
    observer/marker exists. [Evidence](changes/archive/windows-d27-activation-verification.md).
    Appearance and technical request-versus-selection classification remain
    unselected. User-approved 2026-10-16 Windows 5b target: OS-permitted Switch
    and focus over fullscreen/games, no urgency-only downgrade; implementation
    pending, default Mark urgent only and non-permitted requests unchanged.
## KDE follow-ups from the Windows port (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivery evidence; user-owned live checks stay in the condensed entry.

- P1 | KDE follow-ups from the Windows port | Audit 2026-10-05
  ([note](research/cross-platform-core/post-windows-audit.md)); user order
  B1/B2, B3-B5, B7, B8. B1/B2 stale maximize/sticky attempted-state
  refusals repaired offline (one attempt per activation; 843 KWin tests
  pass); B3-B5 shared-core KDE fixtures added, no defect found
  ([record](changes/archive/kde-post-windows-followups.md)). User-owned live check
  (2026-10-05): Meta+M, then Meta+Shift+G refused while maximized, then
  Meta+M restored, as decided. DEFECT: Meta+Shift+G on a tiled window
  floats it sticky but focus moves to another window (Meta+G keeps focus);
  contradicted the exact-toggle focus retention decision. Supplied trace plus
  real Engine replay identified the follow-up foreground flag reconcile
  actuating survivor focus (pre-existing source defect, not B2). Repaired
  offline by skipping that activation over a focused float/sticky subject;
  faithful regression red/green, 846 KWin tests and all gates pass. User
  live re-check (2026-10-05): normal and sticky float focus correct
  ([evidence](changes/archive/kde-post-windows-followups.md#test-system-re-check-user-owned)).
  Q8 offline repeat evidence delivered: second same-reference maximize/sticky
  native-change cycles, duplicate sticky notification convergence and one native
  attempt per delivered maximize callback; existing maximize/fullscreen/geometry
  repeat fixtures reused ([record](changes/archive/kde-repeat-offline-fixtures.md)).
  Still pending, user-owned live: native-change repeats and held-key autorepeat;
  physical delivery/suppression remains
  [TBD](spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator).
  User decisions 2026-10-05: B6 infeasible minimums use origin+minimum on
  both platforms (Windows already does it); KDE delivered offline
  `cf6ab31` (920 KWin / 1107 Rust tests, CI green; client shortfalls get
  three reassertions then quiet acceptance; live steps in
  [record](changes/archive/kde-minimum-origin-placement.md)). Q3 born-maximized admission follows COSMIC:
  tile with a reserved slot and keep the maximize as an overlay, no launch
  unmaximize (KDE and Windows); KDE delivered offline `9b612be` (901 KWin
  tests, CI green; live steps in
  [record](changes/archive/kde-born-maximized-overlay.md): born-maximized
  launch beside a sibling, native restore into the slot, session-restore
  no loop). User Q3 scope decision 2026-10-07 also covers R-MAX-03:
  KDE floating-to-tiled reserved-slot overlay delivered offline (921 KWin /
  1107 Rust tests); no admission clear, later unmaximize lands in the slot,
  synchronous/repeated maximize signals settle. Native check pending in
  [record](changes/archive/kde-maximized-floating-retile-overlay.md).
  B9 overlaid intentional unfloat: user 2026-10-08 selected unmaximize then
  fresh admission. Shared-core regression/KDE delivered offline
  ([record](changes/archive/maximized-intentional-unfloat.md)); Windows refusal
  remains a gap owned by handoff item 15; COSMIC confirmation and KDE native check pending.
  Pinned 11-WM source comparison added. B7
  movement-only underlay A/B delivered offline (Meta+Shift chord or focused
  native user move; host-matched native build and all gates pass; paired
  FFI signature change; C parked); user tested A/B on a single-output setup
  (2026-10-05): all good; multi-output remap checks later. Next: B8 after user acceptance of the existing KDE
  shortcut controls. Core extraction: no new move until macOS starts.
  Shared restart store in core (float intent, tile overrides, possibly positions); platforms supply window identity and storage location (user 2026-10-08).
## Windows parity with the 2026-10-05 KDE session (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivery evidence; physical journeys stay in the condensed entry.

- P1 | Windows parity with the 2026-10-05 KDE session | Next Windows session:
  (a) float-origin directional focus and Meta+Shift+arrow half-snaps
  (R-FLT-07..11; Windows delivered offline 2026-10-12,
  [record](changes/archive/windows-floating-directional-navigation.md), ordinary
  physical single-output user acceptance 2026-10-12; variants/game/DPI pending);
  (b) Q3 born-maximized
  reserved slot plus maximize overlay, replacing the one-shot admission
  clear on Windows (and KDE, same decision); user 2026-10-07: Q3 also
  covers the R-MAX-03 floating-to-tiled case (stays maximized over a
  reserved slot). KDE R-MAX-03 and R-MAX-06 delivered offline, native checks
  pending ([scope record](changes/archive/kde-maximized-floating-retile-overlay.md));
   Windows parity (b) delivered offline 2026-10-12
   ([record](changes/archive/windows-maximized-floating-retile-overlay.md));
   born R-MAX-06 uses the delivered 2026-10-11 setting, R-MAX-03 independent;
    ordinary single-output retile/restore and both-values born Paint launch
    user-accepted physically 2026-10-12; hidden/session/fixed/game/DPI pending.
  (c) B9 overlaid unfloat: unmaximize then fresh-admit, selected 2026-10-08;
  remove the Windows refusal, with the user's COSMIC check as confirmation
   only (handoff item 15; KDE and Windows delivered offline,
   [Windows record](changes/archive/windows-maximized-intentional-unfloat.md),
    ordinary native clear/frame/focus agent-observed 2026-10-12; physical B9,
    marker/restart/failure/game/DPI pending); (d) keyboard resize delivered
  2026-10-11, base `f794cf9` + delivery commit (handoff item 6,
  [record](changes/archive/windows-keyboard-resize.md)); physical interception
  remains user-owned. Non-local workspace modes, including mode-aware hotplug,
  delivered offline 2026-10-15; physical two-output journeys remain user-owned
  ([record](changes/archive/windows-workspace-scope-hotplug.md)).
  [decisions](decisions.md#window-state-float-sticky-maximize-fullscreen)
  [audit](research/cross-platform-core/post-windows-audit.md)
## Reference-consensus additions (delivered)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivery summary; Windows adapter detail lives in the handoff section below.

- P1 | Adopt reference-consensus additions | User 2026-10-07 accepted the
  recommendations from the consensus Table A; items 1-5 shared
  core/KDE delivered (item 1 single-output user-confirmed, items 2-5 offline),
  each through the shared Engine where possible, KDE and Windows unless
  noted ([consensus](research/reference-wm-consensus.md)):
  R-MOV-08 allow vertical move onto another output once local movement is
  exhausted; R-LAY-01 parent split-axis toggle; R-WS-08 previous-workspace
  toggle; R-WS-11 wrapping
  next/previous workspace switch; R-WS-12 move whole workspace to another
  output; R-WS-14 send window to next/previous workspace; R-OUT-04 send
  window to output; R-MAX-09 carry fullscreen across workspace send on
  Windows (observe KDE first); R-SPC-04 float fixed-size windows on
  admission; R-RST-01 keep intentional floats floating across owner
  restart, including Windows sticky floats staying sticky (R-FLT-05, user
  2026-10-07); R-RSZ-01 Windows keyboard resize (overlaps Windows parity (d));
  R-WS-01 separate send-and-stay command beside the existing follow send
  (user 2026-10-07; COSMIC Send/Move pair; stay unbound by default);
  R-MOV-03 setting: COSMIC wrap default, flat sibling
  swap alternative (user 2026-10-07); R-DRAG-08 Windows Win+drag activates
  the mover at press instead of on drop (user 2026-10-07).
  R-LAY-04: PARKED / OPEN by User decision 2026-10-09 until a genuine second
  layout exists; revisit when tabbed stacks are designed after 0.1. Whether tabs
  count as L2 remains a future decision; no active implementation.
  [Findings](research/workspace-local-layout-selection.md).
  Detailed selections 2026-10-07 (items 1-5 and D1 in
  [decisions](decisions.md#move-layout-and-output-commands)); KDE items 1-5 delivered
  offline, remaining implementation pending:
  Q2 R-SPC-04 shared core/Linux planner + KDE delivered offline under user
  decisions 2026-10-08 (normative as delivered); D1 predicate setting and
  functional naming [delivered offline](changes/archive/admission-and-move-settings.md);
  D5 first-fullscreen-exit and D6 tiling-enable check
  [delivered offline](changes/archive/fixed-size-exit-and-enable.md);
  D7 tile-override store [delivered offline](changes/archive/fixed-window-tile-override-restart.md), native checks pending;
  [record](changes/archive/fixed-size-admission.md). Windows Q2
  behavior remains unchanged; handoff item 13 supplies its exact wiring.
  Q3 R-RST-01 Linux planner/KDE intentional-float restart delivered offline
  under user decisions 2026-10-08 (normative as delivered): private
  session-scoped membership store, current-frame adoption, native-success
  settlement and diagnosed-empty availability fallback. Native acceptance
  remains pending; [record](changes/archive/kde-intentional-float-restart.md).
  Windows behavior remains unchanged; handoff item 8 carries its design seams.
  Q4 R-WS-12 core/Linux planner + KDE whole-active-workspace output migration
  delivered offline under user decisions 2026-10-08 (normative as delivered):
  four unbound follow actions, strict native per-output capability, retained
  domain rekey, source refill, floats/sticky split and fullscreen+maximized
  carry (D8 delivered offline 2026-10-09). Native acceptance pending;
  [record](changes/archive/kde-whole-workspace-output-migration.md),
  [D8](changes/archive/migration-overlay-carry.md).
  Windows item 14 now supplies real migration overlay observations and native
  carry, delivered offline 2026-10-14
  ([record](changes/archive/windows-workspace-output-migration.md)).
  Shared Rust core + KDE adapter are delivered KDE-side; separate Windows
  adapter wiring follows. Correctness over non-breakage: Windows build/behavior
  may break provided the handoff below lists the specific changes needed.
  Implementation order:
  1. R-WS-08 + R-WS-11 (KDE delivered; single-output native journey
     confirmed by user 2026-10-07, multi-output/presets pending;
     [record](changes/archive/kde-workspace-history-ring.md)):
     Meta/Win+Ctrl+Tab previous-view two-state toggle;
     Meta/Win+Ctrl+arrows and +H/J/K/L relative switch (left/up previous,
     right/down next). Local/global-unique per-output history/ring, shared
     one history/ring; every successful observed workspace change records,
     including native/send-follow/hotplug, not same-workspace activation or
     output focus alone. Stable IDs, surviving empties valid; removed,
     unassigned or out-of-recording-output-scope IDs clear; no-op until next
     recorded change, no recreation/ordinal reinterpretation. Disconnected
     output history discarded; reconnect selection never consults/restores
     history (1.5). Ring wraps all existing scoped workspaces, trailing empty
     and ordinals beyond 9 included; selection creates nothing. Authentic
     clears conflicting holders; Compatible disables conflicting arrows,
     letters remain (KDE KWin desktop-switch arrows; Windows native Left/Right
     ownership general knowledge, unverified in repo).
  2. R-WS-01 + R-WS-14 (shared core + KDE delivered offline; native journey
     pending; [record](changes/archive/kde-workspace-send-follow-stay.md)):
     keep numbered follow Meta/Win+Shift+digits; relative
     follow Meta/Win+Ctrl+Shift+arrows and +H/J/K/L. Numbered/relative stay
     bindable, unbound. Relative targets ordinal step in item 1 ring, not
     MRU, resolved once before transfer; fills trailing empty, normal
     lifecycle supplies next spare. Follow/stay both absolute/relative.
     Authentic clears KDE KWin window-desktop arrow holders; Compatible
     disables our arrows, letters remain; Windows ownership unknown.
     Explicit follow/stay now also reaches KDE's membership-only
     floating-boundary path, repairing its existing-default gap: default
     follows; stay preserves the source view/native boundary focus.
  3. R-MOV-03 (shared core + KDE delivered offline; native journey pending;
     [record](changes/archive/same-axis-move-setting.md)):
     global KDE `sameAxisMove` / Windows `core.same_axis_move`,
     `group-with-neighbor` default or `swap-with-neighbor`; Windows additive version-1 field,
     missing defaults to group. Apply to subsequent moves without tree
     rebuilding; KDE settings UI control. Swap-with-neighbor replaces R2c only for
     adjacent direct leaf siblings; shares travel with windows. Leaf/group
     rules unchanged; discriminating TBD rows before broadening. User
     2026-10-08: setting and both values take functional names with WM
     tooltips (functional value IDs); breaking pre-release configs accepted,
     no migration; functional rename [delivered offline](changes/archive/admission-and-move-settings.md), Windows settings wiring pending.
  4. R-LAY-01 (shared core + KDE delivered offline; native journey pending;
     [record](changes/archive/parent-orientation-toggle.md)):
     Meta+O / Win+O immediate-parent axis toggle including root,
     preserving order/shares/focus; sole root leaf no-op, no saved admission
     hint, long-edge rule unchanged. No KDE stock holder found; Windows
     orientation lock conflict: Authentic takes over, Compatible disables.
     Pending live check for user: Meta+O toggles root and nested immediate
     parent, twice restores order/shares/focus, lone window is a no-op.
  5. R-MOV-08 + R-OUT-04 (shared core/protocol + KDE delivered offline;
     two-output native journey pending;
     [record](changes/archive/four-direction-output-transfer.md)):
     local restructure/swap/escape first, then cross
     all four directions, including sole root leaf (horizontal too).
     Reciprocal edge-touch + positive-overlap adjacency on FULL
     output rectangles, horizontal too; no candidate no-op, unreadable
     refuses, no wrap. User decision 2026-10-09: select by window centre
     projection, then window-span overlap, then left/top; selection delivered
     offline ([record](changes/archive/position-based-output-selection.md)). Explicit output-follow Meta/Win+Ctrl+Alt+
     arrows and +H/J/K/L; stay bindable unbound. KDE arms absent from
     `kglobalshortcutsrc`, Windows ownership unknown; COSMIC's arm collides
     with resize-shrink, niri's with item 2. Target destination current
     workspace with ordinary send admission (remembered leaf, focus history,
     root) and command follow/stay. Initially tiled-subject eligibility,
     sticky excluded; floating boundaries membership-only, tiled sides
     reflow. Ordinary float transfer stays open.
     Full rectangles select adjacency only; work-area placement and R4 edge
     landing nearest the source remain. Explicit send reuses item-2 follow/
     source-MRU stay, with pinned source/target current-view arrival fences.
     Windows explicit send/selection delivered offline `fda618f`, 2026-10-14
     ([send](changes/archive/windows-explicit-output-send.md),
     [selection](changes/archive/windows-position-based-output-selection.md));
     physical two-output/mixed-DPI/suppression journeys pending.
  Bindings for other accepted additions remain to be chosen.
## Windows handoff checklists, protocol and seams (delivered offline)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Superseded as planning detail, retained as delivery evidence. Native acceptance stays active via Pending live checks and the physical checklist.

- P1 | Windows handoff: reference-consensus additions | D1, user 2026-10-07:
  Windows adapter pieces are wired after their KDE-side delivery.
  ### How to use this handoff

  - Historical source checks 2026-10-08 at
    `9bbc83b6bf37295cb2883d9d31880599e7bdf088` for items 1-5 and shared
    plumbing.
  - Historical revision check 2026-10-08 at
    `db31234f450e11af78406518150438c623ea91e4` for entries 6-12 and the
    matrix/spec corrections below; original revision provenance above is
    preserved. Deliveries through `1c26f2d` (2026-10-10) supersede this
    baseline, including output selection `a8e32ce` and items 17-20. Line
    references below are historical locators; re-find named symbols after pulling.
  - KDE/shared deliveries: item 1 `7f1a9ee`, item 2 `8d476ee`, item 3
    `0dc7518`, item 4 `fa15add`, item 5 `9bbc83b`. Item 1 has user-confirmed
    SINGLE-output live acceptance (2026-10-07, unspecified edge cases/presets).
    Items 2-5 are offline-delivered; multi-output/hotplug/preset and native
    journeys remain pending. None of this proves Windows native behavior.
  - D1: shared core plus KDE are delivered KDE-side; Windows adapter is
    delivered Windows-side. Correctness wins over Windows non-breakage, with
    exact repairs recorded here. Decision 2.3 permits behavior-preserving
    compile-only Windows fixes for items 2-5; those fixes are not completed
    Windows implementations.
  - Progress: scope, acceptance, exact pending work and evidence per
    implementation piece; archive on completion. Update linked
    Ours Windows matrix cells and spec Win cells with dated, revision-bound
    offline evidence; native outcomes stay TBD until user-tested. Preserve
    reference outcomes and product decisions; add decision delivery pointers.

  ### Execution order and shared Windows plumbing

  | Order | Deliverable | Dependency / boundary |
  | --- | --- | --- |
  | 1 | Item 1 local history/ring plus exact Ctrl/Tab input/settings support | Shared input foundation for items 2 and 5. Global-unique/shared runtime and multi-output remain separately pending. |
  | 2 | Item 2 explicit follow/stay, then relative sends | Reuse item 1 ring/action routing, not MRU target selection. |
  | 3 | Item 3 live same-axis setting | Independent; replace compile constant without rebuilding trees. |
  | 4 | Item 4 orientation action | Reuse input/catalog/live settings infrastructure. |
  | 5 | Multi-output foundation `2c05ee5` + `f2169d6`; explicit output send delivered offline `fda618f` 2026-10-14 | Local-first all-four transfer, horizontal cross-focus and pointer drops retained. Explicit Win+Ctrl+Alt+arrows/HJKL follow, four unbound stay rows, ordinary admission to target current workspace, floating-boundary tiled-side reflow and commit-last fences now wired. Native gates: 1,892 passed, 0 failed, 1 ignored ([send record](changes/archive/windows-explicit-output-send.md), [foundation and physical steps](changes/windows-multi-output-foundation.md)). Physical two-output/mixed-DPI/suppression pending; non-local modes, items 12/14/19, migration selection, hotplug and ordinary-float transfer remain separate. |
  | 6 | Keyboard resize R-RSZ-01 (parity d) | Independent; needs fresh Alt-capable trigger plus dedicated resize intent. No dependency on items 1-5 except shared modifier routing. |
  | 7 | Press-focus R-DRAG-08 delivered `9bb2b6f` 2026-10-11 | Win-drag arm only; R-DRAG-07 stationary-source/moving-preview split retained. Offline revalidated 2026-10-17 at `5d953e98`; native gates passed (2,448 passed, 0 failed, 1 ignored). [Delivery and accepted primary live evidence](changes/archive/windows-drag-press-focus.md); physical input/feel and native edge checks remain pending. |
  | 8 | Restart R-RST-01/R-FLT-05 sticky/intentional persistence | Independent of items 1-5; KDE Q3 plus D7 tile-override store delivered offline. Ordinary/sticky intent must remain distinct from automatic fixed origin and recovery authority; Windows mechanism selected 2026-10-10 (prefixed on-window markers, item 8 below). REQ-RST-01c is NORMATIVE D25 (User 2026-10-12), membership/set/focus native acceptance pending. Coordinate D7 membership with item 13. |
  | 9 | Fullscreen send R-MAX-09 (Windows carry; NOT the parity-queue multi-output foundation) | Depends on handoff item 2 follow/stay wiring only; same-output workspace carry, no cross-output claim. |
  | 10 | Float/half-snap parity (a) R-FLT-07..11 | Windows delivered offline; ordinary focus/half-snaps physically user-accepted 2026-10-12, scoped single-output normal owner Notepad/Paint. Tie/sticky/clamp/preset/fixed/game/DPI pending; cross-output item 5. [Record](changes/archive/windows-floating-directional-navigation.md). |
  | 11 | Born-max/floating-retile overlay parity (b) incl R-MAX-03 | Windows delivered offline; ordinary retile/restore and both-values born Paint launch physically user-accepted 2026-10-12, scoped single output. Tray focus observation only; hidden/session/fixed/game/DPI pending. [Record](changes/archive/windows-maximized-floating-retile-overlay.md). |
  | 12 | Non-local workspace modes parity (d) | Offline-complete 2026-10-15: stages A/B/C deliver scope policy/settings, owner selection/send/history/swap/trailing and mode-aware hotplug. Locked gates 2,095 passed / 1 ignored, including 17 new topology tests; single-output native select/previous/relative/send regression passed in all modes with clean stop/settings restore. Physical two-output journeys user-owned in [pending checks](backlog.md#multi-output-setup); shared choices remain tentative ([final record](changes/archive/windows-workspace-scope-hotplug.md)). |
  | 13 | Q2 fixed-size float admission R-SPC-04 | Windows delivered offline 2026-10-11, prerequisite a41fcb5 plus runtime delivery: max-track observation, predicate/schema/UI, D1-D8 lifetime/origin and fixed-origin D7 markers. Native/game acceptance pending; fixed-override maximize route tentative. [Record](changes/archive/windows-fixed-size-admission.md). |
  | 14 | Q4 whole-workspace migration R-WS-12 | Windows local adapter including D8 carry and item-16 largest-edge selection delivered offline 2026-10-14; 1,950 native MSVC tests pass, one ignored ([record](changes/archive/windows-workspace-output-migration.md)). Core/KDE already delivered offline. Physical two-output journeys pending; item 12 remains the non-local-mode dependency. |
  | 15 | B9 / M09 maximized intentional unfloat R-FLT-06 | Windows delivered offline; ordinary scoped native clear/observe/fresh admission and focus agent-observed 2026-10-12. Physical B9/fixed marker/restart/failure/game/DPI pending; independent of Q3 retile. [Record](changes/archive/windows-maximized-intentional-unfloat.md). |
  | 16 | Position-based output selection: Windows window paths and migration delivered offline 2026-10-14 | Window paths delivered `fda618f`; largest-edge migration dependency closed with item 14. Both reuse shared FULL-rectangle ranking with pinned transfer fences; pointer drops retain pointer targeting ([record](changes/archive/windows-position-based-output-selection.md)). Latest native gates: 1,950 passed, 0 failed, 1 ignored. Physical two-output checks pending. |
  | 17 | G-05 per-axis-zero predicate | Windows delivered offline 2026-10-11 with item 13, shared predicate reused; converted partial-zero orientations/default/either and subsequent-admission controls pass. Native hint representation pending. [Record](changes/archive/windows-per-axis-zero-predicate.md). |
  | 18 | G-06 maximized directional focus/move | Windows delivered offline; tiled focus fence and clear-before-tiled-move/float-four-halves physically user-accepted 2026-10-12, scoped single output. Failure/race/preset/fixed/game/DPI pending. [Record](changes/archive/windows-maximized-directional-navigation.md). |
  | 19 | G-37 migration source-refill setting | Windows additive schema-v1 config, UI Apply/Revert and live runtime/scoped MRU delivered offline 2026-10-14 with item 14 ([record](changes/archive/windows-workspace-output-migration.md)); native journeys pending. Shared core/KDE delivered offline 2026-10-10. |
  | 20 | G-D2 maximized send carry | Windows same-output numbered/relative follow/stay verified agent-observed 2026-10-11 with item 2; KDE offline. Physical/cross-output pending; item 5 foundation required. |
  | 22 | D16 ordinary/combo sends and D23 best-effort restore/close cleanup | Windows D16 and D23 adapters delivered offline 2026-10-13; native carry/exit, minimize/restore/close checks pending. D27 2026-10-14 at `f4b9f41`: fresh background current/hidden requests denied without focus/workspace switch; native shell flash heard, visible urgency unproven. Previously attached caller's permitted activation converged; no product classifier/marker. User-approved 2026-10-16 Windows 5b implementation pending via the P2 `Activation request` setting item (standalone entry folded User 2026-10-17): OS-permitted Switch and focus honors activation over fullscreen/games; default urgency-only/non-permitted unchanged. Clean permitted requests, mark/clear and game outcomes TBD; presentation/technical classification unresolved. D23 source slot: User decision 2026-10-17 option B (forget on confirmed rehome, later return fresh); shared-core implementation approved, not implemented ([Tier 1 research](research/d23-minimized-rehome.md)). [D27 evidence/options](changes/archive/windows-d27-activation-verification.md), [Windows D16](changes/archive/windows-d16-floating-transfer.md), [Windows D23](changes/archive/windows-d23-restore-close.md), [verification and exact journeys](changes/archive/windows-item22-verification.md). |
  | 23 | Born-maximized shared policy | Windows delivered offline 2026-10-13: core parsing/default/clear-keep policy consumed directly, shared fixture conformance; behavior, labels, schema and subsequent-admission semantics preserved, including the User 2026-10-17 option-A approved fixed-override maximize routing (shared policy, not implemented). Native gates 1,847 passed, 0 failed, 1 ignored; existing user-owned native journeys pending. [Record](changes/archive/windows-born-maximized-shared-policy.md). |
  | 24 | Underlay default `#662a82da` | Windows render/settings and CLI/proof mirrors delivered offline 2026-10-13, matching KDE candidate C (40% accent blue, for now). Explicit saved/CLI colors preserved; schema v1, no migration. Native gates pass (1,792 passed, one ignored); user black/white/grey visual check at 100%/150% pending. [Record](changes/archive/windows-underlay-default.md). |
  | Later | Unsafe FFI consolidation | Required after current Windows release-gate work (timing tentative); one dedicated minimal FFI module per crate, forbid elsewhere. |

  Q2 R-SPC-04 occupies handoff item 13 below; Q3 R-RST-01 KDE delivery is
  complete offline and updates existing item 8. Q4 R-WS-12 fills item 14.
  Q5 R-LAY-04 is PARKED / OPEN (User decision 2026-10-09) until a genuine
  second layout exists; no KDE piece or Windows handoff implementation is active.

  - Source notation: `src/...` / `tests/...` below are under
    `crates/tiler-windows/`; `core/...` is `crates/tiler-core/src/`;
    `protocol/...` is `crates/tiler-protocol/src/`. KDE paths start `kwin/`.
    Proposed Windows IDs below are new implementation names mirroring KDE
    suffixes, not existing rows; existing Windows digit IDs are retained.
  - Verified input limits: `src/snapkey.rs:1012` `SnapClassify::push` rejects
    fresh Ctrl/Alt chords at :1101; `push_owned` :1146/:1250 also rejects them.
    Consumed holds retain their paired verdict when modifiers change.
    `ChordRemap` :130 and `ChordDisable` :144 carry only VK+Shift.
    `src/settings.rs:389` `parse_chord` ALREADY parses Ctrl/Alt tokens, but
    `vk_for_key_name` :337 supports letters/digits/F1-F24/arrows, not Tab or
    Escape. Add Tab for item 1; Escape currently only passes through for drag
    cancellation (`src/snapkey.rs:54-56`), not a selected binding addition.
    `validate_bindings` (`src/settings.rs:1317/:1363`) and
    `apply_rebind_text` (`src/settings_ui.rs:403/:425`) reject Ctrl/Alt rebinds.
  - Items 1/2/5 share changes to `BindingFamily` (`src/settings.rs:475`),
    `binding_wants_shift` :1032, `binding_canonical_vk` :1044,
    `effective_bindings` :1071, `build_remap` :1154, `build_disabled` :1206,
    `validate_bindings` :1317. Carry exact Shift/Ctrl/Alt plus explicit ACTION
    through routing, duplicate detection, canonical hold slots and release
    pins. VK alone cannot identify focus/relative select/relative follow/output
    follow; an unbound stay row has no canonical default VK but must rebind.
  - Extend `src/snapkey.rs:427` `WorkspaceOp`, `WorkspaceIntent` :445,
    `Classified` :503, `MaskTrigger` :545, `SnapClassify` :580,
    `push` :1012/`push_owned` :1146, `QueuedWorkspaceIntent` :2055,
    `QueuedSnapEvent` :2116 and `classify_and_queue` :2217. Preserve origin
    binding (`SnapOrigin` :1976, `resolve_origin` :2003), down/repeat/up pairing,
    held Disable/rebind behavior, saturation verdict and E8 mask reservation
    (:2303). Do not just remove Ctrl/Alt guards and reuse Shift-derived actions.
    Authentic owns chords independently of foreground eligibility; native
    actions still require fresh origin, lifetime, suspension and scope guards.
  - Schema stays version 1 (`src/settings.rs:31`): binding overrides remain
    `bindings[id] = {state, chord?}` (:280-299), absent means Keep, missing
    `chord` defaults None. New stay rows have no default chords; Keep means
    unbound, not Disabled/effective interception. Repair `effective_bindings`,
    canonical-target validation and `src/settings_ui.rs:175/:231`
    `row_text`/`refresh_info`; do not parse empty sequence as a key.
  - Dynamic Settings rows use `binding_catalog` (:757), `refresh_list`
    (`src/settings_ui.rs:207`), `stage_state` :459 and `collect_draft` :345.
    Support Keep/Disable/Rebind, full-modifier duplicates and actual rebound
    conflict text. Preserve atomic Apply (`do_apply` :495) and saved-file
    Revert (`do_revert` :580). Live routing adoption uses
    `src/tiling_sys.rs:13432` `apply_live_settings` / :13547 `poll_live_settings`.
    Update hard-coded UI text in `src/settings_ui.rs:1088` preset explanation,
    :1093 "Shortcuts (48 rows)" and :1100 rebind/modifier note as rows land;
    unknown OS ownership must not be labeled conflict-free.
  - Presets use `src/settings.rs:1490` `apply_preset` (reset overrides, no
    replacement chords) and `compatible_disabled_ids` :1528 (currently 35).
    Update fixed counts/assertions as rows land. Compatible treatment is
    specified per item below, not copied from KDE foreign-holder tables.
    Windows uses owned-chord interception, not KGlobalAccel holder clearing.
  - Tray: `src/tray.rs:120` `unresolved_conflicts` warns only known-incomplete
    containment (G/F11, opt-in physical L); unknown containment adds no warning.
    `menu_items` :235 and `settings_for_choice` :345 already expose Settings,
    presets and workspace mode. Propagate catalog/presets/status tests; these
    five pieces do not select additional menu commands.

  ### Common definition of done and live-check protocol

  - [ ] Each item meets its behavior, adapter, settings/input and tests below;
    replace compile placeholders where required. Portable tests use real
    retained Engine replies, exact-lifetime and hidden-row policy, not just
    callback/catalog presence. Pure workspace/owner/tiling/settings/classifier/
    tray tests run on Linux; cfg(windows) owner/hook/UI wiring needs Windows.
  - [ ] Run native offline gates from ordinary PS7 with root mise-selected
    MSVC (`rustc -vV` host must be `x86_64-pc-windows-msvc`), per the
    [Windows runbook](windows-dev-environment.md#5-prove-linker-discovery-and-run-native-offline-development-gates):

    ```powershell
    mise exec -- rustc -vV
    mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows
    mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows
    mise exec -- cargo fmt --all -- --check
    mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows --all-targets -- -D warnings
    git diff --check
    ```

  - [ ] CI `.github/workflows/ci.yml`: `windows` :95-128, `rust` :32-47 and
    applicable `kwin` :13-30, `shell` :49-80, `native` :82-93 green after
    publication; `macos` :130-148 remains green.
    Linux handback: `devenv shell --impure -- cargo test --workspace`,
    `devenv shell --impure -- cargo clippy --workspace --all-targets -- -D warnings`,
    `devenv shell --impure -- cargo fmt --all -- --check`; shared changes also
    need applicable KWin/mock-shell/native checks and `just check-portable`
    through Linux/CI. Record unrun gates as pending.
  - [ ] Update the linked Ours Windows matrix cells, spec `REQ-*` Win/status/
    shortcut rows, decision delivery pointers and one archived change record
    per piece. Keep native pending separate from offline completion. Preserve
    existing TBD order/focus/lifecycle outcomes. New ambiguities need shortest
    discriminating matrix rows with unsupported outcomes TBD.
  - Live steps below are a USER queue, not mutation authorization. Read
    [live Windows testing](live-windows-testing.md) before planning them. User
    owns physical input and ordinary-app dogfood; agent native experiments need
    separate classes/resources/chords/duration/end condition and identified
    disposable owned windows. Follow the approved dependency list/mise route;
    installs are user-owned. This docs handoff ran no live tests.
  - EACH live journey: record OS build, revision/diff, artifact path/SHA-256,
    exact owner PID/start/session/integrity, display full/work rectangles/DPI
    and permitted windows; prove independent stop/restore first. User starts
    the authorized tiling owner
    (`mise exec -- just dev trace`; foreground full tiling,
    `lifecycle-smoke` is the non-tiling loop),
    applies the specified preset/settings, performs the steps, then runs
    `mise exec -- just stop`. Verify hook release,
    hidden-window reveal and owned session-setting restoration. Forced loss
    needs separate authority and the exact-owner recovery ladder. Callback/API
    success alone does not prove physical OS suppression.

  ### Item implementation checklists
  - Item 1: Windows local history/ring and exact Ctrl/Tab settings/input/UI
    delivered offline 2026-10-10 (base `91db9d2` + delivery commit,
    [record](changes/archive/windows-workspace-history-ring.md)); native gates
    passed; native CLI toggle/ring/>9 and UI journeys also agent-observed
    2026-10-11 with [item 2](changes/archive/windows-workspace-send-follow-stay.md).
    Physical chords/preset OS effects pending; non-local/multi-output parked.
    KDE adapter delivered offline, no shared Rust core/API changes. Historical checklist:

    #### Item 1 behavior and reference seams

    - Normative: decisions 1.1-1.5; [spec](spec/functional-spec.md#workspaces)
      REQ-WS-08/11; [matrix](spec/reference-outcomes/workspaces.md)
      R-WS-08/11/15/16/17. Two-view toggle, not MRU traversal. Record successful
      observed numbered/relative/toggle/native/foreground/send-follow/hotplug
      view changes; never attempted setters, same-workspace activation or
      output focus alone. Local/global-unique history/rings are per-output;
      shared has one history/ring. Stable previous ID survives emptiness but
      clears on removal/unassignment/leaving recording-output scope. No-op
      until next recorded change; never recreate/reinterpret by ordinal.
    - Ring includes ALL existing scoped order, trailing empty and >9; wrap
      both ends, select creates nothing. Disconnect drops previous AND observed
      baseline. Reconnect selection never consults/restores history; prime a
      fresh baseline and record subsequent observed displacement/return changes.
      Observe before/after hotplug actuation so A->B->D records B, not A.
    - Core currently supplies only existing workspace lifecycle primitives:
      `core/workspace.rs:7` `select_existing`, :16 `resolve_send_target`, :21
      `WorkspaceFacts`, :29 `TrailingPlan`, :35 `plan_trailing`, :88
      `choose_displaced_destination`. Numbered resolvers stop at 9; do NOT
      implement the ring through those resolvers. No history/ring CoreCommand,
      protocol command or reply was added by `7f1a9ee`; this is adapter state.
    - KDE reference: `kwin/src/workspace-native.ts:990` `selectPrevious`,
      :1051 `selectRelative`, :2141 `recordHistoryObservations`, :2185
      `observeSharedHistory`, :2203 `scopedRingIds`, :2224
      `validatePreviousEntries`; `handleTopologySignal` :921 observes hotplug.
      Catalog :152-222; native compiled holder identities at
      `kwin/native-effect/shortcutreconciler.h:55-62`.
      [Offline fixtures @ b7a9843](https://github.com/beefsack/OmniTiler/blob/b7a9843/kwin/tests/workspace-previous-relative.test.ts),
      [delivery/live status](changes/archive/kde-workspace-history-ring.md).

    #### Item 1 catalog, conflicts and acceptance

    | Proposed Windows id | Default chord | Intent | Compatible |
    | --- | --- | --- | --- |
    | `workspace-previous` | Win+Ctrl+Tab | Two-view toggle | Keep |
    | `workspace-prev-h`, `workspace-prev-k` | Win+Ctrl+H, Win+Ctrl+K respectively | Previous ordinal | Keep |
    | `workspace-prev-left-arrow` | Win+Ctrl+Left | Previous ordinal | Disable |
    | `workspace-prev-up-arrow` | Win+Ctrl+Up | Previous ordinal | Keep |
    | `workspace-next-j`, `workspace-next-l` | Win+Ctrl+J, Win+Ctrl+L respectively | Next ordinal | Keep |
    | `workspace-next-down-arrow` | Win+Ctrl+Down | Next ordinal | Keep |
    | `workspace-next-right-arrow` | Win+Ctrl+Right | Next ordinal | Disable |

    - Authentic keeps all nine rows. Win+Ctrl+Left/Right native virtual-desktop
      ownership is UNVERIFIED in repository (decision 1.1), not measured here.
      Extend `src/settings.rs:954` `chord_conflict` past its Ctrl/Alt early
      return: identify those chords with text such as "Windows virtual desktop
      switch (ownership unverified in repository); override needs takeover,
      containment unproven live". Tab/letters/Up/Down get an honest ownership-
      unknown note, not a stock-holder or conflict-free claim. Compatible
      disables these two new rows for that recorded conflict; KDE disables
      all four arrows for different KWin holders. Test presets independently.
    - Exact native integration: `src/tiling_sys.rs:9819` `workspace_do_select`
      activates only after verified hide/reveal (:10061), then obtains fresh
      target focus/geometry. Add observation recording at that completed view
      transition, including when later focus/geometry fails; view evidence,
      not the final action success label, controls history. Dispatch through
      `workspace_tick` :11395 and its selection arm :11530; also cover
      `poll_workspace_cli_request` :11130, `poll_foreground_workspace` :11912
      and `sync_monitor_outputs` :12050. `chord_output` :8508 /
      `src/workspace_owner.rs:25` `output_context` resolve command scope.
    - State/resolvers/invalidation are the `ManagedWorkspaces` changes below;
      input/catalog/UI use the shared plumbing checklist above. No new
      persisted history/settings field or standalone history UI control;
      expose the nine action rows through the existing binding editor.
      Exact state sites: `src/workspace.rs:95` `ManagedWorkspaces`, :244
      `activate`, :255 `select`, :262 `select_trailing`, :297 `resolve_send`,
      :303 `resolve_send_trailing`, :508 `apply_cleanup`, :561
      `displace_output_to`, :603 `reconnect_output`, :673 `workspace_ids`.
      Add pure stable-ID toggle/ring resolvers without activating/appending;
      observe actual completed transitions separately from these setters.
    - [ ] Portable: add observed-change vs failed-attempt tests, repeated
      toggle WS1->WS2->WS3->WS2->WS3, same-view/output-focus isolation,
      surviving empty vs deleted/moved ID, disconnect/reconnect baseline,
      local/global-unique/shared scope fixtures, >9/trailing wrap and unchanged
      inventory. Extend `tests/snapkey.rs` / `tests/settings.rs` for exact
      Ctrl/Tab routing, extra modifiers, held live rebind/disable, queue/mask,
      duplicate modifiers and Compatible vs Authentic rows.
    - Agent-live 2026-10-17 admission: filling workspace 2 exposed spare 3,
      then filling 3 exposed spare 4; idle did not grow the ring. Follow-send
      pruned the emptied source and renumbered positions correctly. Both
      rendered pager sets remain unverified; [evidence and provenance limits](changes/archive/windows-admission-trailing-workspace.md#agent-live-follow-up-2026-10-17).
    - [ ] Windows-only: owner selection/foreground/CLI producers, native
      hook release/suppression and settings Keep/Disable/Rebind Apply/Revert.
      Local-only delivery must explicitly keep non-local/multi-output pending.
    - User journey: occupy WS1/2/3; select 1->2->3; Win+Ctrl+Tab twice must
      show 2 then 3. Select first, previous must reach last existing trailing
      empty; next wraps to first without creation. Populate through ordinal
      10 using normal trailing sends, then verify relative selection >9.
      Re-select same view, toggle unchanged; after verified send-follow toggle
      returns to source. Repeat Left/Right with Compatible (project disabled,
      record native desktop effect) and Authentic (project view switch, record
      whether native desktop ALSO switches), plus disable/takeover-off release.
      Multi-output user: R-WS-15 isolation and R-WS-17 disconnect/toggle/reconnect
      sequence; moved previous D clears on L if no intervening change.

    - `workspace.rs` / `ManagedWorkspaces`: stable-ID previous plus observed
      current baseline per output for local/global-unique, one shared scope
      when non-local modes land. Resolve previous and ordinal relative targets
      without activating, appending or invoking trailing creation; use every
      existing scoped ID, trailing empty and >9 positions, wrap both ends.
    - `tiling_sys.rs` / `workspace_do_select`: dispatch toggle/relative targets
      through existing verified hide/reveal/focus selection. Record actual
      successful view changes from numbered/relative/toggle, native/foreground
      activation, verified send-follow and hotplug, not attempted setters,
      same-view activation or output focus. `ManagedWorkspaces::activate`,
      cleanup, displacement and return must converge on that observation rule.
    - Invalidate removed/unassigned/out-of-scope previous IDs (including global
      swap and returning workspace); never reinterpret an ordinal or recreate.
      Discard BOTH previous and observed baseline on disconnected outputs;
      keep displacement associations separate. Prime a reconnect baseline,
      record any subsequently observed return change, never consult history
      for reconnect selection. Observe before/after hotplug actuation so two
      changes in one handler leave the actual immediately preceding view.
    - `settings.rs`, `snapkey.rs` and settings UI: add one Win+Ctrl+Tab toggle
      and eight separate Win+Ctrl+H/K/Left/Up previous, J/L/Down/Right next
      rows. Add Tab VK recognition and exact Ctrl modifier routing without
      conflating focus/move/digit arms; extend intents, queues, hold/repeat,
      suppression, remap and duplicate validation to include Ctrl (current
      rebind/suppression keys carry VK+Shift only; Ctrl arms are rejected).
      Authentic owns the new chords; Compatible disables Left/Right for the
      native-desktop conflict, keeps letters/Tab/Up/Down enabled with no
      invented holder claims. Physical native suppression needs Windows evidence.
    - Port KDE offline acceptance cases (toggle, producer/scope isolation,
      empty survival/removal, relocation/disconnect/reconnect, >9/trailing wrap,
      no creation, exact modifiers and presets); non-local modes/multi-output
      runtime remain the separately parked Windows work.
  - Item 2: Windows explicit numbered/relative follow/stay, bindable unbound
    stay rows and exact input/settings/UI delivered 2026-10-11 (base `553c65b`
    + delivery commit, [record](changes/archive/windows-workspace-send-follow-stay.md)).
    Native gates passed; scoped CLI/MRU/null/floating/>9 and UI outcomes
    agent-observed. Physical chords/presets and non-local/multi-output pending.
    Shared core/KDE offline delivery retained. Historical handoff:

    Agent-live 2026-10-17 [all-member sends](changes/windows-all-member-workspace-sends.md#agent-live-follow-up-2026-10-17):
    own Settings (automatic fixed-size fixture) passed numbered/relative,
    follow/stay, occupied/empty, hide/reveal and sticky refusal with its
    executable explicitly scoped. Second fixed-size application, both-pager
    interactions and comprehensive survivor/fullscreen matrix unverified.

    #### Item 2 behavior and shared contract

    - Normative: decisions 2.1/2.2/2.3 and R-WS-01 follow/stay selection;
      [spec](spec/functional-spec.md#workspaces) REQ-WS-01/01b/06/14;
      [matrix](spec/reference-outcomes/workspaces.md) R-WS-01/14/18/19/20.
      Numbered follow stays default; separate numbered/relative stay unbound.
      Relative target is item 1 scoped ordinal step, never previous-history,
      resolved ONCE before transfer, wrapping through trailing empty and >9.
      Filling trailing empty invokes ordinary lifecycle for the next spare.
    - Tiled follow focuses mover at target after verified transfer. Tiled stay
      retains source selection/visibility and source focused-removal MRU;
      null focus issues no setter, native removal focus stands. Admission,
      source collapse and destination geometry are identical for both intents:
      valid remembered destination leaf, then valid destination focus history,
      then genuine root fallback, long-edge split. Floating boundaries remain
      membership-only, floating frames untouched, tiled sides reflow; stay
      retains existing native boundary focus, no invented float MRU tracking.
      Sticky/intentional-float eligibility remains as shipped.
    - Exact core: `core/boundary.rs:93-100`
      `CoreCommand::SendToWorkspace {window,target_output,target_workspace,follow}`;
      :170-185 `CoreEvent` carries source `domain/domain_key/windows/outer_gap`
      and `target_domain/target_windows`. :328-337 `SendWorkspacePlan` fields
      `base_revision, policy_version, geometry, focus_domain, focus_leaf,
      follow, operation, preconditions`; :666 `CoreReply::SendWorkspace`.
      `core/session.rs:125` `SessionCommand::MoveToWorkspace` and
      `core/contract.rs:394` `LifecycleIntent::MoveToWorkspace` carry explicit
      follow. `core/session/ops/workspace.rs:103` `propose_send_impl` owns
      admission (:198) and source-MRU/null stay (:269).
    - `core/engine.rs:1865` `transfer_request` converges the complete canonical
      pair and immediately commits planned topology (:2140), not native
      success. Windows calls Engine directly; no second Session transaction or
      JSON roundtrip is needed. `protocol/planner_protocol.rs:3740-3750`
      `SyncCommand::SendToWorkspace` wire op `send-to-workspace` defaults
      omitted `follow` to true; native Windows must pass the actual intent.
    - KDE reference: `kwin/src/workspace-native.ts:1162`
      `resolveRelativeMoveTarget`; `kwin/src/plan-adapter-entry.ts:4964`
      `requestWorkspaceMove`, :5104 `requestWorkspaceRelativeMove`, :5156
      `transferResolvedWorkspace` (floating boundary); `kwin/src/workspace-send-adapter.ts:1309`
      `requestSend`, :2635 `followOnce`, :2915 `confirmStayOnce` (fresh arrival,
      pinned source visibility and exact MRU/null focus, no desktop switch).
      [Fixtures @ b7a9843](https://github.com/beefsack/OmniTiler/blob/b7a9843/kwin/tests/workspace-send-follow-stay.test.ts),
      [record](changes/archive/kde-workspace-send-follow-stay.md).

    #### Item 2 binding and acceptance checklist

    | Windows id | Default chord | Intent |
    | --- | --- | --- |
    | Existing `workspace-send-1`..`workspace-send-9`, `workspace-send-0` | Win+Shift+1..9/0 | Numbered follow; 0 reuse/append trailing |
    | Proposed `send-prev-h/k/left-arrow/up-arrow` (four ids) | Win+Ctrl+Shift+H/K/Left/Up respectively | Relative previous follow |
    | Proposed `send-next-j/l/down-arrow/right-arrow` (four ids) | Win+Ctrl+Shift+J/L/Down/Right respectively | Relative next follow |
    | Proposed `stay-workspace-1`..`stay-workspace-9`, `stay-workspace-0` | Unbound (ten rows) | Numbered stay; 0 reuse/append trailing |
    | Proposed `send-stay-prev-h/k/left-arrow/up-arrow`, `send-stay-next-j/l/down-arrow/right-arrow` | Unbound (eight separate rows) | Relative stay |

    - KDE catalog `kwin/src/workspace-native.ts:225-392` includes shifted-symbol
      aliases for numbered stay; Windows digits share the same VK with symbols
      (`src/settings.rs:830-834`), so no duplicate physical symbol rows.
      Use explicit follow/stay ACTION in shared input/remap/queue work, not
      focus inference or Shift alone. Existing CLI send remains follow unless
      its transport is explicitly extended for needed tests.
    - Authentic keeps follow rows and unbound stay. Windows Ctrl+Shift arrow
      ownership is UNKNOWN (2.1); no new Windows Compatible disables are
      selected from KDE's four Window One Desktop holders. Keep new defaults
      pending evidenced conflicts, display "Windows shortcut ownership unknown;
      containment unproven live" by extending `chord_conflict` :954 past its
      Ctrl/Alt early return (:956-957), including effective rebound chords.
      KDE Compatible disables four arrows; do not claim those Windows holders.
    - Replace `follow:true` in builder/test below with real command wiring.
      No new global follow setting/schema field; per-binding action encodes
      intent. Keep schema-v1 missing binding overrides at Keep/unbound as above.
    - [ ] Portable `src/workspace_owner.rs` / `tests/tiling.rs`: explicit true/
      false events, stamped targets preserve intent, actual Engine admission
      byte/topology equality, stay MRU vs null, hidden target no-write, relative
      wrap/trailing/>9/frozen target. Port real core/KDE fixture semantics.
      `tests/snapkey.rs` / `tests/settings.rs`: all bound and unbound actions,
      modifier arms, rebind-away defaults, full-key duplicates, live holds.
    - [ ] Windows-only: Engine/native-boundary tails, source-selected fences
      before writes AND focus, target hidden/reveal policy, recycled lifetime,
      changed view/mode/gaps, partial transfer reconciliation without replay.
      Run common CI/doc gates; item 1 history records follow but not stay.
    - User: WS1 A,B (focus A then B), WS2 C; Win+Shift+2 follows B, source
      collapses to A. Fresh fixture, bind a stay row through Settings to a
      validated unused chord (example Win+Ctrl+Shift+F6, after checking it is
      unused in the effective catalog); send B, WS1 remains shown/focused A,
      WS2 stays hidden. Select WS2 to inspect unchanged admission. Repeat relative next
      from WS10 into trailing E and previous from WS1 to pre-transfer last E;
      E fills and one next spare appears. Sole-B stay keeps source selected,
      no explicit focus setter; record native focus/lifecycle as TBD evidence.
      Tray-toggle WS2 floating, repeat both intents both directions: floating
      frames stable and only tiled sides reflow. Apply/Revert/unbind restores
      defaults; physical Ctrl+Shift arrows must not leak under Authentic.

    - Compile fix applied: `src/workspace_owner.rs:127-132` constructor sets
      `follow: true` at :131; the exhaustive command match in
      `send_event_binds_target_and_focused_mover` at :1286-1290 requires
      `follow: true` at :1290. Both preserve always-follow Windows behavior.
      Real wiring remains: `build_send_event` :70 must accept explicit
      `follow: bool` instead of the constant. `stamp_send_target` :138/:139
      already uses `..`; preserve intent while replacing target IDs.
    - Thread that parameter through every `build_send_event` caller:
      `src/tiling_sys.rs:10848` (`workspace_do_send`) and test calls in
      `src/workspace_owner.rs:1270,1726,1915,2070,2282,2588` and
      `tests/tiling.rs:2217`. Existing follow tests pass true; add false cases.
    - `src/tiling_sys.rs` / `workspace_do_send` :10552, reply match :10876
      and verified transfer/tail :10945 onward: retain the `SendWorkspace`
      payload, branch on `plan.follow`. Follow selects/reveals target and
      focuses mover; stay hides the transferred mover without selecting or
      revealing target, keeps source selected/visible, applies source-bound
      `plan.focus_domain/focus_leaf` through exact-lifetime leaf/window
      resolution (no setter for null), and reflows only writable source rows.
      Destination admission/structural geometry remain identical; target
      writes obey existing hidden-row fences. Do not infer intent from focus.
    - `src/tiling_sys.rs` / `workspace_do_send_native` :10314 and call :10692:
      thread the same explicit intent through floating boundaries. Membership
      transfer remains native-only, tiled sides reconcile, floating frames
      stay untouched; follow uses verified target selection/mover focus;
      stay keeps source view and existing native boundary focus, no target
      selection. Sticky/intentional-float eligibility remains unchanged.
    - Changed core types searched across the entire Windows crate:
      `SessionCommand::MoveToWorkspace` and `LifecycleIntent::MoveToWorkspace`
      have no Windows sites; no direct `SendWorkspacePlan` constructors or
      destructures. Its added `follow` field reaches `CoreReply::SendWorkspace`.
      Besides production :10876 above, existing payload reads in
      `src/workspace_owner.rs:1210,1513,1602,1946,2099,2163,2215,2311,3016,3225`
      only consume geometry/operation and need no shape repair; extend relevant
      send tests to assert intent/focus. Wildcard reply arms
      :1093,1167,1188,2003,2611,2993,3135,3206,3288 likewise need no shape repair.
    - `src/workspace.rs` / `ManagedWorkspaces`: reuse item 1's scoped existing
      ordinal ring for relative targets, resolve once before mutation, wrap
      both ends including trailing empty and >9; ordinary lifecycle supplies
      next spare. Keep source-empty selection distinct from target admission.
    - `src/snapkey.rs` / `WorkspaceOp` :427, classifier/dispatch/queues and
      `src/settings.rs` catalog/rebind/presets: split explicit follow/stay,
      add Ctrl+Shift H/K/Left/Up previous and J/L/Down/Right next follow;
      numbered 1..9/0 and relative stay register unbound. Carry exact modifiers
      through suppression/repeat/remap (item 1 Ctrl work reused). Windows
      arrow ownership is unknown: do not invent a holder claim.
    - Despite cfg(windows) native actuation, portable `workspace_owner.rs`
      participates in Linux workspace builds. The compile-only fix restores
      full-workspace `cargo test`/`clippy` gates; stay/relative adapter wiring
      remains pending. Port follow/stay MRU/null, admission equality, visibility
      fences, relative wrap/spare and floating-boundary regression coverage.
  - Item 3: Windows schema-v1/live setting/UI/move routing delivered 2026-10-11
      (`755aab8`, [record](changes/archive/windows-same-axis-move.md));
      native gates/CI and agent-observed Apply/Revert/no-rebuild passed. Physical
      directional journey pending; checklist below retained as acceptance detail.
      Sites below under `crates/tiler-windows/`:

     #### Item 3 behavior, references and acceptance

     - Normative: decisions 3.1/3.2; [spec](spec/functional-spec.md#move)
       REQ-MOV-03; [matrix](spec/reference-outcomes/move.md) R-MOV-03/09/10.
        Global `core.same_axis_move`, `group-with-neighbor` default, `swap-with-neighbor`
       alternative. Only R2c adjacent direct LEAF siblings in the same N-ary
       group change; unequal shares travel with window identities. Binary R2a,
       group-neighbor wraps/insertion, escape and output boundaries keep their
       rules. Default `H[A,B*,C,D]` right -> `H[A,H[B,C],D]`; flat ->
       `H[A,C,B*,D]`. No rebuilding, retroactive flattening or new swap verb.
      - Core: `core/directional.rs` `SameAxisMove::{GroupWithNeighbor,SwapWithNeighbor}`,
       :229 `as_wire_str`, :239 `parse_wire`; :202 `MoveIntent.same_axis_move`.
       `core/boundary.rs:48-57` `CoreCommand::Move {window,direction,
       cross_output_transfer,same_axis_move}`; :372 `MovePlanReply`, :670
       `CoreReply::MoveDirectional` unchanged. `core/engine.rs:2531`
       `directional_move_request` threads the typed mode. Windows should use
       these semantics, not reimplement swapping in native geometry.
       `protocol/planner_protocol.rs:3686-3697` `SyncCommand::Move` decodes
       `same_axis_move` string (missing wrap), :3800-3812 maps the validated
       token. Unknown/mistyped values refuse; core has no serde dependency.
     - KDE: `kwin/src/same-axis-move.ts:15/:33` normalize/read;
       `kwin/src/plan-adapter-entry.ts:7414-7427` reloads subsequent-move value;
       `kwin/src/plan-adapter.ts:3100` `requestMove` sends explicit flat token
       at :3178 (default omitted). KCM `kwin/native-effect/unifiedsettings_module.cpp:57`
       `readSameAxisMove`, :115-120 combo; Save/reconfigure live pickup detailed
       in [record](changes/archive/same-axis-move-setting.md).
       [KDE fixtures](../kwin/tests/same-axis-move.test.ts) and
       [core fixtures](../crates/tiler-core/tests/session_movement.rs).
     - Windows schema/control: add `CoreSettings.same_axis_move` at
       `src/settings.rs:99` with serde missing-default and Default :124;
       validate exact tokens in `validate_settings` :1268 (map via shared
       `parse_wire`, reject unknown, no silent invalid->default). Keep v=1,
       startup invalid-file and live last-good behavior. UI adds a global
       "Same-axis move" two-choice control in `src/settings_ui.rs:939`
       `cmd_settings` / control
       construction :1054, `refresh_all` :292 and `collect_draft` :345; Apply
       writes tokens and Revert reloads saved selection. Read last-good per
       move in `keyboard_tick` (`src/tiling_sys.rs:5645`, command :6102-6110),
       with wrap fallback for proof owners lacking `settings_live` (:1210).
       Same-axis-only settings adoption must not trigger tree rebuild/resync.
     - Bindings/presets/conflicts/tray: no new action/chord or conflict entry;
       existing Win+Shift+H/J/K/L/arrows use the setting. Authentic/Compatible
       change bindings only (`src/settings.rs:1490` `apply_preset`), not this core setting.
       Existing tray Settings opens its control, no menu addition.
     - [ ] Portable: schema-v1 old file missing field -> wrap; both token
       roundtrips, invalid value/type refused, atomic save and live invalid file
       keeps last-good. Retained Engine tests for right AND left unequal-share
       leaf swaps, wrap default, group neighbor R-MOV-10 unchanged, binary/
       nested-boundary parity, focus retention, mode change no topology mutation
       until next move. Replace test constant only where exercising settings;
       retain explicit default-wrap fixtures as appropriate.
     - [ ] Windows-only: Settings Apply/Revert and startup/proof fallback;
       verify `keyboard_tick` uses adopted setting, not hard-coded mode; common
       gates/docs update only R-MOV-03/09/10, not item-5 rows.
     - User: prepare R-MOV-03 four-leaf row; default right wraps B/C. Fresh
        row, Settings Swap with neighbor + Apply, right yields A,C,B,D focused B. Repeat
       left and unequal widths, verify widths travel. Change setting on an
       existing nested tree: Apply alone changes no layout; next move uses
       new mode. R-MOV-10 group-neighbor result stays wrap, not whole-group
       swap. Stage another choice then Revert, verify saved choice returns.

     - Applied compile fixes: `src/tiling_sys.rs:6102`, `SnapOp::Move` arm
       constructing `CoreCommand::Move`, sets
        `same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor`;
       `tests/snapkey.rs:915-919`, `engine_focus_moves_through_nested_topology`,
       uses the same constant (production field :6109, test field :919).
       No Windows behavior change.
     - `src/settings.rs:99` / `CoreSettings`, `Default` :124 and
       `validate_settings` :1268: add `core.same_axis_move` within schema
        version 1, serde missing default `group-with-neighbor`, exact values
        `group-with-neighbor`/`swap-with-neighbor`; reject unknown/old values through
        existing invalid-file/last-good behavior, retain atomic saves. Labels
        `Group with neighbor` / `Swap with neighbor`; tooltips COSMIC / i3, sway.
        No aliases or migration for retired `cosmic-wrap` / `flat-swap` IDs.
     - `src/settings_ui.rs` / `cmd_settings` :939, controls :1054,
       `refresh_all` :292 and `collect_draft` :345: add the global
       setting control, display/persist the selected token, validate Apply,
       preserve existing Revert behavior.
     - `src/tiling_sys.rs` / `TileLoop.settings_live` :1210,
       `apply_live_settings` :13432, `poll_live_settings` :13547 and
       `src/settings.rs` / `LiveSettings` :1689, `poll_for_change` :1722:
       adopt valid changes for subsequent moves only, without tree rebuild
       or reconciliation. In `SnapOp::Move` :6102 replace the constant with
       the last-good setting mapped to shared `SameAxisMove`. Other
       input/focus/resize/float move paths retain existing rules.
     - Changed shared constructors searched: the above production and test
       `CoreCommand::Move` sites are the only Windows sites; no Windows
       `MoveIntent`/`SyncCommand::Move` constructors or exhaustive move-field
       destructures require repair. The reply shape is unchanged: existing
       `CoreReply::MoveDirectional` branch at `src/tiling_sys.rs:6173` continues
       applying returned geometry for R2c as well as R2a.
     - Extend portable settings decode/default/validation/round-trip and
       live last-good tests; native settings Apply/Revert tests; move tests
        for N-ary swap-with-neighbor both directions with traveling unequal shares,
       default wrap, group-neighbor/boundary parity and live changes without
       tree rebuild. Linux workspace tests/clippy compile portable Windows
       modules; native Windows UI/owner checks remain Windows-owned.
  - Item 4: Windows Win+O/catalog/presets/input/owner routing delivered 2026-10-11
      (`f5801d8`, test-only CI repair `83085a3`, [record](changes/archive/windows-parent-orientation-toggle.md));
      native gates/CI and agent-observed Settings/owner adoption passed. Physical
      toggle/OS suppression pending; held-repeat choice tentative pending user
      review. Checklist below retained as acceptance detail (Windows paths):

     #### Item 4 behavior, references and acceptance

     - Normative: decisions 4.1/4.2; [spec](spec/functional-spec.md#layout)
       REQ-LAY-01; [matrix](spec/reference-outcomes/layout-commands.md)
       R-LAY-01/05/06. Win+O toggles focused tiled leaf's immediate parent,
       root included, retaining order/shares/focus. Twice returns exact tree.
       Lone root, float/no tiled focus and floating workspace are no-write
       outcomes; focused maximized/fullscreen refuses. Sibling overlays remain
       reserved slots, reproject but skip native geometry/state writes. No
       future-admission orientation hint; long-edge and collapse unchanged.
     - Core: `core/boundary.rs:90-92`
       `CoreCommand::ToggleOrientation {window}`; :200
       `TiledKind::ToggleOrientation`; :250-259 `TiledPlan` carries revision,
       policy version, kind, geometry, focus domain/leaf and optional float
       fields (empty for orientation). :665 `CoreReply::Tiled`.
       `core/session.rs:161` `SessionCommand::ToggleOrientation`,
       `core/contract.rs:418/:460` lifecycle intent/operation;
       `core/session/ops/toggle_orientation.rs:39` `propose_toggle_orientation`
       and :212 `flip_group_axis` preserve direct parent's children/shares;
       :102-121 focused overlay refusal / lone `unchanged`.
       `core/engine.rs:2464` `toggle_orientation_request` ordinary retained
       single-domain handling. `protocol/planner_protocol.rs:3738-3739` wire
       `{"op":"toggle-orientation","window":"<id>"}`, :3857 typed mapping.
     - KDE: `kwin/src/plan-adapter.ts:3269` `requestToggleOrientation` gates
       subject/workspace, :3314 exact command; :1655
       `isToggleOrientationDetail`, :7014 orientation reply validation.
       `kwin/src/plan-adapter-entry.ts:449-456` catalog; native key constant
       `kwin/native-effect/shortcutreconciler.h:126`.
       [Fixtures @ b7a9843](https://github.com/beefsack/OmniTiler/blob/b7a9843/kwin/tests/plan-toggle-orientation.test.ts),
       [record](changes/archive/parent-orientation-toggle.md).
     - Binding id `toggle-orientation`, default Win+O, unshifted. Add
       `ToggleKind::Orientation` at `src/settings.rs:523` with token/Shift
       methods (:530-548), catalog :757 and live effective/remap handling.
       Authentic keeps; Compatible adds this one disabled id (:1528) for
       orientation lock. Existing `chord_conflict` :983 text is
       "orientation lock owns Win+O; override needs takeover". The catalog
       claim is repository-VERIFIED with Microsoft-list attribution (:943-952);
       OS ownership was not independently rechecked by this handoff, and
       physical suppression is UNVERIFIED. No current Windows Win+O action.
     - No persisted core field; schema-v1 absent binding override means Keep.
       Dynamic settings editor exposes Keep/Disable/Rebind; add O (0x4F) to
       classifier inventory, exact Win+O action/queue/mask handling below.
       Default Shift/Ctrl/Alt variants pass through; preserve key pairing and
       existing activation fences. Resolve any unselected autorepeat outcome
       with a discriminating matrix row/TBD, not a new policy in this handoff.
       Dedicated `src/tiling_sys.rs:5645` `keyboard_tick` arm, not a SnapOp movement request;
       assembly uses `src/tiling.rs:1226` `build_reconcile_event_for_floating`,
       replace command, accept only matching Tiled kind, apply geometry with
       fresh fences. Current helpers do not magically dispatch the new action.
     - [ ] Portable Engine/owner: root both axes, nested parent-only, unequal
       shares double roundtrip, focus/minimum allocation, lone no-op then wide
       long-edge admission, float/no-focus/floating-workspace/overlay fixtures;
       focused overlay refusal and sibling write-skip must be distinguished.
       `tests/settings.rs` / `tests/snapkey.rs`: O catalog/preset/conflict,
       rebind/disable, exact modifiers, repeat/release, mask and saturation.
     - [ ] Windows-only owner/input/UI: stale origin/lifetime or suspension
       causes zero writes; real matching Tiled reply actuates without view
       switch, sibling overlays untouched. Common gates/docs: R-LAY-01/05/06.
     - User: ordinary root pair A,B (B focused), Win+O -> V then -> H with
       identities/shares/focus retained. R-LAY-05 nested H[A,V[B*,C]] toggles
       only V. Sole A on wide area: no effect; opening B still uses long edge
       (record native newcomer order/focus, currently TBD). Float/workspace-
       floating/focused overlay: no effect; sibling overlay native state stays.
       Compatible passes Win+O to OS, Authentic toggles only project layout;
       record OS orientation-lock effect/suppression with unchanged displays,
       and test Disable/rebind/Apply/Revert/takeover-off release.

     - `src/settings.rs:523` / `ToggleKind` and token/Shift methods :530-548:
       add orientation; `binding_catalog` :757-827 adds `toggle-orientation`
       default Win+O. Reuse `chord_conflict` :983 orientation-lock text;
       `compatible_disabled_ids` :1528 adds the new id/count, Authentic
       `apply_preset` :1490 keeps it. Dynamic settings UI consumes the catalog;
       test Keep/Disable/rebind and effective conflict text, without claiming
       physical OS suppression before Windows evidence.
     - `src/snapkey.rs:43-58` key inventory: add O (0x4F); `Classified` :503,
       `MaskTrigger` :545, `SnapClassify::push` :1012 / `push_owned` :1146,
       `QueuedSnapEvent` :2116 and `classify_and_queue` :2217: extend the
       toggle family with exact unshifted Win+O, paired downs/repeats/ups,
       origin identity, live suppression/remap, saturation and E8 mask.
       Shift/Ctrl/Alt variants pass through; Compatible disables our binding,
       Authentic takes over the orientation-lock chord. Extend portable
       `tests/snapkey.rs` and `tests/settings.rs` coverage.
     - `src/tiling_sys.rs:6095-6205` directional retained Engine/apply route
       and :6224 toggle queue arm are integration examples, not new SnapOp
       movement semantics: add a dedicated queued orientation arm with fresh
       origin/suspension/lifetime/tiled-workspace/focused-overlay guards.
       Build an ordinary single-domain event with
       `CoreCommand::ToggleOrientation { window }`; accept
       `CoreReply::Tiled` only with `TiledKind::ToggleOrientation`, then use
       `apply_geometry` :4866 with existing writable/hidden/overlay/minimum
       fences and readback. Focus stays on the same window; no view switch.
       `unchanged` / `not-tiled` / no tiled focus are no-write outcomes;
       sibling overlays keep their native state and skip native writes.
     - Port root H/V, nested parent-only, double-toggle unequal-share/order/
       focus, minimum allocation, lone-leaf no-op plus long-edge admission,
       floating-workspace/focus and overlay fixtures. Linux workspace tests
       compile/test portable Windows modules; Windows native
       owner/input/UI and physical Win+O takeover remain Windows-owned.
  - Item 5: Windows directional/focus/drag foundation delivered offline
      `2c05ee5` + `f2169d6` ([record and physical steps](changes/windows-multi-output-foundation.md)).
      Explicit output follow/stay delivered offline `fda618f`, 2026-10-14:
      catalog/settings/UI/exact modifier routing, ordinary current-workspace
      admission and candidate-Engine/membership commit-last transfer, including
      floating boundaries and source-MRU/null stay. Native gates: 1,892 passed,
      0 failed, 1 ignored ([record](changes/archive/windows-explicit-output-send.md)).
      Physical two-output/mixed-DPI/suppression and remaining dependent scope
      remain pending. Held repeats and shifted stay rebinding are tentative,
      pending user review; historical integration locators below are superseded.
      Historical shared delivery and remaining contract below; exact sites
      under `crates/tiler-windows/`:

     #### Item 5 behavior and shared contract

     - Normative: decisions 5.1-5.4; [spec](spec/functional-spec.md#move)
       REQ-MOV-08/08b and [output](spec/functional-spec.md#output)
       REQ-OUT-01/04; [move matrix](spec/reference-outcomes/move.md)
       R-MOV-08/11/12/13 and [output matrix](spec/reference-outcomes/multi-output.md)
       R-OUT-01/04/07. Local restructure/swap/escape wins first; exhausted
       movement crosses all four ways, sole root leaf included. Explicit send
       crosses before exhaustion. No wrapping outputs or cycling workspaces.
     - User decision 2026-10-09: resolve candidates on FULL rectangles with
       exact touching edges and positive perpendicular overlap. Select by
       shared edge containing the moving window centre projection; if none,
       larger overlap with its span along that edge; final left/top tie-break.
       Panels cannot break adjacency; work areas still govern placement.
       No candidate no-op; unreadable refuses; no wrap. Shared core/KDE selection
       delivered offline; item 16 supplies the shared selector and fence details.
       Directional R4 landing is target edge nearest source; explicit send uses
       ordinary remembered-leaf/destination-MRU/root long-edge admission into
       destination CURRENT workspace, not R4 edge insertion. Directional focus
       keeps existing horizontal policy; vertical focus is not selected here.
     - Follow/stay as item 2: follow focuses mover on target output (target
       workspace already shown, no needless desktop switch); stay retains
       source current view/MRU or null no-setter focus. Tiled subject initially,
       sticky/intentional floats excluded; floating-workspace boundaries are
       membership-only with only tiled-side reflow. Ordinary float transfer
       remains OPEN, not implicitly implemented by output send.
     - Core types: `core/boundary.rs:48-57` `CoreCommand::Move` capability flag
       and typed same-axis mode; :170-185 `CoreEvent.directional` source-first
       pair plus `directional_target_outer_gap`, both-domain `windows`.
       `core/directional.rs:147-152` `Output.adjacent` accepts Direction four ways;
       :334 `MoveOperation::CrossOutput`. Actual Engine reply is
       `CoreReply::MoveDirectional(MovePlanReply)` (`core/boundary.rs:670`), where
       `MovePlanReply.cross: Option<MoveCrossView>` (:372-400) carries source/
       target output/workspace, mover window/leaf/direction, target occupancy,
       source root-child index and preconditions. Windows handles `plan.cross`,
       not a nonexistent `MoveOperation` field in the reply.
     - Explicit send `core/boundary.rs:105-112`
       `CoreCommand::SendToOutput {window,target_output,target_workspace,follow}`
       uses `target_domain/target_windows`, returns :669
       `CoreReply::SendOutput(SendWorkspacePlan)` with item-2 fields.
       `core/session.rs:140` `SessionCommand::MoveToOutput`,
       `core/contract.rs:411` `LifecycleIntent::MoveToOutput`;
       `core/engine.rs:1865` `transfer_request` canonical ordinary send and
       :2531 `directional_move_request` R4 planned commit. Geometry/native
       arrival remain Windows adapter work. Same workspace ID on different
       outputs is valid; workspace send stays same-output distinct-workspace.
     - Protocol `protocol/planner_protocol.rs:3758-3766` `SyncCommand::SendToOutput`
       wire `send-to-output` (missing follow true), :1948
       `serialize_send_output_reply` emits distinct kind, move-tiled operation
       + follow and desired focus/geometry. Directional adjacency parsing
       :930-1049 and fingerprint :73-166 include up/down; adapter owns geometric
       uniqueness. Windows direct Engine path needs complete validated pair
       construction, not an invocation of private codec types.
     - KDE mirrors: `kwin/src/plan-adapter-entry.ts:562` `readFullOutputRect`,
       :601 `readOutputTopology`, :677 `selectAdjacentOutput`, :2557
       `observeOutputSendTarget`, :5304 `requestOutputSend` (floating boundary),
       :5571 `focusOutputMover`; `kwin/src/workspace-send-adapter.ts:1434`
       `requestSendToOutput`, :2820 `followOutputOnce`, :2915 `confirmStayOnce`.
       Retain frozen source identity when active mover changes outputs; actual
       source visibility is a separate fence. Both views/modes/gaps/window
       sets and exact lifetime must still match before EACH membership,
       geometry and focus write; partial arrival cannot masquerade as success.
       [Engine fixtures @ b7a9843](https://github.com/beefsack/OmniTiler/blob/b7a9843/kwin/tests/output-send-engine-fixture.test.ts),
       [adapter fixtures @ b7a9843](https://github.com/beefsack/OmniTiler/blob/b7a9843/kwin/tests/output-send.test.ts),
       [record](changes/archive/four-direction-output-transfer.md).

     #### Item 5 bindings, exact native additions and acceptance

     | Proposed Windows id | Default chord | Intent / presets |
     | --- | --- | --- |
     | `send-output-left`, `send-output-down`, `send-output-up`, `send-output-right` | Win+Ctrl+Alt+H/J/K/L respectively | Follow, both presets keep |
     | `send-output-left-arrow`, `send-output-down-arrow`, `send-output-up-arrow`, `send-output-right-arrow` | Win+Ctrl+Alt+Left/Down/Up/Right respectively | Follow, both presets keep |
     | `send-output-left-stay`, `send-output-down-stay`, `send-output-up-stay`, `send-output-right-stay` | Unbound (four canonical rows) | Stay, both presets keep unbound |

     - KDE catalog `kwin/src/plan-adapter-entry.ts:169-201` and native constants
       `kwin/native-effect/shortcutreconciler.h:174-181` establish 8 follow + 4
       stay rows. Ownership UNKNOWN on Windows (5.3), use truthful unknown/
       containment-unproven `chord_conflict` text, extending the Ctrl/Alt early
       return in `src/settings.rs:956-957`. No new Compatible disables;
       do not borrow resize-shrink (COSMIC) or item-2 arm (niri).
       No new global field; schema-v1 binding defaults and full Ctrl+Alt action
       rebinding/queue/suppression reuse items 1/2. Native OS suppression pending.
     - Exact additions: topology from `src/tiling_sys.rs:264` `all_monitors`
       / :282 `monitor_fulls` preserving device/full/work association;
       domain bounds through `workspace_domain_for` :8540. `keyboard_tick`
       :5645 constructs directional event at :6082-6110, handle `plan.cross`
       at :6173 before geometry (:4866 `apply_geometry`), verify membership
       and native target-output arrival before one `actuate_focus` (:5402).
       Explicit output builder beside `src/workspace_owner.rs:70`
       `build_send_event` must construct DISTINCT `SendToOutput`, not merely
       retarget workspace send. Extend `planned_writes` :396 / native
       `desired_entries` (`src/tiling_sys.rs:4211`) for `SendOutput`; add output
       reply/actuation route beside `workspace_do_send` (:10552), not its same-output tail.
     - Output target is already visible: stay must not hide the arriving mover
       simply because item-2 workspace stay hides an INACTIVE destination.
       Preserve source visibility, keep target current view, reflow writable
       visible tiled rows on both outputs; only focus distinguishes follow.
       Membership-only floating boundaries use the same distinction. Native
       placement/transfer capability is part of parked item 9; do not enable
       `cross_output_transfer` before it exists. Failure converges both domains
       without setter replay, guessed rollback or success claims.
     - [ ] Portable: full-rect compass tests, no candidate, forward/reverse
       candidate selection by centre/overlap/left-top, panel gap both axes,
       local-first under both item-3 modes,
       sole-root empty/occupied targets, directional nearest-source landing
       vs explicit remembered/MRU/root admission, same workspace on different
       outputs, follow/stay/null, floating-boundary only-tiled reflow and
       excluded floats/sticky. Use real Engine reply geometry, `plan.cross`
       and `SendOutput` extraction; catalog/modifier/preset/rebind tests.
     - [ ] Windows-only: physical full/work area + mixed-DPI placement, fresh
       output/current-view/mode/gap/lifetime fences during writes, half-applied
       transfer, delayed arrival, no repeated follow; independent owned-window
       stop/restore across outputs. Common gates/docs R-MOV-08/11..13 and
       R-OUT-01/04/07; item 9 and physical acceptance remain explicit blockers.
     - User on a two-output setup: stack outputs, lower V[A*,B], upper X; exhausted
       Win+Shift+Up moves A nearest source below X, source B; fresh sole A also
       crosses. Mirror Left/Right/Down, empty and occupied destinations. With
       local move available it wins; explicit Win+Ctrl+Alt+direction instead
       transfers before exhaustion to CURRENT target workspace. Prepare
       R-OUT-07 remembered Y: admission splits Y long edge; follow focuses A,
       bind `send-output-right-stay` to an unused validated Win+Ctrl+Alt+F6,
       then stay leaves source B focused with target A visible. Panel gap
       R-MOV-13 still crosses, placement respects work area. User-readable
       multi-candidate topology selects by centre/overlap/left-top once
       implemented; unreadable/no-candidate must refuse/no-op with no writes.
       Floating-boundary frames stable/tiled-side reflow; source/target drift
       and lifetime failure cases belong in fixtures, not uncontrolled dogfood.

     - `src/tiling_sys.rs:6082-6110` retained directional event construction:
       supply complete source plus adjacent output's current workspace in
       `CoreEvent.directional`, including target outer gap, and opt into
       `cross_output_transfer` only with native transfer/fence support.
       Derive reciprocal edge-touch + positive-overlap candidates in
       all four directions from FULL monitor rectangles, not work areas;
       select by moving window centre projection, else larger window-span
       overlap, then left/top (User decision 2026-10-09); no candidate no-op,
       unreadable refuses, no wrap. Keep domain
       bounds as work areas. Local move rules and sole-leaf eligibility are
       now core-owned. Focus remains the existing horizontal policy.
     - `src/tiling_sys.rs:6173-6205` currently applies only geometry for
       `CoreReply::MoveDirectional`: handle `plan.cross` (`MoveCrossView`,
       constructed from `MoveOperation::CrossOutput` by the core) and
       native output/membership transfer before both-domain geometry, with
       exact lifetime, frozen domain/gap/mode/current-view fences, fresh
       arrival and one follow. Preserve nearest-source directional landing;
       failed/partial transfer converges without replay or success claims.
     - `src/workspace_owner.rs:70-147` / `build_send_event`,
       `stamp_send_target`: add a DISTINCT output-send builder producing
       `CoreCommand::SendToOutput { window, target_output, target_workspace,
       follow }`; resolve destination CURRENT workspace once. Workspace-send
       remains same-output. Reuse ordinary admission/canonical pair machinery;
       same desktop ID on different outputs is valid (no last-desktop gate).
     - `src/workspace_owner.rs:396-417` / `planned_writes` currently falls
       through for new `CoreReply::SendOutput`; extract its geometry.
       `src/tiling_sys.rs:10848-10884` send builder/reply dispatch and
       `workspace_do_send` are same-output patterns, not output-send wiring:
       add the new reply route and apply command follow/stay (reuse item 2),
       source MRU/null focus, visibility and live-arrival fences.
     - `src/tiling_sys.rs:10314` / `workspace_do_send_native` and
       `src/workspace_owner.rs:162-171` / `SendRoute`: extend output sends
       through floating-workspace boundaries with membership-only transfer
       and tiled-side reflow; sticky/intentional float movers stay excluded.
       Ordinary float output transfer remains OPEN.
     - `src/snapkey.rs:427` / `WorkspaceOp`, classifier/queues/suppression,
       `src/settings.rs:757-827` / `binding_catalog` and preset/rebind validation:
       distinct output-follow Win+Ctrl+Alt+arrows/HJKL and four unbound
       directional stay rows; exact Ctrl+Alt modifier routing, hold/repeat/
       release and origin lifetime. Reuse item-1/2 modifier work; Windows
       shortcut ownership is unknown, do not invent stock-holder claims.
     - Portable Linux workspace tests/clippy pass with no Windows edits.
       Port core/KDE compass, local-first, sole-leaf, full-rect panel-gap,
       multi-candidate selection, ordinary remembered/MRU/root admission, shared
       desktop, follow/stay, floating-boundary and stale/lifetime fixtures;
       native Windows acceptance remains Windows-owned.

  - Item 6: keyboard resize R-RSZ-01 (parity d) delivered 2026-10-11, base
    `f794cf9` + delivery commit ([record](changes/archive/windows-keyboard-resize.md)).
    Native gates, retained-Engine geometry, agent-observed exact-owner CLI
    grow/repeat/reversal/edge/overlay refusals and Settings Apply/Revert passed.
    Physical suppression/rebound adoption remains user-owned; remaining native
    refusal/minimum journeys stay pending. Both presets keep unknown-owner
    resize defaults tentatively, pending user review. Historical checklist: Table A accepted
    2026-10-07 (Windows keyboard trigger via shared Engine pixel path; P1
    adoption entry in this file, [consensus](research/reference-wm-consensus.md);
    KDE match stays B). Catalog text already documents the rows; the trigger
    is the gap.
    Second agent-live sweep 2026-10-17: physical chords remain blocked by the
    deliberate injected-input fence; harmless scoped probe changed no frames
    or dispatches. No shortcut acceptance inferred; Win+M was not sent.
    Review 2026-10-18: injected Win+M remains unsafe because the normal owner
    ignores it and the shell could minimize unrelated windows; no chord sent.
    Physical pairing/interception remains pending, separately from CLI geometry.
    Native follow-up 2026-10-18: exact-owner resize refused intentional-float
    and sticky subjects with identical post-toggle pair frames; toggle-back
    restored tiled intent/topmost. No physical chord or pixel-geometry claim.
    Separate PMv2 trace run reached the existing portable 360 px width floor
    and one subsequent `unchanged` request with stable pair/focus. The harness
    wrongly expected the helper's smaller declared minimum; no product defect
    or policy change. Larger app-hint/vertical/nested minima remain pending.
    Final raw settings-byte, successful SPI and actual taskbar-state readbacks
    matched preimages; owner/helpers stopped and ledger/requests cleared.

    #### Item 6 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#resize) REQ-RSZ-01 pixel
      `cosmic_v1` path plus REQ-RSZ-01b Windows trigger; [matrix](spec/reference-outcomes/resize.md)
      R-RSZ-01 (Ours KDE: 12px press 0, then 14/16/18/20 on repeat, adjacent
      shares only, inwards reverses, minima clamp; Ours Windows:
      no-counterpart). Four outward and four inward rows, each with letter
      and arrow defaults (16 physical chords): Win+Alt grows outwards,
      Win+Alt+Shift shrinks inwards.
      Step schedule `(10 + 2 + 2 * press_index).min(20)` (`core/cosmic_v1.rs:228-240`;
      first press 12px). Dedicated resize intent; never `SnapOp::Move`.
    - Core/engine/protocol (verified 2026-10-08 at `db31234`):
      `core/boundary.rs:69-74` `CoreCommand::Resize {window, direction, mode,
      press_index}`; `core/engine.rs:892` dispatch plus :3044-3108
      `resize_request` (retained converge, `ResizeCapabilities
      {keyboard_resize: true, pointer_resize: false}`, `ResizePlanReply::
      from_keyboard`); `core/contract.rs:830-883` capability gates;
      `core/session/ops/resize.rs:49` `propose_resize`, gate :134, keyboard
      derivation :1383-1504; `protocol/planner_protocol.rs:208-211`
      `parse_mode`, :2683 `SyncCommand::Resize` parse, :2734 core mapping,
      :3712-3719 wire shape, :3824-3834 typed mapping, :1797-1858 reply
      serializer, :2112 reply dispatch, :2314 retained eval.
    - KDE reference: `kwin/src/plan-adapter.ts:3197` `requestResize` (fences:
      disabled, invalid direction/mode, busy, observe, activeExcluded,
      workspace-floating, fullscreen, maximized; repeat state
      `repeatFocused/repeatDirection/repeatMode/repeatNext` :2110-2113 and
      :3241-3257; dispatch body `{op: "resize", window, direction, mode,
      press_index}` :3253-3257); `kwin/src/plan-adapter-entry.ts:4102`
      catalog entry, delegates :7536/:7844; `kwin/tests/plan-adapter.test.ts:417`
      resize-param test plus :898 busy-refused.
    - Current Windows gap: `src/settings.rs:693` `RESIZE_ROWS` (8 ids, Alt
      outwards defaults, Alt+Shift inwards defaults), `binding_catalog`
      :769-777 marks them `implemented:false` ("not intercepted on Windows:
      Alt chords pass through untracked"). `src/snapkey.rs:104` `SnapOp`
      has only Focus/Move; `snap_repeat_live` :301 pins Win-only repeats;
      `key_op` maps :870/:1176/:1208/:1254. `src/tiling_sys.rs:5645`
      `keyboard_tick` builds Focus (:6096-6100) and Move (:6102-6110) only.

    #### Item 6 adapter checklist

    - `src/snapkey.rs`: add a Resize op beside `SnapOp` :104 (`as_str`
      :109-114); extend `snap_repeat_live` :301 with an Alt-pinned arm
      (Win+Alt held, Shift selecting inwards vs outwards per
      `binding_wants_shift` `src/settings.rs:1038`); extend `key_op` maps
      :870/:1176/:1208/:1254 and `push` :1012 / `push_owned` :1146
      Ctrl/Alt acceptance scoped to these documented resize chords only;
      carry exact Alt (+Shift) plus ACTION through intents, queues
      (`QueuedSnapEvent` :2116, `classify_and_queue` :2217), hold/repeat/
      release pins and E8 mask (:2303). Unrelated Ctrl/Alt-rebind refusal
      (`validate_bindings` :1317, `apply_rebind_text`
      `src/settings_ui.rs:403/:425`) stays.
    - `src/settings.rs`: flip each `RESIZE_ROWS` row to `implemented:true`
      as it lands; extend `chord_conflict` :954 past the Alt early return
      with truthful ownership text (Windows Alt-chord ownership is
      unrecorded: "ownership unverified in repository; containment unproven
      live", never conflict-free). Presets: `compatible_disabled_ids`
      :1528 lists implemented conflicting rows only; NO Compatible-disable
      or Authentic-takeover is recorded for Alt resize chords, so preset
      treatment is TBD, not a silent keep. KDE clears stock kwin Switch
      Window arrows through KCM Apply/Force/Revert ([shortcuts decision](decisions.md#shortcuts-conflicts-and-presets)
      2026-09-28); the Windows analogue is unselected.
    - `src/tiling_sys.rs` `keyboard_tick` :5645: add a dedicated Resize arm
      (not a `SnapOp::Move` request) constructing an ordinary single-domain
      event with `CoreCommand::Resize {window: focused mover, direction,
      mode, press_index}`; `press_index` from per-focus/direction/mode
      repeat tracking mirroring KDE :3241-3257; pre-dispatch fences mirror
      KDE :3197-3239 (suspension/lifetime, floating subject, floating
      workspace, fullscreen/maximized overlay). On `CoreReply::Resize`,
      extract via existing `planned_writes` (`src/workspace_owner.rs:412`)
      and apply through `apply_geometry` :4866 with the Move-arm fences
      (:6172-6205); pointer-resize precedent :13040-13060.
    - [ ] Portable: `tests/snapkey.rs` exact Alt/Shift routing, extra
      modifiers, held live rebind/disable, queue/mask, duplicate modifiers;
      `tests/settings.rs` catalog flags, conflict text, preset TBD behavior;
      `tests/tiling.rs` retained-Engine resize geometry: 12px first press,
      14/16/18/20 repeats, adjacent-shares-only, inward reversal, one-sided
      minima clamp, no Move semantic. Port `crates/tiler-core/tests/session_resize.rs`
      semantics (`horizontal_both_directions` :358, `vertical_both_directions`
      :389, `edge_refusals_are_unchanged` :516, `normalization_and_clamp_to_exhaustion`
      :565, `focus_retained_and_plan_carries_semantics` :648).
    - [ ] Windows-only: owner hook release/suppression plus settings
      Keep/Disable/Rebind Apply/Revert for the 8 rows; native chord
      suppression needs Windows evidence.
    - User journey: use horizontal and vertical tiled pairs, focusing the
      member with a shared boundary in each requested direction. Win+Alt+
      H/L/J/K plus arrows grow outward once each (12px before clamps);
      outer edges without a resize boundary remain no-ops. Win+Alt+Shift variants
      shrink inward (reversal); repeat-hold one direction to the 20px cap;
      drive one pair into its minimum (clamp one-sided, no axis search);
      Apply/Revert plus takeover-off release; unrelated Ctrl/Alt chords
      still pass through.
    - DoD: all 8 rows resize through the shared pixel path with
      press_index steps; no Move semantic; catalog/conflict/preset-tests
      updated with preset TBD recorded; matrix `R-RSZ-01` Ours Windows cell
      updated with dated offline evidence, native pending.

  - Item 7: press-focus R-DRAG-08 delivered 2026-10-11, base `9d12c7f`
    plus delivery commit ([record](changes/archive/windows-drag-press-focus.md));
    native gates passed; synthetic no-move press, Esc and stationary-preview
    drop agent-observed. Physical/resize/refusal-race checks user-owned;
    native title-bar path intact (same-topology apply observed). Historical
    checklist: User decision 2026-10-07 (R-DRAG-08;
    [decisions](decisions.md#pointer-drag-and-drop)): a Meta/Win client move
    drag focuses the dragged window at press on both platforms. Windows
    changes from activate-on-drop; KDE timing needs a live check ([pending
    live checks](backlog.md#pending-live-checks)).

    #### Item 7 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#drag) REQ-DRAG-08 plus
      REQ-DRAG-03 producer note; [matrix](spec/reference-outcomes/mouse.md)
      R-DRAG-08 (Ours Windows today: foreground retained during hold, B
      activated on valid drop; Ours KDE: exact focus timing TBD).
      R-DRAG-07 stationary-source/moving-preview split and parked underlay
      C stay unchanged.
    - Current Windows exact (verified 2026-10-08 at `db31234`): Win+Left
      arm `windrag_down` (`src/tiling_sys.rs:12427`) validates tiled-only
      published origins, live identity/tag/token (`no-token`,
      `stale-snapshot`), captures gesture start (`capture_gesture_start`
      :12342), sets `gesture_producer` "windrag", `windrag_start_cursor`,
      `windrag_bound`, `move_kind` Move and `active`. Focus is bound later
      at drop-settle in `gesture_tick` :12900-12935 via `actuate_focus`
      (:5402, E8-prime plus attach plus one setter with exact readback),
      logged `windrag-focus`; failed actuation refuses with snap-back, no
      plan. Model: `src/win_mouse.rs:38` `WinDragSnapshot`, :51
      `WinDragEdge`, :71 `validate_windrag_down`, :90 `WinDragKind`, :104
      `WinDragQueue`, :329 `settle_pointer_journey`. Title-bar drag runs
      the native modal loop (OS focuses at press; no project code); Esc
      cancel is `clear_windrag_gesture` :12531 (no plan, no geometry change).
    - KDE: Ours KDE cell timing is TBD by live check, not by adapter work;
      oracle grab/drag route diagnostics live at
      `kwin/src/plan-adapter-entry.ts:5888-6073` (`grabSource`) and
      :6864-6894. Port evidence: `kwin/tests/drag-press-evidence.test.ts`.
    - Core/drop seam: `core/boundary.rs:113-125` `DragDrop` carries the
      mover and hover prior, not native press-focus authority. Windows
      `src/tiling_sys.rs:13018-13028` constructs that command only at drop;
      :13051-13074 applies the retained reply. Press activation adds no
      Engine mutation or layout write to that unchanged drop contract.

    #### Item 7 adapter checklist

    - `src/tiling_sys.rs`: bind mover focus in `windrag_down` :12427 right
      after the `active` insert (all START fences already passed:
      unmanaged, unknown-subject, busy, identity-changed, stale-snapshot,
      no-token, no-pre), using the same `actuate_focus` :5402 authority as
      the drop path. On press-actuation failure, refuse the arm with
      snap-back and no plan (mirror :12912-12928). At drop-settle
      :12900-12935, avoid redundant activation only after fresh foreground
      readback; preserve settle-time refusal if the required focus cannot
      be verified. Keep every other gate (START identity/lifetime, member match,
      sticky/float refusal, domain routing, same-output fence) and the
      Engine plan path unchanged. `windrag_up` :12508 and
      `clear_windrag_gesture` :12531 unchanged; title-bar path untouched.
      No new chord; no settings/catalog/preset row is touched.
    - [ ] Portable: `src/win_mouse.rs` model tests (`validate_windrag_down`
      edges, queue cap/discipline, `settle_pointer_journey` ordering) plus
      new press-ordering cases: unfocused-B press binds focus before any
      move; no-move press yields focus with zero layout writes; Esc cancel
      keeps focus with zero writes; drop at edge transfers with the mover
      focused; failed press actuation snaps back with no plan. Port
      `kwin/tests/drag-press-evidence.test.ts` ordering assertions.
    - [ ] Windows-only: owner-side arm/drop/cancel with exact foreground
      readback; native foreground evidence on press.
    - User journey: unfocused-B Win+Left press with no move (B focused,
      layout unchanged); Esc cancel (B stays focused, no write); press,
      move, drop at A's edge (transfer, B focused); underlay C remains
      parked.
    - DoD: press-focus on Win client drag with no drop-path regression;
      R-DRAG-07 split preserved; matrix `R-DRAG-08` Ours Windows cell
      updated with dated offline evidence, KDE timing still live-TBD.

  - Item 8: R-RST-01/R-FLT-05 delivered 2026-10-11, base `e48aed1` plus
    delivery commit ([record](changes/archive/windows-restart-intent.md));
    native gates and owned-helper double-restart/sticky-scope journeys passed.
    Physical/app dogfood pending; post-restart un-stick TBD;
    REQ-RST-01c NORMATIVE D25, membership/set/focus native acceptance pending;
    D7 fixed-origin marker now delivered offline with item 13
    ([record](changes/archive/windows-fixed-size-admission.md)). Stronger names and exact-owner float/sticky
    CLI are tentative, pending user review. Historical checklist follows.
    Agent-live 2026-10-17: two centered-start restarts preserved both sticky
    origins, intentional float, set/membership and provable return focus.
    The first-observation `duplicate-window` write defect is contained by
    Windows-only, lifetime-keyed observation holds (2026-10-17): a captured
    fixed-size first-tiled anchor converges without tiled focus, falls through
    to duplicate admission, and previously exposed three marked floats to
    tiled writes. Mixed baseline reproduction displaced all three; six protected
    mixed restarts made zero tiled writes and adopted on the next observation.
    Seven regressions and locked gates pass (2,465 passed, 0 failed, 1 ignored).
    Shared-core rejection remains unchanged: its valid-input KDE repair needs
    a separate reply/projection decision. Windows/Rust/KWin and other relevant
    CI streams pass; only the known unrelated API guard fails.
    [Repair evidence and limits](changes/archive/windows-restart-hydration-rejection.md).
    Second agent-live regression 2026-10-17 passed ordinary/sticky restart and
    a newly owned equal-track Settings fixed anchor: first-tick rejection held
    writes, exact frames survived, adoption completed next observation. Shared
    no-focus core decision stays parked; no full restart/logout claim.
    Full restart acceptance and initial-desktop restoration are not established;
    [historical receipts and cleanup limitations](changes/archive/windows-restart-intent.md#agent-live-follow-up-2026-10-17).
    Restart R-RST-01 plus R-FLT-05 sticky persistence. User
    decisions 2026-10-07 (R-FLT-05, R-RST-01 float identity;
    [decisions](decisions.md#restart-persistence)): sticky floats stay
    sticky across owner restart including Windows, delivered with the
    R-RST-01 work. KDE Q3 delivered offline 2026-10-08 under user decisions
    2026-10-08 (normative as delivered);
    [record](changes/archive/kde-intentional-float-restart.md).
    Windows code/behavior unchanged in that delivery. KDE D7 tile-override
    restart store [delivered offline](changes/archive/fixed-window-tile-override-restart.md);
    Windows still lacks float persistence and tile overrides remain a gap.
    User decision 2026-10-10 (mechanism, option A): on-window `SetProp`
    markers, same pattern as `OmniTilerSticky`; intentional-float and
    fixed-window tile-override markers added. Names carry the distinctive
    project prefix to minimise collision with other processes. A marker
    restores classification only, never authorizes writes or recovery
    ownership; written after native success, removed on unfloat/sticky-off/
    re-float; unreadable means no intent, logged. No separate store file and
    no recovery-ledger change. Post-restart un-stick remains TBD;
    REQ-RST-01c is NORMATIVE D25 with native acceptance pending.

    #### Item 8 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#startup) REQ-RST-01/01b plus
      REQ-FLT-05; [matrix](spec/reference-outcomes/restart-persistence.md)
      R-RST-01 (Ours KDE: healthy runtime store hydrates intentional F before
      first planning, degraded store diagnoses then adopts empty; Ours Windows:
      runtime store resets while settings persist without layout restore) and
      [matrix](spec/reference-outcomes/floating.md) R-FLT-05. Sticky stays
      sticky and intentional floats survive owner restart. REQ-RST-01c
      membership/set/focus is NORMATIVE D25, native acceptance pending.
      KDE's selected private runtime store
      is not a selected Windows storage scheme.
    - Core seams (verified 2026-10-08 at `db31234`): `core/session.rs:546`
      `exceptions` map, `core/session/world.rs:433` `is_exception` (Engine
      float/settle reads at `core/engine.rs:835/:1467`), `core/session.rs:148`
      intentional-float transition. Exceptions ride the session, never the
      settings file.
    - KDE Q3 runtime (2026-10-08 offline delivery): `kwin/src/plan-adapter.ts`
      `startIntentBootstrap` hydrates before planning; the existing Q2
      successful-application point calls `updateSettledIntentFromFlight`.
      `crates/omnitiler/src/float_intent_store.rs` owns the private
      runtime membership file; existing Planner1 gains `ReadFloatIntent` /
      `WriteFloatIntent`, with same-UID + current-KWin-owner authorization.
      Bus ID + KWin unique owner fence the namespace. Membership only; no
      geometry/focus/stacking/pre-sticky history or automatic fixed origin.
      No effect dependency or new shared adapter extraction. These are KDE
      implementation seams, not instructions to port D-Bus/file storage to Win.
    - KDE design handoff (user decisions 2026-10-08, Q3 plus D7 delivered offline): recovered
      ordinary floats retain the live frame with no hydration writes. Only
      settled explicit intent is durable; scoped/hidden/minimized absence
      does not clear it. Missing state is empty; corrupt/unreadable/mismatched
      reads diagnose then proceed empty. Write failure preserves native/local
      truth and the next settled membership update rewrites the full set.
      Storage ACK is distinct from native application; no atomicity claim.
      Ordinary intent and automatic fixed origin remain separate (R-RST-04,
      Windows item 13). D7 adds disjoint fixed-window tile membership to the
      same store, no positions: persist after successful application, clear
      on re-float/close, prune only complete inventory. Matched overrides
      survive hint/predicate changes; missing/degraded/unmatched records
      recompute (fixed -> untouched float). Windows must deliver this with
      item 13 after selecting its item-8 persistence mechanism; no KDE D-Bus
      storage requirement for Windows. Recovery ownership must never derive from marker alone.
    - Windows exact (verified 2026-10-08 at `db31234`): runtime intent
      `state.floated: BTreeSet<WindowKey>` (`src/tiling_sys.rs:1452`,
      init empty :13813); float toggle inserts :7361/:8103, unfloat
      removes :7668, close cleanup :4150; domain-release carry via
      `float_carry_tokens` (`src/workspace_owner.rs:207`, used :4286;
      fixtures :1352 `float_carry_survives_a_fresh_engine_session`);
      Engine read `engine_is_float` :3963. Sticky lane: `state.sticky` map
      plus on-window SetProp markers `src/tiling.rs:450`
      `sticky_marker_value` (1 = pre-sticky tiled, 2 = pre-sticky float)
      and :457 `parse_sticky_marker`; `src/product_hide.rs:487-547`
      read/install/remove; adoption fixtures
      `src/workspace_owner.rs:2894` roundtrip and :3245
      `sticky_adopt_consumes_both_markers_as_normal_float_then_tiles`.
      Runtime maps (`member_tokens`, `hidden_claims`, `member_identity`,
      `member_tags` :1280-1303) never serialize. These existing maps and
      recovery records are not a selected durable float-intent mechanism.

    #### Item 8 adapter checklist

    - `src/tiling_sys.rs` + `src/workspace_owner.rs`: on fresh observation
      after owner restart, restore the accepted intentional-float/sticky
      classification before first tiling. Current marker-adoption fixtures
      consume both origins as ordinary float (:3245); that is the gap,
      not the intended restart outcome. Preserve exact-lifetime admission
      fences (`member_tokens`/`member_identity`/`member_tags`,
      `visible_lifetime_ok`, `reused_hwnd_stale`) and independent recovery.
      Do not authorize writes from a marker or guessed HWND alone.
      Mechanism: prefixed on-window markers (user decision 2026-10-10 above).
      REQ-RST-01c membership/set/focus is NORMATIVE D25, native acceptance pending. Existing
      live sticky toggles retain the two pre-sticky origins; the exact
      post-restart un-stick result is not selected by R-FLT-05 and stays TBD.
      No new shortcut/catalog row or preset is selected.
    - [ ] Portable: `float_carry_survives_a_fresh_engine_session` :1352,
      marker roundtrip :2894, sticky-adopt :3245, plus new offline-restart
      cases: surviving sticky marker re-adopts as sticky (both origins),
      intentional ordinary float survives, marker mismatch fails closed,
      recycled HWND refuses. Keep same-owner un-stick origin tests; leave
      post-restart un-stick assertions TBD under R-FLT-05/R-RST-01. Core:
      `crates/tiler-core/tests/restart_disjoint_adoption.rs` and
      `crates/tiler-core/tests/session_lifecycle.rs` exception
      fixtures. Windows: `tests/restore.rs` (reveal/skip/missing/ambiguous),
      `tests/storage.rs` roundtrip/refusal, `tests/lifecycle.rs`.
    - [ ] Windows-only: owner restart with live windows across all three
      lanes; ledger/journal crash recovery unchanged.
    - User journey: two sticky floats (one made sticky from tiled, one
      from intentional float), one ordinary intentional float and one tile;
      restart owner; both sticky windows remain sticky and the ordinary
      float remains floating. Observe and record un-stick results for both
      origins without selecting a post-restart outcome; membership/focus
      native acceptance stays TBD under NORMATIVE REQ-RST-01c (D25).
      No saved-layout restoration.
    - DoD: sticky/intentional floats survive restart without identity
      guessing; post-restart un-stick remains TBD; REQ-RST-01c native acceptance pending;
      matrix `R-RST-01` and `R-FLT-05` Ours Windows cells updated with
      dated offline evidence, native journey TBD.

  - Item 9: fullscreen send R-MAX-09 delivered same-output 2026-10-11,
    base `6b76589` plus delivery commit ([record](changes/archive/windows-fullscreen-workspace-carry.md));
    native gates passed; CLI numbered follow/stay, relative follow and exit
    to target allocation agent-observed. Physical/flag-race/app-owned/game/
    cross-output pending; User 2026-10-14 fullscreen-follow mover focus
    delivered offline, physical recheck pending. Exact-owner fullscreen CLI
    remains tentative, pending user review. Historical checklist:
    Table A accepted 2026-10-07 carries state without restoring first. Depends on
    handoff item 2 follow/stay wiring only; same-output workspace carry,
    never the parity-queue multi-output foundation.

    #### Item 9 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#maximize) REQ-MAX-09;
      [matrix](spec/reference-outcomes/maximize-fullscreen.md) R-MAX-09
      (Ours Windows today: tiled maximized B sends through the retained
      Engine route with follow, fullscreen B refuses with no writes;
      Ours KDE: same-output desktop-send outcome TBD, R4 cross-output
      Engine path cited). KDE observe-first/maximize leg stays OPEN; only
      workspace carry is selected, never output-fullscreen carry.
    - R-MAX-05 boundary (not a send outcome): the project-toggle refusal on
      app-owned fullscreen without a preimage is a separate invariant
      (user decision 2026-10-07 R-MAX-05; [matrix](spec/reference-outcomes/maximize-fullscreen.md)
      R-MAX-05 Ours Windows; `src/tiling_sys.rs:6808`
      `fullscreen-refused-app-owned`). This item preserves that toggle path
      untouched; it selects fullscreen workspace-send carry only.
    - Current Windows gates (verified 2026-10-08 at `db31234`):
      `workspace_do_send` (`src/tiling_sys.rs:10552`) pre-dispatch overlay
      gate :10713-10737 (retained mover: fullscreen refuses
      `send-refused-fullscreen` :10728-10730, maximized proceeds through
      `send_flags_stable` plus `is_zoomed_now` rechecks); native boundary
      `workspace_do_send_native` :10314 with its own gate :10374; retained
      tail :10900-10945 (same fullscreen/maximized branches, post-tag
      lifetime recheck, `assign`, verified transfer before follow);
      builder/reply :10848-10884. `src/workspace_owner.rs:70`
      `build_send_event`, :138 `stamp_send_target`, :170-180 `SendRoute`,
      :396 `planned_writes`. Core: `core/engine.rs:1858-1865`
      `transfer_request`, `core/boundary.rs:93-112`
      `SendToWorkspace`/`SendToOutput`.
    - KDE seams, not carry acceptance: numbered workspace send enters
      `kwin/src/plan-adapter-entry.ts:4964` `requestWorkspaceMove`; tiled
      routing uses `kwin/src/workspace-send-adapter.ts:1309` `requestSend`
      and :2635 `followOnce`. Its :1363-1365 `non-tiled-focus` gate and
      native send behavior are context for the observe-first KDE R-MAX-09
      row, not authority to select a KDE fullscreen outcome.

    #### Item 9 adapter checklist

    - `src/tiling_sys.rs`: at the :10728-10730 and :10914 fullscreen
      branches, let a project-fullscreened mover proceed exactly like the
      maximized leg beside it: live flag-stability recheck, ordinary
      `transfer_request` admission (remembered leaf, destination focus
      history, root fallback), target allocation kept, overlay geometry
      never written, command follow/stay per handoff item 2 (follow
      selects/reveals target and focuses mover after verified transfer;
      stay hides the mover without selecting target and applies source
      MRU/null focus), verified transfer before follow. Retain every
      exclusion (unmanaged, suspended, identity/lifetime) and setter
      isolation; R-MAX-05 toggle path untouched. Any genuinely unresolved
      mover subcase gets a TBD governing matrix row, never an invented
      refusal.
    - [ ] Portable: `src/workspace_owner.rs` send fixtures
      (`send_event_binds_target_and_focused_mover` :1251,
      `send_plan_scopes_source_reflow_through_writable_subset` :2229) plus
      new fullscreen-carry cases: true-Engine source/target allocation
      equality with the maximized leg, overlay geometry skip (zero native
      writes for the mover), follow vs stay focus/visibility, maximized vs
      fullscreen distinction; `tests/tiling.rs` retained-Engine send
      coverage with lifetime/arrival/recovery.
    - [ ] Windows-only: Engine/native-boundary tails, source-selected
      fences before writes and focus, hidden target reveal policy, recycled
      lifetime, partial-transfer reconciliation without replay.
    - Settings/input/catalog/presets: reuse handoff item 2's follow/stay
      actions and exact-modifier routing. Current numbered send rows are
      `src/settings.rs:823-826` / `workspace_row` :830; this carry change
      selects no additional binding, conflict, preset or UI control.
      `apply_preset` :1490 / `compatible_disabled_ids` :1528 stay under
      item 2's recorded policy, not a new fullscreen-specific policy.
    - User journey: project-fullscreened B on WS1, WS2 occupied; follow
      send (B arrives still fullscreen, source collapses to A, B focused);
      fresh fixture, stay send (WS1 stays shown on A, B fullscreen on
      hidden WS2, select WS2 to verify); maximized leg repeats unchanged;
      native restore/exit of arrived B behaves as before.
    - DoD: workspace fullscreen carry without prior restore; exclusions and
      R-MAX-05 toggle refusal intact; matrix `R-MAX-09` Ours Windows cell
      updated with dated offline evidence, KDE leg still observe-first,
      native TBD.

  - Item 10: float/half-snap parity (a), R-FLT-07..11. User decision
    2026-10-05 float-nav ([decisions](decisions.md#window-state-float-sticky-maximize-fullscreen));
    KDE delivered offline ([record](changes/archive/kde-floating-directional-navigation.md)).
    Windows delivered offline 2026-10-12 from base `e506350`
    ([record](changes/archive/windows-floating-directional-navigation.md)); native
    MSVC gates pass. Focus candidate/reconcile fixtures include retained maximum
    and visible non-home sticky targets; automatic fixed floats retain D8 no-touch.
    Ordinary focus/half-snaps user-accepted physically 2026-10-12, scoped normal
    owner, single output, Notepad/Paint, base `e506350` plus combined deliveries.
    Tie/sticky/clamp/preset/fixed/game/DPI/multi-output pending. Historical seams below.
    Second agent-live sweep 2026-10-17 added no physical chord acceptance:
    injected input is deliberately filtered, so scripted focus/move/half-snaps
    were not sent. Existing physical acceptance and pending variants stand.

    #### Item 10 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#floating) REQ-FLT-07/08/09/
      09b/10/11; [matrix](spec/reference-outcomes/floating.md) R-FLT-07..11.
      Tile-origin focus lands on tiles, floats never targeted; float-origin
      focus selects same-domain floats by top-left axis distance, considering
      sticky entries before ordinary entries in encounter order; nearer
      ordinary entries can win. Up/Left ties keep first, Down/Right last. Miss
      retains focus and reuses the recorded horizontal output-edge behavior
      without COSMIC workspace cycling; float-origin Meta/Win+Shift+arrow
      snaps to the stateless work-area half, stays floating and focused.
      Quarters, maximize and repeated-outward transfer states deferred.
    - KDE exact (verified 2026-10-08 at `db31234`):
      `kwin/src/plan-adapter.ts:944` `floatSubjectEntry` (focused
      same-domain float/sticky gate), :974 `selectFloatFocusTarget`
      (sticky entries first, then ordinary in one axis-distance race;
      Up/Left admit non-positive deltas and keep first tie, Down/Right
      strictly positive and keep last), :1036 `floatHalfSnapRect`
      (inner-gap formula over work-area bounds, trunc halves), :2460
      `requestFloatFocus` via dispatch :2433, :2533 `requestFloatMove` via
      :3129 (exactly one `setGeometry` plus focus retention; no Engine
      admission, no dispatch, no overlay clear, repeats request the same
      half again). Fixtures: `kwin/tests/plan-float-focus.test.ts`,
      `kwin/tests/plan-float-move.test.ts:144-282` (four-direction
      arithmetic, trunc, sticky parity, stateless repeats, stale refusal).
    - Core: `core/boundary.rs:58-67` Focus op (`float_subject` flag),
      `core/engine.rs:2840/:3022` focus proposal,
      `core/session/ops/focus.rs:26` `propose_focus`; cross-output float
      focus leg covered in
      `crates/tiler-core/tests/session_float_focus_cross.rs`.
      Half-snaps are adapter-native geometry (no Engine plan) on KDE;
      Windows must do the same, never an Engine admission.
    - Current Windows exact (verified 2026-10-08 at `db31234`):
      `src/tiling_sys.rs:5911-5935` sticky refusal (`focus-refused-sticky` /
      `move-refused-sticky`) plus float refusal (`focus-refused-floating` /
      `move-refused-floating`); `CoreCommand::Focus` :6096 with
      `float_subject:false` :6100; focus actuation `actuate_focus` :5402;
      float read `engine_is_float` :3963.

    #### Item 10 adapter checklist

    - `src/tiling_sys.rs` `keyboard_tick` :5645: before the Engine
      Focus/Move build, branch on float subject (`engine_is_float` :3963
      or `state.sticky` member, mirroring the :5911-5935 gates): Focus
      runs the ported `selectFloatFocusTarget` over same-domain observed
      floats (one distance race, sticky encounter-order first, same tie rules) via
      `actuate_focus` :5402; miss retains focus and reuses the existing
      horizontal output-edge path, never workspace cycling and never a new
      crossing. Move runs the ported `floatHalfSnapRect` over the work-area
      bounds plus inner gap, performs exactly one geometry write with the
      existing writable/hidden/minimum fences, and retains focus on the
      same subject; no Engine call. Keep the refusal vocabulary for
      ineligible subjects, invalid geometry, stale subjects and the
      deferred quarter/max/outward states.
    - [x] Offline: `tests/tiling.rs`, cfg(windows) retained Engine fixtures and
      existing `tests/snapkey.rs` catalog/pairing coverage (existing
      focus/move rows, no new chords): nearer ordinary vs farther sticky,
      first/last tie selection, miss
      retention, repeated half-snaps stateless with exact allocation,
      tile-origin skip, deferred states still refused. Port
      `kwin/tests/plan-float-focus.test.ts` axis/tie cases and
      `kwin/tests/plan-float-move.test.ts:144-282` arithmetic/repeat/stale cases.
    - Settings/input/catalog/presets: existing `FOCUS_MOVE_ROWS`
      (`src/settings.rs:575`) / `binding_catalog` :757-768 are implemented;
      retain their Keep/Disable/Rebind and `compatible_disabled_ids` :1528
      conflict policy. Half-snaps use current inner-gap settings; no new
      shortcut, preset rule or persistent snap-state setting is selected.
    - [ ] Windows-only: single-write readback, minimum-hint clamp,
      mixed-DPI work-area placement; native suppression evidence.
    - User journey: float-to-float Win+arrows (ordinary pair, then sticky
      pair: verify encounter-order tie rules); Win+Shift+arrows all four halves
      twice (same half again); miss holds focus; tile-origin Win+arrows
      ignore floats.
    - DoD: REQ-FLT-07..11 behave with deferred states still deferred;
      matrix `R-FLT-07..11` Ours Windows cells updated with dated offline
      evidence, native TBD.

  - Item 11: born-max/floating-retile overlay parity (b), Q3 including
    R-MAX-03. User decisions Q3 plus Q3 scope 2026-10-07
    ([decisions](decisions.md#window-state-float-sticky-maximize-fullscreen)): born-maximized
    tiles with a reserved slot and keeps maximize as an overlay, no launch
    unmaximize, on KDE and Windows; Q3 also covers R-MAX-03
    floating-to-tiled admission, replacing the one-shot restore on both.
    KDE R-MAX-06 delivered offline, R-MAX-03 delivered offline in
    `29c75fe` ([record](changes/archive/kde-maximized-floating-retile-overlay.md),
    born-max [record](changes/archive/kde-born-maximized-overlay.md)).
    R-MAX-06 now has the 2026-10-11 setting: Windows both values delivered
    offline ([record](changes/archive/windows-born-maximized-setting.md));
    R-MAX-03 delivered offline 2026-10-12, independent of that setting
    ([Windows record](changes/archive/windows-maximized-floating-retile-overlay.md)).
    Native MSVC offline gates pass; ordinary retile/restore and both-values born
    Paint launch user-accepted physically 2026-10-12 under item 10's conditions.
    Tray toggle previous-focus non-restoration is observation only; no change
    requested. Hidden/session/fixed/game/DPI/multi-output remain pending.

    #### Item 11 behavior and reference seams (historical handoff)

    - Current Windows seams: `retile_keep_overlay`, `admit_slotless_maximized`,
      `admission_clear_planned`, `clear_maximize_at_admission` and assembly
      slot seeding reuse born-overlay machinery. R-MAX-03 no longer clears;
      D5 first fullscreen exit is delivered offline with item 13. Historical
      line numbers and pending implementation descriptions below are superseded
      by the Windows record; native checklist remains open.

    - Normative: [spec](spec/functional-spec.md#maximize) REQ-MAX-03/03b/06;
      [matrix](spec/reference-outcomes/maximize-fullscreen.md) R-MAX-03
      (Ours KDE: floating skips slot seeding, retile reserves the slot,
      native unmaximize lands in it; Ours Windows: one-shot retile restore
      still in code) and R-MAX-06 (Ours KDE: reserved-slot overlay, no
      launch clear; Ours Windows: one admission-time clear attempt,
      retained slots/fullscreen/floating domains exempt). No launch
      unmaximize, no one-shot retile restore. First fullscreen exit is separate
      (D5 core/KDE delivered offline, Windows pending); B9 unmaximize/fresh-admit is item 15;
      R-MAX-08 focus fence/unmaximize-before-move is selected 2026-10-10,
      implemented separately under handoff item 18.
    - KDE exact (verified 2026-10-08 at `db31234`):
      `kwin/src/plan-adapter.ts:5933` `clearMaximizeAtAdmission` (skips
      fullscreen, skips attempted ids via `maximizeAdmissionAttempts`,
      clears only the non-fixed exact-ref-held born-fullscreen exit
      `heldInitialFullscreen`, echo arm/clear, observed refetch); call
      sites :5476/:5791; env `clearMaximize` :626; overlay = retained
      tile slot plus `fit_excluded` / `skip-maximized` isolation with
      quiet repeated/synchronous signals (matrix R-MAX-03/06 KDE cells).
      Fixtures: `kwin/tests/plan-adapter.test.ts`.
      Carried projection is `kwin/src/plan-adapter.ts:2721-2740`, payload
      `fit_excluded` :6071 and native `skip-maximized` :8338-8339. Shared
      `core/engine.rs:478-512` converges the carried set; core projection
      `core/boundary.rs:802-807` omits excluded frames from observed-clamp
      assessment while retaining planned geometry. No new Engine behavior
      is selected by this parity piece.
      The decisions register records KDE R-MAX-03 delivered offline at
      `29c75fe`; Windows parity and native acceptance remain pending.
    - Windows exact (verified 2026-10-08 at `db31234`):
      `src/tiling.rs:550` `should_clear_maximize_at_admission`,
      :567 `should_admit_slotless_maximized`; callers
      `admission_clear_eligible` (`src/tiling_sys.rs:3785`),
      `clear_maximize_at_admission` :3810 (attempt-key
      mark-before-native-call, `restore_zoom_placement`, assembly
      :3802-3898), `admit_slotless_maximized` :8726 (is_member,
      `member_rects`, holdable-key, lifetime gates),
      `admit_born_fullscreen` :8622. First-exit lane kept:
      `FullscreenMeta` read/write :3400-3496, `enter_fullscreen` :3520,
      `exit_fullscreen_owned` :3574, `track_fullscreen_holds` :3713.
      Current overlay refusal lane: `float_toggle_refusal` (`src/tiling.rs:402`)
      and `sticky_toggle_refusal` (:418); B9 intentional-maximized-unfloat
      refusal is a separate implementation gap. Fixture:
      `floating_retained_max_stays_slotless_then_tiled_seeds_and_plans`
      (`src/tiling_sys.rs:17164`).
      Row assembly is `src/tiling_sys.rs:4243`, retained maxima :4423-4468;
      slot seeding calls `src/tiling.rs:581` `should_seed_member_slot` at
      :4432, with canonical retained rectangles from `src/tiling.rs:480`.

    #### Item 11 adapter checklist

    - `src/tiling.rs` + `src/tiling_sys.rs`: replace the one-shot clear on
      both admission legs (slotless maximize :8726 and tiled clear :3810;
      :8622 is the separate born-fullscreen hold) with the reserved-slot
      overlay: retain membership, seed the slot (`member_rects`), skip native
      clear and geometry writes while overlaid, land native unmaximize in
      the slot, settle repeated maximize/geometry signals quiet with no
      unsolicited toggles or fighting. Retire
      `should_clear_maximize_at_admission` one-shot use; keep the held
      born-fullscreen single exit unchanged within this unit (D5 is separate).
      B9 unmaximize/fresh-admit is separately owned by item 15; preserve
      all unrelated overlay refusals. No new chord, setting or R-MAX-08 change.
    - Settings/input/catalog/presets: reuse current native/project maximize
      and workspace tiled/floating controls. Catalog toggles remain in
      `src/settings.rs:799-818`, workspace default :250-263, Settings UI
      `src/settings_ui.rs:314/:394` and tray `src/tray.rs:179-275`; no new catalog row or
      `compatible_disabled_ids` :1528 policy is selected by Q3.
    - [x] Offline Engine/adapter: both born-setting values, reserved slot and
      sibling share, restore projection, quiet repeat and reused lifetime.
    - [ ] Remaining native/application scenarios: born-max beside a sibling
      (sibling keeps its allocated share), hidden-workspace retile,
      floating-toggle retile, native restore to the exact retained slot,
      quiet repeated signals, reused IDs, session-boundary restored max
      with no loop.
    - [ ] Windows-only: native maximize/unmaximize roundtrip into the
      slot; graceful/crash recovery of overlaid members.
    - User journey: born-maximized launch beside a sibling (slot reserved,
      overlay quiet); native unmaximize lands in the slot; repeat
      maximize/unmaximize settles quiet; floating workspace with maximized
      A beside ordinary B, toggle tiled (A stays maximized over its slot,
      B takes its share), native unmaximize lands in the slot; repeat
      with a session-restored maximized A at a user-owned session
      boundary, no loop.
    - DoD: one-shot clear replaced by reserved-slot overlay on both legs;
      first-fullscreen-exit separate (D5 Windows delivered offline) from B9; D6 fixed
      automatic workspace-enable exceptions stay slotless per item 13;
      matrix `R-MAX-03` /
      `R-MAX-06` Ours Windows cells updated with dated offline evidence,
      native TBD.

  - Item 12: non-local workspace modes parity (d). Decisions 1.2/1.5
    ([decisions](decisions.md#workspaces)): local and
    global-unique keep per-output history/rings, shared keeps one
    history/ring. KDE mappings delivered offline. Stage A delivered offline
    2026-10-15: opt-in shared-core scope,
    mapping, history, ring, swap and migration plans; Windows schema-v1
    `core.workspace.mode` backfill/validation and restart-required Settings
    selector. Locked gates: 2,024 passed / 1 ignored; KDE planner fixture
    build passed ([record](changes/archive/windows-workspace-scope-policy.md)).
    Stage B delivered 2026-10-15: owner adoption and restart-required drift,
    numbered/previous/relative selection and follow/stay sends through hook,
    foreground and CLI paths; completed-only scoped history, global-unique
    two-output swap effects and shared logical view/ring/history/trailing
    cleanup with output-bound Engine domains. Existing local tests unchanged.
    Locked build/test/format/strict-clippy/diff gates passed: 2,078 passed,
    0 failed, 1 ignored. Single-output native CLI selection, sends and mode
    retention passed in all modes; clean graceful stop and restore verified
    ([record and handover](changes/archive/windows-workspace-scope-owner.md)).
    Stage C delivered offline 2026-10-15: adopted-mode topology dispatch,
    global scoped discard/re-prime/observations and shared order/domain sync
    without project displacement. Sibling Engine domains and true pending/drag
    state survive; ordinary idle orphan domains prune. Seventeen simulated
    topology regressions pass; final locked gates 2,095 passed / 1 ignored,
    single-output native select/previous/relative/send repeated in every mode
    with verified clean stop and saved-settings restoration
    ([final record](changes/archive/windows-workspace-scope-hotplug.md)).
    Item 12 is offline-complete; physical R-WS-17/multi-output acceptance stays
    user-owned in [pending checks](backlog.md#multi-output-setup), unsupported outcomes TBD.
    Shared startup anchor is tentative pending the two-output discriminator.
    Prerequisites (handoff items 1/2, multi-output foundation, explicit output
    sends and local whole-workspace migration) delivered.
    - KDE: consume shared workspace-scope policy (optional, coordinate with KDE stream).

    #### Item 12 behavior and reference seams

    - Normative: [spec](spec/functional-spec.md#workspaces) REQ-WS-08/11
      (Win columns: all-mode history/toggle/relative and mode-aware topology
      delivered offline 2026-10-15; physical multi-output pending);
      [matrix](spec/reference-outcomes/workspaces.md) R-WS-08/11 plus
      R-WS-15 (per-output isolation vs shared), R-WS-16 (removed-ID
      invalidation), R-WS-17 (hotplug history/scope). KDE mappings are
      selected and the spec carries scoped history/rings; any exact
      Windows outcome not defined is a TBD governing row, never invention.
    - Historical KDE seams (verified 2026-10-08 at `db31234`):
      `kwin/src/workspace-native.ts:990` `selectPrevious` (shared branch
      :995 `selectPreviousShared` :2250; per-output `previousByOutput`,
      `previousInScope`, `swapGlobalIfVisibleElsewhere` for global-unique),
      :1051 `selectRelative` (shared branch :1059 `selectRelativeShared`
      :2281; `rebuildGlobalMapping` vs `rebuildLocalMapping`,
      `scopedRingIds` :2203), :2141 `recordHistoryObservations`, :2185
      `observeSharedHistory`. Fixtures:
      `kwin/tests/workspace-previous-relative.test.ts`.
      Current shared Rust policy `core/workspace.rs:7-17` resolves local
      ordinals, and :35 `plan_trailing` handles local cleanup. It does not
      supply the KDE global/shared mapping machinery; preserve D1 ownership
      if this adapter parity needs a new shared-core contract.
    - Historical Windows gaps (verified 2026-10-08 at `db31234`, superseded
      by stages A/B/C above):
      `src/workspace.rs:95-109` `ManagedWorkspaces` fields are
      outputs/displaced/membership/next_ws/next_output/default_tiled/tiled:
      no previous-history or relative-ring resolver, no mode-scope field.
      Workspace order exists; mode provision is
      per-workspace tiled/floating ONLY: `is_tiled` :150, `set_tiled`
      :166, `default_tiled` :133/:139, settings
      `core.workspace.default_tiled` (`src/settings.rs:250-263`), settings
      UI :314/:394, tray per-workspace toggle (`src/tray.rs:179-275`). No
      global/local/shared scope setting exists anywhere in settings, UI
      or tray; the tray workspace-tiling action is a per-workspace
      tiled/floating toggle, never a global mode selector. Routing:
      `src/tiling_sys.rs:9819` `workspace_do_select`, verified
      hide/reveal :10061, `workspace_tick` :11395, selection arm :11530,
      `poll_foreground_workspace` :11912, `sync_monitor_outputs` :12050,
      `chord_output` :8508, `src/workspace_owner.rs:25`
      `output_context`. Cleanup `plan_cleanup`/`apply_cleanup`
      (`src/workspace.rs:453-508`) invoked :9228-9234 and :10074-10080.

    #### Item 12 adapter checklist

    - `src/workspace.rs` `ManagedWorkspaces`: add mode scope
      (local/global-unique/shared) plus per-output previous/observed
      history and scoped ring resolvers from handoff item 1 (pure
      stable-ID resolve, no activation/append/trailing creation);
      global-unique swaps the visible-elsewhere entry instead of
      duplicating selection (KDE `swapGlobalIfVisibleElsewhere` parity),
      shared keeps one history/ring. Record actual completed transitions
      at :10061 (including failed focus/geometry), `workspace_tick`
      :11530, foreground :11912 and monitor sync :12050; invalidate
      removed/unassigned/out-of-scope ids through the existing cleanup;
      discard disconnected-output history, never consult history for
      reconnect selection. Membership/visibility/recovery routing follows
      the transfer. No new shortcut, no new preset, no tray command.
    - [x] Portable: local vs global-unique vs shared isolation (R-WS-15),
      surviving-empty vs removed/moved id (R-WS-16), disconnect/displacement/
      reconnect/return scope with conditional second toggle (R-WS-17),
      output-focus-alone non-recording, cross-output previous and relative
      selection, >9/trailing wrap per scope.
      `kwin/tests/workspace-previous-relative.test.ts` cases covered by shared
      policy and Windows owner/topology fixtures; physical outcomes remain TBD.
    - Settings/input/catalog/presets: scope configuration is now selected
      tentatively in [decisions](decisions.md#workspaces) and delivered in
      stage A; startup adoption and restart-required drift delivered in stage B
      and reverified in stage C.
      Existing workspace shortcut rows
      (`src/settings.rs:819-826`) and handoff items 1/2 supply the selected
      chords; `compatible_disabled_ids` :1528 remains their policy. Scope
      configuration exposure is `core.workspace.mode`; the new three-choice
      Settings selector is separate from per-workspace tiling and the tray's
      tiled/floating action. Existing shortcuts/presets need no new rows.
    - [x] Windows-only offline: owner selection/foreground/CLI producers and
      mode-aware simulated hotplug; single-output native CLI regression passes.
    - [ ] Windows-only physical: hotplug, multi-output and keyboard journeys on
      a multi-output setup, listed in [pending checks](backlog.md#multi-output-setup).
    - User journey: two-output L/R with WS1/WS2 on L; L WS1->WS2, change
      R, focus L, previous twice (local: WS1 then WS2; R unchanged);
      shared mode WS1->WS2->WS3 then previous twice (WS2 then WS3);
      disconnect/reconnect return-scope sequence per R-WS-17.
    - DoD: non-local modes behave per scoped history/ring with undefined
      outcomes as TBD rows; matrix `R-WS-15..17` Ours Windows cells updated
      with dated offline evidence, native TBD.

  - Item 13: Q2 fixed-size float admission R-SPC-04. User accepted the
    ordinary fixed-size admission addition 2026-10-07; exact D1-D8 edges
    are user decisions 2026-10-08 (normative as delivered).
    KDE/shared delivered offline; Windows delivered offline 2026-10-11,
    prerequisite `a41fcb5` plus runtime delivery
    ([Windows record](changes/archive/windows-fixed-size-admission.md)).
    D1 predicate setting and functional naming delivered offline
    ([record](changes/archive/admission-and-move-settings.md)); D5/D6 delivered
    offline ([record](changes/archive/fixed-size-exit-and-enable.md)); D7
    delivered offline ([record](changes/archive/fixed-window-tile-override-restart.md)).
    Windows predicate, D5/D6 and fixed-origin D7 marker wiring are complete
    offline; native/game acceptance pending. Hydrated fixed-override maximize
    routing under both born-maximized values is tentative; R-SPC-13 native TBD.
    Second agent-live sweep 2026-10-17: Calculator min-only/unknown-max tiled
    correctly under the default predicate (not a positive fixed or either-axis
    leg); fresh born-maximized Paint cleared/retained maximize under the two
    settings. An owned equal-track tiler Settings anchor passed the protected
    restart case. Wider fixed/maximized and partial-zero matrix remain pending.
    [Record](changes/archive/fixed-size-admission.md),
    [decisions](decisions.md#fixed-size-admission),
    [matrix](spec/reference-outcomes/special-windows.md#q2-fixed-size-admission-discriminators-2026-10-08).

    #### Item 13 behavior and reference seams (historical handoff)

    - Normative addition: REQ-SPC-04 floats ordinary fixed-size clients.
      Decided edges (user 2026-10-08/10) REQ-SPC-04a..h: equal usable bounds;
      zero is unset per axis (G-05, item 17), partial-zero tiles under
      both-axes-fixed and floats under either-axis-fixed via its nonzero axis;
      missing/full-zero/sentinel excluded; admission-only hint
      status; same-live-client user tile overrides; fixed-floating base under
      native maximize; born-fullscreen first exit classifies as newly admitted
      (fixed floats with no writes, otherwise tiles; prior fixed floats
      restore); workspace enable checks every window being tiled (fixed stays
      untouched float, others tile; user tile overrides stay tiled);
      startup/restart adoption plus tile-override persistence in the same
      intentional-float store with recompute fallback;
      membership-only automatic admission with zero native target writes.
      Either-axis alternative is now `either-axis-fixed` (`Width or height fixed`),
      default `both-axes-fixed` (`Width and height both fixed`); both use the
      existing hint-validity guards. Breaking pre-release configs accepted, no
      migration. D6 fixed/maximized workspace enable keeps the floating base
      without a slot; explicit user tile overrides still use R-MAX-03's
      reserved slot. Windows borderless-game
      inference stays selected; B9 unmaximize/fresh-admit is separately
      delivered offline on KDE, Windows handoff item 15 pending.
    - References: COSMIC @3d55cba0 `src/shell/layout/mod.rs:46-52`,
      `src/shell/element/surface.rs:565-595`, `src/shell/mod.rs:2957-3022`,
      `src/shell/workspace.rs:1440-1454,1491-1519`. Fullscreen birth/restore
      are distinct (`src/shell/mod.rs:2754-2807,2960-2968`). COSMIC admission
      placement/focus is not permission to touch games; our D8 is no-touch.
    - Shared exact sites in this staged Q2 delivery: `core/size_hints.rs`
      `FixedSizePredicate`, `is_fixed_size_with` / `fixed_size_reason_with`; `core/session/world.rs`
      `converge_observation`, automatic/override sets; transactional
      `PendingDesired` in `core/session.rs` and `ops/lifecycle.rs` /
      `ops/float.rs`; `core/engine.rs` `set_fixed_size_admission` default OFF;
      `core/seed.rs` `EngineWindow` origin/overlay fields. Existing Linux
      Planner in `protocol/planner_protocol.rs` `Planner::new` enables it;
      Windows uses `Engine::new` directly and stays OFF. This is no new
      adapter-logic extraction. Fixtures: `crates/tiler-core/tests/fixed_size_admission.rs`,
      `crates/tiler-protocol/tests/planner_fixed_size.rs`.
    - KDE exact sites: `kwin/src/plan-adapter.ts` `withFixedSizeFloats`,
      `fixedClients` exact-reference records, `commitFixedStage` after applied
      command success, `retileAutomaticFixed`; optional `fixed_auto` and
      `fixed_suppress` wire assertions carry origin/override through domain
      release/adoption. `kwin/src/plan-adapter-entry.ts` `markTiledAndResync`
      resets sighted automatic and ordinary-admission pins only after confirmed release
      so enable checks current hints/predicate; explicit user tile overrides
      survive; current domain sightings protect omitted/moved identities. Native removal
      evicts identity, scoped/minimized absence does not. Real Planner and
      production-entry fixtures: `kwin/tests/fixed-size-admission.test.ts`,
      `kwin/tests/fixed-size-workspace-entry.test.ts`.

    #### Item 13 Windows adapter checklist (historical seams)

    - Max-side data is missing today: `src/tiling_sys.rs:733-817`
      `seed_minmaxinfo`, `query_outer_min_track`, `min_hint_for` extract only
      minimum track; maximum track is seeded at :743-747 but never extracted.
      Extend the same one bounded WM_GETMINMAXINFO query, preserving timeout,
      aggregate budget and failure-as-unknown rules. Normalize both sides
      with the same fresh DPI/frame-inset conversion at
      `src/tiling.rs:99-154` `normalize_min_track` / `min_hints_from_outer`;
      no geometry/style/resizeable inference of fixedness.
    - Wire visible/verified-hidden/retained rows through
      `src/tiling_sys.rs:804` `min_hint_for`, :827 `hidden_hint_for`, :4243
      `assemble_domain_rows`; unknown max hints never classify. Keep overlay
      hint retention lifetime-bound; do not reuse an automatic float's stale
      minimum as evidence for fresh fixed-size admission.
    - Select classification before native slot/restore/write side effects:
      `src/tiling_sys.rs:3656-3668` eligibility, :3785
      `admission_clear_eligible`, :3810 `clear_maximize_at_admission`, :8726
      `admit_slotless_maximized`, :8622 `admit_born_fullscreen`, and
      `src/product_hide.rs:825` `admit_managed_claim`. Fixed/maximized birth
      floats without clear/slot. On the FIRST born-fullscreen exit for the
      same lifetime, query current usable hints and apply the configured
      predicate before maximize-clear/restore/slot side effects: fixed becomes
      an untouched automatic float, non-fixed tiles; prior fixed floats
      restore floating. Later fullscreen toggles retain ordinary admission
      identity. No writes during fullscreen or to automatic fixed floats.
      On workspace enable, check EVERY tile candidate, including arrivals
      while floating and formerly ordinary tiles whose hints changed; reset
      ordinary admission/automatic marks but preserve explicit user tile
      overrides and intentional/sticky rules. Fixed stays/becomes untouched
      float, non-fixed tiles, including under maximize; only explicit tile
      overrides use item 11's reserved-overlay route. Never change the captionless full-monitor
      inference or app-owned fullscreen/preimage rules as part of Q2.
    - Carry live lifetime-bound origin and override through
      `src/tiling.rs:1226` `build_reconcile_event_for_floating`, :1281
      `build_reconcile_event`, `src/workspace_owner.rs:70` `build_send_event`
      and domain row assembly. This delivery's only Windows edits initialize
      `fullscreen`, `sticky`, `fixed_auto`, `fixed_suppress` to false in
      those constructors and two native preview-test literals. Enable
      `Engine::set_fixed_size_admission(true)` only after complete observation,
      origin and no-touch wiring; never infer automatic origin from an
      intentional floating flag or fixed hints alone.
    - Same live client keeps explicit tile/sticky intent through hide/show,
      scope changes and domain release; native lifetime replacement clears
      it. Workspace enable checks every window being tiled (fixed stays
      untouched float, others tile); intentional/sticky stay under existing
      rules, user tile overrides stay tiled. Restart recomputes absent
      authoritative identity with fallback. D7 is delivered on KDE only:
      successfully applied fixed-window tile overrides persist membership in
      the same intentional-float store, no positions. Clear on re-float,
      verified close and complete-inventory prune; retain through hint/predicate
      changes. Missing/degraded/unmatched records recompute fixed to untouched
      float, with bounded outcome logs and no hold/retry. Windows lacks float
      persistence: coordinate with item 8's mechanism before implementing D7;
      this handoff delivers no Windows runtime behavior.
    - Add `CoreSettings.fixed_size_predicate` / `core.fixed_size_predicate`
      within schema version 1: missing defaults to `both-axes-fixed`, alternative
      `either-axis-fixed`; validate using shared `FixedSizePredicate::parse_wire`.
      Unknown values use existing invalid-file/last-good handling. UI labels
      `Width and height both fixed` / `Width or height fixed`; tooltips COSMIC /
      Hyprland (Wayland), sway. Adopt last-good changes with
      `Engine::set_fixed_size_predicate` for subsequent admissions only, without
      resync/reclassification; live user tile overrides still win. Log effective
      values with other settings. KDE `fixedSizePredicate` and protocol
      `fixed_size_predicate` are delivered offline; Windows remains handoff-only.
      No new shortcut, preset or hook; no migration. Automatic membership must
      emit bounded correlated decision evidence and cause no game geometry,
      focus, stacking/keep-above or effects/input interference. Do not call
      the explicit-float placement/focus path for automatic admission.
    - [x] Portable/offline: full hint table, opt-out unchanged behavior, fresh
      foreground/hidden startup, both hint transitions, explicit unfloat /
      sticky overrides through hide/domain adoption, ID reuse, fixed/max
      birth and enable/native restore, born/prior-float fullscreen, first-exit
      hint/predicate changes and repeated toggles, fixed/non-fixed arrivals
      while floating, changed hints/predicate at enable, explicit tile
      overrides vs ordinary admission pins, intentional/sticky controls,
      failure paths and all setter no-write accounting; D7 success-only persist,
      restart tile hydration, re-float/close/complete-prune, hint-loss/enable,
      degraded/legacy/mismatched fallback and non-fixed controls. Both new classification
      points must emit bounded correlated decision diagnostics.
    - [ ] Windows-only: fresh max-track/DPI/inset observations, timeout/hung
      clients, native overlay/restore and fixed borderless/exclusive games;
      user owns live Windows checks. Linux tests prove none of these.
    - [x] Windows offline: predicate default/variants/invalid and subsequent-admission
      tests, setting/schema/UI/live wiring; same-axis uses item 3's renamed
      values, with old IDs rejected and no aliases. User owns live checks.
    - DoD: decided clauses implemented with current Windows
      rules preserved; dated Ours Windows matrix/spec evidence, no native
      claim without user testing; user decisions 2026-10-08 approved, changed
      D1/D5/D6/D7 Windows wiring delivered offline; native checks above pending.
      Current evidence: MSVC locked four-package build/test, format, strict
      clippy and diff checks pass; 325 Windows library tests. Owned message-only
      marker readback/hydration covers retained-maximized hint loss, both born
      modes and identity failures; this is not an ordinary-app/game journey.

  - Item 14: Q4 whole-active-workspace output migration R-WS-12. The addition
    was accepted 2026-10-07; D1-D9 detail is user decisions 2026-10-08
    (normative as delivered). Core/Linux planner/KDE delivered offline;
    Windows local adapter, D8 carry and largest-edge selection delivered offline
    2026-10-14 ([Windows record](changes/archive/windows-workspace-output-migration.md));
    locked native MSVC build/test/fmt/strict clippy/whitespace gates pass:
    1,950 passed, zero failed, one ignored. D8 core/KDE delivered offline
    2026-10-09. Depends on the multi-output
    foundation (item 5), scoped history (item 1), and item 12 for non-local
    modes. [Record](changes/archive/kde-whole-workspace-output-migration.md),
    [spec](spec/functional-spec.md#workspaces) REQ-WS-12/12a..i,
    [matrix](spec/reference-outcomes/workspaces.md#r-ws-21-migrate-mode-capability).

    #### Item 14 behavior and shared contract

    - Four directional active-only, follow-only actions, all UNBOUND by default.
      Resolve FULL-output-rectangle reciprocal adjacency, no wrap; choose
      largest shared edge, then left/top (User decision 2026-10-09; selection
      delivered offline in core/KDE and Windows; item 16 dependency closed).
      Hidden R-WS-12 baseline has no counterpart; explicitly select its active
      leg. KDE local/global-unique requires strict native per-output desktops;
      shared/false/unreadable refuses. Windows has no KWin option: establish
      separable per-monitor views through its existing project workspace owner,
      not a guessed registry policy or an emulated native capability flag.
    - Preserve backing ID, tree/order/shares/remembered focus/tiling mode;
      insert after target current and show migrated, prior target stays hidden.
      Source selects last remaining scoped entry, existing minimum-two/trailing
      spare converges; empty allowed. Carry workspace floats with class/origin,
      sticky stays source. Carry fullscreen plus maximized members (changed D8,
      user 2026-10-08, core/KDE delivered offline): no refusal in moved members or
      affected views; native move only with no extra fullscreen
      size/position/focus writes; an explicit user move is not unwanted
      interference. Preserve maximized overlay/reserved slot, no unmaximize;
      native re-fit owns overlay geometry on the target.
    - Shared `core/boundary.rs` `CoreCommand::MigrateWorkspace { direction }`
      and `CoreReply::MigrateWorkspace(MigrateWorkspacePlan)`; event target has
      the SAME workspace ID on a DIFFERENT output and empty `target_windows`.
      Destination's DIFFERENT current workspace remains adapter-owned, including
      its currency fences (overlays are not a refusal). `core/engine.rs` `migrate_workspace_request` rekeys via
      `core/session/world.rs:251` `relocate_domain`, not per-window re-admission.
      Reply geometry/focus/active_window/member counts and postconditions mean
      retained `planned`, never native completion. Protocol `migrate-workspace`
      is codec-only; Windows calls the public Engine types directly.
    - Follow moved active client only after every member/output/membership and
      both views are verified; empty/sticky-active uses native output switching
      without invented focus, minimized clients stay minimized. Fullscreen
      follow is native-only, with no focus setter or unverified focus claim.
      Verify overlay arrival by identity/membership/output, not its old frame;
      re-read live overlay flags before geometry/focus writes and suppress them
      while overlaid/fullscreen respectively. Keep lifetime/currency fences and
      reconcile mode changes; do not replay or skip a carried member.
      History 1.3/1.5
      invalidates out-of-scope previous IDs; remove ONLY moved ID from hotplug
      return associations. Partial/uncertain native writes reconcile observed
      domains without replay, guessed rollback or false completion.

    #### Item 14 exact Windows adapter sites

    - `src/workspace.rs:95` `ManagedWorkspaces` currently stores output order,
      active index, membership, displaced origins and tiled state. Add one
      whole-workspace reassignment primitive, preserving those stable IDs and
      state; use `workspace_members` :659 / `workspace_ids` :673 for exact
      membership/order, and `active_id` :236 / `activate` :244 for source/target
      view transitions. `displace_output_to` :561 / `reconnect_output` :603
      must forget only the explicitly moved ID. Do not simulate this by N sends
      or move the destination workspace back to source.
    - Add a migrate event builder beside `src/workspace_owner.rs:70`
      `build_send_event`; permit empty active client, carry every workspace
      member's overlay/sticky/class/origin flags and target-scope currency.
      `stamp_send_target` :148 is SendToWorkspace-specific; do not reuse its
      command. Extend `planned_writes` :406 and native
      `src/tiling_sys.rs:4211` `desired_entries` for the new typed reply.
      `verify_membership_transfer` :164 proves ONE mover only: migration must
      prove the complete set plus retained mapping and both current views.
    - Add a distinct route beside `src/tiling_sys.rs:10552` `workspace_do_send`
      (event construction :10848, Engine handle :10865, membership proof :10952)
      and :10314 `workspace_do_send_native`; their eligibility and :11017
      `workspace_send_follow` are window-send/same-output tails, not a whole
      migration implementation. Resolve full rectangles via `all_monitors`
      :264 / `monitor_fulls` :282, bounds via `workspace_domain_for` :8540.
      Fence all windows/related natives, source/target current views, mode,
      lifetime and overlay state before the first and every later setter;
      overlay state suppresses geometry/fullscreen focus, never a carry refusal. Verify
      arrivals before geometry/view/focus completion. Protect hidden/minimized
      clients and existing product-hide/recovery authority through partial writes.
    - Compile-only edits in this delivery: `src/tiling.rs:1263/:1315`,
      `src/workspace_owner.rs:98/:115`, and native preview-test constructors
      `src/tiling_sys.rs:16719/:16927` initialize `EngineWindow.maximized` to
      false. These are NOT fresh overlay observations. Replace placeholders
      only when the Windows adapter supplies actual overlay observations;
      retain existing fullscreen inference and game/preimage policies.
    - Proposed Windows IDs `migrate-workspace-left/right/up/down`, defaults
      empty, follow-only, no new holder claim. Add catalog family/routing in
      `src/settings.rs:475` `BindingFamily` / :757 `binding_catalog`;
      `src/snapkey.rs:427` `WorkspaceOp` / :445 `WorkspaceIntent` currently
      encode digit select/send, not migration. Reuse items 1/2's explicit ACTION
      rebinding and full-modifier queue/hold pairing, never a canonical empty VK.
      Both presets retain unbound defaults; dynamic settings rows remain
      rebindable with no new tray command. Actual rebound chord conflict policy
      follows existing validation, not a fabricated default conflict.
    - KDE mirrors: `kwin/src/plan-adapter-entry.ts` `requestWorkspaceMigrate` /
      `observeMigrateWorkspace`, `kwin/src/workspace-native.ts`
      `commitWorkspaceMigration`, `kwin/src/workspace-send-adapter.ts`
      `requestMigrateWorkspace` / `progressMigrate` and per-setter fences.
      Port `kwin/tests/workspace-migrate.test.ts` and
      `workspace-migrate-engine-fixture.test.ts` invariants, not duplicated
      adapter architecture; no core extraction is selected.
    - [x] Portable/offline 2026-10-14: retained unequal-share/nested tree, floats/origins, sticky
      active and minimized/null focus, empty lifecycle, insert-after-current,
      scoped modes/history/return siblings, all-direction adjacency,
      fullscreen+maximized carry with no extra overlay writes, stale
      revision/ref replacement/removal/partial-arrival
      recovery; actual Engine reply extraction and unbound rebind/preset tests.
    - [ ] Windows-only: whole-member physical output transfer, mixed DPI/work
      areas, native arrival/view/focus timing, hidden/minimized/transient lifetime
      and carried-overlay native-move-only writes, owned-window recovery after partial
      writes. User owns live journeys on a two-output setup.
    - User journey/DoD: select WS2 `H[A,V[B*,C]]` on L, R shows occupied WS3;
      bind migration right, verify same WS2/tree/shares, WS3 hidden, L last
      remaining, B focus only after arrival. Repeat empty, intentional/automatic
      float plus sticky, fullscreen+maximized carry (Windows D8 offline) and
      displaced-origin reconnect.
      Update Ours Windows/spec with dated offline evidence; native outcomes TBD
      until user-tested. User decisions 2026-10-08 approved; D8 carry
      core/KDE and Windows delivered offline; physical two-output journeys pending.

  - Item 15: B9 / M09=A maximized intentional unfloat R-FLT-06 (User
    2026-10-08). Shared-core/KDE delivered offline; Windows delivered offline
    2026-10-12 ([Windows record](changes/archive/windows-maximized-intentional-unfloat.md)).
    Native MSVC gates pass; scripted native effects plus real Engine cover
    clear-before-admission, restored frame, narrow refusal/later-press recovery,
    post-clear lifetime/focus/domain/fullscreen races and fixed override. Native
    ordinary clear/frame/focus agent-observed 2026-10-12 in scoped Notepad/Paint;
    physical B9, fixed marker/restart, failure/race/game/DPI remain pending. Checklist below
    records the delivered contract and remaining native acceptance.
    [Record](changes/archive/maximized-intentional-unfloat.md),
    [spec](spec/functional-spec.md#7-floating-r-flt),
    [row](spec/reference-outcomes/floating.md#r-flt-06-float-toggle-over-a-maximized-window).
    - Replace `src/tiling.rs` `float_toggle_refusal` maximize refusal ONLY
      for an ordinary currently floating target being explicitly tiled.
      Preserve normal-to-float overlay refusals, fullscreen handling,
      sticky gates, native identity and verified effect/settlement fences.
    - One native unmaximize attempt per explicit activation; observe the
      same live client unmaximized before Engine fresh admission or geometry
      writes. Use the restored frame and ordinary new-window rule, never an
      old slot. Reuse existing native observation/settlement patterns, no timers.
    - Refused/unobserved/raced clear: log cause, retain float membership and
      usable preimages, end only that operation, keep other functionality and
      later explicit presses available. No automatic clear retry or fighting.
    - Fixed automatic or intentional floats use the existing D3/D7 explicit
      user tile override on successful admission (items 8/13); no automatic
      re-float. Q3 born-max/workspace-retile overlay retention stays item 11.
    - Acceptance: clear-before-geometry real Engine/native-setter regression,
      ordinary nonmax unfloat unchanged, refusal/unobserved clear with no
      stuck state and later-press recovery, exact-ref/focus/fullscreen races,
      fixed override after subsequent observation/restart. Update Windows
      REQ-FLT-06/06b and Ours cell with offline evidence; native journey TBD.

  - Item 16: position-based output selection (User 2026-10-09). Shared core/KDE
    delivered offline; current Windows window paths verified/wired offline
    `fda618f`, 2026-10-14 ([Windows record](changes/archive/windows-position-based-output-selection.md)).
    Foundation directional movement/cross-focus already used the shared
    window selector; explicit sends now reuse it with pinned transfer fences.
    Pointer drops use pointer containment. Native gates: 1,892 passed, 0 failed,
    1 ignored. Largest-edge migration subsequently delivered with item 14
    ([record](changes/archive/windows-workspace-output-migration.md)); latest
    gates 1,950 passed, zero failed, one ignored. Physical journeys pending.
    [Record](changes/archive/position-based-output-selection.md),
    [spec](spec/functional-spec.md#move) REQ-MOV-08/08b,
    [output](spec/functional-spec.md#output) REQ-OUT-01/04,
    [workspaces](spec/functional-spec.md#workspaces) REQ-WS-12/12c.
    - Reuse `tiler_core::output_selection` (`OutputEntry`, `Candidate`,
      `candidates_in_direction`, `select_for_window`, `select_for_migration`).
      Supply complete, readable FULL monitor topology with unique stable IDs
      and a matching source entry. `Err(UnreadableTopology)` refuses;
      `Ok(None)` is a no-op. No candidate skipping, output wrap or origin-distance
      fallback. Existing item-5 monitor/event sites remain the wiring seams.
    - Exhausted moves and explicit output sends use the moving window's native
      frame in the same coordinates as FULL monitor rectangles. Left/Right
      project y, Up/Down x; half-open shared intervals [start,end) and doubled
      centres preserve odd-extent halves. A containing edge wins; multiple
      containing edges tie directly by left/top. Only if none contains the
      centre, maximize window-span overlap; final x, then y, then stable ID.
      Migration uses largest shared-edge length, then the same tie-break,
      never the focused window. Deliberate COSMIC origin-distance deviation.
    - Keep full rectangles for selection only; domain bounds/placement stay
      work-area based. Present the selected reciprocal pair to the Engine;
      reverse multiplicity must not refuse. Local move rules still win,
      R4 lands nearest the source, send uses ordinary admission and follow/stay.
      Migration carries workspace floats, keeps sticky windows on source.
    - Pin both dispatch source and chosen target through every native transfer,
      membership, geometry, arrival and follow/stay check. Never re-rank after
      relocating the mover; it now has a target frame. Re-read topology and
      verify the pinned candidate still touches, retaining current-view,
      domain/gap/mode/member/lifetime fences and partial-transfer reconciliation.
      KDE mirrors: `observeDirectionalDomain` and `r4RawFresh` carry
      `pinnedSource.targetOutput`; `requestOutputSend` ranks by frame;
      `observeMigrateWorkspace` ranks by shared-edge length. Focus navigation
      retains its existing policy.
    - [ ] Portable: all four directions/two candidates, centre precedence,
      seam/odd extent, overlap fallback, left/top/identity ties, multiple
      containing edges, reverse multiplicity, unreadable other output/source
      mismatch, no candidate, migration largest-edge/equal-edge and no window
      dependency. Core tests: `position_based_output_selection.rs`.
    - [ ] Adapter/real Engine: non-left/top move and send, non-top largest-edge
      migration, FULL-vs-work-area placement, exact native arrival/follow/stay,
      delayed arrival and chosen-target removal/drift mid-flight. Port the
      `output-send-engine-fixture.test.ts`, `workspace-migrate-engine-fixture.test.ts`
      and `plan-directional.test.ts` discriminators to Windows's existing harness.
      Linux portable tests prove no Windows native behavior. User owns live checks
      on a multi-output Windows setup; update Windows matrix/spec only with actual evidence.

  - Item 17: G-05 per-axis-zero fixed-size predicate (User 2026-10-10,
    REQ-SPC-04a). Coordinate with item 13: zero means unset on each axis;
    equal partial-zero hints (640,0) and (0,480) are not fixed under
    `both-axes-fixed`, but their genuinely fixed nonzero axis makes them fixed
    under `either-axis-fixed`. Keep full-zero and unbounded-sentinel guards.
    Shared-core predicate + KDE delivered offline 2026-10-10; Windows delivered
    offline 2026-10-11 with item 13's max-track/schema/UI/Engine opt-in
    ([record](changes/archive/windows-per-axis-zero-predicate.md)).
    Exact seams: `core/size_hints.rs` `fixed_size_reason_with` /
    `is_fixed_size_with` now require equal nonzero bounds per axis;
    `kwin/src/plan-adapter.ts` `isFixedSize` mirrors it. No signature/wire
    change or Windows predicate copy. Reuse item 13's bounded hint observation
    and `Engine::set_fixed_size_admission` opt-in plus
    `Engine::set_fixed_size_predicate`; do not rebuild retained classifications.
    Port `fixed_size_admission.rs` partial-zero/admissions-only cases and
    `kwin/tests/fixed-size-admission.test.ts` real-Planner discriminators.
    - [x] Offline acceptance: positive fixed, both partial-zero orientations, full-zero,
      missing, sentinel and subsequent-admission/no-reclassification controls.
    - [ ] Native hint representation and game no-touch checks remain pending.

  - Item 18: G-06 maximized directional focus/move (User 2026-10-10,
    REQ-MAX-08 NORMATIVE; resolves G-D1). Both platforms follow COSMIC:
    directional focus no-ops while the focused window is maximized (leave via
    unmaximize or Alt+Tab); directional move unmaximizes first, then moves.
    Replace Windows focus-permit/movement-refusal, retaining pointer refusals
    and fullscreen policy. Shared opt-in focus fence + KDE delivered offline
    2026-10-10; Windows delivered offline 2026-10-12 from base `e506350`
    ([record](changes/archive/windows-maximized-directional-navigation.md)). Native
    MSVC gates pass; pre/post-clear authorization, fullscreen precedence and
    coordinated float targets covered offline. User-accepted physically
    2026-10-12 under item 10's conditions: maximized tiled focus fence and
    clear then tiled move/float all four halves. Failure/race/held-repeat/preset,
    fixed-size, game/DPI/multi-output pending.
    Windows keeps the local fresh-flag guard because reconcile omits maximize;
    shared Engine still owns ordinary tiled move. Historical seams:
    - `core/engine.rs` `set_maximized_focus_fence` propagates to retained/fresh
      sessions; default OFF preserves Windows. `core/session/ops/focus.rs`
      fences local/cross-output focus for a maximized non-fullscreen origin.
      Enable only with fresh observed flags; KDE wire still omits maximize
      for born-maximized admission and uses its local fence instead.
    - `src/tiling_sys.rs` directional origin overlay gate near `SnapOp::Move`
      / `overlay_refusal_for` (currently :5980-6000) must fence focus from
      the maximized origin and replace move-refusal with one nonactivating
      `restore_zoom_placement` attempt (:3303). Dispatch only after observed
      clear and fresh same-lifetime/focus/domain checks; async `dispatched`
      is not observed completion. Keep fullscreen/gaming/suspension fences.
      `build_reconcile_event_for_floating` / command assignment (:6082-6111)
      currently omit overlay flags; opt-in alone does not replace this guard.
    - Keep focus TO a maximized target valid from an ordinary origin; do not
      blanket-reject `revalidate`'s `allow_maximized` focus path (:3249).
      Coordinate ordinary float half-snaps with item 10's pending routing.
    - KDE reference: `PlanAdapter.requestFocus`, `requestFloatFocus`,
      `requestMove` and `dispatchMove`; one clear, exact-reference echo,
      fresh directional reread, no delayed intent/retry. Port
      `maximized_focus_send.rs`, `plan-maximized-focus-move.test.ts` and
      `g06-g37-entry-discrimination.test.ts` (real retained Engine).
    Acceptance: focused-max focus in all
    directions; fresh isolated maximized mover clears before ordinary move;
    ordinary navigation and pointer/fullscreen controls unchanged. Native
    focus/state/geometry timing checks pending on both platforms.

  - Item 19: G-37 Source workspace after migration (User 2026-10-10,
    REQ-WS-12d/e). Coordinate with items 1/14: Windows
    `core.migration_source_refill`, KDE `migrationSourceRefill` + KCM;
    `last-remaining-workspace` default (`Last remaining workspace`, tooltip
    COSMIC) / `most-recently-used-workspace` (`Most recently used workspace`,
    tooltip bspwm, i3, awesome). Additive schema-v1 field, missing=default,
    no migration; live changes affect subsequent migrations. MRU uses item
    1.2 per-output previous stable ID snapshotted before map mutation; it is
    eligible only while live in the remaining source-output scoped ring,
    excluding the migrated ID. Surviving empties qualify; missing, removed,
    migrated or out-of-scope entries fall back to last remaining. Preserve
    a still-scoped live source current view in both modes. Keep scoped
    invalidation, destination insertion and empty migration unchanged.
    Shared-core/KDE setting and MRU delivered offline 2026-10-10; Windows config,
    settings UI and runtime wiring delivered offline 2026-10-14 with item 14
    ([record](changes/archive/windows-workspace-output-migration.md)). Exact seams:
    - `core/output_selection.rs` `MigrationSourceRefill::{LastRemaining,
      MostRecentlyUsed}`, `parse_wire` / `as_wire_str` and pure
      `select_migration_source_refill(mode, previous, remaining_scope)`;
      caller owns live scope/history invalidation, selector creates nothing.
    - KDE `kwin/src/migration-source-refill.ts` mirrors selection;
      `WorkspaceNativeAdapter.commitWorkspaceMigration` snapshots history
      before rekey and chooses source refill after scoped invalidation;
      entry `configChanged` calls `setMigrationSourceRefill` for live reread.
      KCM `unifiedsettings_module.cpp` / `.ui` owns the functional labels,
      WM tooltips, defaults and typed reconfigure, application unconfirmed.
    - Windows `src/settings.rs` `CoreSettings` / `Default` /
      `validate_settings`: ADD the version-1 string field and exact-token
      validation are delivered; `src/settings_ui.rs` owns the control and
      Apply/Revert. The migration route reads current `settings_live` after
      `apply_live_settings` adoption, using item 1's per-output observed history.
    - Port core selector cases, `migration-source-refill.test.ts` and
      production-entry real-Engine `g06-g37-entry-discrimination.test.ts`.
    Acceptance: MRU differs from last-remaining, no eligible
    entry fallback, migrated/removed entry exclusion, output-local history,
    default/invalid/live settings, retained moved workspace state; native
    journeys pending on both platforms.

  - Item 20: G-D2 maximized workspace-send carry (User 2026-10-10,
    REQ-MAX-09 maximize leg).
    Windows same-output numbered/relative follow/stay and native unmaximize
    agent-observed 2026-10-11 at base `553c65b` + item-2 delivery commit
    ([record](changes/archive/windows-workspace-send-follow-stay.md)); native
    gates passed. Physical/flag-race and cross-output legs remain pending.
    Keep maximize on arrival: Windows existing
    policy, cross-platform target; KDE carry delivered offline 2026-10-10.
    Deliberate COSMIC/niri
    deviation: a send relocates the whole window/state, unlike in-layout move.
    Verify Windows retained-overlay send/follow and send-and-stay through
    ordinary send integration; do not replace it with item 18's unmax-move.
    Exact seams: `src/tiling_sys.rs` `workspace_do_send` retains the
    pre-dispatch `is_zoomed_now` check and post-plan `send_flags_stable`
    overlay check before `workspaces.assign` (:10718-10737/:10885-10945).
    Keep source/target allocations and geometry overlay skip; item 2 adds
    follow/stay without restoring the mover. Native-boundary sends use
    `workspace_do_send_native`. KDE reference:
    `observeSendTarget` / `observeOutputSendTarget` and same-output
    `workspace-send-adapter-entry.ts` observer admit maximize/fullscreen while
    refusing float/sticky/independent fit exclusions;
    `WorkspaceSendAdapter.crossOutputMoverLive` / `apply` require flag-stable
    arrival and skip overlay geometry. Port
    `workspace-send-maximized-carry.test.ts` follow/stay and `output-send.test.ts`
    cross-output carry/flag races. Actual KDE native maximize carry is pending
    observation; no restore/remaximize workaround was inferred offline.
    Fullscreen stays item 9/Table A 2026-10-07; KDE D28 carry is delivered
    offline and whole-workspace D8 carry remains distinct. Acceptance: maximize on
    arrival, source reflow/target slot and follow/stay, native unmaximize on
    target; live checks pending, fullscreen controls unchanged.

  - Item 21: workspace tiling toggle Win+Y delivered offline 2026-10-13
    ([record](changes/archive/windows-workspace-tiling-shortcut.md)). Default
    selected user 2026-10-10, matching COSMIC `ToggleTiling` Super+Y
    (`cosmic-comp/data/keybindings.ron:85`, pinned `3d55cba0`). Catalog,
    classifier/queue and owner dispatch reuse the tray's per-workspace toggle;
    Authentic takes the recorded Mixed Reality conflict, Compatible disables
    it (41 conflicting rows; 87 catalog rows). Rebind/disable and takeover-off
    are covered offline; Apply/Revert use existing settings paths. Native
    locked gates passed: 1,803 passed, 0 failed, 1 ignored. Other currently
    unbound actions stay unbound. Native/physical toggle, tray sync, presets,
    Apply/Revert and takeover-off acceptance remains user-owned below. KDE
    Meta+Y delivered offline 2026-10-11
    ([record](changes/archive/kde-workspace-tiling-shortcut.md)); its
    physical/restart/preset checks remain below.

  - Item 22: 0.1 triage Windows follow-ups (updated User 2026-10-13).
    D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 are approved;
    no triage units remain tentative.
    Shared core delivery serves Windows; Windows adapter/native work follows
    the shared contract. D01's fullscreen focus fence is delivered in KDE
    with shared verification; Windows has its own foreground rules. Consider native
    approval/denial and explicit switching in the Windows handoff, without
    importing KWin stacking or asserting a Windows fence gap. D03/D09/D10/
    D12/D13 use existing shared defaults, with native verification pending.
    D22 survivor-share preservation is verification-only under shared removal,
    no setting; Windows native float-out reflow remains pending.
    Approved D16 ordinary/combo shared delivery is complete: intentional floats
    transfer through `MoveDeferred`, retaining float class/size without either
    tree changing; live fullscreen/maximize travels natively. KDE is delivered
    offline; Windows adapter delivered offline 2026-10-13
    ([Windows record](changes/archive/windows-d16-floating-transfer.md)); native acceptance remains
    ([record](changes/archive/d16-floating-transfer.md)). No setting. Same-output
    exact frame/no geometry write confirmed User 2026-10-13. Windows adapter steps
    completed offline 2026-10-13 (1,818 passed, 0 failed, 1 ignored):
    (1) In `src/tiling_sys.rs` `workspace_do_send`, narrow the current
    `send-refused-floating` gate to admit ordinary intentional floats and
    floating+fullscreen/maximized combos on tiled-to-tiled sends. Keep
    `send-refused-sticky` and automatic-fixed refusal using existing
    `engine_is_automatic_fixed` / fixed-origin state. The floating-source
    `workspace_do_send_native` boundary route already carries native float frames;
    it does not deliver the refused tiled-to-tiled deferred shapes.
    (2) Preserve floating state from `assemble_domain_rows` / `domain_rows` /
    `build_send_event`; bind the focused float mover's exact live overlay flags
    from `overlay_now` through `stamp_engine_event`. Existing `build_send_event`
    defaults `maximized` false and `stamp_fixed_admission_flags` only stamps
    fullscreen/fixed flags: floating+maximized needs a send-specific flag path,
    without changing tiled/survivor/admission encoding or protocol semantics.
    (3) Explicitly handle `SendWorkspace(plan)` with `plan.operation` equal to
    `MoveDeferred`: no fabricated leaf, no mover geometry, no source/target
    geometry application (including `workspace_send_stay`'s source-only tail),
    and no maximize restore/fullscreen toggles. Keep `workspaces.assign`,
    `verify_membership_transfer`, `send_fences_hold` / `send_overlay_gate`,
    identity/scope/lifetime/hide/reveal checks and existing follow/stay routing.
    (4) Retain core float geometry, adapter `float_rects` / `floated` intent and
    native overlay restoration state across membership movement. Destination
    exit returns to the floating frame, never tiled admission; restart float
    memory follows the window identity. Keep destination tiles and slotless
    source untouched.
    Acceptance: both combos x follow/stay x numbered/relative, ordinary/combo
    WS1 -> WS2 -> WS1 roundtrip, exact arrival flags/frame, zero geometry/overlay
    writes, destination exit still floating, sticky/automatic-fixed refusals,
    sole-window source and empty target. Extend `workspace_owner.rs` send-event,
    retained-overlay, writable-subset and sole-stay seams; native acceptance
    remains pending. Windows delivery includes explicit suppression of destination
    follow geometry, even with drifted tiles; fake-window combo readback deferral
    is not evidence of successful native transfer. Shared/KDE delivery alone did
    not change the Windows source behavior.
    Approved D23 shared rework delivered offline 2026-10-13
    ([evidence](changes/archive/d23-rework.md)); Windows adapter delivered offline
    2026-10-13 ([Windows record](changes/archive/windows-d23-restore-close.md)):
    retained minimize allocation (divergence, not an approved exception) replaced
    with release/reflow/MRU and session-only best-effort stored-slot rejoin. COSMIC
    `3d55cba0` source gate passed. Former retained-allocation sites:
    `crates/tiler-windows/src/tiling_sys.rs` `observe_window` (iconic returns
    `ObserveFailure::Known(.., SkipReason::Minimized)` with no frame),
    `TileLoop::observe` (that arm pushes a frameless `RetainedRow { rect: None,
    .. }`), `assemble_domain_rows` (the retained-occupancy lane rides the
    `member_rects` last-known rect as a tiled `floating: false` `MemberView`),
    `crates/tiler-windows/src/workspace_owner.rs` `domain_rows` (keeps the
    observation complete so Engine membership survives), and
    `workspace_close_cleanup` (retained members stay enumerated; only absent
    HWNDs drop). Adapter steps completed offline: (1) drop the minimized window from the carried
    Engine observation on minimize so `Session::converge_observation` releases
    its leaf via `remove_leaf_from_tree`, reflows survivors, resolves focus via
    retained/MRU-or-none, and stores the `MinimizedSlot` (homing domain + leaf
    id + restore context); (2) on native restore, re-carry it purely tiled for
    best-effort rejoin beside a former neighbor, same side/group/index and
    comparable old share. Shared best-effort rejoin is delivered; exact equality
    is only a fast path, not a validity prerequisite. Resize/new-window/
    other-sibling-close/swap do not invalidate alone. Fresh admission only if
    no former neighbors remain, domain gone or sent elsewhere; (3) never write focus on
    restore - the core keeps pre-restore MRU focus (the converge focus arm skips
    `observed_focus` when it names a just-restored window), leaving host restore
    focus native; (4) sole-minimize keeps the workspace via
    `Engine::committed_session_is_empty` honoring `has_live_minimized_slot`,
    with focus none; (5) native reveal stays in
    `crates/tiler-windows/src/model.rs` the `show.minimized` lane
    (nonactivating restore), unchanged; (6) report minimized-window close via
    additive `CoreEvent.closed_windows` on the valid domain observation, using
    verified closed HWND lifetime ids (never minimize or transient absence).
    Engine forgets slots after successful convergence and before retirement;
    absent/empty ids preserve existing behavior. Retain undelivered reports
    until planner acceptance; include hidden workspaces and multiple-domain
    close bursts, and do not fabricate empty membership on unreadable native
    evidence. Native cleanup routes include `workspace_close_cleanup`; keep
    close-vs-minimize classification at observation/lifetime boundaries.
    Shared contract: `MinimizedSlot` docs
    plus `minimized_slot`/`minimized_count`/`has_live_minimized_slot` accessors
    (`crates/tiler-core/src/session/world.rs`),
    `Session::forget_minimized_slot` / `Engine::forget_minimized_slot`, slot-aware
    `committed_session_is_empty` and the disjoint-reseed guard treating a stored
    slot as known (`crates/tiler-core/src/engine.rs`). Test hooks: portable
    Engine minimize/restore convergence tests run on Linux
    (`crates/tiler-core/tests/minimize_restore.rs`); `cfg(windows)` owner wiring
    (`observe`/`assemble_domain_rows`/`domain_rows` carry) needs Windows CI plus
    user-owned native minimize/restore journeys (tile, minimize, sibling reflow,
    best-effort restore, sole-minimize empty workspace, close-slot cleanup).
    Portable regressions pass for focus change, resize, new window, sibling
    close, swap, other minimize/restore, gaps/resolution, restart, closed-slot
    cleanup and fallback; Windows adapter-boundary regressions now cover those
    seams, including real resize/swap, observed-restored focus and hidden iconic
    refresh/automatic close fanout. Locked native gates: 1,845 passed, 0 failed,
    1 ignored; no native journeys performed. Source-slot lifetime after send
    elsewhere is decided User 2026-10-17, option B (forget on confirmed
    rehome; shared-core implementation approved, not implemented).
    No setting. Approved D27 needs supported activation-request/urgency
    observation and default Mark urgent only (no workspace/focus change).
    User-approved 2026-10-16 Windows 5b: OS-permitted Switch and focus requests
    switch/focus even over fullscreen/game foreground, no urgency-only downgrade;
    default and non-permitted requests unchanged. Implementation pending;
    explicit Alt+Tab/taskbar selection is separate.
    KDE's native-activation exception and no-guard decision are host-specific,
    not Windows evidence; do not change KDE behavior for Windows 5b. Offline
    verification 2026-10-13 found only foreground observation, no dedicated
    Windows activation-request classifier or urgency observer/marker; it does
    not prove implemented urgency-only behavior. D01/D03/D09/D10/D12/D13/D22/
    D25/D26/D27/D28 received verification-only disposition, with exact native
    journeys pending ([record](changes/archive/windows-item22-verification.md)).
    Approved D25 (a) keeps host workspace set, re-observes membership without
    moves, restores focus only where provable, rebuilds layout via re-adoption
    and keeps intentional floats via existing restart store; no setting.
    Approved D26 (b) restores persisted owned gaps/settings and re-observes host
    set/membership/maximize. Intentional-float memory is lost on logout/login
    (reopened apps new identities), a listed 0.1 known limitation; revisit
    cross-login memory after 0.1 with shared restart store. Native dogfood/live
    checks will assess both with item 8; membership/set/focus acceptance remains
    pending, no saved-layout restore or new cross-login identity recovery.
    D28 (b) selects KDE fullscreen carry for 0.1, matching item 9 Windows
    carry; preserve existing Windows delivery evidence and pending native/game
    legs. No new Windows carry behavior is selected by this KDE decision.
    Post-0.1 schema/UI/live wiring follows P2 `0.1 triage settings follow-up`:
    approved D09/D10/D27 two-option settings, including D27's default Mark urgent
    only / Switch and focus. See [current settings scope](decisions.md#settings-tray-and-first-run).
    Windows adapter evidence is recorded above; verification-only native
    outcomes remain pending, with no new acceptance claimed from source review.

  - Item 23: born-maximized shared policy delivered offline 2026-10-13
    (User option C). Windows `settings.rs`, `tiling.rs` and Settings radio
    reads consume `tiler_core::born_maximized::BornMaximized` parsing/default
    and clear/keep choice. Exact `parse_wire` validation preserves strict
    invalid-file refusal/last-good adoption; schema-v1 `core.born_maximized`,
    labels/tooltips, lifetime-bound subsequent-only adoption, native gates,
    one-shot attempts and independent born/retile latches remain intact.
    Hydrated fixed-origin overrides preserve their tentative reserved-slot
    maximize route under both values, including hint loss. Windows consumes
    all eight `test-fixtures/born-maximized.json` rows in settings/admission
    tests, with strict saved-file boundary coverage. Native MSVC build/test,
    fmt, strict clippy and diff checks pass (1,847 passed, 0 failed, 1 ignored).
    Shared core/protocol audit since `2c05ee5` found no additional Windows
    wiring gaps; existing D16/D23 integrations cover the other behavior deltas.
    Existing native launch/settings/restart/game/DPI/multi-output checks
    remain pending ([Windows record](changes/archive/windows-born-maximized-shared-policy.md)).
    KDE keeps its conformance-pinned copy until post-0.1 Push code down
    ([core record](changes/archive/born-maximized-shared-policy.md)).
  - Item 24: underlay default (User 2026-10-13), pending Windows implementation.
    Match the delivered KDE kcfg default `#662a82da` (ARGB: alpha 0x66 = 40%,
    RGB #2a82da), for now. Set `DEFAULT_COLOR_ARGB` in
    `crates/tiler-windows/src/group_underlay.rs` to `(0x66, 0x2a, 0x82, 0xda)`
    and `DEFAULT_UNDERLAY_COLOR_HEX` in `crates/tiler-windows/src/settings.rs`
    to `#662a82da`. Align CLI help in `crates/tiler-windows/src/main.rs`,
    default/parity and raster assertions in the underlay/settings modules and
    `crates/tiler-windows/tests/tiling.rs`, and the default-color trace gate in
    `scripts/windows-group-underlay.ps1`. Preserve explicit saved/CLI colors;
    no migration or alpha/blend changes. Acceptance: native Windows build/tests,
    missing-value/default and explicit-value round trips; user-owned live visual
    check on black, white and grey wallpapers at 100%/150% before 0.1.

  ### Source discrepancies to preserve and report

  | Existing assertion | Current source / implementation gap | Handoff treatment |
  | --- | --- | --- |
  | [R-OUT-01](spec/reference-outcomes/multi-output.md#r-out-01-scrolling-assessment-move-left-onto-an-occupied-output) Ours Windows claimed exhausted-horizontal R4; [R-MOV-05](spec/reference-outcomes/move.md#r-mov-05-backfill-edge-move-with-no-left-neighbor-scrolling) claimed horizontal crossing with Up/Down excluded. | `src/tiling_sys.rs:6099/6105` sets `cross_output_transfer:false` in a single-domain event; no Windows cross actuation. Shared core supports four-direction R4. | Corrected both cells 2026-10-08 at `db31234` to local-only current-code facts. Handoff item 5 is sequenced after the single-output queue (18 before 10/11); no native acceptance claimed. |
  | [R-WS-10](spec/reference-outcomes/workspaces.md#r-ws-10-send-b-away-empty-middle-retained-vs-removed) and [R-WS-16](spec/reference-outcomes/workspaces.md#r-ws-16-previous-after-the-visited-empty-workspace-is-removed) Ours Windows said no removal path; spec REQ-WS-10 repeated it. | `src/workspace.rs:508` removes stable IDs; `src/tiling_sys.rs:9228-9234` retirement cleanup and :10074-10080 selection invoke it. `core/workspace.rs:35` plans eligible invisible-empty removal, protecting the trailing spare and minimum count. | Corrected both cells and REQ-WS-10 2026-10-08 at `db31234`. Exact native scenarios remain TBD; handoff item 1 invalidates actually removed IDs and tests surviving empties separately. |
  | Existing higher-level item-2 summary says Compatible disables arrows, while decision 2.1 identifies stock holders only on KDE and says Windows ownership unknown. | `src/settings.rs:954-957` currently returns None for all Ctrl/Alt chords; no Windows holder evidence for new send/output arms. Verified 2026-10-08 at `db31234`; status outstanding, no new policy selected. | KDE arrows are disabled by KDE Compatible. Windows unknown new arms keep defaults with unknown-ownership text until evidenced policy is recorded; do not manufacture Windows holder claims. |
  | Current settings/rebind helpers assume every implemented action has a canonical default chord. | `src/settings.rs:1044` canonical helper returns None for empty defaults; `build_remap` :1182 skips that action; `validate_bindings` :1405 rejects it. Keep-empty is currently active/effective (:1082); UI `refresh_info` (`src/settings_ui.rs:248`) labels it disabled. Verified 2026-10-08 at `db31234`; status outstanding, plus `settings_ui.rs:286` resize note. | Items 2/5 must wire unbound ACTION targets explicitly, make Keep-empty non-intercepting and display unbound distinctly from Disabled. These are adapter limitations, not a decision reversal. |

## macOS port milestone history (delivered milestones condensed)

*Archived 2026-10-24 from backlog.md `## Open work` during backlog consolidation; retained verbatim as delivery evidence.*

Delivered milestone evidence; ordered next work and pending acceptance stay active in the backlog.

  Earlier milestones below retain their original evidence scope. The kickoff
  superseded the sequencing of 2026-10-10. Documentation review and reference-source pins
  done ([kickoff](changes/archive/macos-port-kickoff.md)); Phase 0 decisions
  recorded 2026-10-14 ([decisions](decisions.md)); probe crate, live-testing
  protocol, helper lifecycle and read-only Space membership delivered; signed
  bundle with stable designated requirement and Accessibility grant (via
  LaunchServices launch) verified; owned-window AX legs live-proven (identity
  join, observers, frame set/restore, focus/raise, stalled-app timeout);
   Carbon registration and tap lifecycle work. SLPS focus target/restore now
   pass (142/114 ms dispatch-through-agreement): the tentative oracle joins
   SLPS front + NSWorkspace + direct per-app AX focus/main + CG ownership;
   systemwide AX is diagnostic-only. Main-thread agreement polling now pumps
   AppKit rather than starving workspace updates.
  Phase 1 [threading/lifecycle skeleton](changes/macos-phase-1-skeleton.md)
  delivered offline: shared Engine owner, minimal configurable-modifier input
  boundary, per-app AX observer/actuation owners, typed handoffs, bounded
  stop/reap, structured logs and failure-isolation fixtures. Native tap install,
  live tiling and durable stranded-window recovery are not enabled by this
  skeleton. [Window-policy/classification slice](changes/macos-window-classification.md)
  implemented offline: additive core predicates and actual Windows-copy
   conformance, typed AX/CG classifier and fake Engine fixtures. User-approved
   option A (2026-10-15) removes restored-fullscreen wire omission and keeps
   current adapter/core behavior; exact reference/native outcomes remain TBD
  ([discriminator](spec/reference-outcomes.md#macos-classification-discriminators)).
  Cross-platform CI is green, including Linux workspace/native-effect gates.
   User-approved 2026-10-15: skeleton details, classification and shared
   `window_policy`; basic tiling dogfooding MVP first, then native Spaces,
   then effects; Authentic takes conflicting Mission Control shortcuts and
   safe native settings with Revert; both DMG and Homebrew; native fullscreen
   initially; Tier 2 unchanged. [Scoped tile development mode](changes/macos-scoped-tile.md)
    supplies native facts/topology/actuation/input; 406 macOS tests, strict
    clippy/fmt and signed bundle verification pass. Scoped explicit-tap tiling
    is live-proven on owned helper/TextEdit windows: admission, tap focus/move,
    float, minimize/close reflow, fullscreen exclusion and normal/kill frame
    preservation/input release. Real panels/sheets remain native untouched
    children; separate AXSheets identity stays TBD. Calculator's live bundle
    is `com.apple.calculator`; its hintless non-resizable window stays clamped
    within a larger tiled slot without a write storm. Scope/fullscreen input
    release is implemented with AX-owner-published facts and callback atomics.
     The foreground preview subsequently removed product scope/lifetime guards
     and selected tap-only input. Physical Carbon delivery is diagnostic-only;
     actual OS timeout-notification evidence remains open (explicit re-arm and
     watchdog exit already pass).
    Hint-source/classification and physical multi-display/game checks remain;
    native Space move/switch/create probes subsequently ran as summarized above
  ([record](changes/macos-phase-0.md)); offline
  [native-Spaces routes and recovery plan](research/macos-port/native-spaces-routes.md)
  now distinguish SIP-on bridged-move claims, blocked compat restoration,
  Mission Control alternatives and read-only hotkey mapping limits. Readiness research
  done (`f095042`): setup/TCC/signing runbook, sourced prior-art survey and
  tentative plan, which recommended public AX/AppKit. User direction
  (2026-10-03) supersedes that default: lower-level, lower-jank yabai-style
  route first, optional higher-level AeroSpace-style alternative later;
  default tier 2 (public plus private APIs, SIP enabled, no Dock injection).
  Plan revised to tier 2 from nine pinned local sources (`df018fb`,
  `05d1d1f`; [record](changes/archive/macos-tier2-plan.md)). Phase 0 inputs
  are adopted: Apple Silicon only, macOS 15 minimum, persistent self-signed
  development identity, configurable Meta with Ctrl default, all Rust,
  per-app AX/input/engine threading and native Spaces first. Core extraction
  (K2/K3) and the shared restart store remain consumer-driven reviews; the
  window-policy slice is implemented offline above. Carry the non-native capability-first
  spikes and one registration plus one interception hotkey path into Phase 0,
  then Phase 1 lifecycle/recovery and hotkey probes on a Mac.
  [API layer/events](research/non-native-tiler-host-interaction.md#d1-api-layer-authority-events-and-reconcile),
  [hotkeys/consent](research/non-native-tiler-host-interaction.md#d2-hotkeys-consent-conflicts-secure-input-taps-focus),
  [candidate lessons](research/non-native-tiler-host-interaction.md#candidate-lessons-for-this-project),
  [evidence](changes/archive/non-native-tiler-host-interaction.md).

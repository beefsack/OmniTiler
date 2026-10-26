# Workspace arrival focus and Windows convergence

## Goal and scope

- Selecting/showing a nonempty workspace restores its remembered focus, or its
  most recently focused eligible member when memory is absent (User 2026-10-26).
- With no member focus history, use the first eligible member in deterministic
  layout child order, User-approved 2026-10-26.
- Diagnose and repair final-focus loss after rapid Windows workspace switches.
- No setting, directional no-focus change, empty-workspace focus change,
  fullscreen/game gate weakening, or adapter-owned focus policy.

## Acceptance

- Shared-core regressions cover remembered, MRU, history-free, removed-memory,
  hidden-arrival and empty destinations across selection/show routes.
- Record the user decision and per-WM differences in the functional matrix and
  minimal no-focus discriminator; retain undecided questions as TBD.
- Reproduce the rapid-switch defect with realistic native-shaped event ordering
  before correction; final intent converges without overriding genuine competing
  foreground or weakening lifetime, safety and at-most-once boundaries.
- Fixture-only native Windows checks cover rapid toggles and real supersession,
  independent foreground/visibility readbacks and verified restoration.

## Approach and bounded units

1. Diagnose the supplied trace and production ordering, without live mutation.
2. Implement and verify minimal shared-core arrival selection and Windows
   convergence corrections, with desired-behavior regressions first.
3. Independently review focus/live boundaries; verify offline and fixture-only
   native behavior, then record accepted evidence and archive this note.

## Decisions and evidence

- User 2026-10-26: per-workspace MRU fallback; no-history first member.
  i3/sway per-workspace order and COSMIC MRU are the behavioral rationale;
  Hyprland cursor/candidate fallback is deliberately not adopted.
- User-approved 2026-10-26: deterministic layout child order breaks no-history ties.
- User 2026-10-26: replace the tentative synchronous queue completion with
  exact-identity in-flight tracking, using the existing foreground WinEvents.
  No blocking, timer or timeout. A same-window user click while that exact
  lifetime remains in-flight is indistinguishable and accepted as our landing.
- Live scope: disposable fixture windows only, owner scoped to the fixture
  executable, exact-owner workspace controls and identity-guarded test input
  only if needed; no ordinary applications, registry, game or secure-desktop
  mutation. End condition is stopped owner, restored windows/settings, fixture
  shutdown and no recovery/control residue. Physical shortcut acceptance stays
  user-owned.

## Accepted offline evidence

- Shared core now resolves ordered, deduplicated remembered/MRU/layout
  candidates, applying existing staging eligibility to each. An initially
  single-candidate implementation stopped at an ineligible top candidate;
  independent review identified this and RED/GREEN regressions now prove
  fall-through past remembered-sticky and first-layout-sticky candidates.
  Held/unreadable/floating domains retain their existing vetoes. This does
  not add floating-workspace or intentional-float focus history.
- Core: 853 library tests plus all integration suites pass; strict all-target
  package clippy and format checks pass. Coverage includes hidden stay-arrival
  with genuinely absent history, MRU versus first order, remembered priority,
  stale/removal pruning, empty destinations and existing global-swap routes.
- Incident: every switch emitted its correct remembered target, but a prior
  accepted asynchronous activation landed during the next switch. Foreground
  bracketing vetoed that switch as `foreground-changed`, with no replacement
  effect. The original trace alone did not establish the foreground cause;
  the confirmed ordering now has a discriminating desired-behavior regression.
- Windows retains only the last accepted, immediately unproven target's full
  HWND/PID/process-creation key plus membership token/tag. Existing foreground
  events flow through the owner drain to the diff in delivery order, before
  the next setter. Exact identity plus fresh lifetime proves our landing;
  another readable foreground clears as supersession. Lifetime end, immediate
  proof, newer accepted setter and generation reset clear or replace state.
  Unreadable identity never proves landing; missing target baseline clears
  conservatively. No retry/replay or new focus authority.
- A landing after the drain but inside the later nonactivating switch can
  precede event delivery. Its exact still-in-flight identity is exempt only
  from that bracket's supersession veto; all remaining focus gates still run.
  No message pump intervenes between owner drain and focus setter. Independent
  review checked that ordering and the existing fullscreen/game, lifetime and
  at-most-once boundaries.
- Removed `WM_NULL`/`SendMessageTimeout` completion, its 50ms budget, completion
  diagnostics and barrier-specific tests. Kept baseline/live foreground identity
  diagnostics; managed membership alone never establishes causal attribution.
- RED/GREEN: rapid prior-activation mid-switch and actual foreground-event
  regressions failed on the old path and pass now. Event-lifecycle regressions
  first exposed an incomplete bracket-only version; actual sink/queue/drain
  plumbing now proves switch-free clearing. Coverage includes foreign and
  transient supersession, recycled lifetime, same-PID new window, new setter,
  stale drained events and target departure. Post-setter
  native identity/tag drift remains ambiguous, never accepted as stale proof.
- Latest gates: `mise exec -- cargo test --locked -p tiler-core -p tiler-windows`
  passes (853 core and 1,025 Windows library tests; one existing ignored Windows
  test; all integration/doc suites). Strict all-target clippy for both packages,
  workspace fmt and `git diff --check` pass.

## Native evidence and outcome (2026-10-26)

- Native fixture-only verification used the current event-driven payload,
  independently joined to source revision, executable hash and exact process
  lifetime in external evidence. Non-input fixture focus requests acquired
  native foreground before each leg; no user application was controlled.
- Four bursts of four alternating occupied-workspace selects: 16 accepted,
  no refusals, 125-375ms between dispatches, ending twice on each destination.
  Each next request followed owner consumption, not foreground proof. Thirteen
  accepted/unproven setters later converged; three already-converged arrivals
  needed no setter. No `foreground-changed` holds. All four final native
  foreground/visibility states matched fresh accepted views and remained stable
  on a later read. Same-tick saturation is not claimed: back-to-back unconsumed
  CLI requests correctly refuse under the existing pending-request gate.
- Separate rapid sequences ending on both destinations captured the exact
  owned native pager rectangle twice per ending. Stable images show the correct
  active cell, occupied inactive cell and empty trailing cell, corroborated by
  fresh accepted workspace state, native foreground and visibility. Pager-visible
  logs alone were not used as rendered-current-state proof.
- A separately owned, out-of-scope fixture competitor acquired foreground
  mid-switch. The production identity trace proved `foreground-changed`, no
  focus dispatch and competitor retention. Timing was opportunistic; the hit
  was confirmed by baseline/live identity and the native readback, not just a
  successful focus request.
- Initial multi-second paced pairs were insufficient rapid-burst evidence;
  the continuous consumption-paced script replaced them. An older settings
  preimage was likewise replaced with immediate per-leg byte copies. Earlier
  game-held staging stopped without takeover. Two stalled first-spawn drivers
  preceded product commands; inspected stepwise execution replaced them.
- Final rapid/pager cleanup matched fresh settings bytes and sampled shell/input
  state; exact owners stopped/restored, fixtures shut down, and no owned process,
  recovery ledger or pending-control residue remained. External evidence retains
  raw artifacts and provenance; no local-system identifiers are stored here.
- Outcome: rapid-switch focus-loss P0 closed. Physical workspace chords,
  ordinary applications, games and multiple-output checks remain user-owned,
  not implied by fixture verification. Refused/unresponsive native activation
  remains unproven and is not retried. Directional/general no-focus questions
  remain undecided. Layout-order tie-breaking is User-approved 2026-10-26.
  Next action for this change: none.

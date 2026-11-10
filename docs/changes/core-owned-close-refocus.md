# Core-owned positive-close refocus

## Status, goal and scope

- Active design for approval, 2026-11-10. Product decisions below are approved;
  numbered proposals P1-P6 are not yet approved. Implementation and native
  acceptance are pending. This note changes no contract or implementation.
- Goal: consistently reflow and refocus after positive close of the focused
  window on KDE, macOS and Windows, without inventing observed foreground or
  fighting newer input. Follows consistency principle 1 and
  [R-CLOSE-01](../spec/reference-outcomes/close.md#r-close-01-backfill-close-the-middle-tile-scrolling).
- Include the established ordinary-float-close-to-tiled-survivor case
  ([R-CLOSE-04](../spec/reference-outcomes/close.md#r-close-04-close-a-focused-ordinary-float-over-tiles),
  [float decision](../decisions/window-state.md#transient-dialogs-and-float-visibility)).
- Non-goals: unrelated host-caused focus-loss repair; new float/sticky/fullscreen
  candidate eligibility; native clear-focus; workspace/Space switching; unhide,
  unminimize or automatic fullscreen exit; retries; migrating other focus
  effects; pre-publishing a next-on-close successor.

## Approved product decisions (User 2026-11-10)

1. Core owns close refocus on all platforms. On positive close of the focused
   window, select the pre-close MRU surviving tiled member in the source
   `(output, workspace)` domain, including ordinary float close, under existing
   automatic-focus eligibility and fullscreen/game fences. Do not broaden
   surviving float, sticky or fullscreen eligibility.
2. If native close fallback already chose core's winner, do nothing. If it chose
   a different survivor, make one bounded correction, never retry. Newer
   observable user focus/input, commands, view changes, unknown foreground and
   failed safety checks supersede the attempt.
3. The close-focus request is retained state, not a one-shot effect. Publish its
   target, request id and distinct positive-close cause in every publication
   until satisfied, superseded, ineligible or refused. Satisfaction means
   observed focus equals target. Each adapter attempts each id at most once,
   including when intermediate publications are skipped. Other effects stay
   unchanged.
4. Use report close -> core -> latest publication -> native focus first. Evaluate
   round-trip latency in live testing. A pre-published successor is only a
   possible later extension, not part of this change.
5. Sole close retains the shown empty workspace with no focused member and keeps
   the focused output on that output. OmniTiler focuses nothing on another output
   or workspace. This includes the
   [R-CLOSE-03 multi-output leg](../spec/reference-outcomes/close.md#r-close-03-close-the-sole-window-on-the-shown-workspace),
   matching COSMIC, Hyprland, i3 and sway. Do not force-clear native foreground
   such as KWin's desktop window or a frontmost macOS app.
6. Add a distinct positive-close request cause, coordinated core/v4 contract,
   protocol codec, world-wire fixture and adapter support. Permit this narrow
   exception to macOS explicit-command-only native focus, retaining native
   lifetime, visibility, session and readback fences. Observed focus stays
   truthful. Adapters may refuse; later observation teaches core the outcome.

- Preserve positive close evidence as the sole trigger: incomplete inventory,
  bare absence and off-current/unreadable Space omission never qualify. Preserve
  hidden/minimized, held-scope, lifetime, output, view and fullscreen/game fences.
  A shown workspace may remain unfocused; no unrelated focus-loss repair.

## Evidence and current failure

- Core source references below are relative to `crates/tiler-core/src/`;
  `world_protocol.rs` is in `crates/tiler-protocol/src/`.

- Current source: `session/ops/lifecycle.rs:1075-1100` already selects tiled MRU
  on focused removal, then deterministic surviving-leaf fallback, with no target
  for an empty domain. `session/world.rs:2093-2139` repairs MRU by appending new
  leaves, but observed tiled focus wins convergence and can overwrite pre-close
  history. Freeze the winner before that ingestion.
- Current source: `engine.rs:1494-1558` deliberately returns `focus-mismatch`
  after focused close with no observed focus; `engine.rs:2731-2805` also limits
  focusless projection. `world_publish.rs:258-270` maps that failure to
  `Hold(Unreadable)`. Survivor frames then disappear. The existing
  `close_focused_holds_until_survivor_focus_recovers` test pins the old hold.
- Prior offline investigation, not rerun for this design: three of four macOS
  `scoped_close_reflow` cases fail (positive close, held-unreadable interval and
  missing survivor frames). See the
  [port blocker](v4-phase3-macos-port.md#blocker-positive-close-with-no-observed-focus).
- Current source: `world_report.rs:297-388` and the strict effect codec in
  `world_protocol.rs:5314-5361` have only explicit-command, admission and cursor
  effect causes. `world_focus.rs:133-183` retains historical feedback for the
  latest emitted one-shot batch, not a persistent activation request.
- Current source: `WorldReportV4.focused_global` is empty when no lifetime is
  observed; it does not distinguish known no-client focus from unreadable or
  foreign foreground. The observation watermark advances for all invalidation,
  not specifically user input (`world_report.rs:1309-1348`). Neither a cursor
  position nor a larger observation number proves user intent. This is a
  contract evidence gap, not permission to guess; see P3 and question Q1.
- Prior source investigation: KWin 6.8.1 at
  `1b798c05c19b1124edb1e16623191fcd2c128fd0` invokes `activateNextWindow` before
  `windowRemoved` (`src/workspace.cpp:979-1009`, `src/activation.cpp:433-485`).
  Its fallback can prefer showing-desktop, under-mouse or transient parent before
  desktop focus-chain MRU, with output restriction by default, then desktop
  window. The plain case matches R-CLOSE-01, not all variants. The older floor
  `e53bb3be` uses global rather than per-output current desktop in this path.
- Prior adapter source investigation: KDE observes `activeWindow` and applies
  effects through its setter; Windows uses `SetForegroundWindow` and readback;
  macOS observation tickets cannot dispatch focus. None of these source facts
  establishes native acceptance of this design.

## Proposed design

### P1. One retained request and stable identity

- Proposed: one optional global `pending_close_focus` in `WorldEngine`, since
  only the positively closed globally focused lifetime can originate a request.
  A close cohort removes all positively closed members before choosing the one
  survivor; it never produces one request per removed window or output.
- Proposed retained fields: opaque `id`, `target` lifetime, source `DomainKey`,
  immutable `issued_revision`, source closed lifetime(s), issuing observation
  watermark, command high-water and shown-view/input safety basis. The last
  fields are cancellation evidence, not authority to mutate native state.
- Proposed id: `close-focus-<issued_revision>`, opaque to consumers; mint once at
  the accepted publication revision, unique within the bound adapter/core
  generation. No command sequence is fabricated and no extra counter is needed.
  Revision exhaustion follows existing fail-closed rules. Duplicate reports
  return cached bytes; restaging a command batch never remints the request.
- Proposed: a fresh handshake/generation retires old requests, notifies no native
  action and cannot replay them. Bind adapter accounting to the full
  `(adapter incarnation, core generation, id)` context. A transport reconnect
  that keeps this context keeps accounting; an adapter that loses accounting
  must establish a fresh handshake before applying. Never persist close intent
  across restart. Do not infer survival from older internal incarnation-reset
  helpers; the public fresh-handshake rule is authoritative.

### P2. Freeze selection before focus convergence; separate geometry from focus

- Proposed insertion point: capture pre-close focus, source home and retained
  MRU before `world_run_v4.rs` calls `converge_logical_pre` or any observation
  convergence (`:198-201`). Compute the existing removal selection against the
  surviving linked tree, excluding the entire positive-close cohort, using
  staged lifetime/eligibility facts without recording post-close focus first.
  Do not reorder history from the host fallback before freezing the target.
- Reuse existing tiled MRU/removal fallback and automatic-focus gates, not
  workspace-return float/sticky memory (`world_tiling_views.rs`) or admission
  activation. Closing an ordinary focused float uses its retained source home
  and tiled MRU; surviving tile geometry is unchanged. No new candidate search
  beyond existing removal selection; an unsafe winner authorizes no request.
- Proposed: remove the close-specific focus-mismatch hold for otherwise readable
  positive-close reconciliation and permit survivor projection with no observed
  focus. Keep all independent topology, constraint, completeness and held-scope
  failures. This must work even when no activation is eligible or permitted;
  emitting a request is not a prerequisite for reflow.
- Distinguish the selected logical successor from actual foreground. Never put
  the target into the report's `focused`, a fabricated AX/WinEvent/KWin fact or
  foreground-dependent border/presentation. Preserve the frozen target while
  ingesting truthful host fallback; later observations remain truthful even
  when they disagree. Record accepted logical focus through existing memory
  rules without treating native activation as proved.
- No survivor: clear local logical member focus, keep the shown empty view and
  source focused-output intent, publish `closeFocus:null`, and issue no native
  focus or clear-focus call. Native desktop/app foreground can remain; do not
  falsify observations or claim control over the host's own fallback.

### P3. Raw safety evidence and request lifecycle

- Proposed evidence extension, requiring approval and producer feasibility
  proof: distinguish report foreground as `bound`, `no-client` or `unknown`,
  consistently with existing `focused`; add a close-bound raw input/focus guard.
  Illustrative report keys are `foregroundState` and nullable `closeFocusGuard`
  with source closed lifetime, close observation stamp and
  `interveningInput:none/present/unknown`. `none` must mean a genuinely covered
  native interval since the close boundary, not absence of an event in an
  incomplete capture. A native focus event with unknown cause is not user proof.
  Exact acquisition and wire fields remain open under Q1; these are proposals,
  not claims that existing adapters can produce them.
- A known no-client state tied to the qualifying close may originate the
  request; unreadable/unknown foreground cannot. The close action itself is
  not newer input. Native close fallback to a different eligible survivor may
  be corrected only with a covered no-newer-user-action interval. Its observed
  identity alone is neither a veto nor permission. Adapters also recheck this
  guard immediately before dispatch, including while a reply is in flight.
- Proposed lifecycle (all transitions atomic with accepted core state):

  | Event | Result |
  | --- | --- |
  | Qualifying focused positive close, shown readable source, safe survivor | Freeze target; mint once unless observed foreground already equals target |
  | Observed focus equals target, including native fallback without a tiler call | Satisfied; retire; no native activation |
  | Unrelated observation or skipped native publication, guard still valid | Re-emit identical request id/revision/target in latest snapshot |
  | New admitted/consumed command, including a semantic refusal; newer user input/focus; view/output change | Supersede and retire; explicit input wins |
  | Unknown/foreign foreground or unproved guard; hidden/minimized target, held scope, lost lifetime/home, game/fullscreen veto | Ineligible/superseded; retire; no later restoration retry |
  | Correlated refusal or safety no-call in later observation | Retire, never remint for the same close |
  | Attempt remains unproven or feedback is temporarily missing | Retain identical request; spent adapter never calls again; late satisfaction/refusal can retire it |
  | Target positively closes | Retire old request; a genuinely new qualifying focused close may originate a new request, never a precomputed chain |

- Proposed: unrelated geometry/observation revisions do not renew activation.
  Re-emission alone never marks a request attempted in core. A different host
  fallback must not silently supersede it as though user-selected; conversely
  no generic observation-sequence heuristic may override a real user choice.
- No timer, deadline retry, successor substitution or repair after retirement.
  A request awaiting feedback is bounded to one global row, not a retry queue.

### P4. Wire lane, feedback and versioning

- Proposed reply field, always present and nullable; counters retain the
  contract's quoted-decimal encoding:

  ```json
  "closeFocus": {
    "id": "close-focus-42", "target": "C", "rev": "42",
    "cause": "positive-close", "out": "L", "ws": "W1"
  }
  ```

- Proposed: closed `CloseFocusCause::PositiveClose` vocabulary on a separate
  retained-state type, not a new `OneShotEffect` or fake explicit/admission cause.
  No `cmdSeq`, transfer, reveal, raise/stack or `awaitShown` grant. Source domain
  fields freeze scope and must match the target's current core home.
- Proposed: reuse existing report `focusFeedback` rows, matched against the
  separate retained request by exact opaque `(id,target,rev)` equality, never id
  prefix parsing. Preserve `setter/reason/pre/post/foregroundProven/arrival/
  attemptMs`; spend before call or safety no-call, retain original attempt stamp,
  latch the first correlated feedback once. Do not add adapter verdict ingress.
  Unknown/malformed/mismatched feedback cannot grant or renew focus. Observed
  focus equality alone still satisfies the request; historical feedback never
  overrides current observed foreground.
- Existing `world_focus.rs` classifies proven, refused and unproven; proposed
  reuse derives refusal from raw facts and safety no-call reasons. Mere
  disagreement after accepted dispatch is not proved refusal; retain as
  unproven without retry. Late evidence for a retired id grants no new work.
- Proposed rollout: amend the still-pre-release v4 contract in place, coordinated
  with all codecs/producers/consumers and fixtures, rather than call this v5.
  This is a strict breaking shape change, not backward-compatible optional data:
  `check_keys_with_optional` rejects unknown keys. Old readers cannot consume new
  replies. Require one matching build/fixture set, no dual-shape fallback and no
  live acceptance until matching components pass conformance. If independently
  deployed v4 compatibility is required, use an explicit version bump instead;
  approval is Q2. Existing migration policy permits temporarily broken ports,
  not silently running mismatched versions.
- Implementation must update required-key checks, cross-field validation,
  digests/cache equality, exact and upper-bound sizing, empty/bootstrap replies,
  bounded raw evidence and negative fixtures. Bound the outbound lane to zero or
  one request; keep immutable historical recordings unchanged and add/amend only
  the designated v4 conformance examples.

### P5. Latest-only adapter execution and accounting

- Proposed common sequence: validate bound latest reply, resolve current target
  lifetime/home and shown native workspace/Space, apply permitted nonactivating
  survivor geometry, then re-read foreground/input/session/visibility and all
  existing automatic-focus safety fences. No dependency on other one-shot
  effects or cross-output visibility barriers is created.
- Already target: no setter; report truthful observation/equality evidence.
  Otherwise, if id is unspent and the close guard still permits it, mark spent
  before one bounded native attempt. Safety refusal/no-call also spends the id.
  Report raw feedback through the next normal capture; losing feedback does
  not permit another call. Superseded ids cannot execute from an older outbox.
- Proposed bounded accounting: one current close-request record plus a monotonic
  retired/spent issuing-revision fence within the bound connection context;
  reject older publications and ids. Retiring/null publication cannot make an
  older cached request executable again. Native callback completion is bound to
  the same id and exact native lifetime, never a reused handle. Do not grow an
  unbounded spent-id set. Proof of this compaction is a required burst/reconnect
  test, not an existing implementation claim.
- KDE: do not suppress or reconfigure KWin's earlier fallback. After core
  roundtrip, re-read `activeWindow`; same winner means zero assignments, covered
  different-survivor fallback permits one assignment. A later activation must
  not start a tug-of-war. Keep output/current-desktop and activity/visibility,
  transient/grab, fullscreen/game and lifetime checks. The activeWindow setter
  can raise or switch desktop, so verify the target is already on the shown
  source domain; never use that setter to realize an unshown target.
- Windows: reuse foreground acquisition/readback and `SetForegroundWindow`
  mechanics, including OS refusal, elevated/lifetime/session and existing
  in-flight landing fences. No force-foreground workaround or synthetic input.
- macOS: proposed separate typed close-request dispatch from observation tickets
  into the existing native focus outbox/owner route; not a fabricated command
  ticket. Reuse AX lifetime/process, current native Space, visible/non-minimized,
  session, watermark, game/fullscreen and fresh readback fences. The prior used
  for live comparison is the captured native fallback, not the now-closed
  lifetime. Outbox saturation, cancellation or ownership loss is no-call/spent,
  never deferred focus after a user switch. Admission/hover intents remain
  non-actuating under the existing macOS policy.

### P6. Bounded implementation order

1. Resolve Q1/Q2; approve raw evidence and wire/fixture contract first. Demonstrate
   real producer acquisition or explicit unknown/refusal for each platform; no
   new interception mechanism is implicitly selected by this design.
2. Core retained request, pre-convergence selection and focusless close reflow,
   with focused core/codec tests and strict sizing/replay coverage.
3. Coordinated KDE/macOS/Windows consumer and producer support with genuine
   producer -> codec -> core -> latest-reply -> mocked actuator journeys.
4. Full affected offline/conformance gates; independent review of native
   actuation and public-contract changes. Then separately authorized fixture-only
   native legs under each platform live guide, followed by user physical/latency
   acceptance. Do not turn source inspection or mock setters into native proof.

## Acceptance and required discriminators

- [Shared close discriminators](../spec/reference-outcomes.md#core-owned-positive-close-refocus-discriminators)
  carry unresolved outcomes as TBD. Existing R-CLOSE rows retain their historical
  evidence distinctions; approved intent is not retrospective native evidence.

| Required sequence | Required assertion / evidence |
| --- | --- |
| `H[A,B,C]`, focus A,C,B, positively close B | Freeze C before observed fallback changes history; survivor order preserved, proportional reflow, latest request C; genuine producer plus core regression |
| Host already C; reset and host instead A | First case zero native focus calls; second exactly one correction when safely attributed; no repeated call after mismatch |
| User selects A after close, including before first reply and during apply | User wins, no stale activation; independently ordered native input/focus evidence, not cursor/obs-number inference |
| Burst of observations skips issuing publication; then command burst | Same id survives unrelated skipped revisions; command supersedes it; at-most-once across cached replies, outbox duplicates and transport reconnect |
| Focused ordinary float closes over A/C tiles | Tiled MRU selected, tile frames unchanged; no float/sticky/fullscreen candidate expansion |
| Survivors hidden/minimized or scope held; restore later | No activation/unhide/unminimize or retry; independently readable survivor geometry may reflow, held scope stays fenced |
| Close on non-current workspace/native Space or on another output | Background reflow only where readable; no activation, native view switch or cross-output fallback |
| Fullscreen/game foreground, target or obstructing sibling | Existing automatic-focus vetoes unchanged; no fullscreen exit or new game identification |
| Incomplete inventory, bare absence, off-Space omission, duplicate close | No invented close/request or id renewal; positive close remains distinct from absence |
| R-CLOSE-03 sole close, single and L/R multi-output legs | Shown empty workspace and focused-output intent retained, no member request, no other-domain focus and no native clear call; truthful host foreground retained |
| Refused setter, safety no-call, accepted/unproven and lost feedback | Correct terminal/retained accounting, at-most-once, no replay or remint; late target observation satisfies without setter proof requirement |
| Generation/adapter replacement, reused native id, late feedback | Old request/context cannot actuate or settle a fresh lifetime |
| macOS `scoped_close_reflow` four cases and old core hold regression | Replace old hold expectation with survivor frames plus distinct pending focus; preserve held-Space safety and truthful observed focus; rerun all four, not only previously failing cases |

## Decision and contract amendments at implementation completion

- Amend in place, not now: `decisions/focus.md` automatic guards and active-window
  memory for the bounded close request; `decisions/workspaces.md` lifecycle focus
  and sole-close focused-output preservation, explicitly retaining the ban on
  unrelated host-loss repair; `decisions/window-state.md` ordinary float close.
- Amend `decisions/macos.md` complete-world mapping with only the approved typed
  close exception and unchanged admission/observed-focus safety. Amend
  `decisions/commands.md` latest-only rule to distinguish retained close state
  from dropped one-shots; amend `decisions/contract.md` with approved wire rollout.
- Update `porting/world-contract.md`, protocol fixtures and adapter port notes
  together. Resolve the macOS port blocker record only after its regression and
  native evidence are established; do not equate this design with slice-1 closure.
- Keep this note active through design approval and implementation; archive only
  after accepted implementation evidence and durable decision amendments.

## Approval questions and risks

- **Q1: raw foreground/input attribution (P3).** Existing `focused:""` and generic
  `obs` cannot satisfy both correct-the-host and preserve-newer-user-focus rules.
  Recommended: approve explicit known-no-client/unknown distinction plus causal
  close-bound guard facts; unknown guard refuses rather than guessing. First
  prove native acquisition on each adapter, then finalize exact field shape.
  Alternative: treat any different/empty foreground as supersession using current
  data. That is simpler but abandons approved correction/focusless-close cases
  and requires a new product decision. No such relaxation is adopted here.
- **Q2: version boundary (P4).** Recommended: coordinated breaking amendment to
  pre-release v4, following current migration policy. Alternative: explicit next
  contract version if old-v4 interoperability/deployment requires discrimination.
  Neither old-reader tolerance nor a compatibility adapter is proposed.
- All P1-P6 details require design approval; Q1/Q2 are the material unresolved
  choices. Native feasibility of causal guard evidence is unproved. If it needs
  new hooks/interception or cannot be proven on a platform, return the evidence
  and choices before expanding scope; do not silently disable consistency.
- Risks: a host fallback can precede lifecycle delivery (especially KWin), user
  focus can race the roundtrip, and host setters can raise/switch unexpectedly.
  Conservative fences may suppress valid corrections. Lost feedback can leave
  one inert pending row; it must never become a retry. Native restart/lifetime
  reuse can defeat incorrect spent accounting. Roundtrip latency/visible double
  focus remains unmeasured and belongs to later live/user acceptance.
- Design verification: `git diff --check` passed; changed relative links and
  heading targets were checked. Independent records review found no actionable
  findings. No implementation, test execution or live acceptance is claimed;
  Q1/Q2 and all P1-P6 proposals remain open for approval.

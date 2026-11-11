# Core-owned positive-close refocus

## Status, goal and scope

- Active implementation. Product decisions Q1/Q2 and bounded choices P1-P6
  are approved. Core, protocol/codecs, designated v4 fixtures and macOS support
  are implemented; post-correction offline verification and independent review
  are accepted. Native acceptance remains pending.
  KDE and Windows adapter adoption remain separate backlog work.
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
   observable user focus/input under Q1's report-order attribution, commands,
   view changes, unknown foreground and failed safety checks supersede the attempt.
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
7. **Q1 decided: same-report attribution.** Attribute focus observed in the same
   ingress report as the qualifying positive close to native close fallback.
   Target equality satisfies without activation; a different eligible same-domain
   survivor permits one bounded correction. In a later report, any observed-focus
   change relative to the close report supersedes the request, except target
   equality satisfies it. Commands/view changes still supersede. Accepted cost:
   a user click between close and its first report may be overridden once.
   A->B->A flips entirely between captures remain invisible, as today. No causal
   input guard or new input interception is part of this design.
8. **Q2 decided: coordinated pre-release v4 amendment in place**, no version bump
   and no dual-shape fallback. Update all matching components and fixtures.

- This delivers existing core policy, not a new adapter-owned or host-owned
  selection policy: [engine lifecycle](../decisions/engine.md#cosmic-v1-lifecycle-semantics)
  (`engine.md:117`) selects source-domain tiled MRU on focused removal and clears
  focus when none remains;
  [ordinary float close](../decisions/window-state.md#transient-dialogs-and-float-visibility)
  (`window-state.md:80`) refocuses MRU without changing tiles;
  [workspace lifecycle](../decisions/workspaces.md#workspace-lifecycle-focus-and-history)
  (`workspaces.md:68`) retains shown empty workspaces unfocused. Core-policy
  ownership in approvals 1/3 restates this baseline. A different host fallback
  during positive close is close-caused, not unrelated "host-caused focus loss";
  the bounded correction does not conflict with that decision's no-repair rule.

- Preserve positive close evidence as the sole trigger: incomplete inventory,
  bare absence and off-current/unreadable Space omission never qualify. Preserve
  hidden/minimized, held-scope, lifetime, output, view and fullscreen/game fences.
  A shown workspace may remain unfocused; no unrelated focus-loss repair.

## Baseline evidence and original failure

- Core source references below are relative to `crates/tiler-core/src/`;
  `world_protocol.rs` is in `crates/tiler-protocol/src/`.

- Pre-change source: `session/ops/lifecycle.rs:1075-1100` already selects tiled MRU
  on focused removal, then deterministic surviving-leaf fallback, with no target
  for an empty domain. `session/world.rs:2093-2139` repairs MRU by appending new
  leaves, but observed tiled focus wins convergence and can overwrite pre-close
  history. Freeze the winner before that ingestion.
- Pre-change source: `engine.rs:1494-1558` deliberately returns `focus-mismatch`
  after focused close with no observed focus; `engine.rs:2731-2805` also limits
  focusless projection. `world_publish.rs:258-270` maps that failure to
  `Hold(Unreadable)`. Survivor frames then disappear. The existing
  `close_focused_holds_until_survivor_focus_recovers` test pins the old hold.
- Prior offline investigation, not rerun for this design: three of four macOS
  `scoped_close_reflow` cases fail (positive close, held-unreadable interval and
  missing survivor frames). See the
  [port blocker](v4-phase3-macos-port.md#blocker-positive-close-with-no-observed-focus).
- Pre-change source: `world_report.rs:297-388` and the strict effect codec in
  `world_protocol.rs:5314-5361` have only explicit-command, admission and cursor
  effect causes. `world_focus.rs:133-183` retains historical feedback for the
  latest emitted one-shot batch, not a persistent activation request.
- Pre-change source: `WorldReportV4.focused_global` is empty when no lifetime is
  observed; it does not distinguish known no-client focus from unreadable or
  foreign foreground. The observation watermark advances for all invalidation,
  not specifically user input (`world_report.rs:1309-1348`). Neither a cursor
  position nor a larger observation number proves user intent. Q1 deliberately
  chooses report-order attribution instead of requiring causal input evidence.
  Known-no-client versus unknown still needs the minimal distinction in P3.
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

## Approved implementation design

### P1. One retained request and stable identity

- One optional global `pending_close_focus` in `WorldEngine`, since
  only the positively closed globally focused lifetime can originate a request.
  A close cohort removes all positively closed members before choosing the one
  survivor; it never produces one request per removed window or output.
- Retained fields: opaque `id`, `target` lifetime, source `DomainKey`,
  immutable `issued_revision`, source closed lifetime(s), close-report observation
  watermark and observed-foreground baseline, command high-water and shown-view
  basis. The baseline is the close report's truthful focus, not the logical
  winner; no input counter or causal guard is retained.
- Request id: `close-focus-<issued_revision>`, opaque to consumers; mint once at
  the accepted publication revision, unique within the bound adapter/core
  generation. No command sequence is fabricated and no extra counter is needed.
  Revision exhaustion follows existing fail-closed rules. Duplicate reports
  return cached bytes; restaging a command batch never remints the request.
- A fresh handshake/generation retires old requests, notifies no native
  action and cannot replay them. Bind adapter accounting to the full
  `(adapter incarnation, core generation, id)` context. A transport reconnect
  that keeps this context keeps accounting; an adapter that loses accounting
  must establish a fresh handshake before applying. Never persist close intent
  across restart. Do not infer survival from older internal incarnation-reset
  helpers; the public fresh-handshake rule is authoritative.

### P2. Freeze selection before focus convergence; separate geometry from focus

- Freeze insertion point: capture pre-close focus, source home and retained
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
- Remove the close-specific focus-mismatch hold for otherwise readable
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

### P3. Decided report-order attribution and minimal foreground bit

- Source-read findings: none of the current focus strings preserves the needed
  empty-known versus empty-unknown distinction. Existing native reads can supply
  it with cause preservation; it is not already an ingress field:

  | Adapter | Pre-change producer and native evidence | Required preservation |
  | --- | --- | --- |
  | KDE | `kwin/src/world-observe.ts:326-333,837-866` collapses literal null activeWindow, getter failure/undefined, invalid id and scope-foreign focus to `activeId=""`; `:1796` publishes that string, and `kwin/src/world-report.ts:2889` encodes it. Inventory completeness does not prove this separate read succeeded | Preserve the outcome of the existing activeWindow read: literal null can prove no-client; throw/undefined, unbound desktop/native object or scope-foreign focus stays unknown. No additional native read or input hook |
  | Windows | `crates/tiler-windows/src/tiling_sys/world_capture.rs:1686-1690` joins caller-read foreground HWND to observed lifetimes, otherwise emits empty. Zero and nonzero unmatched HWNDs therefore collapse. Existing native guard at `world_native.rs:2417-2438` distinguishes zero (`None`), failed identity (`Unknown`) and exact native identity (`Known`) | Preserve zero versus nonzero/unbound from the existing foreground read; keep nonzero foreign/desktop/unbound or failed identity unknown, never infer none from no managed match. Reuse existing identity reads. This is capture/port work, not a claim of an existing Windows v4 producer |
  | macOS | `crates/tiler-macos/src/native_tile_backend.rs:1289-1329` reads app-local focus and production selected-oracle evidence, then `.ok()` discards failure causes. `ffi.rs:5944-5955` already reads focused/main AX statuses; `:5991-5996` returns an error for no selected window. `runtime_native.rs:412-438` collapses failed agreement/current-domain checks to no retained focus; `world_runtime.rs:796-800` emits empty | Preserve the already-read AX statuses: only genuine NoValue on both focused and main legs, with coherent frontmost/process evidence, can prove windowless; unsupported, unreadable, stale, foreign or missing membership stays unknown. Main-window fallback is not no-client. Extend existing frontmost revalidation (`ffi.rs:6013-6027`) to the windowless branch before publishing it; no extra AX query or diagnostic system-wide route |

- Minimal wire addition: required report boolean `foregroundKnown`,
  used with the existing `focused` string. `true` plus nonempty means proven bound
  foreground; `true` plus empty means positively known no-client; `false` requires
  empty and means unknown/unreadable/unbound/foreign. Reject inconsistent pairs.
  Do not add an origin enum, native identity payload, input history or close guard.
  No-client must come from the fresh native read, not retained focus absence.
   The required field is implemented in core/codecs/macOS, not native acceptance.
- **Decided Q1 algorithm:** freeze the winner and retain the close report's
  observed-foreground baseline. In each accepted later ingress report, check
  target equality first (satisfied), then unknown foreground (retire), then a
  change from that baseline (superseded). Otherwise keep the same request.
  Baseline equality is not an assertion that no user action occurred; it is the
  user-approved attribution rule. A known no-client baseline can originate a
  request. A foreign/unknown or ineligible fallback cannot authorize correction.
- Latest-only native apply stays intact: core processes ingress reports even if
  their publications are skipped, so it can retire on a later observed change
  before the adapter ever applies the request. The adapter applies only the
  latest publication, never intervening publications, and spends each id once.
  A successful adapter attempt becomes target equality in a later report and
  is recognized as satisfaction, not misclassified as user supersession. A
  failed/unproven attempt with unchanged baseline stays inert, without retry,
  until superseded or satisfied; a conclusive refusal may retire the request
  under the existing terminal rule but never renews the spent id.
- Retained lifecycle bookkeeping (attribution and latest-only rules above are
  decided; transitions remain atomic with accepted core state):

  | Event | Result |
  | --- | --- |
  | Qualifying focused positive close, shown readable source, safe survivor | Freeze target; mint once unless observed foreground already equals target |
  | Observed focus equals target, including native fallback without a tiler call | Satisfied; retire; no native activation |
  | Later report has unchanged known foreground; native publication skipped | Re-emit identical request id/revision/target in latest snapshot |
  | Later report changes observed focus from close-report baseline, not to target; new admitted/consumed command, including a semantic refusal; view/output change | Supersede and retire |
  | Unknown/foreign foreground; hidden/minimized target, held scope, lost lifetime/home, game/fullscreen veto | Ineligible/superseded; retire; no later restoration retry |
  | Correlated refusal or safety no-call in later observation | Retire, never remint for the same close |
  | Attempt remains unproven or feedback is temporarily missing | Retain identical request; spent adapter never calls again; late satisfaction/refusal can retire it |
  | Target positively closes | Retire old request; a genuinely new qualifying focused close may originate a new request, never a precomputed chain |

- Unrelated geometry/observation revisions do not renew activation. Re-emission
  alone never marks a request attempted in core. The same-report fallback is
  not supersession. A later sampled focus change is attributed to user intent
  even if the host caused it; A->B->A between captures is invisible. A click in
  the close-to-report gap may be corrected once, the explicitly accepted cost.
- No timer, deadline retry, successor substitution or repair after retirement.
  A request awaiting feedback is bounded to one global row, not a retry queue.

### P4. Wire lane, feedback and versioning

- Reply field, always present and nullable; counters retain the
  contract's quoted-decimal encoding:

  ```json
  "closeFocus": {
    "id": "close-focus-42", "target": "C", "rev": "42",
    "cause": "positive-close", "out": "L", "ws": "W1"
  }
  ```

- Closed `CloseFocusCause::PositiveClose` vocabulary on a separate
  retained-state type, not a new `OneShotEffect` or fake explicit/admission cause.
  No `cmdSeq`, transfer, reveal, raise/stack or `awaitShown` grant. Source domain
  fields freeze scope and must match the target's current core home.
- Reuse existing report `focusFeedback` rows, matched against the
  separate retained request by exact opaque `(id,target,rev)` equality, never id
  prefix parsing. Preserve `setter/reason/pre/post/foregroundProven/arrival/
  attemptMs`; spend before call or safety no-call, retain original attempt stamp,
  latch the first correlated feedback once. Do not add adapter verdict ingress.
  Unknown/malformed/mismatched feedback cannot grant or renew focus. Observed
  focus equality alone still satisfies the request; historical feedback never
  overrides current observed foreground.
- Existing `world_focus.rs` classifies proven, refused and unproven; shared
  reuse derives refusal from raw facts and safety no-call reasons. Mere
  disagreement after accepted dispatch is not proved refusal; retain as
  unproven without retry. Late evidence for a retired id grants no new work.
- Decided Q2 rollout: amend the still-pre-release v4 contract in place, coordinated
  with all codecs/producers/consumers and fixtures; no version bump.
  This is a strict breaking shape change, not backward-compatible optional data:
  `check_keys_with_optional` rejects unknown keys. Old readers cannot consume new
   replies. Require one matching build/fixture set, no dual-shape fallback and no
   live acceptance until matching components pass conformance. Existing migration
   policy permits temporarily broken ports, not silently running mismatched
   versions.
- Implementation must update required-key checks, cross-field validation,
  digests/cache equality, exact and upper-bound sizing, empty/bootstrap replies,
  bounded raw evidence and negative fixtures. Bound the outbound lane to zero or
  one request; keep immutable historical recordings unchanged and add/amend only
  the designated v4 conformance examples.

### P5. Latest-only adapter execution and accounting

- Common sequence: validate bound latest reply, resolve current target
  lifetime/home and shown native workspace/Space, apply permitted nonactivating
  survivor geometry, then re-read foreground/session/visibility and all
  existing automatic-focus safety fences. No dependency on other one-shot
  effects or cross-output visibility barriers is created.
- Already target: no setter; report truthful observation/equality evidence.
  Otherwise, if id is unspent and existing native safety/freshness permits it,
  mark spent before one bounded native attempt. Safety refusal/no-call also spends the id.
  Report raw feedback through the next normal capture; losing feedback does
  not permit another call. Superseded ids cannot execute from an older outbox.
- Reject stale/superseded publication envelopes before selecting an
  attempt; delivery churn alone does not spend an unchanged re-emitted request.
  Once selected for native application, a safety no-call spends it. Existing
  native invalidation and foreground rechecks still veto stale writes; they do
  not introduce causal input evidence or require applying skipped publications.
- Bounded accounting: one current close-request record plus a monotonic
  retired/spent issuing-revision fence within the bound connection context;
  reject older publications and ids. Retiring/null publication cannot make an
  older cached request executable again. Native callback completion is bound to
  the same id and exact native lifetime, never a reused handle. Do not grow an
  unbounded spent-id set. Proof of this compaction is a required burst/reconnect
  test, distinct from native acceptance.
- KDE: do not suppress or reconfigure KWin's earlier fallback. After core
  roundtrip, re-read `activeWindow`; same winner means zero assignments, the
  latest valid request permits one assignment under existing safety fences.
  Attribution to same-report fallback versus later change stays in core. A later
  activation must not start a tug-of-war. Keep output/current-desktop and activity/visibility,
  transient/grab, fullscreen/game and lifetime checks. The activeWindow setter
  can raise or switch desktop, so verify the target is already on the shown
  source domain; never use that setter to realize an unshown target.
- Windows: reuse foreground acquisition/readback and `SetForegroundWindow`
  mechanics, including OS refusal, elevated/lifetime/session and existing
  in-flight landing fences. No force-foreground workaround or synthetic input.
- macOS: separate typed close-request dispatch from observation tickets
  into the existing native focus outbox/owner route; not a fabricated command
  ticket. Reuse AX lifetime/process, current native Space, visible/non-minimized,
  session, watermark, game/fullscreen and fresh readback fences. The prior used
  for live comparison is the captured native fallback, not the now-closed
  lifetime. Outbox saturation, cancellation or ownership loss is no-call/spent,
  never deferred focus after a user switch. Admission/hover intents remain
  non-actuating under the existing macOS policy.

### P6. Bounded implementation order

1. Apply approved Q1/Q2 and wire/bookkeeping choices.
   Implement the minimal known-foreground bit by preserving existing read
   outcomes on each platform; verify genuine windowless versus unknown cases.
   No new input interception or causal-input protocol is part of the work.
2. Core retained request, pre-convergence selection and focusless close reflow,
   with focused core/codec tests and strict sizing/replay coverage.
3. Coordinated KDE/macOS/Windows consumer and producer support with genuine
   producer -> codec -> core -> latest-reply -> mocked actuator journeys.
4. Full affected offline/conformance gates; independent review of native
   actuation and public-contract changes. macOS native acceptance is the user-run
   physical procedure below, replacing the automated close-focus harness route
   (User 2026-11-11). KDE/Windows native acceptance remains separate under their
   live guides. Do not turn source inspection or mock setters into native proof.

## Acceptance and required discriminators

- [Shared close discriminators](../spec/reference-outcomes.md#core-owned-positive-close-refocus-discriminators)
  carry unresolved outcomes as TBD. Existing R-CLOSE rows retain their historical
  evidence distinctions; approved intent is not retrospective native evidence.

| Required sequence | Required assertion / evidence |
| --- | --- |
| `H[A,B,C]`, focus A,C,B, positively close B | Freeze C before observed fallback changes history; survivor order preserved, proportional reflow, latest request C; genuine producer plus core regression |
| Host already C; reset and close-report focus instead A | First case zero native focus calls; second exactly one correction under same-report attribution; no repeated call after mismatch |
| Close-report baseline A; compare later report focus D versus target C in separate runs; repeat while first publication is skipped | D supersedes before native apply; C satisfies first. Core sees ingress reports even when adapters skip publications; later equality cannot resurrect a retired request |
| User clicks A between close and close report; repeat A->D->A entirely between captures | Accepted cost: first may be overridden once; second is invisible. No causal input guard/hook; native freshness still vetoes stale apply |
| Burst of observations skips issuing publication; then command burst | Same id survives unrelated skipped revisions; command supersedes it; at-most-once across cached replies, outbox duplicates and transport reconnect |
| Focused ordinary float closes over A/C tiles | Tiled MRU selected, tile frames unchanged; no float/sticky/fullscreen candidate expansion |
| Survivors hidden/minimized or scope held; restore later | No activation/unhide/unminimize or retry; independently readable survivor geometry may reflow, held scope stays fenced |
| Close on non-current workspace/native Space or on another output | Background reflow only where readable; no activation, native view switch or cross-output fallback |
| Fullscreen/game foreground, target or obstructing sibling | Existing automatic-focus vetoes unchanged; no fullscreen exit or new game identification |
| Incomplete inventory, bare absence, off-Space omission, duplicate close | No invented close/request or id renewal; positive close remains distinct from absence |
| R-CLOSE-03 sole close, single and L/R multi-output legs | Shown empty workspace and focused-output intent retained, no member request, no other-domain focus and no native clear call; truthful host foreground retained |
| Refused setter, safety no-call, accepted/unproven and lost feedback | Correct terminal/retained accounting, at-most-once, no replay or remint; late target observation satisfies without setter proof requirement |
| Generation/adapter replacement, reused native id, late feedback | Old request/context cannot actuate or settle a fresh lifetime |
| Native null/windowless versus read failure, unsupported AX, foreign/unbound native window and main-window fallback | Minimal foreground bit is truthful for each producer; unknown never originates focus; no diagnostic system-wide AX route or inferred none from omission |
| macOS `scoped_close_reflow` four cases and old core hold regression | Replace old hold expectation with survivor frames plus distinct pending focus; preserve held-Space safety and truthful observed focus; rerun all four, not only previously failing cases |

## Decision and contract amendments at implementation completion

- Amended in place: `decisions/focus.md` automatic guards and active-window
  memory for the bounded close request; `decisions/workspaces.md` lifecycle focus
  and sole-close focused-output preservation, explicitly retaining the ban on
  unrelated host-loss repair; `decisions/window-state.md` ordinary float close.
- Amended `decisions/macos.md` complete-world mapping with only the approved typed
  close exception and unchanged admission/observed-focus safety. Amend
  `decisions/commands.md` latest-only rule to distinguish retained close state
  from dropped one-shots; `decisions/contract.md` with approved wire rollout and
  `decisions/engine.md` with the pre-convergence selection/reflow boundary.
- Update `porting/world-contract.md`, protocol fixtures and adapter port notes
  together. Resolve the macOS port blocker record only after its regression and
  native evidence are established; do not equate this design with slice-1 closure.
- Keep this note active through design approval and implementation; archive only
  after accepted implementation evidence and durable decision amendments.

## Decided questions and risks

- **Q1 decided, User 2026-11-10:** same-report native-fallback attribution and
  later-report focus-change supersession, with target equality satisfying first.
  Close-to-report click override and invisible between-capture flips are accepted.
  Core ingress attribution preserves latest-only adapter application. No causal
  guard, input history or new interception is required.
- **Q2 decided, User 2026-11-10:** coordinated in-place pre-release v4 amendment,
  no version bump or dual-shape fallback. Old strict readers are not compatible.
- Approved bounded implementation choices, without reopening Q1/Q2:
  - P1: one global retained request, revision-derived id, baseline/command/view
    bookkeeping and connection-scoped lifetime/reset rules.
  - P2: exact pre-convergence freeze point, existing removal fallback reuse,
    focusless survivor projection and logical-memory/native-observation split.
  - P3: one required `foregroundKnown` bit and the per-adapter read-outcome
    preservation above; no new cause/input data. The bit plus existing `focused`
    encodes bound, no-client and unknown foreground.
  - P4: nullable `closeFocus` shape and typed cause, existing exact-tuple feedback
    reuse/latching and sizing/validation changes, under the decided v4 rollout.
  - P5: existing native paths including typed macOS observation dispatch, bounded
    once-per-id current-record/revision-fence accounting and no-call semantics.
  - P6: contract/fixture agreement, coordinated implementation, offline review,
    then native and physical acceptance order.
- Risks: deliberately coarse report-order attribution can override a gap click
  once or cancel on a later host focus change. Unknown/windowless read handling
  must stay conservative, especially macOS unsupported versus NoValue and main
  fallback. Existing native freshness cannot expose flips entirely between
  captures. Lost feedback can leave one inert pending row; it must never become
  a retry. Lifetime reuse/spent-fence compaction and roundtrip latency/visible
   double focus remain implementation and later native acceptance obligations.

## Current implementation evidence

- Core freezes one global source-domain tiled-MRU request before fallback
  convergence, removes the positive-close focusless reflow hold, and preserves
  independent safety holds. Focused core tests include float/cohort/sole-close,
  supersession, unknown foreground, feedback and lifetime cases.
- Codecs require `foregroundKnown` and nullable `closeFocus`, with closed cause,
  digest and byte sizing coverage; active v4 messages/scenarios are amended,
  immutable historical recordings unchanged. See the
  [amended contract](../porting/world-contract.md#retained-positive-close-focus).
- macOS uses bounded exact-context spending and typed existing-owner dispatch,
  fresh native baseline/readback and raw feedback; it never fabricates observed
  focus. Admission/hover remain nonactuating.
- Offline verification: core library 2129 passed; core integration binaries
  pass except the existing `unsafe_guardrail::workspace_unsafe_posture` three
  allowlist violations. Full post-correction protocol suite passes: 295 tests,
  including world-wire, close-focus wire and the v4 transcript. macOS library 1516 passed
  with only the three existing
  trailing-workspace lifecycle failures; all four `scoped_close_reflow` cases
  and all five close-filtered integration cases pass.
- Genuine macOS-capability mock journey skips the issuing close publication
  and two later retained publications, applies only the final codec-roundtripped
  request, counts one native-backend call and satisfies on truthful target
  observation. Accepted-but-unchanged readback plus raw feedback retains the
  identical id; production reapplication queues no command or second setter.
  Other tests cover final-null supersession and stale/context/lifetime fences.
- Independent review verified latest-only actuation and exact-context spending.
  Corrections clear stale cross-process foreground to unknown on fresh
  contradictory frontmost evidence, downgrade unproven known-empty provenance,
  and classify raw feedback directly against its target without a synthetic
  admission effect. Seven added ingress regressions and the genuine owner
  journey pass; follow-up review accepts both corrections. Delayed cross-app
  snapshots can conservatively clear focus to unknown; no new ordering or
  foreground assertion is introduced.
- `git diff --check` passes. Native close-to-refocus timing is pending; no
  native acceptance is claimed.

### Mechanical lint and unsafe-root follow-up (2026-11-11)

- Strict all-target clippy now passes for `tiler-core` and `tiler-protocol` after
  six remaining mechanical fixes, without lint suppressions or behavior changes.
  The two identical incomplete-inventory hold branches are merged with
  short-circuit OR, preserving their priority over all later hold reasons.
- Fresh core library verification passes all 2129 tests; the close-focus and
  dynamic-lifecycle modules also pass separately (29 and 15 tests respectively).
- The live fixture's sole macOS FFI root, `src/menu_macos.rs`, is now explicitly
  approved by the unsafe guardrail. Exact-path matching, required-root checks
  and fail-closed scanning are unchanged; all 31 guardrail self-tests pass.
  The workspace check still fails on exactly the two unchanged Windows fixture
  examples, now tracked under [Windows fixture unsafe-root consolidation](../backlog.md).
  No other macOS fixture source weakens `unsafe_code`.
- An additional full protocol test invocation timed out in `intent_data_fixtures`
  after earlier suites passed; it is not a fresh full-protocol pass. The prior
  complete protocol evidence above remains historical. Native acceptance is
  unchanged and pending.

### Native preparation and acceptance limit

- Matching signed macOS and fixture bundles build offline with the existing
  persistent development identity. Bundle-attributed preflight reports
  Accessibility trusted, posting available, an unlocked session and inactive
  Secure Input. This is permission/preparation evidence, not close-focus proof.
- A bounded native Space lifecycle preparation created one owned empty Space,
  selected it, returned and removed it; source topology and current Space were
  restored. The integrated preparation failed its final protected-window frame
  equality check. A later native sample recovered the exact original frame
  without a window write. Full before/after CG comparison still reports unequal
  offscreen system-service rosters, with no surviving-window geometry changes;
  it is not a clean baseline-comparison result.
- No product, tap or fixture-window integration was launched. The created Space
  and awake helper were removed, Mission Control is closed and original focus
  is observed. Preparation remains FAIL; the held owned-Space route and
  baseline-comparison discrepancy were not resolved. Automated close-focus
  harness work is superseded by the user-run route below, not by a retroactive
  preparation pass. No close-to-refocus timing samples or slice-1 native
  acceptance exist yet.

### User-run macOS native acceptance

1. Use the existing signed product bundle built from `52d3ff22`, not the fixture
   bundle or a directly executed binary. Its recorded executable SHA-256 is
   `28e5bb52f45f2c5355c2f91b0d4280de71684b2d5e9e0da87915e589d384f079`.
   The later preparation record and this lint/documentation update do not change
   product behavior, so this bundle needs no rebuild. From the build-output root
   containing `target/macos/OmniTilerProbe.app`, verify it and make a fresh log
   directory. Stop if verification fails, the hash differs or the directory
   already exists; do not overwrite an earlier run:

   ```sh
   codesign --verify --deep --strict target/macos/OmniTilerProbe.app
   shasum -a 256 target/macos/OmniTilerProbe.app/Contents/MacOS/tiler-macos
   mkdir target/macos/close-refocus-manual
   open -n --stdout target/macos/close-refocus-manual/preflight.json --stderr target/macos/close-refocus-manual/preflight.err target/macos/OmniTilerProbe.app --args preflight
   ```

2. Once preflight has finished, read its output:

   ```sh
   cat target/macos/close-refocus-manual/preflight.json
   cat target/macos/close-refocus-manual/preflight.err
   ```

   Require `ax_trusted:true`, `post_event_access:true`, unlocked session and
   inactive Secure Input; stop on a permission shortfall or unexpected error.
   Save work first. Stop any existing tiler through its own normal stop route;
   do not overlap instances. Open three disposable ordinary TextEdit document
   windows on one ordinary Space, mentally labelled A, B and C. Keep other
   TextEdit documents out of this check: the scope covers the entire app.

3. Launch a bounded, TextEdit-only product run through LaunchServices, using the
   same signed-bundle route as the [launch docs](../live-macos-testing.md#required-preflight):

   ```sh
   open -n --stdout target/macos/close-refocus-manual/tile.jsonl --stderr target/macos/close-refocus-manual/tile.err target/macos/OmniTilerProbe.app --args tile --user-start --scope com.apple.TextEdit --input tap --seconds 120 --log verbose
   ```

   This manages ordinary app windows, not fixtures, for at most 120 seconds plus
   startup/teardown. It does not manage the controlling terminal. Wait for tiling
   to settle before judging close behavior. If the run expires before all legs,
   retain its logs and use a fresh log directory for another bounded run.

4. Tiled close: physically focus A, then C, then B. Close B's window normally
   (not Quit TextEdit); leave the pointer still and make no further input until
   focus settles. Expect C, the pre-close MRU tiled survivor, to receive focus;
   A/C keep their order and reflow. Repeat with a fresh B if useful, always
   re-establishing A,C,B focus order.

5. Float close: with A/C tiled, create a fresh B, focus it and press Ctrl+G to
   float it. Focus A, C, then floating B; close B normally. Expect tiled C to
   receive focus, with A/C tile frames unchanged by closing the float.

6. Sole close: close the other test documents, leaving only one on this Space;
   focus and close that last document. Expect the same workspace to remain shown
   with no focused managed window, no Space switch and no OmniTiler activation
   on another workspace/output. A frontmost windowless app or native host
   fallback is not a request to force-clear native foreground.

7. Optional, only with two displays already available: leave a test document D
   on the other display's shown Space and repeat the sole-close leg on the first.
   Expect the emptied source workspace/output to stay selected, with no tiler
   cross-output focus jump to D. Do not attach/reconfigure displays for this
   check; report native fallback separately if it occurs.

8. For each leg, report actual final focus, whether refocus felt immediate or
   delayed (rough milliseconds only if you can estimate), any wrong-window
   flash/double focus, reflow jump, flicker or other jank, and whether it repeats.
   Include app/leg, source revision, single/two-display leg, and the two tile log
   files with document content/titles redacted. Physical feel is the acceptance
   result; these observations alone do not prove exact setter counts or separate
   a matching native fallback from a tiler correction.

9. Stop by letting the bounded run expire. Inspect `tile.jsonl`/`tile.err` for
   terminal outcome and `shutdown_clean:true`, and confirm interception and the
   active border have gone. `open` returning, or Ctrl+C in its terminal, does not
   stop the app. If urgent or hung, use Activity Monitor to quit only the product
   process matching the `tilestarted` PID in this run's log; report forced/unclean
   teardown rather than success. Close only the disposable test documents after
   the product stops, retaining logs for the report. Window geometry is left in
   place on stop; no automatic restoration is promised.

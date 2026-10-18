# KDE 0.1 triage native verification

## Goal and acceptance

- Verify pending single-output D01-D28 triage journeys against the committed KDE
  adapter, prioritizing independent native observations across the matrix.
- Distinguish registered-shortcut callback/D-Bus evidence from physical input,
  pointer interaction, visual UX and real-game acceptance.
- Preserve existing applications and restore a freshly captured desktop,
  configuration, effects, shortcuts and development-lifecycle baseline.
- No product source changes; admission/capacity observations are report-only.

## Approach and bounded units

1. Pin source/lifecycle ownership, capture exact baseline and establish disposable
   normal resizable Wayland clients without touching existing application windows.
2. Exercise admission/dialog/close, tree movement/resize and floating/workspace
   families independently. Stop a leg at its first refusal or fixture mismatch;
   retain the full gating evidence before restoring and re-baselining.
3. Review native before/after state and request linkage, record pass/fail/not-testable
   per unit and advance only the precise verified backlog legs.
4. Verify teardown/restoration and archive the accepted outcome.

## Scope boundaries and current evidence

- D28 real-game acceptance and all unused physical-key routes remain user-pending.
- Multi-output journeys are excluded. Known asynchronous maximize/fullscreen
  diagnostic refusals, fixed-client controller-restart partial observation and
  two-tiled-domain send-back holds remain parked, not new retry targets.
- Prior native evidence and fixture lessons are retained in
  [KDE live acceptance](kde-live-acceptance.md); new evidence uses a fresh baseline.

## Admission continuation, 2026-10-18

- **Stopped with incomplete restoration. No new live acceptance or backlog
  advancement.** The admission attempt did not preserve the protected application
  layout. Continuing after the runtime-store and geometry surprises invalidated
  its subsequent journey-pass claims. Earlier evidence below is unchanged.
- Source: `52936a0c`; no product source or tests changed.
- The actual runtime float-intent file was backed up before startup this time,
  together with configuration, effects, desktop identities and shortcut records.
  The retained full native inventory and wire capture start after development
  startup, so they do not establish a complete pre-start native/seed transaction.
- Startup replaced the saved two-member float store with empty membership. Its
  preimage was preserved; calling those memberships stale is unsupported. A later
  gap change also resized protected applications. Tests continued rather than
  stopping at the first mismatch; post-restart activity later changed their order.

| Leg | Accepted disposition |
| --- | --- |
| Admission defaults and choices | Source inspection only; not native Settings acceptance. |
| Settings Save/reopen, Cancel, Defaults | Not-testable: staged plugin discovery failed; no successful UI transaction. |
| Policy-only Apply | Direct configuration/reconfigure observations preserved frames in the early inventories; not UI Apply acceptance and not a clean-baseline journey pass. |
| Exhaustion, Largest/Closest and explicit float-toggle retry | Functional observations only, after the baseline failure; no accepted live pass. |
| Workspace switch, no automatic retry and still-full retry | No accepted live pass; saved per-leg native inventories are incomplete. |
| Restart persistence | Captured intent-loss transaction, below; not a pass. |
| Newest-first shrink eviction | Not reached: positive allocations did not exercise eviction. |
| Allow overlap | Functional observation only; no accepted live pass. |
| Two-domain send-back and numbered-selection discriminator | Not attempted in this continuation; existing classifications remain unchanged. |
| Exact cross-output R4, exhausted drag, remaining D units | Not exercised; single-output and pointer limitations remain. |

### Parked restart intent-loss investigation

- The complete restart wire chain reads one persisted float and one tile override.
  Its first accepted observation holds the disposable scope as `unreadable`; the
  next persistence write drops the float while retaining the tile override. The
  next controlled observation writes five floats and no tile overrides, without
  an intervening explicit toggle. This is a persistence-loss signal, not proof
  that cross-restart topology or admission order must be preserved.
- `kwin/src/world-entry.ts` removes float membership when the reply lacks an
  untouched/no-hold row, including held rows. Later capacity-float classification
  can overlap a retained tile override, which persistence filters from the tile
  side. These are concrete producer paths to investigate, not a selected policy
  change or a completed regression/fix.
- Options: reproduce held-first-observation intent loss independently of startup
  packing; or separately specify startup admission/override precedence if the
  replay exposes a genuine policy ambiguity. Recommendation: first add a
  producer-shaped offline replay that seeds float plus tile membership, accepts
  an unreadable scope, then accepts a complete observation with the same live
  identities. Assert no uncommanded loss of settled float intent; distinguish
  legitimate capacity outcomes from fixed-window override loss. Do not require
  persistence of the previous tree.
- No product source changed and no defect fix was committed. Rust workspace,
  KWin and settings suites were not rerun for this evidence-only update. Fresh
  executable and KWin bundle builds succeeded during the attempt.

### Continuation restoration and next gate

- Final read-only comparison: `kwinrc`, project shortcut-clearing configuration
  and the actual runtime float-intent store match their saved bytes. The store
  was restored by exact atomic replacement while the Planner was unowned and
  controller unloaded, then independently compared; its two original memberships
  and private file mode are retained. No development restart was used to restore it.
  Byte equality does not include metadata: `kwinrc` and runtime-store modification
  times differ from the captured originals.
- Desktop identities/current desktop, single-output configuration, loaded effects,
  project shortcut tuples and KGlobalAccel component/action inventories match the
  captured baseline semantically. KWin remained alive; disposable clients and
  owned probes are gone. Planner unowned, controller unloaded, development **DOWN**.
- **Protected application restoration is incomplete:** Firefox and Ghostty have
  exchanged their original left/right frames; their sizes and desktop/output
  membership match the early retained inventory. Ghostty is active instead of
  Firefox. No corrective movement or focus operation was performed on them.
  This run therefore violated protected-window preservation, not merely an
  observational acceptance condition.
- Independent evidence review rejected blanket restoration and journey-pass
  claims. Raw wire/native responses and byte comparisons remain outside the
  repository; no local identities are recorded here. No new tentative product
  decision or replacement-baseline acceptance was made.
- Next exact step: obtain disposition of the protected layout/focus mismatch,
  then prove a stopped-start fixture that leaves existing applications unchanged,
  captures native state and full wire traffic before startup, and isolates gap
  changes from their workspace. Investigate the parked intent-loss replay offline
  before restarting. Only afterward resume admission, two-domain send-back and
  the single captured selection discriminator, in that order.

## Initial outcome, 2026-10-18

- **Blocked before any D-unit journey ran. No backlog acceptance advanced.**
- Source: `94e7844e`; fresh Rust executable and KWin bundle builds passed.
  No product source or tests changed.
- Development started with floating defaults and an owned desktop created while
  stopped. Registered `omnitiler-workspace-3` invocation dispatched a numbered
  selection and returned `selection-unknown`; native current-desktop read-back
  remained unchanged. A trailing selection subsequently applied with zero reported
  view writes, a repeated numbered selection refused again, and a second numbered
  selection also applied with zero reported view writes and unchanged native view.
- Continuing selection attempts after the first refusal violated the planned
  clean-leg boundary. None is accepted journey evidence. A later inventory found
  that the original empty backing desktop had retired and the owned desktop had
  become the second backing desktop. No disposable client was launched.
- Route: KGlobalAccel `invokeShortcut` by registered action name, bypassing xkb;
  no synthetic keyboard or pointer input and no physical-key acceptance.
- Summarized command/dispatch/result journals and native inventories survive,
  but canonical Planner requests/replies were not retained. The attempted journal
  marker capture was empty, so the intended true-positive marker proof was not
  established. Summary counts alone do not diagnose selection or actuation.

## Per-unit disposition

All entries below are dated 2026-10-18. "Not-testable" means this attempt did not
establish the required fixture or input route, not that the product failed.
Previously recorded native/offline evidence is unchanged.

| Unit | Outcome | Exact remaining acceptance |
| --- | --- | --- |
| D01 | Not-testable | Fullscreen admission/overlay-close fixture not reached. |
| D02 | Not-testable | Modeless/transient and modal exclusion, unchanged tiles and focus fences. |
| D03 | Not-testable | Chained after-focused admissions with native focus/pointer proof. |
| D04 | Not-testable | First ordinary window fills empty shown workspace and gains native focus. |
| D05 | Not-testable | Float-focused newcomer splits remembered tiled neighbor's long edge. |
| D06 | Out of scope | Multi-output journey. |
| D07 | Not-testable | Ordinary close/refocus/fresh-reopen order, ratios and focus; not fixed-client D7. |
| D08 | Not-testable | Closing sole tile leaves same empty shown workspace with null focus. |
| D09 | Not-testable | Two-child neighbor middle-index movement fixture. |
| D10 | Not-testable | Orthogonal escape through root wrap. |
| D11 | Not-testable | Flat triple perpendicular move produces `V[H[A,C],B]`. |
| D12 | Not-testable | Same-axis escape inserts into ancestor axis. |
| D13 | Out of scope | Local-wrap/output-crossing multi-output journey. |
| D14 | Not-testable | Outward edge no-op and nearest matching-edge-axis nested resize. |
| D15 | Not-testable | Genuine hover/click-at-press route not established. |
| D16 | Not-testable | No new ordinary/combo send leg; prior bounded send evidence unchanged. |
| D17 | Not-testable | Remembered B after selecting away and back; no owned client fixture reached. |
| D18 | Not-testable | Tall/wide send-back anchor remains pending; known held-scope leg not retried. |
| D19 | Not-testable | Ordinary float hides away and restores frame/remembered focus. |
| D20 | Not-testable | Overlapping floats raise on native focus; no stacking oracle established. |
| D21 | Not-testable | Genuine pointer move/resize plus project-resize refusal. |
| D22 | Not-testable | Float middle 25 share from 50/25/25, survivors approximately 67/33. |
| D23 | Not-testable | No minimize/restore/close native fixture reached. |
| D24 | Not-testable | Focused float close leaves tiled geometry unchanged and focuses MRU B. |
| D25 | Not-testable | Orderly restart with live disposable applications not reached. |
| D26 | User-pending | End-session boundary not exercised. |
| D27 | User-pending | Real-game active/idle native activation acceptance not exercised. |
| D28 | User-pending | Real-game fullscreen carry is still required; no substitute game claim. |

## Blockers and next discriminator

- **Fixture establishment, not a diagnosed product defect:** a native desktop
  existed, but its numbered selection refused. Accepted zero-write selection
  summaries do not distinguish an already-selected logical view, missing logical
  bindings or an unapplied retained transfer. Full topology, mode, bindings,
  mapping, request and reply are missing. An adoption-gap or silent-no-op defect
  claim is therefore unsupported.
- Options: correct fixture construction if its logical mapping was invalid;
  preserve a justified refusal if the target was outside the logical ring; or
  correct adoption/view application only if a complete trace proves a product gap.
  Recommendation: resolve the desktop/store baseline first, then capture one
  complete trailing-selection transaction with native before/after read-back.
- Producer-shaped regression outline, conditional on that trace: start with a
  current occupied desktop, an empty backing desktop and a stopped-created owned
  desktop; use the captured mode/output bindings and exact observed topology.
  Replay observe, numbered selection and trailing selection; assert the resolved
  logical ring, returned view transfer and resulting native view, including a
  genuine unknown-index refusal control. Do not encode the current unproven
  adoption hypothesis as a normative test.
- **Baseline capture defect:** the helper inspected a data-directory path instead
  of the runtime float-intent store documented in
  `crates/omnitiler/src/float_intent_store.rs`. The actual store contains membership
  for the existing applications and was modified during the run; its preimage was
  not captured. User 2026-10-18 accepted leaving it as found until session end
  rather than guessing an empty preimage. Future capture must back up the actual
  runtime file, including absence, before development starts.
- Existing asynchronous overlay diagnostic, fixed-client restart and two-domain
  held-scope investigations remain parked; no new evidence resolves them.
- No product decision or replacement-baseline acceptance was made during the run;
  the later fixture disposition is recorded above and in
  [KDE live acceptance](kde-live-acceptance.md#fixture-failure-and-restoration-blocker).

## Restoration and evidence review

- **Desktop restored: no.** Count is unchanged, but the original second desktop's
  identity/name was replaced by the owned fixture desktop. Its original identity
  was not recovered. The desktop and uncertain runtime-store state were preserved;
  their later accepted disposition is linked above. No fabricated identity or
  broad configuration rollback occurred.
- Native observer endpoint comparisons show Ghostty and Firefox identities,
  frames, membership and active state unchanged; the same observer's current
  desktop and cursor fields also match. Separate loaded-effect, KGlobalAccel-owner
  and complete project-shortcut-tuple captures match the preimage.
- The temporary floating-default key was removed. The final persisted native
  tiling-group inventory matches its preimage; the retained intermediate captures
  do not establish the exact cleanup mechanism. That byte-level comparison does
  not prove semantic health of pre-existing persisted groups.
  Configuration still differs by the second desktop identity/name.
- No disposable application ran. Exact helper script teardown was observed;
  development teardown stopped the owned Planner and unloaded the controller.
  A final independent status read reports Planner unowned, controller unloaded,
  development **DOWN**.
- Independent evidence review rejected the initial claims of full restoration,
  restored float-intent bytes and diagnosed workspace defects. Only the bounded
  observations above are accepted. Raw inventories retain native identifiers and
  remain outside the repository; the durable record intentionally omits them.
- The changed desktop/store disposition is now accepted as linked above. Repair
  preimage/wire capture, then run one discriminating trailing-selection fixture
  before resuming any D-unit journey.

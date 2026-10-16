# Windows visibility diagnostics

## Goal and scope

- Reconstruct Windows hide/reveal outcomes, managed visibility/cloak changes,
  exception transitions and floating/exception refusals from normal run logs,
  with elapsed timing and existing opaque window tokens.
- Diagnostics only: no admission, visibility, focus, recovery, shared-core or KDE
  behavior changes. Never collect titles, application content or native IDs.
- Complements the [Xbox incident diagnosis](../windows-gaming-coexistence.md#incident-diagnosis-and-prevention-options-2026-10-16);
  it neither diagnoses Xbox mode nor claims incident recovery.

## Acceptance and approach

- Reuse existing Windows single-writer structured logging and bounded per-second
  diagnostic patterns. State the concrete flood-prevention reason at each bound.
- Report attempts separately from observed outcomes; preserve uncertainty and
  reasons. Sampling and suppressed-count summaries must not imply a full history.
- Cover hide/reveal success and refusal/failure, visibility/cloak transitions,
  exception origins and floating-workspace refusals with offline regressions.
- Keep large existing modules from growing by extracting cohesive diagnostics
  where useful. No unrelated refactoring or dependencies.

## Bounded work and verification

1. Inspect Windows observation, product-hide, workspace-selection and exception
   paths; implement bounded diagnostics and focused regression coverage.
2. Review behavior neutrality, redaction, bounds, and incident reconstruction.
3. Run locked four-package Windows build/test and strict all-target clippy,
   full rustfmt and diff checks; publish and confirm hosted CI.
- Baseline: 2,286 tests passed, none failed, one ignored. Live incident reproduction
  and physical gaming acceptance are not required for offline delivery.

## Delivered offline (2026-10-16)

- `visibility_diag.rs` owns categorical payloads, sampled-state baselines and
  bounded batching; `migration_visibility.rs` extracts the existing migration
  view effects to keep `migration_run.rs` below the production soft cap.
  Existing oversized `workspace.rs` and `product_hide.rs` are unchanged.
- Normal `workspace-hide` / `workspace-reveal` rows carry opaque window token,
  tick, action correlation, verified outcome, categorical refusal/failure detail
  and action elapsed milliseconds. Coverage includes selects, sends, shared and
  swap views, migration views, rollback and hidden-claim audit retirement.
  Rollback, migration and audit reveals identify their phase. No raw error text
  is logged. Existing shutdown/restart aggregate teardown reporting is unchanged.
- `visibility-transition` independently reports nullable native `visible` and
  `cloaked`, product-hide claim, overlay state, classification reason, and sample
  tick/time. Eligible/retained observations reuse existing facts; otherwise a
  read-only member-lifetime-fenced visibility/cloak probe reports unknown when
  the fence or read cannot establish evidence. Claims never prove invisibility.
- `exception-transition` names intentional float, sticky, born-fullscreen,
  automatic-fixed and Engine exception lanes, with entered/changed/exited reason.
  Gesture refusals distinguish Engine exception from floating workspace or both.
- Every tiling run-log row gains process-relative monotonic `mono_ms`; delayed
  samples carry `observed_ms`. Suspend/resume includes tick and nullable
  `suspended_ms`; work-area changes include old/new work rectangles. These are
  elapsed clocks, not wall-clock or Xbox-mode detection.

## Bounds and accepted limits

- Visibility and exception batches each emit at most once per second with at
  most 16 entries, matching the launcher diagnostic pattern to prevent noisy
  observations from flooding normal logs. Stable samples emit nothing; overflow
  stays pending with `omitted`, throttled tokens count in `coalesced`, and lost
  intermediate samples explicitly mark `history_lost`, including out-and-back
  changes with no net change. These are sampled transitions, not an event journal.
- Hide/reveal legs retain at most 64 per-window rows each, with an explicit
  attempted/logged/omitted summary beyond the cap, including failure returns.
  A rollback has its own bounded leg. Single-window sends are intrinsically
  bounded; audit rows retire each claim once.
- Repeated rehide attempts aggregate into one `rehide-sweep` per second, with
  exact outcome counts, at most 16 sample entries and four remembered tokens per
  bucket. `omitted` counts unlisted occurrences. This prevents persistent native
  failures from producing per-tick logs; corrected transients still count.
- Native state is sampled only at existing reconcile/maintenance observations,
  not during fullscreen suspension. Unknown values and suppressed history remain
  explicit. Read-only probes add work proportional to managed members lacking
  fresh facts; no performance or live desktop acceptance is claimed.
- Implementation choices are the monotonic clock, nullable evidence, existing
  one-second cadence and bounded samples above. No behavior policy is changed.

## Verification and outcome

- Added 27 offline regressions: normal-level payloads, redaction, categorical
  outcomes, independent cloak/overlay changes, native-invisible fencing,
  deduplication, throttle/overflow and out-and-back histories, exception labels,
  rehide counts, rollback phases and migration omission summaries.
- Review corrected an initial observation-only snapshot that could not establish
  native invisibility, lost-history labeling and silent alternate hide/reveal
  legs. Final accepted evidence preserves actual effect ordering and membership
  mutations; diagnostics never feed admission, focus, recovery or layout policy.
- Locked native four-package build/test: **2,313 passed, 0 failed, 1 ignored**
  (baseline 2,286/0/1). Strict all-target clippy (`-D warnings`), full rustfmt and
  `git diff --check` pass. Windows-only source changes; no core/KDE/macOS changes,
  dependencies, live hook runs or desktop mutations. Hosted CI accompanies the
  delivery commit. Xbox recovery and physical gaming acceptance remain unclaimed.

## Agent-live follow-up (2026-10-17)

- Scoped single-output ordinary-app switches at normal logging level, without
  trace, emitted verified `workspace-hide`/`workspace-reveal` rows with opaque
  token, tick, action correlation, categorical outcome and elapsed milliseconds.
  Visibility transitions distinguished visible observations from hidden claims.
- Final run `01dd5d9ee3d0f724`: 10 hide, 10 reveal and 15 visibility-transition
  rows; no malformed hide/reveal payloads. Earlier counts of 10/9/14 were a
  mid-run harvest, superseded by the final post-stop log. The managed-token set
  matched the two identity-verified ordinary-app claims. Initial capture lacks
  a complete artifact provenance sidecar; normal-level emission is directly
  evidenced, not every live acceptance condition.
- Bounds/overflow and exception transitions were not stressed by this sample;
  offline coverage remains their evidence. No Xbox reproduction or recovery
  claim. Locked four-package gates passed: 2,458 passed, 0 failed, 1 ignored;
  build, fmt, strict all-target clippy and diff check passed. No source change.

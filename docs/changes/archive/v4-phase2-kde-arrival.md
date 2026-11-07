# v4 KDE arrival

## Goal and scope

- Port arrival evidence for the approved v4 KDE vertical slice using the existing
  contract and native member/frame/transfer lanes.
- Setter acceptance is not arrival: require fresh authoritative native state and
  exact parent, output, workspace, operation, revision and attempt correlation.
- Exclude focus feedback, state targets, scaled metrics and reconnect/rehome.
  Populated selection may remain pending while focus feedback is absent; keep
  that limitation explicit and the active phase-1 migration record intact.

## Acceptance

- Audit existing producer, actuation and retained evidence before adding code;
  port only missing arrival behavior without changing the shared contract.
- Regressions distinguish accepted setters from proven arrival and reject stale,
  mismatched or ended-lifetime evidence.
- Prove a one-output workspace send with exact-scoped fixtures in real KWin,
  correlated wire evidence and native home/frame observations. No native effect
  is required unless the existing arrival route needs it.
- Full adapter and scoped Rust checks pass without new skips; preserve both
  stage5 production-route proofs and immutable recorded fixtures.
- Independent integrated review has no unresolved in-scope finding.

## Approach and bounded units

1. Audit the approved arrival contract and existing KDE implementation/tests;
   identify the smallest missing port and any shared-contract question.
2. Implement and regress that gap; remove touched code only when proved obsolete,
   separately from behavior changes.
3. Run offline gates, a small fixture-only native proof using the established
   control approach, and integrated review. Record exact supported, unsupported
   and degraded behavior before acceptance.

## Current state

- Audit found the frame gate compared a logical transfer workspace directly to
  native observed homing. It now resolves through the accepted reply's binding;
  unknown/unbound targets still hold. Existing member/view/frame attempts and
  native readback/retention paths remain in use; no new lane is needed.
- A distinct-namespace same-output send regression failed at destination-frame
  convergence before the fix. It now passes with correlated logical-target and
  native pre/post member evidence, no source frame and no duplicate membership.
  Accepted setters without native change, unbound targets, wrong home/output
  and unreadable home do not prove arrival.
- Full adapter typecheck/build/tests and scoped core/protocol/service tests pass.
  Formatting, strict scoped all-target clippy and locked build also pass.
  Independent scoped arrival/resize-diagnostics review found no blocker. No
  obsolete live path was identified for removal.

## Accepted native evidence and limits

- A real-session, exact-class-scoped run joined two normal resizable Wayland
  fixture windows by caller title/PID to unique native UUIDs. Fresh occupancy
  selected workspaces above the occupied range. No native effect was loaded.
- Seven genuine reports and 58 complete call/return pairs were correlated by
  sender/reply serial, producer, generation, source and native watermark; one
  final parked wait had no return. Controller evaluation, initial intent read,
  handshake and matching dispatch/applied pairs were captured before commands.
  Every reported/controlled window belonged to the joined fixture set.
- The focused fixture's numbered send retained the same window parent and command
  sequence while its destination native desktop was created and bound. Its
  member attempt named the logical target with exact native pre/post homes and
  original attempt stamp. A fresh native-home report retired the transfer at
  revision 6; revision 7 and an independent native snapshot matched the
  destination frame. The source view stayed selected and the surviving source
  fixture reflowed. Queue acceptance alone was not the oracle.
- This native route publishes null destination frames while the transfer remains,
  then retires it before frame convergence. It proves the existing integrated
  arrival path, not execution of the changed translated frame gate. The gate's
  direct proof is the offline red/green regression with a retained transfer.
- Immediate Wayland frame setter readbacks retained the old rectangle; the next
  native report and independent snapshot proved convergence. No settling signal
  or completion-contract change was needed for this run.
- The first well-formed tiled send named the nonfocused fixture and correctly
  refused under the existing focused-mover rule. Source audit identified that
  gate before using the actually focused fixture; no production behavior changed
  to accommodate the setup. An earlier malformed source shape was rejected
  before filing. Initial fixture launch and capture-parser environment/format
  errors were corrected before relying on their evidence.
- Exact helper unloads and object absence preceded controller teardown. The owned
  Planner, controller, fixture process/windows/socket and capture process are
  gone; the original view was reselected. Normal created-desktop lifecycle churn
  and stale project shortcut registration records were left untouched. The
  generated bundle is unscoped and non-trace again.
- Independent review of the native evidence found no blocker within these limits.

## Outcome

- Accepted: native member arrival and subsequent destination frame convergence,
  with logical/native namespace-safe retained-transfer frame gating and existing
  lifetime, freshness and safety fences. Full adapter suite: 990 tests, zero
  failures/skips; scoped Rust suites pass, including both enabled stage5 proofs.
- Not claimed: physical shortcut delivery, shown/compositor visibility, focus
  readiness or focus-bearing populated-selection completion. `focusFeedback`
  remains empty and border presentation remains off. State targets, scaled
  metrics and reconnect/rehome are unchanged. This remains development-only.
- Dogfood re-check: send the focused tiled window between workspaces, then check
  continued selection, ordinary tiling and commands. Resize classification stays
  in the separate active diagnostics note, pending real-key re-check evidence.

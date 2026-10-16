# Windows review decisions (2026-10-16)

- Goal: record the approved pager, settings, conflict and activation decisions
  and apply the three bounded Windows corrections.
- Scope: pager retry/queue simplicity; installed-only Game Bar conflicts;
  Windows OS-permitted activation in Switch and focus; related durable records.
- Non-goals: live testing, packaged-app artwork, KDE/macOS behavior changes,
  or deciding minimized-source-slot behavior before Tier 1 reference research.

## Acceptance and approach

1. Pager: retain reasoned 10-second positive refresh and 5-second negative
   retry; fixed 5-second creation/spawn retries; identity-deduplicated icon
   queues without arbitrary caps; only the latest pending click survives.
2. Game Bar: successful absence produces no Win+G conflict or residual note;
   distinguish query failure from absence and report its handling for review.
3. Activation: Windows Switch and focus honors OS-permitted requests even
   over fullscreen/game foreground; default urgency-only and denied requests
   remain unchanged. Stop if the correction would change KDE semantics.
4. Record each approved decision with its date, reconcile related records and
   reference rows, and remove backlog-only physical-check trackers.
5. Verify each coherent correction with locked four-package Windows build,
   tests and strict all-target clippy, workspace format and diff checks, then
   hosted CI. No live evidence is claimed.

## Evidence and outcome

- Baselines: CI 38876735949 (`bfc3ccbd`) and 38876893770 (`8df22a01`)
  passed all eight jobs. Working tree was clean at the latter revision.
- Pager cleanup delivered offline: reasoned fixed retries, uncapped deduplicated
  icon queues, latest-click-only dispatch and regression coverage. Independent
  review found no blocking issues; stale comment corrections were applied.
- Locked four-package build/tests/strict all-target clippy, workspace format
  and diff checks pass: 2,279 passed, zero failed, one existing ignored test.
- Pager hosting, overcapacity and sizing approvals are promoted to decisions
  and reconciled with research/reference records. Other corrections pending.

# KDE native state settlement

## Goal and scope

- Remove false maximize/fullscreen refusal and shortcut-failure diagnostics caused
  by same-stack reads of asynchronously applied native flags.
- KDE adapter only; no core/protocol changes, retries, replay, new timers, geometry
  policy, or live acceptance claim. Retain the shared legacy fullscreen float guard.

## Acceptance and approach

- Issue each setter once, then confirm exactly once at the first fresh accepted
  observation boundary after issuance, never on the capture that issued it.
- Preserve lifetime, scope, focus, generation and newer-input supersession fences;
  an abandoned attempt never acts or reports a later unrelated outcome.
- Keep fullscreen-clear and maximize back-to-back without an applied-fullscreen
  gate between setters. Report genuine native refusal truthfully.
- Cover delayed settlement and silent refusal using native properties, renewed
  wrappers and QML array-likes, plus intervening-command/restart regressions.

## Bounded work and verification

- Source triage found same-stack inference in `world-native.ts`:
  `setMaximizedNative`, `setFullscreenNative`, and `applyAuthorizedMaximize`.
  The authorized result's false return also produces `shortcut-failed` in
  `world-entry.ts`; core-owned maximize intent did not remove this race.
- Pending native-state confirmation is integrated with entry lifecycle cleanup.
  The accepted-path poll precedes new authorized maximize issuance, so its own
  capture cannot confirm it. Rejected/stale replies cannot confirm or replay it.
- The first boundary consumes once. Held/incomplete/unreadable target observations
  terminate narrowly, never claim application, and never wait or retry. Live scope,
  canonical focus and listed lifetime are rechecked alongside captured scope.
- Sequential verification:
  - `devenv shell --impure -- npm test --prefix kwin`: 835 passed, zero failures.
  - `devenv shell --impure -- npm run typecheck --prefix kwin`: passed.
  - `devenv shell --impure -- git diff --check`: passed.
- Producer-shaped entry regressions cover actual delayed applied flags for
  maximize/restore, fullscreen enter/exit and fullscreen-to-maximize; silent
  refusal; direct/queued supersession; stale/rejected replies; generation mismatch
  with rediscovery and a fresh command; stop and fresh entry isolation. The harness
  renews wrappers and uses QML array-likes, braced IDs and native property names.
- Unit regressions cover held/incomplete domains, topology/capture incompleteness,
  unreadable/moved live scope, focus-ref mismatch, busy/gesture fences and exactly
  once failure. Existing both-flags ordering and native refusal coverage remains.
- Regression extension exposed two fixture expectations: a departed active target
  fails the focus fence before the liveness fence, and fake broker queue/take
  replies must follow the rebound generation. Tests reflect those contracts;
  neither required a production workaround.

## Outcome

- Delivered adapter-only correction; no core/protocol or shared legacy float
  refusal changes. No new durable product decision. Backlog and coverage matrix
  distinguish offline verification from renewed live diagnostic/B1 acceptance,
  which remains pending.

# D23 follow-ups: sole-restore focus diagnostic + close-all slot assessment

Goal: resolve the two known D23 follow-ups without product behaviour change beyond D23 as approved.
1. Sole-restore false focus-mismatch: restoring a workspace's sole minimized window must not report a focus-mismatch diagnostic when the converged state is correct (MNZ-02 writes no focus; MNZ-03 sole minimize leaves focus none). Genuine mismatches still report (Observability).
2. Close-all slot-only retention: assess leak (growth, stale reuse, wrong restore); minimal fix only if clear and uncontentious, else assessment only.

Scope:
- Shared core focus-match on reconcile-request path; regression coverage.
- Close-all assessment with file:line evidence; code change only if trivial.
- Records: this note, D23 archive note known-risks/followups, backlog D23 row.

Non-goals: settings, Windows behaviour change, macOS, live testing (none authorized).

Acceptance:
- Red/green regression: sole restore projects with focus none and no mismatch; unknown/ghost focus still reports mismatch with converged state retained.
- Close-all: fix + regression, or recorded assessment only.
- Full gates: cargo test --workspace, clippy -D warnings, fmt --check, kwin npm test, typecheck, bundle; CTest only if native code touched (not touched).
- One independent read-only review.
- D23 archive note + backlog row updated. Push (fetch/rebase first; concurrent Windows agent).

Approach: worker 1 implements focus fix + close-all assessment; worker 2 runs full gates; worker 3 independent review; Lead integrates records + push.

Units:
1. Focus fix + close-all assessment (done, uncommitted).
2. Full gates.
3. Independent read-only review.
4. Records + push.

Verification: unit 1 targeted gates (6/6 minimize_restore, crate tests, clippy, fmt); unit 2 full gates; unit 3 review.

Tentative decisions (pending user review):
- Focus fix shape: focus-none convergence whose observed focus names a converged tile of the same domain projects with focus none; all other focus-none and cross-domain cases still reject. Justification: converge skips focus adoption only for just-restored minimized windows, so the condition identifies the D23 no-focus-write restore.
- Close-all: assessment only (deferred). Core cannot distinguish close from minimize (identical omission signal); explicit destroyed-signal needs protocol + both-adapter changes; map caps could evict live minimize slots. Memory-only consequence: one small slot per distinct closed id per session lifetime plus one retained session per emptied-with-slots domain; no wrong restore (restore validation + disjoint reset + fresh ids; KWin internalId unique per instance; Windows adapter not driving minimize paths).

Outcome: delivered. Focus fix plus close-all assessment; full gates green;
independent review ACCEPT with non-blocking followups (cross-domain
focus-none regression; carry retention bound into records). Evidence: cargo
test --workspace, clippy -D warnings, fmt --check, kwin npm test (1413/1413),
typecheck, bundle build; CTest skipped (native untouched).

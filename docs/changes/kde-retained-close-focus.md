# KDE retained close-focus v4 adoption

## Scope and acceptance

- Adopt the approved [retained positive-close contract](../porting/world-contract.md#retained-positive-close-focus) in the KDE producer, strict reader and native consumer. No core policy change, input hook, alternate target search or retry.
- Preserve literal null foreground as known no-client; unreadable, unbound and scope-foreign remain unknown. Require both new wire fields without defaults or dual shapes.
- Apply only the latest publication and spend each exact bound id before a native attempt or safety no-call. Native target equality needs no setter; other eligible fallback allows one mechanically fenced correction. Keep raw feedback and bounded lifetime accounting across context-preserving transport loss; reset only at fresh handshake.
- Prove a genuinely produced request after skipping its issuing publication and two unchanged publications, with exactly one setter from the latest and feedback naming the immutable issuing revision. Cover no-call, null/new cancellation and lifetime accounting. Native acceptance is separate.

## Implementation and evidence

- `world-observe.ts` and `world-report.ts` implement truthful `foregroundKnown` and strict required nullable `closeFocus`. `world-state.ts` retains one current request and a retired-revision fence. `world-apply.ts` consumes the retained request independently of the effects lane, after geometry, using the existing safety fences and a last-moment captured-versus-fresh foreground guard.
- Review exposed a fabricated burst proof and a missing fresh-foreground baseline fence. The proof now ingests four real broker publications, skips the first three and applies only the fourth; third/foreign foreground and unknown baseline tests pin informative spend-once no-calls. No adapter supersession policy was added.
- Genuine current producer bytes are recorded in `test-fixtures/kde-v4-active`; both stage5 chains are recorded in `test-fixtures/stage5-v4-active`. Historical bytes remain unchanged. Recorded creation receipts use a bounded initial recorder scheduling hold; replay mints before the receipt and checks a delayed-discovery case without modifying timestamps or the clock oracle.
- Independent source review accepts the offline adoption. A targeted rerun passes all 16 consumer/accounting tests and the genuine-producer burst test. Final integrated checks pass Rust core/protocol/CLI 3283 tests, zero failed or ignored, with only the unsafe-posture test filtered; KWin 1272/1272, zero skipped; and both TypeScript configurations. The separate shared-roster disconnect correction now passes its genuine serial journey and independent review. Offline integrated acceptance is complete; native acceptance remains open.

## Dependencies

- Core/protocol already implement the retained amendment. Minimal required-field repairs in broker validation skeletons and Rust test helpers are separate from this adapter adoption.
- Current recordings require both this adoption and the required `sharedNativeRoster` handshake amendment. Frozen `world-wire/v4`, `stage5` and `stage5-v4` families remain historical; no compatibility shape is introduced.

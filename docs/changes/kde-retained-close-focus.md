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

## Live recheck (2026-11-11, incomplete)

- The shown-workspace sample had three tiled clients, not the required two:
  two test windows plus a pre-existing client (`cont-e12-pres.json`, revision
  79). Native traces show focused test window -> surviving test window ->
  pre-existing client on successive closes (`lead-final-dev-log`:2574,2634).
  This is bounded focus-succession evidence, not acceptance of the exact
  two-window/MRU/spend-once case.
- A separate sole-test-window close produced one automatic correction to the
  pre-existing client on the other output, without repeated attempts in the
  captured interval (`cont-sole-journal.txt`, `lead-final-dev-log`:2832-2855).
  The claimed post-close presentation was not retained. Global foreground
  alone does not prove the empty output stayed shown and unfocused.
- Independent evidence review leaves native acceptance open. The saved close
  journals and full trace do not establish the retained request's issuing
  identity, MRU-target equality or complete spend accounting; absence of a
  focus line in a filtered capture is not zero-setter proof. Repeat the exact
   two-client shown-workspace case and retain before/after per-output native
   currents, foreground and correlated close-focus publications.

### Post-correction bounded close sample (2026-11-11)

- The [dynamic-workspace recheck](kde-dynamic-workspaces.md#post-correction-native-checks-2026-11-11-partial) now has exactly two tiled members on one logical output/workspace: closing the natively activated member leaves its sole survivor focused and reflowed at revision 38 (`c32-pres-after-closep.json`). Sole close at revision 40 leaves the workspace shown with no managed focused client; both native per-output currents and zero pending plans/transfers are present in the post-close trace (`d03`, `d05`). No retry loop is observed in the bounded capture.
- Native focus-application acceptance remains open. The run did not retain the full `closeFocus` publication or immutable request identity, and filtered absence of attempt lines does not prove zero setter, one bounded correction or spend-once accounting. Generic presentation setter lines are not focus setter evidence. No broader PASS or checklist closure follows from this sample.

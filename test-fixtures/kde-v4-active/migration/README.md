# Recorded disjoint migration fixtures, active family (genuine v4 core bytes)

Recorded from the retained real `WorldEngine` (`raw_world_eval`, default
config) against the same synthetic offline producer as the historical
family: two outputs (`out-a`/`out-b`), one native desktop each
(`wa1`/`wb1`), `win-a` on `out-a`, `win-b` on `out-b`, focused `win-a`.
No conversion from any older wire version: every reply below is a fresh
genuine engine product for the recorded report bytes, bound by the
test-only `MiniBroker` transport shim
(`kwin/tests/world-wire-harness.ts`). Produced by the bounded recorder
module `kwin/tests/world-migration-record.ts` (standalone CLI
`kwin/tests/world-migration-record-cli.ts`; production
`startWorldAdapterEntry` + genuine `raw_world_eval` + `createMiniBroker`)
through the production `requestWorkspaceMigrate("right")` route.

Retained input sequence (mirrors the historical README):

1. `handshake.request.json`: production `buildHandshakeRequest`,
   answered by the real core, minting the serving generation
   (`world-eval-0`). Carries the required `sharedNativeRoster`
   capability (`true` here).
2. `settings.request.json`: production `buildSettingsRequest`
   (`set-1`, rev `1`) with the entry-default snapshot, accepted.
3. `capture-seed-1.*`: production report + capture envelope, stamped by
   the MiniBroker, accepted by the real core with a transfer-free reply
   (asserted, single seed).
4. `queue.request.json`: the entry-filed `migrate-directional` right
   (`clientCmd` 1), filed through `MiniBroker.answerQueue`.
5. `capture-drain.*` + `MIGRATE_DRAIN.reply.json`: the next capture
   drains the command. The bound drain reply retains the migration
   transfer `op-1` (`out-b`/`ws-1`, `cmdSeq` 1) plus its paired
   source-refill child `op-1-refill` (`out-a`/`ws-6`); the migrated row is
   unbound (`bind` `""`) until the creation-flow proof; the actuator-lane
   `result` is nulled (terminal results ride the take lane only).
6. `take.request.json` / `MIGRATE_TAKE.result.json`: the terminal
   `migrate-workspace` result (`out-a`/`ws-1` right to `out-b`/`ws-1`,
   members `["win-a"]`, `floats` 0), byte-identical in shape to the
   historical take result.

Current-shape lanes (never patched): every reply carries the required
`intentDesired` lane (`[]` here, no intent writes ride this scenario)
and the required nullable `focusRequest` lane (`null` here, no close
intent rides this scenario). The report carries the required
`foregroundKnown` bit.

Volatile normalization (mechanical, world-bytes precedent): only the exact
per-run entry `source` nonce and its bound `req` echo are rewritten to the
stable `src-a` token in the four capture files via exact-value replacement
(no JSON re-serialization; inner report/reply bytes untouched). `gen`
(`world-eval-0`), `nativeInvalid`, `obs`/`rev`/`clock` are deterministic
across runs (verified by a second recording: byte-identical modulo the
source nonce).

Boundary: the historical `test-fixtures/kde-v4/migration` family is
preserved read-only history (predates `sharedNativeRoster`,
`foregroundKnown`, `focusRequest`, `intentDesired`). The core's workspace
model has since grown more trailing spares, so the core-chosen refill here
is `ws-6` (historical: `ws-3`); the take result and the op-1/op-1-refill
pairing semantics are unchanged.

What this is NOT: the binding envelope (`source`, `nativeInvalid`,
`clock`, nulled `result`) is applied by the test-only `MiniBroker`
transport shim, not the production broker; the producer is synthetic
(`pid` 1234 is a scripted fact, no real paths, pids, or native UUIDs ride
here); nothing here is live native proof or production-service output.

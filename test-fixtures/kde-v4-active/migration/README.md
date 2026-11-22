# Recorded disjoint migration fixtures, active family (genuine v4 core bytes)

Recorded from the retained real `WorldEngine` (`tiler-protocol` `world_eval`,
merged from the former `raw_world_eval` with byte parity, default
config) against the same synthetic offline producer as the historical
family: two outputs (`out-a`/`out-b`), one native desktop each
(`wa1`/`wb1`), `win-a` on `out-a`, `win-b` on `out-b`, focused `win-a`.
No conversion from any older wire version: every reply below is a fresh
genuine engine product for the recorded report bytes, bound by the
test-only `MiniBroker` transport shim
(`kwin/tests/world-wire-harness.ts`). Produced by the bounded recorder
module `kwin/tests/world-migration-record.ts` (standalone CLI
`kwin/tests/world-migration-record-cli.ts`; production
`startWorldAdapterEntry` + genuine `world_eval` + `createMiniBroker`)
through the production `requestWorkspaceMigrate("right")` route.

Retained input sequence (mirrors the historical README):

1. `handshake.request.json`: production `buildHandshakeRequest`,
   answered by the real core, minting the serving generation
   (`world-eval-0`). Carries `sharedNativeRoster: false` (independent-native
   profile, via the test-only `startWorldAdapterEntry` override) so this
   fixture exercises the disjoint non-roster migration apply route; the
   separate real-broker producer suite remains the KDE shared-roster proof.
2. `settings.request.json`: production `buildSettingsRequest`
   (`set-1`, rev `1`) with the entry-default snapshot, accepted.
3. `capture-seed-1.*`: production report + capture envelope, stamped by
   the MiniBroker, accepted by the real core with a transfer-free reply
   (asserted, single seed).
4. `queue.request.json`: the entry-filed `migrate-directional` right
   (`clientCmd` 1), filed through `MiniBroker.answerQueue`.
5. `capture-drain.*` + `MIGRATE_DRAIN.reply.json`: the next capture
   drains the command. The bound drain reply retains the unheld migration
   transfer `op-1` (`out-b`/`ws-1`, `cmdSeq` 1) plus its paired unheld
   source-refill child `op-1-refill` (`out-a`/`ws-2`, bound `wb1`); the
   migrated row is unbound (`bind` `""`) until the creation-flow proof;
   `WorldReplyV4` omits the retired `effects` key, carries no `result` field
   (terminal results ride issuing `Vec<BatchSettlement>` and the
   requester-only `TakeCommandResult` lane), and carries required nullable
   `dragPreview: null`.
6. `take.request.json` / `MIGRATE_TAKE.result.json`: the terminal
   `migrate-workspace` result (`out-a`/`ws-1` right to `out-b`/`ws-1`,
   members `["win-a"]`, `floats` 0), byte-identical in shape to the
   historical take result.

Current-shape lanes (never patched): every reply carries the required
`intentDesired` lane (`[]` here, no intent writes ride this scenario)
and the required nullable `focusRequest` lane (`null` here, no close
intent rides this scenario). Each capture report carries the required
`foregroundKnown` bit and `focusChanges`. The scripted desktop surface has
no complete focus interval: `events` is null at genesis and at the drain,
with `after` respectively null and `"3"`.
Bound replies also carry required nullable `gestureSettledAssoc` (null:
the MiniBroker strips native gesture ingress and fabricates no association).

Volatile normalization (mechanical, world-bytes precedent): only the exact
per-run entry `source` nonce and its bound `req` echo are rewritten to the
stable `src-a` token in the four capture files via exact-value replacement
(no JSON re-serialization; inner report/reply bytes untouched). `gen`
(`world-eval-0`), `nativeInvalid`, `obs`/`rev`/`clock` are deterministic
across runs (verified by a second recording: byte-identical modulo the
source nonce).

Boundary: the historical `test-fixtures/kde-v4/migration` family is
preserved read-only history (predates `sharedNativeRoster`,
`foregroundKnown`, `focusRequest`, `intentDesired`, and removal of `effects`).
Under the independent-native (`sharedNativeRoster: false`) profile, the
core-chosen refill here is `ws-2` (bound `wb1`, order 0 on `out-a`); the take
result and the unheld `op-1`/`op-1-refill` pairing semantics are unchanged.

What this is NOT: the binding envelope (`source`, `nativeInvalid`,
`clock`, `gestureAssoc`, `gestureSettledAssoc`) is applied by the test-only
`MiniBroker` transport shim, not the production broker; the producer is synthetic
(`pid` 1234 is a scripted fact, no real paths, pids, or native UUIDs ride
here); nothing here is live native proof or production-service output.

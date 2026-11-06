# Recorded disjoint migration fixtures (genuine v4 core bytes)

Recorded from the retained real `WorldEngine` (`raw_world_eval`, default
config) against a synthetic offline producer: two outputs (`out-a`/`out-b`),
one native desktop each (`wa1`/`wb1`), `win-a` on `out-a`, `win-b` on
`out-b`, focused `win-a`. No conversion from any older wire version: core
replies retain their content with the transport binding described below.

Retained input sequence:

1. `handshake.request.json`: production
   `buildHandshakeRequest` (own identity + per-output KWin capabilities),
   answered by the real core, minting the serving generation.
2. `settings.request.json`: production
   `buildSettingsRequest` (`set-1`, rev `1`) with the exact entry-default
   10-tuple snapshot (gap 8, outer 8, `per-output-local`, ...), accepted.
3. `capture-seed-1.*`: production `buildWorldReportV4InnerJson` +
   `buildCaptureEnvelope`, stamped by the test-only `MiniBroker`
   (`kwin/tests/world-wire-harness.ts`), accepted by the real core with a
   transfer-free reply (asserted, single seed).
4. `queue.request.json`: direct `migrate-directional` right, filed through
   `MiniBroker.answerQueue`.
5. `capture-drain.*` + `MIGRATE_DRAIN.reply.json`: the next capture drains
   the command. The bound drain reply retains the migration transfer `op-1`
   (`out-b`/`ws-1`, `cmdSeq` 1) plus its paired source-refill child
   `op-1-refill` (`out-a`/`ws-3`); the migrated row is unbound (`bind`
   `""`) until the creation-flow proof; the actuator-lane `result` is
   nulled (terminal results ride the take lane only).
6. `take.request.json` / `MIGRATE_TAKE.result.json`:
   the terminal `migrate-workspace` result (`out-a`/`ws-1` right to
   `out-b`/`ws-1`, members `["win-a"]`, `floats` 0).

Semantics the consumer tests rely on: stable core logical ids (`ws-1`
rides the move, refill names `ws-3`), one-shot migration + refill pairing
(`-refill` suffix), unbound target (hold, never guess), exact
`MIGRATE_TAKE` match.

What this is NOT: the binding envelope (`source`, `nativeInvalid`,
`clock`, nulled `result`) is applied by the test-only `MiniBroker`
transport shim, not the production broker; the producer is synthetic
(`pid` 1234 is a scripted fact, no real paths, pids, or native UUIDs ride
here); nothing here is live native proof or production-service output.

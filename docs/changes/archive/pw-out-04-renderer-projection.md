# PW-OUT-04 — Narrow the renderer read projection

Implemented the recommended option in the [protocol review](../protocol-correctness-review.md#pw-out-04--narrow-renderer-replies-without-dropping-scenario-oracles). The renderer read retains binding, freshness, coverage, the scenario window oracle, paint descriptors and reduced preview. It omits the three world arrays and the nested presentation pager, suspension verdict and revision. Suspension still controls preview emission internally. Full replies and the native v2 relay are unchanged.

The checked Rust writer, cached read oracles, KDE strict reader and synthetic read inputs use the same narrower schema. Historical projections refuse; active Stage 5 projections drive the subscriber and the real-broker byte tests.

Consumer inventory: KDE relays the retained fields; its freshness gate uses the
outer `rev` and `obs`. The scenario runner uses exact window sets, home and
allocation, not any dropped field. Broker service and recorder tests use the
same read endpoint; assertions about full verdicts belong to the capture reply.
No deferred native-port call site uses this endpoint.

Before the cutover, the KDE reader rejected a narrowed copy of the active
preview projection, accepted the obsolete broader shape, and accepted a
zero-width preview that the checked Rust writer's carried-rectangle gate rejects.

The checked Rust writer also refused malformed preview producer identities
after the parity change; before it, the writer emitted an invalid instance
that KDE already refused. The prevalidated broker path retains its validated
gesture association and binds the cached canonical bytes without parsing.

The existing Stage 5 v4 recorder regenerated both chains. Only the 13 active
read projections were replaced, with generation normalized to the existing
manifest. All retained projection bytes and all 14 native v2 dispatches match
the previous evidence after generation normalization and the approved key
removals. The frozen fixture families compare byte-identical to `ff6eaddc`.

Verified: Rust formatter, clippy, and all-target tests (3,590 passed) for the
selected crates, standalone raw evaluator test, fresh offline evaluator build,
KDE typecheck and tests (1,490 passed, one skipped), and hermetic scenario
tests (131 passed). An initial KDE run overlapped Cargo builds and timed out
in evaluator-backed cases; running without the competing build passed.
The independent wire review found no remaining issue. No live behavior was tested.
No context-dependent compatibility path was added; older strict readers must
upgrade with the writer.

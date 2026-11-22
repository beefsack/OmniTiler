# Consolidate offline engine evaluators

Merge the two retained-engine examples into `tiler-protocol`'s `world_eval`: the richer engine-only seam has no `omnitiler` dependency and the existing protocol CLI need not move. Preserve ordinary and settlement-bearing report bytes, refusals, generation and stdin lifetime. Keep the typed transcript regression and the separate `omnitiler` broker oracle for real FIFO, endpoint and binding evidence (PW-BR-05). Production routes and fixture bytes are out of scope.

Build the originals and compare their stdout to the surviving executable before removing the raw evaluator. Exercise the protocol transcript, the raw shared-roster regression, and the KDE engine consumers. Then retarget the callers, remove the duplicate, and run the focused and all-target Rust gates, KDE typecheck and tests with both examples prebuilt, and fixture and whitespace checks.

## Transcript parity

At `1fa340eb`, the two engine-only examples were 267 and 120 lines (the review listed 273 and 128); the broker was 425. The review's 19 and 12 KWin reference-file counts still held. The surviving engine example is 254 lines including the moved shared-roster test, a net reduction of 133 example lines. All 18 KWin engine spawns and the standalone migration recorder moved; the three real-broker spawns and producer harness stayed unchanged.

Before deletion, the original protocol CLI and the merged CLI emitted identical stdout on all 12 durable `world_eval_v4.jsonl` requests and six additional malformed and unknown-method cases. The independent typed transcript test passed unchanged. The original raw CLI and the merged CLI emitted identical stdout on the same 22-line shared-roster test stream (19 replies), including append feedback, replay, malformed envelopes, stale generation and reconnect. The moved example test passed. A lockstep comparison on the original KWin/recorder inputs matched 535 replies across 62 retained-engine sessions, including 406 calls to the settlement method and executed, null and refused command results. The real broker was not routed through either engine comparator.

## Cutover verification

`cargo fmt --all -- --check`, strict all-target Clippy, and the locked all-target test gate for `tiler-core`, `tiler-protocol`, and `omnitiler` passed (3,590 tests). The focused example test passed. After offline builds of both examples, KWin typecheck passed and its offline suite passed 1,490 tests with the same one skip as the original. The recorder produced ten files identical to the original run. The frozen fixture roots still match `ff6eaddc`; `git diff --check` passed.

Developer gates for the retained-engine example:

```sh
cargo build --offline -q -p tiler-protocol --example world_eval
cargo test -p tiler-protocol --example world_eval
```

Build the separate broker oracle for offline KWin tests with `cargo build --offline -q -p omnitiler --example broker_world_eval`.

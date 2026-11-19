# Pre-KDE external review fixes

The review of `cb6a31e9..6912e88b` found two ways a valid capture could lose usable actuation authority, plus three contract problems. Keep the prepared transition, strict validation and FIFO. Correct the supported typed, checked broker and KDE offline routes before the KDE rebuild. This is not a KDE rebuild, a macOS or Windows change, or native desktop acceptance.

## Changes and offline evidence

| Finding | Fix and discriminator |
| --- | --- |
| F1: an unrepresentable group underlay rejected the world | Hide only the underlay as `unrepresentable`, with no clamping or relaxation of reply validation. Typed and checked-broker captures preserve the healthy output and execute the unrelated command while the chord stays held. Rust and KWin strict decoders accept the new reason and reject an unknown one. The gesture regression compares identities across lanes rather than pinning the allocator's `leaf-1`. |
| F2: a migrated float lacked membership authority | The issuing and delayed typed and checked replies carry controlled membership at the destination without taking over the float's frame. KWin checks each row before dispatch and again before setters. An unhomed unauthorized row holds arrival and dependent view actions; matching readback proves arrival without writing. |
| F3: KDE inferred a refill parent from the operation spelling | Both strict codecs require nullable `TransferTarget.parent`. Prepared and checked replies carry the explicit relation on issuing and retained publications. A genuine checked-broker reply with unrelated opaque operation IDs exercises the real KWin reader and actuator without a requester result: the paired refill waits for arrival; an unparented suffix lookalike acts independently. Exactly eight active recordings changed; removing only the added `parent` members restores their prior bytes. |
| F4: valid queues could receive no receipts with no typed explanation | Pre-inspection capture refusals return `NotAdmitted` and do not enumerate receipts. A valid inspected refusal keeps `NotRun`; an invalid inspected batch has a batch-only refusal, and exact replay remains a cached publication with `NotRun`. The checked broker restores the FIFO head after a post-drain native-capability mismatch. Both commands remain pending and a good capture executes focus B then maximize current focus, with only B maximized and no repeat toggle. Typed coverage also distinguishes stale/reused request fences and malformed-queue precedence. Internal drain faults still rotate. The scoped F4 run passed fmt, strict Clippy and 3,540 Rust tests with no skips. |
| F5: the KDE port guide still required removed `closeFocus` | Describe required nullable `focusRequest`, required `foregroundKnown`, and latest-only, once-per-request fenced feedback. This corrects the guide, not the wire shape. The new presentation, migration D5/D7 and admission decisions are recorded in their existing topic entries, pending user review. |

## Deferred and unchanged

- `test-fixtures/kde-v4-active/migration/settings.request.json` predates the recorder defaults `switchTarget:"cursor-output"`, `freshPlacement:"cursor-output-last-focused"` and `focusFollowsCursor:false`. It was deliberately left unchanged. The approved migration reply changes are limited to the new `parent` members.
- Windows still compares or parses the legacy `-refill` operation suffix in `crates/tiler-windows/src/tiling_sys/world_diff.rs:3323,3716`. Related tests are `world_diff_shared_convergence_tests.rs:1060`, `world_diff_tests.rs:3088,3136` and `world_native_roundtrip_tests.rs:71` in the same directory. The Windows rebuild must use the typed `parent` relation, not operation spelling. No Windows code or tests changed here.
- Native desktop acceptance and end-to-end checked-service latency under cold admission and a 200-window command burst are separate work. The structural report harness excludes broker delivery and decode; the broker timing workload uses a small genesis reply, not native actuation.

## Completion gates

An independent read-only audit found no material contradictions in the supported v4 routes, regression oracles or documentation, and confirmed the eight-path F3 recording scope.

The final gates are offline: scoped Rust format, all-target build, strict Clippy and all-target tests without skips; KWin typecheck and full tests with both offline Qt probes; Stage 5 production routes and canonical byte oracles; frozen fixture comparison; launcher, native-effect and helper tests. Measure the instruction-count gate, the release structural steady p95 at 10/50/200 windows (1/4/10 ms caps), and the unchanged fresh/warm broker workload separately from other checks.

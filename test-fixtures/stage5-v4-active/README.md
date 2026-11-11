# Stage 5 v4-active counterpart fixtures

Active v4 protocol target for the stage5 full/s2 chains. Genuine offline
`broker_world_eval` entry recordings through the real
`startWorldAdapterEntry` facts/gesture lanes (`DiscoverWorld`
handshake+settings, `DescribeWorld` captures, `ReadWorldPresentation`
reads; exactly those three methods), produced by the actual entry recorder
`kwin/tests/world-stage5-v4-record{,-cli}.ts`. Each chain (`full`, `s2`)
retains three genesis captures and every intermediate envelope in manifest
order, plus the capture replies, read projections, and production-subscriber
dispatch renders.

Boundary: frozen `../stage5-v4` (hidden-border lineage, no
`sharedNativeRoster`/`foregroundKnown`/`closeFocus`) and frozen `../stage5`
(facts inputs) are read-only history. This family is the current conformance
target. The legacy v3 `../stage5` tree also supplies the facts inputs the
recorder answers with; those inputs are unchanged.

New keys versus frozen `../stage5-v4` (all producer-genuine, never patched):

- Handshake request carries the required `sharedNativeRoster` capability
  (`true` for this shared-native KDE route).
- Every capture report carries the required `foregroundKnown` bit.
- Every capture reply carries the required nullable `closeFocus` lane
  (`null` here: no close intent rides these chains).

The projections carry the genuine visible anchored border (7 keys, `win-a`
subject with a joining witness) and the found `win-a` underlay, matching the
`../stage5-v2` producer lineage; the frozen `../stage5-v4` hidden border
stays historical.

Normalization: only the exact top-level `gen` value is rewritten to the
harness-fixed `gen-stage5-v4-active` in every gen-carrying sibling
(settings request/reply, handshake reply, envelopes, replies, projections,
dispatches, manifest). Every other byte and key order is preserved,
including `source`, `sourceSeq`, `nativeInvalid`, `req`, clocks, and all
report/facts/gesture/revision/witness lanes.

Known seams (not evidence of native behavior): the recorded native report
uses fake zero-inset frames, the run parks one `Wait` lease, and these files
carry no native or auth evidence. Scheduling seam on the record side: the
recorder holds the first capture send one bounded 100ms so the serving
broker clock is safely positive when genesis-0 evaluates; the entry learns
the broker epoch from the reply clock, so the genesis-1 creation receipt
stamps at attemptMs ~= 100+ (never zero). No synthetic clocks, no byte
patching: genuine report ingestion, only later. Clock-epoch replay seam: the
recorded attempt stamps postdate a fresh broker clock, and the trailing spare
creation is minted at genesis-0 while its receipt rides genesis-1, so the
Rust proof pre-reads every replay byte before discovery (the handshake resets
the broker clock), feeds genesis-0 immediately to mint on the fresh clock,
then takes one bounded sleep past the maximum recorded stamp before feeding
genesis-1 on. A postdated mint would latch `ambiguous-creation` permanently.
Attempt bytes and the clock oracle are untouched; only scheduling adapts.

Reproduce with fresh output paths outside the repository (refuses to write
inside the repo or overwrite existing chain directories):

- Build the bridge binary and bundle the recorder CLI, then run it with an
  explicit `--bridge-binary` path:

  `node "$RECORDER_JS" --out-dir "$OUT_DIR" --chain both --bridge-binary "$BRIDGE_BIN"`

  Then rewrite only the exact recorded run `gen` token to
  `gen-stage5-v4-active` in every sibling (no other byte changes).

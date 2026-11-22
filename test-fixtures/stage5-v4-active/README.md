# Stage 5 v4-active counterpart fixtures

Active v4 protocol target for the stage5 full/s2 chains. Genuine offline
`broker_world_eval` entry recordings through the real
`startWorldAdapterEntry` facts/gesture lanes (`DiscoverWorld`
handshake+settings, `DescribeWorld` captures, `ReadWorldPresentation`
reads; exactly those three methods), produced by the actual entry recorder
`kwin/tests/world-stage5-v4-record{,-cli}.ts`. Each chain (`full`, `s2`)
retains two genesis captures and every intermediate envelope in manifest
order, plus the capture replies, read projections, and production-subscriber
dispatch renders.

Boundary: frozen `../stage5-v4` (hidden-border lineage, no
`sharedNativeRoster`/`foregroundKnown`/`focusRequest`) and frozen `../stage5`
(facts inputs) are read-only history. This family is the current conformance
target. The legacy v3 `../stage5` tree also supplies the facts inputs the
recorder answers with; those inputs are unchanged.

New keys versus frozen `../stage5-v4` (all producer-genuine, never patched):

- Handshake request carries the required `sharedNativeRoster` capability
  (`true` for this shared-native KDE route).
- Every capture report carries the required `foregroundKnown` bit.
- Every capture report carries `focusChanges` with an empty event list, ordered
  from the previous incorporated reply revision (`after: null` at genesis).
  The recorder retains each genuine request nonce.
- Every capture reply carries the required nullable `focusRequest` lane
  (`null` here: no close intent rides these chains) and the required nullable
  `dragPreview` lane, and omits `result` (command outcomes ride issuing
  `Vec<BatchSettlement>` and requester-only `TakeCommandResult`). In `full`,
  `reply-preview-0.json` carries a non-null live `dragPreview` (`id: "1"`,
  `win: "win-b"`, with `frame_auth: "hold"` and `frame_hold: "gesture-barrier"`
  on `win-a`/`win-b`), and `reply-cover2-0.json` retains that same non-null
  `dragPreview` (`frame_auth: "controlled"`, `frame_hold: ""`); the
  corresponding dispatches (`dispatch-preview.json`, `dispatch-preview-b.json`,
  `dispatch-cover2.json`) carry `dropPreview: null` under the independent
  live-dispatch hide veto.
- Capture replies also carry nullable broker `gestureSettledAssoc`, pinned
  independently of current ingress. Retained gesture outcomes remain present
  on later fresh publications; operation/switch outcomes use the current
  `(win,kind)`/`out` keys.
- Genesis-0 publishes the shared `rosterPlan`. Genesis-1 returns exact
  `rosterFeedback` for its one tail append and the same-apply native frame
  attempts; the broker then publishes the settled backing without a plan.
- Re-recording scope: when the two `envelope-genesis-1.json` frame attempts
  change (for example, adding required `attemptId`), both `full` and `s2`
  chains are re-recorded end-to-end as coherent recorder cohorts rather than
  patching individual envelopes. Because each capture's envelope and reply are
  coupled by fresh per-run `source`/`req` nonces and monotonic/dispatch clock
  readings (`clock`, `monotonicMs`, `attemptMs`), all 38 envelope and reply
  files move on those incidental fields while non-frame payload semantics stay
  unchanged.

The projections carry the genuine visible anchored border (7 keys, `win-a`
subject with a joining witness) and the found `win-a` underlay, matching the
`../stage5-v2` producer lineage; the frozen `../stage5-v4` hidden border
stays historical.

Normalization: only the exact recorded `gen` token, including in a
roster plan or feedback, is rewritten to `gen-stage5-v4-active`
(settings request/reply, handshake reply, envelopes, replies, projections,
dispatches, manifest). Every other byte and key order is preserved,
including `source`, `sourceSeq`, `nativeInvalid`, `req`, clocks, and all
report/facts/gesture/revision/witness lanes.

Known seams (not evidence of native behavior): the recorded native report
uses fake zero-inset frames, the run parks one `Wait` lease, and these files
carry no native or auth evidence. Scheduling seam on the record side: the
recorder holds the first capture send one bounded 100ms so the serving
broker clock is safely positive when genesis-0 evaluates; the entry learns
the broker epoch from the reply clock and stamps the same-apply geometry
attempts at attemptMs ~= 100+ (never zero). No synthetic clocks, no byte
patching: genuine report ingestion, only later. On replay, the Rust proof
pre-reads the chain, feeds genesis-0 to issue the plan, then waits past the
maximum recorded frame-attempt stamp before feeding genesis-1. The append
join uses exact generation, revision, step and native order readbacks, not
the clock. The attempt bytes and clock oracle are untouched.

Reproduce with fresh output paths outside the repository (refuses to write
inside the repo or overwrite existing chain directories):

- Build the bridge binary and bundle the recorder CLI, then run it with an
  explicit `--bridge-binary` path:

  `node "$RECORDER_JS" --out-dir "$OUT_DIR" --chain both --bridge-binary "$BRIDGE_BIN"`

  Then rewrite only the exact recorded run `gen` token to
  `gen-stage5-v4-active` in every sibling (no other byte changes).

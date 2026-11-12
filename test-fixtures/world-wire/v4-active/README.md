# world-wire v4-active family

Active handshake/scenario family carrying the required `sharedNativeRoster`
capability. Frozen `../v4` is read-only history; this family is the current
conformance target.

- Source of truth: `manifest.json` in this directory. Unchanged vectors are
  referenced by relative `../v4/...` paths, not copied.
- Changed bytes (generated with the real strict `encode_handshake_request`
  from typed requests, never string-patched):
  - `messages/handshake-*.json` (8 valid requests). Rule: `sharedNativeRoster`
    is true for shared-native vectors and false for emulated and output-local
    Spaces vectors; this is fixture data, not an implication from `hostOwned`. `handshake-native`
    (true) and `handshake-emulated` (false) give true/false encode round-trip
    coverage.
  - `messages/invalid-handshake-shared-roster-{missing,type,unknown}.json`
    (reject-wire negatives for the new key: shape on omission, type on
    wrong-type string, shape on unknown extra key alongside a valid new key).
  - `scenarios/raw-attempt-settlement.json`,
    `scenarios/bootstrap-settings.json`, `scenarios/raw-fact-availability.json`,
    `scenarios/executable-identity.json`, `scenarios/native-domain.json`,
    `scenarios/window-state.json`, `scenarios/linked-selectors-warp.json`,
    `scenarios/persistent-above.json`,
    `scenarios/visibility-order-failure.json` and
    `scenarios/visibility-cross-output-supersession.json`: handshake inputs
    regenerated via the encoder with explicit `sharedNativeRoster: false`.
    These are generic pre-amendment host profiles (including `hostOwned: true`
    vectors in `native-domain` and `linked-selectors-warp`), so `false`
    preserves their authored independent-roster semantics; none of them
    targets the shared roster, and no outcome assertion changes. Every step
    assertion is unchanged (each scenario diff is only the inserted key
    lines, one per handshake). Capability values are explicit fixture data,
    never inferred from `hostOwned` (macOS `hostOwned` output-local Spaces
    vectors are `sharedNativeRoster: false`; only the KDE shared-native
    message profiles listed above carry `true`).
- Historical frozen v4 handshakes (no `sharedNativeRoster`) explicitly reject
  under the new strict shape; see the frozen-rejection test in
  `crates/tiler-protocol/src/connection_protocol/tests.rs`.
- Reply witnesses (138 WorldReply: 113 valid-wire, 25 reject-wire) are
  current-shape vectors recorded through the genuine strict reply codec
  (`decode_reply_v4` then `encode_reply_v4`, never string-patched):
  - The 113 valid-wire witnesses plus the 3 handshake-contextual rejects
    decode context-free; each was re-encoded from its typed value with
    byte-identical output, re-decode stability and exact writer
    round-trip.
  - The 22 context-free rejects are deliberate mutations of the freshly
    encoded valid witnesses at the manifest `invalid.pointer`, preserving
    the manifest exact kind/message and the counterfactual-repair oracle
    (copying the source witness value at that pointer back decodes): all
    are single-pointer mutations.
    `invalid-above-unsupported` shares wire bytes with `reply-above-true`
    (contextual only); the warp and sticky rejects decode context-free
    and refuse only under their declared handshake capabilities.
  - Frozen `../v4` reply bytes are untouched history. The retired
    `raise`/`stack` effect shapes refuse as unknown values; their former
    valid-wire witnesses and effect-lane reject derivatives are retired
    from the active census (negative coverage for the retired kinds is
    pinned by the codec unit test).
- Visibility adapter replies (the four `visibility-order-failure` adapter
  steps and the six `visibility-cross-output-supersession` adapter steps)
  are genuine `WorldEngine` products: each step input is the scenario base
  report driven through `handle_report_v4_for_test` with the scripted
  command series, then encoded with the genuine strict reply codec
  (`decode_reply_v4` then `encode_reply_v4`, never string-patched).
  Reproduction route: `crates/tiler-protocol/tests/visibility_engine_record.rs`
  reads the scenario base inputs manifest-relative and asserts the fresh
  engine bytes equal the committed step messages, with the scripted
  focus/visibility oracles held. The scenario chains use commandless and
  selection dummy requests plus view flip-flops to advance visibility
  revisions; only the final reply per adapter step is kept. Selection-backed focuses
  carry a real `afterShown` dependency; focuses minted on the
  already-selected view are genuinely bare (`afterShown` null) and stay
  gated on the explicit-cause plus command-sequence coupling, output-arm
  arrival brackets and foreground equality. The `fallback: true` hide path
  is pinned by the `visibility_driver_fallback_true_consumer_edge` unit
  test (genuine codec round-trip of the no-focus hide reply) rather than
  by scenario bytes.
- Retained intent-desire amendment: the required `intentWrites` lane is now
  `intentDesired` (typed `IntentDesire`, unique by `win`, opaque canonical
  string or null, no alias). All 138 active reply witnesses plus the ten
  embedded scenario adapter replies were converted through the genuine
  strict reply codec: each valid-wire (plus the three contextual) witness
  decodes under the new schema and re-encodes byte-identical
  (`decode_reply_v4` then `encode_reply_v4`); the 22 context-free rejects
  keep their deliberate single-pointer mutations with unchanged kind/message
  and repair oracle. Scenario jq assertions now read
  `.output.message.intentDesired`. Frozen `../v4` bytes are untouched.
- Retained intent journey (`scenarios/intent-retained-journey.json`): the
  save, two skipped commandless retained publications, proven-refusal null
  clear and retained clear are genuine `WorldEngine` products: each step
  input is a v4 report driven through `handle_report` on
  `WorldEngine::new_test_bound` (native sticky setup, base v4
  `report-intent-float` rebound to `adapter-1`/`gen-1`), then encoded with
  the genuine strict reply codec (`decode_reply_v4` then `encode_reply_v4`,
  never string-patched). The refused proof stamps the genuine save reply's
  own `state_revision`. Reproduction route:
  `crates/tiler-protocol/tests/intent_retained_record.rs` replays the
  recorded report wires on a fresh engine and asserts the fresh bytes equal
  the committed reply bytes exactly, with the retained-map oracles held
  (publication never drains, clear retained again after rollback, typed
  core rows unique by `win` with no aliases).
  Normal tests require the committed file and never create it. Explicit
  re-recording uses `UPDATE_INTENT_RETAINED_FIXTURE=1` with the full permitted
  Rust suite; its producer validates the same transition and byte-count oracles
  before writing.

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

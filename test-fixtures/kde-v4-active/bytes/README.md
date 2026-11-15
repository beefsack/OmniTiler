# kde-v4-active byte fixtures (current producer family)

Byte-exact production payloads recorded by `kwin/tests/world-bytes.test.ts`
in generator mode (`UPDATE_KDE_V4_BYTE_FIXTURES=1`) through the real entry
plus the genuine `raw_world_eval` engine. Never hand-edited.

Boundary vs the preserved historical `test-fixtures/kde-v4/bytes` family:

- `handshake.json` carries the `sharedNativeRoster` capability
  (`kwin/src/world-connection.ts`, `tiler-protocol` connection family).
- Every `report-*.json` carries the `foregroundKnown` provenance bit
  (`kwin/src/world-observe.ts`).
- `reply-observe.json` carries the required nullable `focusRequest` lane
  (`kwin/src/world-report.ts`) and required nullable `dragPreview` lane (`null`
  on this non-gesture path), with no `result` field (command outcomes ride
  issuing `Vec<BatchSettlement>` and requester-only `TakeCommandResult`).
- Bound replies carry required nullable `gestureSettledAssoc`; the byte
  fixture's non-gesture path truthfully emits null.

Regenerate only via the test's generator mode (`UPDATE_KDE_V4_BYTE_FIXTURES=1 npm test`
in `kwin/`); normal runs assert byte-identically with volatile `source`/`req`
masked, never re-serialized.

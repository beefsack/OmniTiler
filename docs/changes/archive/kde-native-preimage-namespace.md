# KDE native-preimage namespace boundary

## Goal and scope

- Treat a native-preimage file from a different bus or KWin owner as a usable
  empty seed, matching a missing file. Never import its old UUIDs for restoration.
- Preserve degraded/blocking corrupt and unreadable storage, exact lifetime
  fences and persist-before-write. The first persist atomically replaces the
  stale file through the existing private store path.
- Exclude sticky-release/rejected-unfloat policy, native-state correlation,
  executor retry semantics and live testing.

## Decision and approach

- User 2026-11-09: a different session or compositor instance has no surviving
  windows to restore from this file; namespace mismatch is empty, not degraded.
  This boundary is recorded in R12 and the dated decision index.
- Change only the store parser's namespace branch; the service's existing
  successful-empty reply then makes the unchanged client seed usable.
- Verify Rust store/service output and its equivalent client wire envelope
  through actual preimage, recovery and executor components. These are split
  language-boundary regressions, not a single end-to-end service/adapter process.

## Acceptance and bounded work

1. Reproduce old namespace rejection, fix bus/owner reads and prove replacement.
2. Verify usable-empty seed permits an authorized native target to arm and
   persist before its setter; corrupt seed still blocks with no native writes.
3. Independently review the complete diff and run affected offline suites:
   core, protocol, service, KDE adapter and scenario runner. Keep immutable
   fixtures unchanged.
4. Record exact evidence and remaining traced user ST checks, then archive.

## Verified outcome (2026-11-09)

- The parser now returns empty on bus-id or KWin-owner mismatch. The unchanged
  service emits `outcome=ok`, zero counts and no records. The client therefore
  accepts the seed and recovery no longer disables fresh native-state lanes
  merely because the file belongs to an earlier namespace.
- Store regressions cover both namespace components, first-persist replacement
  and old-record exclusion. A private-bus service regression proves owner
  replacement, successful empty wire output, persist and reread. Existing
  corrupt, oversized, unreadable and private-file safety checks remain green.
- Client regressions consume equivalent service reply fields through the real
  preimage store, recovery coordinator and native executor. The executor's
  authorization predicate consults the actual recovery/store gates. A fullscreen
  target arms and dispatches persistence with no setter or attempt yet; the
  stored acknowledgement permits exactly one setter and accepted attempt.
  Old `degraded/namespace-mismatch` and `degraded/corrupt` envelopes each produce
  one pre-arm `stale` drop, with no persistence, setter, attempt or spent marker.
- This proves the namespace-dependent gate composition, not the entire entry
  callback (settings, gestures, scope and other independent fences), all four
  native setters, or a single cross-language end-to-end journey. The production
  seed gate is shared by maximize, fullscreen, sticky and above; none of their
  production authorization/execution code changed.
- Initial coverage checked recovery beside executor authorization rather than
  through it. That test wiring was corrected and independently re-reviewed;
  no significant finding remains. No production semantic workaround was needed.

### Before/after regressions

- With the original production namespace branch restored temporarily,
  `cargo test -p omnitiler --lib native_preimage_store::tests::namespace_mismatch_reads_usable_empty_and_write_replaces -- --skip workspace_unsafe_posture`
  fails: `Some("namespace-mismatch")` versus expected `None` (0 passed, 1 failed).
  With the fix restored, the 16 tests selected by
  `cargo test -p omnitiler --lib preimage -- --skip workspace_unsafe_posture`
  pass, including the real private-bus service test.
- Substituting the old service envelope into the fixed client scenario fails its
  no-drop assertion with actual
  `native-state-dropped target=fullscreen window=win-a reason=stale` (44/45
  executor tests pass). This is test-only old-envelope injection, not a Rust
  service running inside Node. With the successful-empty envelope restored,
  all 100 preimage/recovery/executor tests pass. Corrupt is a negative control:
  it remains blocked before and after, so a failing corrupt-control expectation
  is neither required nor desirable.

### Full offline gates

| Gate | Result |
| --- | --- |
| `cargo test -p tiler-core -- --skip workspace_unsafe_posture` | 2551 passed |
| `cargo test -p tiler-protocol -- --skip workspace_unsafe_posture` | 288 passed |
| `cargo test -p omnitiler -- --skip workspace_unsafe_posture` | 341 passed |
| KDE `npm run typecheck` | passed |
| KDE `npm test` (includes unscoped/non-trace build) | 1207/1207, zero skipped |
| `node --test scripts/kde-scenarios.test.mjs` | 130/130 |
| `cargo fmt --all -- --check`; `git diff --check` | passed |

- Immutable world-wire/stage5 fixtures are unchanged. No live test or runtime
  store modification ran. Physical ST remains open; the sticky-release/rejected-
  unfloat item remains separate, with no new diagnosis from this fix.

## Traced user ST re-run

- Follow the [live guide](../../live-kwin-testing.md), append a test workspace
  beyond the existing roster, and use ordinary disposable subjects with known
  separate tile/float baselines. Do not delete or reset the runtime store.
- Before startup, retain the cursor, then start the traced development session:

  ```sh
  CURSOR=$(journalctl --user --show-cursor -n 0 --no-pager | sed -n 's/^-- cursor: //p')
  just dev trace
  ```

- Retain the printed verified KWin owner PID and combined/Planner log locations.
  Before keys, require `world:ready`, `world:discovered`, matched dispatch/applied,
  `world:settings status=accepted` and `world:preimage-seed outcome=ok`. A
  mismatched file now yields Planner `op=preimage-read outcome=ok stored=0
  returned=0` and `native-recovery-seed outcome=ready pending=0`; same-namespace
  leftovers can instead require recovery completion before dependent writes.
  Corrupt/unreadable storage still degrades: stop and retain it, not a reset.
  Missing evaluated readiness is a stop, not a reason to restart around it.

| Separate baseline | Physical keys, releasing and waiting after each | Required evidence |
| --- | --- | --- |
| Ordinary tile | `Meta+M`, `Meta+M`, `Meta+F11`, `Meta+F11` | Two maximize then two fullscreen command/dispatch pairs; native on/off roundtrips, persistence/native execution rather than namespace-driven pre-arm `stale`. This is not the composed maximize-under-fullscreen acceptance journey. |
| Ordinary tile | `Meta+Shift+G`, `Meta+Shift+G` | Two sticky command/dispatch pairs; all-desktop/above acquisition then return to single home/tiled mode; retain native facts and sibling frames. |
| Ordinary tile | `Meta+G`, `Meta+Shift+G`, `Meta+Shift+G`, `Meta+G` | Float, sticky, prior float, then tiled; retain callback/results and native facts. At rejection, retain exact UI message/time and whether float dispatch or clear/stale work exists. |

- Stop with Ctrl-C, then save the cursor-bounded journal using the retained
  verified PID assigned to `KWIN_PID`:

  ```sh
  journalctl --user --after-cursor "$CURSOR" _PID="$KWIN_PID" -o cat --no-pager
  ```

- A usable seed followed by `stale` requires inspection of the other
  authorization predicates; do not attribute every stale drop to storage.
  Trace still does not promise target values, command sequences, attempts or
  terminal results from Planner summaries. Preserve any unresolved sticky/
  unfloat refusal for the separate diagnostic unit. No bus monitor is required.
- These are the three traced diagnostic sequences established by the
  [physical triage](../v4-phase2-kde-state-targets.md#discriminating-capture-and-proposed-correction-units),
  not acceptance of the unrun original ST steps 4-6 or the remaining storage,
  recovery and menu legs.

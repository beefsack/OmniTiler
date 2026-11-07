# KDE command ingress stall

## Goal and scope

- Fix the command-routing stall after a numbered workspace send without changing core completion semantics or the v4 wire contract.
- Check every adapter command builder with a production caller against the real v4 decoder and correct existing mismatches.
- Retire invalid commands at the FIFO head so later commands can proceed.
- Keep resize diagnostics and native arrival settlement separate.

## Evidence and acceptance

- User-observed: multi-output tiling worked well before a workspace move stopped shortcut progress. This is not multi-output acceptance evidence.
- The supplied log records repeated `send-numbered` ingress rejections (`snapshot-invalid`, `command-invalid`), later focus commands accumulating, and continuing capture/service replies. It does not establish a KWin crash or a transfer-arrival hang.
- Establish the builder/decoder mismatch and FIFO cause in source and regression tests.
- Exercise every command builder with a production caller through `decode_command_v4` in one regression; leave uncalled legacy interfaces unchanged and unverified.
- Prove an invalid FIFO head cannot prevent subsequent valid commands or capture progress.
- Run full adapter checks and proportionate Rust verification, with no new skips.

## Approach and bounded units

1. Investigate and fix existing adapter command construction and invalid-head handling; add production-path regressions.
2. Inspect the diff and run offline gates. Native arrival remains a separate verification surface.
3. Record accepted evidence and remaining arrival limitations separately.

## Accepted outcome and evidence

- Source-confirmed cause: numbered and relative sends used retired implicit source `home-then-cached-live`; the decoder accepts only `home-then-action`. Queue-time `snapshot-invalid` / `command-invalid` rejection then reinserted the same malformed FIFO head indefinitely.
- All three production implicit-send call sites now use `home-then-action`; the adapter type no longer allows the retired token. No other called-builder mismatch was found.
- Queue rejection drops the head only for the exact `snapshot-invalid` / `command-invalid` pair emitted by the broker's queue-time shape gate. Transient refusals, generation changes, and ambiguous transport retain their existing recovery paths.
- `kwin/tests/world-command-ingress-stall.test.ts` exercises every command builder with a production caller through `raw_world_eval` and the genuine strict decoder, using sequenced MiniBroker captures. It includes the native toggle-float callback and checks that each tested command actually rides its report.
- The poison-head regression mutates a filed send at the transport seam to the retired token, proves those bytes fail genuine decoding, and relays the documented production queue rejection mapping. Later focus and select captures are accepted, and commandless observation advances revision with a nonempty inventory. This composes the real decoder with a queue shim, not a live broker or KWin test.
- Direct verification: `npm run typecheck` and full `npm test` pass (985 tests, zero skips). Scoped `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture --nocapture` passes. No new skips were added.
- Independent source review found no blocking correctness issues; the narrow shape-rejection drop preserves unrelated recovery paths.
- Test setup initially failed on unused bindings, timer/capture draining, and discarded accumulated replies. These harness defects were repaired before the accepted gates; they were not native or product failures.

## Deferred interfaces and limits

- Uncalled and unverified by this regression: `cmdSelectWorkspace`, `cmdSend`, `cmdSetWorkspaceTiling`, `cmdMigrateWorkspace`, `cmdSwitchView`, `cmdFocus`, and `explicitSource`. The unconstructed `operation` union arm also remains unchanged and unverified.
- Source inspection establishes that uncalled `cmdFocus` still carries a v4-removed direction key, and `cmdSwitchView` plus the `operation` arm name removed aliases. Their cleanup is separate from this stall fix.
- No live KWin mutation was performed for this increment. The offline reproduction needs only one output; multi-output acceptance is not claimed.
- Native arrival settlement and resize diagnostics remain separate unfinished work. This fix establishes command-ingress liveness, not completed workspace arrival.

## Re-test

- Move a window from workspace 1 to 2, then exercise focus, workspace selection, and ordinary tiling again. Numbered and relative sends must no longer repeat `command-invalid` or block later shortcuts. Record any remaining native-home or arrival issue separately.

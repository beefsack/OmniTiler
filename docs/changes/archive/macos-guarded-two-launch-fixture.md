# macOS guarded two-launch fixture controller

## Contract

The signed `fixture-restart` harness must retain one authenticated fixture/window lifetime and one private intent store across two separately identified, non-overlapping product launches. A clean terminal outcome and independent disappearance of launch one's exact PID/lifetime are separate requirements. Recheck the fixture join, topology, current owned Space and store fingerprint before launch two. Stop rather than actuate when identity or ownership cannot be established. Teardown must use exact ownership for windows, processes and Spaces, and preserve and report the private store and launch logs as residue whenever product absence is unproven.

M3 captures independent AX and CG frames at first settlement and second hydration, before new input to launch two. Float rectangles match by exact window and bounds; tiled rectangles match as multisets, not by slot. The isolated store matches by bytes and metadata. M4 first establishes the base, then a distinct composed overlay. The overlay must still be in place after launch one stops, and the tiled base, not the overlay, must be observed after launch two starts. A native AX zoom cannot satisfy the overlay check. No product behavior changes.

The restart leg establishes its owned Space before spawning windows. A matching
M3 frame capture alone is insufficient: product stop leaves geometry in place,
so require real launch-two application as well as an exact persisted binding
for each intended float. M4 has no durable float intent; unchanged absence of
a row is distinct from hydrating a row.

## Outcome

### Offline evidence

The original controller's offline gate passed 1,741 library tests (3 ignored), 152 integration tests, and strict library/binary Clippy.

Hardening regressions exercised the embedded timeout policy, pending and unreaped product cleanup, and stale first-launch terminal evidence. The focused `fixture_frame::tests` and `live_leg::tests` runs passed 8 and 31 tests respectively.

The restart suite passed 27 tests after the module split. Behavioral tests exercise overlap and missing terminal proof, a fresh guard refusing store drift, frame-write failure or timeout stopping before launch two, cleanup on an unbound second launch, and finalization exactly once. The nested interrupt regression checks that no key edge posts after the outer run's interrupt is latched.

The integrated `cargo test --locked -p tiler-macos --lib --test integration` gate passed: 1,766 library tests passed (3 ignored), 152 integration tests passed, and none failed.
Strict `cargo clippy --locked -p tiler-macos --lib --bins -- -D warnings` passed. Strict integration-target Clippy failed on the nine pre-existing `dead_code` diagnostics in untouched `crates/tiler-windows/src/log_sink.rs`; this is a baseline exception, not a Clippy pass.

### Implemented design

- M3 proves exact persisted float rows after the first stop: one row per intended float, decoded `float:true`, bound to the fixture PID, bundle and exact launch lifetime, with no unexpected or duplicate float row. A full guard (clean terminal evidence, prior lifetime gone, no overlap, signed fixture join and owned current-Space topology, exact store fingerprint) precedes the one effect. Between products, with no product alive, one authenticated signed-fixture AX frame write nudges a known tile by a fixed inset; the writer's commit boundary rechecks the fixture lifetime and owned Space, and independent AX and CG reads must show exactly the requested target frame with every other window unchanged. A fresh full guard then precedes launch two. After hydration the exact pre-nudge AX/CG floats and tile-rectangle multisets are restored (tile slots may swap), so the return is non-vacuous.
- M4 performs no nudge. The overlay must change the target and both rectangle multisets relative to the base, then survive launch one's stop unchanged per window. After launch two, the same caller/window IDs must occupy the exact base AX and CG rectangle multisets, with the overlay target no longer in its overlay frame. Exact slot assignment is not the contract. Record raw target `AXZoomed` at the base, overlay, inter-launch capture and hydration; `true` at any of those points refuses. An unchanged empty store (absent or directory-only) is valid; any intent row refuses.
- The controller resolves the fixture bundle before allocating launch state and records a pending fixture only at `open`. A pending product can be cleared only by independently proving the PID from its matching `tilestarted` announcement gone. An unproven product leaves launch logs and the private store in reported residue; launch one's terminal outcome cannot stand in for launch two's.
- The embedded frame writer returns an error at its cooperative deadline, allowing full owned teardown instead of exiting the process. Its wait for the AX worker is capped at the earlier of the worker limit and the remaining deadline. A timed-out write is not replayed; a synchronous AX call already in flight cannot be pre-empted.
- After launch one settles, the controller rechecks the exact owned current Space and every joined window's placement before chord input. A nested post cannot clear an interrupt latched by the outer leg. The display-awake companion budgets both product lifetimes plus 120 seconds for setup, the inter-launch proof, hydration and teardown (300 seconds for the documented runs).
- The store fingerprint checked at first stop, before the inter-launch proof, immediately before launch two and at the hydration capture is authoritative; no input is posted to launch two. A readable fingerprint that changes after the second stop is reported but does not invalidate the hydration proof, since a clean shutdown may rewrite the same logical row. An unreadable snapshot still refuses. Absence of the store is explicitly labelled `unchanged-absence`, never reported as hydrated rows.
- No product behavior changed. No native launch was performed and there is no native evidence for this controller.

### Later signed run

The build, sign, verify and launch recipes are in the [macOS live-testing guide](../../live-macos-testing.md). After its preflight and permission gates, replace the `DESIGNATED_REQUIREMENT` placeholder with the complete `designated => ...` requirement printed by `just fixture-verify`, then run:

```
mise exec -- just launch --timeout 300 -- fixture-restart --mode m3 --windows 4 --floats 2 --seconds 90 --preset compatible --signer 'DESIGNATED_REQUIREMENT'
mise exec -- just launch --timeout 300 -- fixture-restart --mode m4 --windows 2 --seconds 90 --preset compatible --signer 'DESIGNATED_REQUIREMENT'
```

For either command, require `outcome:completed`, independently verified absence and clean terminal evidence for each product stop, clean teardown, `restart.launch2.post_input:false`, and an unchanged store fingerprint at hydration capture.

- **M3:** Require `m3-exact`. The first stop leaves exactly the intended durable float bindings and the captured pre-stop arrangement. With no product running, one signed AX write must move the selected tile to an inset that is not one of the original tiles; independent AX and CG observations confirm the nudge without moving any other window. The second launch restores each float's exact frame and both exact tile-rectangle multisets, so an unchanged nudge cannot pass.
- **M4:** Require `m4-base-restored`. Launch one applies a distinct composed overlay, and a capture between the products proves that overlay is still in place. Launch two restores the exact tiled base in AX and CG before new input. Native `AXZoomed:true` or the presence of an intent row is a failure, not successful hydration.

These commands are for a later signed run. Offline tests exercise guard and cleanup decisions but cannot establish native M3 or M4 behavior.

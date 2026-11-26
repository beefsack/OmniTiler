# macOS guarded two-launch fixture controller

## Contract

The signed `fixture-leg` harness must retain one authenticated fixture/window lifetime and one private intent store across two separately identified product launches, never overlapping them. A clean terminal outcome and independent disappearance of launch one's exact PID/lifetime are separate requirements. Recheck the fixture join, topology, current owned Space and store fingerprint before launch two. Stop rather than actuating when identity or ownership cannot be established; use existing exact-owner teardown for windows, processes and Spaces, and delete the owned private store on every exit.

M3 captures independent AX and CG frames at first settlement and second hydration, before new input to launch two. Float rectangles match by exact window and bounds; tiled rectangles match as a multiset, not by slot. The isolated store matches by bytes and metadata. M4 first establishes the exact base, then an active composed overlay; after restart the arrangement must be the base, not the overlay, with the store unchanged. No product behavior changes.

The restart leg establishes its owned Space before spawning windows. A matching
M3 frame capture alone is insufficient: product stop leaves geometry in place,
so require real launch-two application as well as an exact persisted binding
for each intended float. M4 has no durable float intent; unchanged absence of
a row is distinct from hydrating a row.

## Units and acceptance

1. Implement and offline-test the restart lifecycle, comparison and fixture-leg CLI. Exercise overlap refusal, store drift, stop teardown, exact frames, swapped tile slots and mismatches.
2. Independently review lifetime, restart ordering and cleanup safety.
3. Run the offline macOS library/integration tests and strict lib/bins Clippy, then record evidence and publish only the scoped files. The first native run is separate evidence.

## Outcome

### Offline evidence

`mise exec -- cargo test --locked --offline -p tiler-macos --lib --test integration` passed: 1741 library tests pass with 3 ignored, 152 integration tests pass, zero failures. Each exact harness was prewarmed with `--list`, which listed 1744 library cases and 152 integration cases. `mise exec -- cargo clippy --locked --offline -p tiler-macos --lib --bins -- -D warnings` passed. An earlier Clippy run caught ungrouped guard arguments; grouping the fixture binding resolved it before the final gate.

### Implemented design

- M3 proves exact persisted float rows after the first stop: one row per intended float, decoded `float:true`, bound to the fixture PID, bundle and exact launch lifetime, with no unexpected or duplicate float row. A full guard (clean terminal evidence, prior lifetime gone, no overlap, signed fixture join and owned current-Space topology, exact store fingerprint) precedes the one effect. Between products, with no product alive, one authenticated signed-fixture AX frame write nudges a known tile by a fixed inset; the writer's commit boundary rechecks the fixture lifetime and owned Space, and independent AX and CG reads must show exactly the requested target frame with every other window unchanged. A fresh full guard then precedes launch two. After hydration the exact pre-nudge AX/CG floats and tile-rectangle multisets are restored (tile slots may swap), so the return is non-vacuous.
- M4 performs no nudge: it proves a distinct composed maximize overlay before the first stop and requires the exact pre-overlay base in AX and CG after restart.
- The store fingerprint checked at first stop, before the nudge, immediately before launch two and at the hydration capture is authoritative; no input is posted to launch two. A fingerprint read after the second stop is informational only, since a clean shutdown may rewrite the same logical row. Absence of the store is explicitly labelled `unchanged-absence`, never reported as hydrated rows.
- No product behavior changed. No native launch was performed and there is no native evidence for this controller.

### Later signed run

The build, sign, verify and launch recipes are in the [macOS live-testing guide](../../live-macos-testing.md). After its preflight and permission gates, replace the `DESIGNATED_REQUIREMENT` placeholder with the complete `designated => ...` requirement printed by `just fixture-verify`, then run:

```
mise exec -- just launch --timeout 300 -- fixture-restart --mode m3 --windows 4 --floats 2 --seconds 90 --preset compatible --signer 'DESIGNATED_REQUIREMENT'
mise exec -- just launch --timeout 300 -- fixture-restart --mode m4 --windows 2 --seconds 90 --preset compatible --signer 'DESIGNATED_REQUIREMENT'
```

Require a JSON envelope with `outcome:completed`, clean terminal evidence for both stops and teardown, no second input (`launch2.post_input:false`), mode verdict `m3-exact` or `m4-base-restored`, and an unchanged store fingerprint at capture. These commands are documented for later use only; no native launch or native evidence exists here.

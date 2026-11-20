# Expedite asynchronous KDE tiled crossings

The return from KWin's native output send is not always proof of arrival. Keep the existing exact-arrival, target-view and native setter guards, but let a newly proven arrival of the exact dispatched ordinary tiled mover advance the next capture without waiting for the native-event debounce.

Scope: KWin host and deterministic coverage only; no planner, wire, routing or gesture behavior change. Preserve synchronous same-apply placement and at-most-once transfer dispatch. Do not retry based on time alone.

Acceptance: testing must observe the mover's native proof signal causing a prompt capture and its reply issuing the final tiled frame; sibling/commandless/stale/gesture events must not promote capture. Compare exhausted moves and explicit sends on two asymmetric outputs with and without a destination sibling, including the synchronous reverse control. Check the new-window drift correction separately, and pass the Rust/TypeScript gates and frozen-fixture comparison.

## Outcome

After a successful ordinary tiled send returns without exact arrival, KWin can expedite one fresh capture when the mover itself signals and live reads prove the expected output and single native desktop. The hint is keyed by native lifetime and the dispatched transfer. It cannot write a frame or resend the window, and cannot bypass freshness, safety, sealed work or gesture handling. Synchronous arrival still uses the same apply.

In a fixture-scoped native comparison, the asynchronous direction reached exact arrival capture in 1.2–2.1 ms after the output signal, down from 106.8–119.5 ms. Three of four final frame requests followed in 27.4–30.0 ms, down from 121–126 ms in those cases. In the fourth, a sibling geometry change superseded the expedited reply; ordinary catch-up made the final request. All cases settled correctly and the synchronous reverse still requested its final frame in the transfer apply.

A controlled native same-size centre drift after exact setter readback was corrected. Neither the test fixture nor a separately launched terminal reproduced that drift spontaneously on admission.

Rust format, Clippy, serial tests, frozen-fixture comparison, TypeScript typecheck and the full KWin suite passed.

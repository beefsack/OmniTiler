# Shared Float Geometry

## Scope and acceptance

- Prove through separate `WorldEngine` reports that rect-less re-float restores remembered placement, not a different observed tile frame; removing the first-float retained-placement guard must break the regression. Distinguish macOS observed geometry from core's allocation in the existing offline adapter test.
- Preserve a native float's dropped frame and destination home on release onto a valid empty output and on the next quiet observation. No redundant frame write or token; keep held/unknown fences and occupied destination behavior. Composed maximize must converge to the same ordinary base.
- No protocol or adapter behavior changes. Native delivery is outside this change.

## Approach and verification

1. Add the first-float review coverage before touching drag behavior.
2. Reproduce the empty-output regression on the ordinary arm, then trace the observation/gesture seam and repair the existing shared behavior. The [prior record](../v4-phase1-core-protocol.md) rules out source-adopt plus membership-commit, clear-only, and restage-only approaches. Preserve native free-framed pointer behavior per [the drag decision](../../decisions/visuals.md#pointer-and-drag-behavior).
3. Run focused regressions, core and protocol suites, macOS lib and integration suites, formatting and scoped strict all-target clippy.

Ordinary floats do not bind gesture sessions. The unshielded sweep removes the source exception and admits an empty-destination float without a frame. The composed arm reaches the same sweep after dropping its overlay shield. Recovery belongs after successful destination convergence: adopt the observed native frame using prior core membership, retire the previous home's retained base and any superseded placement token, and leave membership to the existing observation path.

## Evidence

- First-float negative control: temporarily removing only the retained-placement exclusion made the new real-report re-float test return the newly observed tile `(404,8,388,544)` instead of the remembered float `(0,0,40,90)`. The guard was restored.
- With the guard restored, the re-float regression passes. The macOS real-core adapter test also passes with an explicit check that the native frame differs from the layout allocation.
- Empty-output negative control: before the fix, the paired regression fails on the ordinary release row: floating on `out-b`, frame authority `Untouched`, no frame token, but `effective_frame: None` instead of the observed drop `(900,50,400,300)`.
- The repaired core passes all 59 composed-maximize module tests, including exact ordinary/composed release and quiet geometry, the held/unknown output fences, and the existing occupied output route.
- Independent review caught a second ordering case: an `Updated` report can rehome membership before the composed release while its layout remains shielded at the start home. Adding that report reproduced the null frame on the composed arm. The destination sweep now accepts the gesture's retained start home as its source proof.
- Final `cargo test --locked -p tiler-core`: 2,793 passed across 21 suites, none failed or ignored.
- Final offline gates also passed: protocol 120 tests; macOS library 1,723 passed, 3 ignored; macOS integration 152 passed. Formatting and strict all-target Clippy for core, protocol and macOS passed.
- The initial macOS integration run exposed a focus-walk fixture race: manually injected arrival sequences could outrun the native worker's readback sequence and render a focus report stale. Arrivals now use the worker's observation stream and wait for application before the next chord. The focused test and full integration suite passed without changing the assertions or production routing.

## Limits

The shared-core outcomes and macOS adapter mapping are proven offline; physical pointer delivery and convergence on native macOS, Windows and KDE hosts are not established by these tests.
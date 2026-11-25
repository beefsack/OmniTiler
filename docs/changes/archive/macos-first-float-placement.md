# macOS First-Float Placement (Offline)

- **Status:** Complete (2026-11-25), offline verification only.
- **Source:** [macOS v4 open-decisions review](../macos-v4-open-decisions-review.md) Package 3, P1 — first-float placement verification, per [world-rendering.md](../../decisions/world-rendering.md#float-toggle-intent-and-output-targeting).
- **Boundary:** No native, fixture, or live leg. Signed-fixture proof remains. `docs/backlog.md` is orchestrator-owned; this note carries the slice's status wording.

## Goal

Close the decided gap between intent and placement: `ToggleFloat` carries no intent-time rectangle, and core resolves the execution-time placement. Before this change, a rect-less first float of a tiled window published the centered 60% work-area fallback, so macOS (which sends `float_rect: None`) would have moved the window on first float. The decided rule is in place at the current frame.

## Scope

- Keep the adapter's intent rectangle-free: `world_command_map::operation_command` continues to publish `ToggleFloat { float_rect: None }`; no macOS edit was needed and none was made.
- Core: a rect-less first float of a subject core already holds as a tiled leaf adopts that subject's current observed frame as its durable placement.
- Core: the existing sealed-cut lane for pre-existing ordinary floats is unchanged; the centered 60% fallback remains for subjects with no truthful current frame.
- Offline regressions: a core engine-level first-float placement test, the inverted same-batch cut test, and a real-core macOS regression through the production command and `consume_world_reply`.
- Documentation: placement rules, spec row, reference outcome, review/backlog status.

Non-goals: signed-fixture/native acceptance; Windows parity for fresh post-placement facts; sticky placement (its own lane keeps the centered fallback); any wire, handshake, capability, or layout-API change.

## Approach

- Two lanes in `world_run_v4`, keyed by what core already holds for the subject, both frozen from the pre-observation effective (graded) rows:
  - **First float (new):** any batch command is a rect-less `ToggleFloat`; a fact whose window core holds as a tiled leaf, has a readable rect, `fullscreen == Some(false)`, is not natively maximized, is not core-composed-maximized, has no retained float geometry, and has no pending settlement placement supplies `window → rect`. The lane needs no sealed focus cut: the toggle request and the fact arrive in the same observation, and the tiled frame is the truthful current frame.
  - **Pre-existing float (unchanged):** the commandless sealed-cut lane for ordinary floats, including its pending-placement filter and post-command re-filter.
- Injection resolves the execution-time subject once (`CurrentFocus` already resolved to a window), looks the subject up in the first-float lane and then the cut lane, and sets `float_rect` only on the owned dispatched copy. Explicit rectangles never enter either lane.
- A remembered placement outranks the observed frame: the first-float lane requires `retained_float_geometry_any(window).is_none()`, so float → unfloat → re-float restores the previous float placement instead of adopting the current tile slot. `WorldLayout::retained_float_geometry_any` is a read-only query beside `is_tiled_window_any`.
- Excluding fullscreen subjects preserves the never-capture-fullscreen-frame rule and leaves the deferred fullscreen reveal at the fallback; excluding native/core maximize and pending placements keeps published placements from fighting in-flight work; requiring a readable non-zero rect keeps the centered fallback when no truthful frame exists.
- Any executed float toggle retires the report's captured frame for its subject, regardless of outcome, so later commands in the same batch cannot reuse the pre-toggle frame.

## Units

1. Core placement lane, retained-geometry query, injection, in-batch invalidation.
2. Core regressions and the inverted cut expectation.
3. macOS real-core adapter regression through the production command and reply consumption.
4. Documentation and status.

## Accepted evidence

Offline gate at the final tree (2026-11-25):

- `cargo fmt --all -- --check` — clean.
- `cargo test --locked -p tiler-core` — PASS (lib 2,471 passed / 0 failed; integration targets pass).
- `cargo test --locked -p tiler-macos --lib` — **1,723 passed / 0 failed / 3 ignored**.
- `cargo test --locked -p tiler-macos --test integration` — **152 passed / 0 failed**.
- `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` — clean.

Focused regressions:

- Core: `world_rapid_input_tests::cut_tiled_frame_becomes_first_float_geometry_in_place` — a tiled subject's rect-less first float publishes exactly the observed tile frame (not the centered fallback), and the same-batch re-float restores that durable placement rather than re-adopting the pre-toggle frame.
- Core: the existing fullscreen case in `world_rapid_input_tests::cut_float_frame_comes_from_resolved_subject_until_its_float_state_changes` and `world_coverage_gaps_tests` keep the centered fallback for fullscreen subjects; the remembered-placement case keeps the retained frame for an unsealed snapshot.
- macOS: `world_dispatch::tests::macos_first_float_lands_at_current_tile_frame` — production `world_command_map::operation_command` command (asserted `float_rect: None`), real `macos_capabilities()` core and macOS-shaped facts; the `Float` result and the subject's `effective_frame` are the observed frame `(100,0,40,90)`, not the layout allocation or the centered fallback `(160,120,480,360)`; the subject row stays `Untouched` with the unspent token, and `consume_world_reply` stages only the sibling reflow frame with no deferral.
- macOS: `world_dispatch::tests::fullscreen_toggle_float_ordinary_first_float_stays_write_free` still passes (authority, token, counter deltas unchanged); its comment now says the placement is the execution-time frame.

## Material decisions

- The rule is global on `float_rect: None`, not capability-scoped: the decision says "no macOS exception", and no handshake discriminator exists that would scope it without a strict-schema amendment. macOS and KDE (which also sends a null rect with an explicit window) therefore land in place; Windows is unaffected because it sends an explicit rectangle.
- First float uses the observed frame only when it is truthful: fullscreen, native maximized, core-composed maximized, newly admitted (not tiled), remembered-geometry, and pending-placement subjects keep the fallback or remembered frame.
- The pre-existing-float cut lane and its sealed-focus-cut correlation are deliberately untouched: only the first-float class changes.
- An in-place first float needs no frame write, so its placement is proven by the next readable report instead of staying pending; the tests that asserted a pending token on that path now assert the proven in-place frame.

## Verification limits

- No native, signed-fixture, or live leg ran. The macOS proof uses the real core, the real capability handshake, the production command map, and the production reply consumer, but synthetic facts and replies.
- The signed-fixture proof of the live toggle (AX/CG frame read before and after, no subject write) remains open, as does Windows fresh-post-placement-facts parity.

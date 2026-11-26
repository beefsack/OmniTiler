# macOS First-Float Placement

- **Status:** Offline implementation (2026-11-25), followed by signed-fixture acceptance and a macOS re-float correction (2026-11-26).
- **Source:** [macOS v4 open-decisions review](../macos-v4-open-decisions-review.md) Package 3, P1 — first-float placement verification, per [world-rendering.md](../../decisions/world-rendering.md#float-toggle-intent-and-output-targeting).
- **Boundary:** Single-display signed fixture; Windows fresh post-placement facts remain separate.

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

- The rule applies to `float_rect: None`, not to a specific platform capability. KDE, macOS, and v4 Windows use it when initially floating a tiled window. Windows dispatch sends no rectangle on that path (`tiling_sys/dispatch.rs`, `float_toggle_command`); Windows native and test verification are tracked separately.
- First float uses the observed frame only when it is truthful: fullscreen, native maximized, core-composed maximized, newly admitted (not tiled), remembered-geometry, and pending-placement subjects keep the fallback or remembered frame.
- The pre-existing-float cut lane and its sealed-focus-cut correlation are deliberately untouched: only the first-float class changes.
- An in-place first float needs no frame write, so its placement is proven by the next readable report instead of staying pending; the tests that asserted a pending token on that path now assert the proven in-place frame.

## Verification limits

- The 2026-11-25 macOS proof used the real core, capability handshake, command map, and reply consumer, but synthetic facts and replies. The following signed-fixture proof is macOS-only; Windows fresh post-placement facts remain a separate parity check.

## Signed-fixture evidence (2026-11-26)

- `fixture-leg --chord float-placement --chord focus-h --seconds 75 --signer REQUIREMENT` drove the native tap on one controller-owned Space. The first rect-less float left the subject's exact AX and CG frames at the tiled preimage. The product logged zero frame-set attempts for the subject and one for the sibling, whose frame expanded. Ten tap edges matched and five commands forwarded.
- Before the adapter correction, retiling and then rotating the same subject created a demonstrably different tile slot. The next Ctrl+G reflowed the sibling again but left the subject at its rotated tile frame, not at the retained first-float frame. The controller stopped cleanly, restored the starting Space, and left no residue.
- The historical correction staged a one-shot core frame operation only on the current, matching rect-less `ToggleFloat` ticket when it carried a different frame for the visible subject. The real-core regression first failed with only the sibling staged (one row instead of two).
- The corrected signed leg repeated the same sequence with ten matched/suppressed edges and five forwarded inputs. First float: zero subject setters and one sibling setter. Re-float: one subject setter and one sibling setter, with the subject's native AX and CG positions and sizes returning exactly to its first-float bounds, not the rotated tile. Product shutdown was clean and restored the starting Space without residue or a retained receipt.

The ticket-specific gate has since been replaced by the retained v4 float-frame
lane. An ordinary snapshot without an operation still authorizes no writes,
and an in-place first float still requires no setter. The signed evidence
above predates that replacement; latest-world recovery, composed float
unmaximize, and directional float half-snap need their own native proof.

## Retained float-frame signed-fixture retest (2026-11-27)

- With `7e462765` present, the first float kept the exact tiled AX/CG
  frame `(1124,38,1108,1123)` with zero subject setters and one sibling
  setter. Retile/rotate placed the subject at `(8,603,2224,558)`.
  Re-float restored the exact remembered AX/CG frame with one subject
  setter and one sibling setter. The next two observations caused no
  duplicate setter. Owned Space and product cleanup completed with no
  residue or retained receipt.
- The issuing reply was consumed normally. The signed controller cannot
  suppress it, so observation-only recovery after a held/skipped reply
  remains offline-only. Composed float unmaximize and directional float
  half-snap also remain offline-only: the existing aliases cannot deliver
  a multi-action sequence to the same floated window.

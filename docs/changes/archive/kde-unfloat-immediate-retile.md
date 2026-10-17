# KDE immediate unfloat re-tile

## Goal and scope

Fix Meta+G unfloat on the full-state reply route so the focused window
immediately occupies its tile, without requiring a later focus change.
Preserve existing float behavior and host safety fences. No new product
behavior or live acceptance is included.

## Acceptance

- Diagnose the reported sequence from existing development logs and source.
- Add a producer-shaped regression that fails before the correction.
- Apply the smallest causal core/KDE correction and document any learned host
  shape at its point of use.
- Pass the Rust workspace tests and KWin suite; retain exact pass counts.
- Record the offline result and the remaining user-owned native re-check.

## Delivered correction

- Settle intentional-float departures and explicit unfloat tile overrides before
  the post-accept capture in `kwin/src/world-entry.ts`. This lets the same reply
  apply the tile frame, including for fixed-size clients.
- Keep float arrivals and re-float override withdrawal after capture: explicit
  float placement still needs the pre-command non-floating classification.
  No extra enumeration, duplicated classifier or relaxed safety gate was added.
- The ordering invariant is documented at the point of use. No core, protocol,
  Windows or macOS changes were required; no product decision changed.

## Evidence and outcome

- User report (2026-10-17): with two tiled terminals, float succeeds; unfloat
  opens the correct gap but leaves the toggled window at its floating frame
  until focus moves to the other terminal.
- Source diagnosis: the accepted reply clears intentional-float membership and
  records explicit tile overrides after the fresh observation is captured.
  Application still sees stale floating/fit-excluded flags and holds the tile
  frame; the sibling reflows. A later capture clears the stale flags.
- Existing log summaries show accepted revisions with geometry writes and holds,
  consistent with the source diagnosis; they contain no per-window payload
  proving which frame was held.
- Full-state route context: [record](core-full-state-replies.md).
- `kwin/tests/world-unfloat-retile.test.ts` drives the production entry with QML
  array-likes, renewed native wrappers, braced UUIDs, native property names and
  Qt zero/unbounded hints. Before the production correction, both unfloat tests
  failed at the retained floating frame; the fullscreen guard test passed.
  After correction, all three pass. The ordinary sequence asserts both float
  placement and immediate unfloat with no intervening event, plus no writes on
  a converged repeat. The fixed-size case proves the immediate write attempt;
  a native client may still clamp its requested tile frame.
- Final offline gates (2026-10-17): `cargo test --workspace` **2888 passed,
  0 failed, 80 test-result groups**; KWin `npm test` **1772 passed, 0 failed,
  266 suites**; KWin `npm run typecheck` and `git diff --check` pass.
- Native acceptance remains pending; offline geometry setters do not establish
  native compositor/client behavior.

## User re-check

1. Load the updated script with `just dev-off`, then `just dev` (foreground)
   or `just dev-on` (detached). `just reload` replaces only the Planner and
   does not load this script correction.
2. On one workspace with two tiled terminals, focus one and press Meta+G.
   Confirm it floats and the other terminal fills the available tile area.
3. Without changing focus, press Meta+G again. Confirm the toggled terminal
   immediately occupies its tile and the other terminal reflows; no vacant
   tile should remain waiting for a focus change.
4. Repeat the toggle pair, then change focus. Confirm there is no delayed snap
   into a previously vacant tile.

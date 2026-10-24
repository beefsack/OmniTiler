# Integrated Plasma Structural Feasibility

## Goal

Resolve offline whether KWin's native Custom Tile structures can carry the
recorded authored-layout workflow and should become this project's layout
authority.

## Scope And Evidence

- Source-only API and package research is retained in
  [structural research](../../research/integrated-plasma-structural-feasibility/).
- The recorded scope is persistent authored topology, placement/preselection,
  drag/split/cancel, workspace/output rebinding and structural indication:
  [ten capabilities](../../research/integrated-plasma-structural-feasibility/kwin-api-surface.md#capability-verdict-map).
  The [single-script composition](../../research/integrated-plasma-structural-feasibility/package-composition.md#recommendation-for-unit-03)
  was a proof carrier, not a production architecture or packaging selection.
- Existing research pins KWin `45ec9a6d` (6.7.3). This verdict checks the relevant
  source at the requested `8438567a`; inspected local blobs under
  `upstream KWin source at pinned commit 8438567a` matched revision-pinned GitHub raw SHA-256 values.
  Older line references remain historical, not citations to the newer pin.
- Offline reads only. No live action, diagnostic, harness run or residue work.
  Attempt-01 artifacts (never retained in-repo; no `results/` directory exists under `docs/changes/archive/integrated-plasma-structural-feasibility/`) are
  inadmissible to any verdict; the
  [staged protocol](../../research/integrated-plasma-structural-feasibility/proof-protocol.md)
  is unexecuted design, not safety or acceptance evidence.

## Offline Verdict - 2026-10-09

- **B: structurally plausible only as version-coupled KWin research; park native
  Custom Tiles as production layout authority.** Root lookup and structural
  mutation/association routes exist, but a complete supported, stable and
  runtime-safe workflow is not established. Source does not prove technical
  impossibility, runtime safety, gaming compatibility or performance acceptance.
- **Recommendation:** retain the selected portable Rust Engine and KWin direct
  geometry. Replacing Engine topology authority would revisit the approved
  [architecture](../../decisions.md#engine-architecture-and-convergence);
  mirroring it into native trees would add the second authority explicitly
  excluded by [Engine Operations](../../decisions.md#engine-operations-and-policy).
  No product direction changes follow from this verdict.
- [Simplicity](../../principles.md#simplicity): native trees still require product
  admission, empty-branch, transfer and recovery policy plus version retesting;
  they do not remove the portable policy requirement. [Resilience](../../principles.md#resilience):
  deferred tile destruction, unacknowledged persistence and lost cross-output
  associations add failure/recovery obligations; the recorded crash-class
  [Custom Tile findings](../../live-kwin-testing.md#custom-tile-safety-findings)
  preclude treating collapse as cleanup. [Gaming](../../principles.md#gaming-compatibility):
  post-placement overrides and built-in drag assignment have no retained proof
  of harmlessness or imperceptible cost. Neither backend gains gaming acceptance
  from source inspection.

### Open-Question Classification

- "Answered offline" below establishes API/implementation facts only.
  "Requires live" identifies evidence missing for a revived Custom Tile path;
  these cases are moot for current production under the decisions above.
  Candidate cases are user-owned evidence requirements, not executable approval.

| Recorded question | Classification and offline answer at `8438567a` | Minimal remaining user-owned live case, if revived |
| --- | --- | --- |
| 1. Correct root by workspace/output | **Answered offline:** documented `rootTile(output, desktop)`; manager roots are per desktop. Deprecated manager lookup is unnecessary. [root], [timer] | None for API reachability. |
| 2. Leaves, geometry, identity and parents | **Answered offline:** QObject properties expose children/parents/geometry/windows, not stable persistent tile IDs. Handles cannot be durable identity. [tile], [restore] | Binding/handle checks below, not an ID guarantee. |
| 3. Arbitrary split and ratio | **Answered offline:** invokable split plus writable relative geometry; ratio changes obey minimum-size and adjacency constraints. [split], [ratio] | One horizontal and one vertical split with constrained ratio readback; no removal in that run. |
| 4. Persist/restore authored topology | **Requires live:** 2 s save timer, internal desktop/output JSON, no exposed flush/save acknowledgement; window associations are not serialized. Pure direction change emits no `layoutModified`. [timer], [persist], [direction] | Fresh isolated scope: change geometry, separately direction-only, then compare in-memory and saved topology and reload; include single-child, floating and minimum-size cases. A timed wait alone is not proof. |
| 5. Keyboard preselection | **Requires live:** compose shortcut, `windowAdded`, `manage`; Wayland placement precedes `windowAdded`, so assignment is a post-placement override. [add], [manage] | One eligible native Wayland client after one armed callback; record chosen association, acknowledged frame and visible intermediate placement. Physical input separately proves delivery (old T9/T9b only invoked the callback). |
| 6. Drag target, split and cancellation | **Requires live:** hit-test exists, but finish has no cancel flag; built-in Shift/edge assignment precedes the finish signal. [pick], [finish] | Separate owned drag-to-empty-leaf and drag+Esc cases; compare association, tree, frame and focus before/after, with built-in assignment distinguished (T2/T7). |
| 7. Tile association versus raw geometry | **Answered offline:** `manage`/`unmanage` are invokable; raw frame writes do not create association. `window.tile` reads requested state, not acknowledged commit. [manage], [window-tile], [commit] | One association followed through Wayland acknowledgement; a boolean return/requested tile is insufficient. |
| 8. Branch preservation, close, empty leaves and collapse | **Answered offline:** placement does not author trees; close unmanages, empty leaves persist, explicit remove can promote a non-root single-child layout and defers destruction. This is not stability evidence. [placement], [close], [remove] | Close one owned client and verify retained empty leaf (T3a/T3b/T5a). Collapse (T5b) is a separate crash-class case only after a fresh safety design; never remove then split in one run. |
| 9. Workspace/output rebinding and reconnect | **Answered offline:** trees are scoped by desktop/output. Explicit cross-output transfer forgets Custom association; unplug recovery can conditionally restore Custom mode by geometry hit-test, not durable tile identity. [timer], [restore], [transfer], [unplug], [custom-pick] | One desktop out/back (T6); separately transfer one owned client across two outputs and verify fresh target association. Disconnect/reconnect must check the conditional restore and selected leaf separately. |
| 10. Separate Plasma structural indicator | **Moot under current decisions:** no supported external tile-state interface established; in-process script push or a source-coupled native bridge adds composition. Pager only indicates workspaces. [indicator], [headers], [registration] | Only if selected later: one structural change through a chosen bridge with exact revision/ordering readback; no indicator is selected here. |

### Remaining Research And Protocol Gaps

| Open question | Classification and minimal evidence |
| --- | --- |
| QJSEngine/QML enum/rectangle conversion, association acknowledgement, stale handles and repeated structural calls | **Requires live if revived:** fresh root decode, one split/readback and one acknowledged association; removal-only invalidation/recovery belongs to a separate crash-class case. No fixed timer is a deletion barrier. [binding research], [remove], [commit], [live guide] |
| Script/package lifecycle and restoration (T8; protocol T1/T2 cleanup invariants) | **Requires live if revived:** exact returned script ID load/run/unload with absence/restoration readback; separately install/upgrade/remove one disposable package and compare owned files/config. The old package protocol is not current authorization. [package risks], [protocol], [live guide] |
| Nested isolation, private bus/KGlobalAccel, parent socket, teardown and hotplug | **Moot for this offline verdict; requires live for any future reusable test route.** The historical [nested spike] did not establish these. The current [live guide] records a proven launcher shape but explicitly only checkpoint isolation proof; it does not validate this structural journey. A fresh private-environment ownership/setup/teardown proof must precede one structural case; isolated shortcuts and hotplug each need their own case. Unsafe nested path and old retries stay stopped. |
| Gaming/noninterference and sustained cost | **Requires live if revived:** user-owned without-tiler comparison for a normal-to-fullscreen/borderless game transition, checking frames, focus, effects and physical input plus incremental frame-time/input/CPU cost. Ordinary-client success is not game acceptance. [gaming principle] |

- The [old T-case matrix](../../research/integrated-plasma-structural-feasibility/proof-protocol.md#11-test-matrix-mapped-to-stages)
  is fully covered above: T2/T7 drag; T3a/T3b placement; T4 persistence;
  T5a/T5b empty/collapse; T6 desktop; T8 lifecycle; T9/T9b preselection.
  Its reversal invariants require authoritative before/after proof too.
- Source correction to historical capability 4/9 research: the unplug
  `None`/stored-`Custom` check permits restoration; it does not exclude it.
  The restore call reselects by stored geometry center. This is distinct from
  explicit transfer loss and proves neither identity continuity nor live success.
  [unplug], [custom-pick]
- No specific live probe is prerequisite to this offline B verdict. Reopening
  native layout authority requires a user scope/architecture choice first,
  then a fresh safety design and separately authorized user-owned cases under
  the current live guide. Do not execute the retained harness as a next step.

### Citation Key

- KWin source links below are pinned to `8438567a`; research links retain their
  original pins and conclusions as historical review input.

[root]: https://github.com/KDE/kwin/blob/8438567a/src/scripting/workspace_wrapper.h#L382-L401
[tile]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/tile.h#L27-L48
[timer]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/tilemanager.cpp#L57-L95
[restore]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/tilemanager.cpp#L288-L340
[split]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/customtile.h#L20-L48
[ratio]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/customtile.cpp#L53-L257
[persist]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/tilemanager.cpp#L342-L405
[direction]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/customtile.cpp#L416-L424
[add]: https://github.com/KDE/kwin/blob/8438567a/src/workspace.cpp#L911-L954
[manage]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/tile.cpp#L377-L437
[pick]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/customtile.h#L60-L74
[finish]: https://github.com/KDE/kwin/blob/8438567a/src/window.cpp#L1069-L1110
[window-tile]: https://github.com/KDE/kwin/blob/8438567a/src/window.h#L590-L595
[commit]: https://github.com/KDE/kwin/blob/8438567a/src/xdgshellwindow.cpp#L849-L855
[placement]: https://github.com/KDE/kwin/blob/8438567a/src/placement.cpp#L34-L58
[close]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/tile.cpp#L28-L37
[remove]: https://github.com/KDE/kwin/blob/8438567a/src/tiles/customtile.cpp#L273-L342
[transfer]: https://github.com/KDE/kwin/blob/8438567a/src/window.cpp#L3890-L3903
[unplug]: https://github.com/KDE/kwin/blob/8438567a/src/placementtracker.cpp#L93-L134
[custom-pick]: https://github.com/KDE/kwin/blob/8438567a/src/window.cpp#L3677-L3702
[headers]: https://github.com/KDE/kwin/blob/8438567a/src/CMakeLists.txt#L478-L690
[registration]: https://github.com/KDE/kwin/blob/8438567a/src/scripting/scripting.cpp#L693-L719
[indicator]: ../../research/integrated-plasma-structural-feasibility/kwin-api-surface.md#10-expose-read-only-structuralcurrent-window-state-to-a-separately-packaged-plasma-panel-indicator-through-a-supported-interface
[binding research]: ../../research/integrated-plasma-structural-feasibility/kwin-api-surface.md#residual-uncertainties-source-level-by-design
[package risks]: ../../research/integrated-plasma-structural-feasibility/package-composition.md#residual-risks-and-uncertainties
[protocol]: ../../research/integrated-plasma-structural-feasibility/proof-protocol.md
[nested spike]: ../../research/integrated-plasma-structural-feasibility/nested-kwin-feasibility.md#verdict
[live guide]: ../../live-kwin-testing.md
[gaming principle]: ../../principles.md#gaming-compatibility

## Delivery And Verification

- Two sequential units: research, then independent evidence review.
- Acceptance: cited offline verdict, all recorded capability/residual questions
  classified, unsafe path stopped, production decisions preserved, backlog
  disposition explicit; documentation diff/link inspection and `git diff --check`.
- Verified: local links/anchors and source logic inspected; independent review
  accepted the verdict and unplug correction; `git diff --check` clean.
  Outcome: offline verdict delivered; closed by User decision 2026-10-09
  (see Closure below).

## Closure 2026-10-09

- User decision 2026-10-09: CLOSED with the offline B verdict above. Rust Engine
  authority and direct geometry confirmed; KWin Custom Tiles not adopted as
  layout authority. Reopen only on a material KWin change. Attempt-01 files are
  archived evidence, not pending live checks; no live mutation was run.

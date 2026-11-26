# macOS and core offline review rectification

Correct the reviewed MX6 inventory, macOS explicit float-frame application, and shared float-handoff defects. Keep existing inventory authority, first-float behavior, and v4 wire rules. Promote confirmed 0.1 decisions and correct stale change records without claiming new native acceptance.

- MX6: scope CG liveness to roster candidates. Exercise auxiliary CG owners with the sole hidden fixture, incomplete CG, and unknown SLS.
- Explicit float frames: use the retained v4 operation lane; verify skipped/held issuing replies, unmaximize, half-snap, one-time application, and initial no-write float.
- Core: preserve composed maximize on refused release and preserve observed float placement and retire source state when a native re-home lands in an occupied destination.
- Records: MX6 diagnosis, historical test counts and phase status, and approved 0.1 decisions. Sticky gate checks 6 and 32 remain unchanged pending a live probe.

Verify red before each product repair, then run the core, macOS, world-sequence equivalence, and strict Clippy offline gates. Run affected adapter tests if shared fixtures change. Native MX6 and float follow-up remain separate from offline verification.

MX6 was reproduced with two auxiliary CG-owned windows in the genuine adapter
matrix: a fresh hidden home emitted `applied=1` but the reply still carried
`held-incomplete-inventory`. Removing the redundant raw-owner scan cleared it;
the unreadable-CG and unknown-SLS legs still held. No core guard changed.

All three adapter regressions staged only the tiled sibling on the old
ticket gate: skipped-issue re-float, composed-float unmaximize, and
directional half-snap. The uniform retained operation lane staged the
exact subject frame once in each case; first float stayed write-free.

The core regressions proved that a refused existing-layout release lost its
composed bit and that an occupied-destination re-home left stale placement at
its held source. Clear the bit after a successful move (keep the distinct
empty-destination restage behavior), and adopt the observed destination
placement per mover after convergence. Retire only that mover's source
exception, geometry, and provenance, with no compensating frame operation.
KDE and Windows already supply the required native frame and output facts.

## Offline verification

- `tiler-core`: 2,517 unit and 320 integration tests passed; no doctests.
- `tiler-macos`: 1,744 library tests passed (3 ignored); 152 integration
  tests passed. No failures.
- `omnitiler` world-sequence equivalence: 3 passed.
- Strict all-target Clippy for `tiler-core` and `tiler-macos` exited 101:
  the macOS integration dev dependency exposes the nine pre-existing
  non-Windows `tiler-windows/src/log_sink.rs` dead-code diagnostics.
- No shared adapter fixtures changed.

The historical test count at `abb31e92` is 2,835, distinct from this run.
The 0.1 policy selections and sequencing now live in the owning decision
records; the sticky probe and the original checks 6 and 32 remain open.

## Native proof still required

- MX6: use the signed fixture's `pager-switch` on a controller-owned
  occupied-to-empty Space round trip. Correlate verified current Space,
  CG completeness/owner IDs/bounds, fresh SLS membership, roster home,
  re-home result, and indicator generation/current ordinal; inspect the
  painted highlight. Require no setter on the hidden fixture and clean
  owned topology restoration. Keep unrelated multi-display work separate.
- Float lane: rerun signed `float-placement` (exact AX/CG frame and zero
  subject setter on first float; retile/rotate, then restore the remembered
  frame with one subject setter on re-float). Separately demonstrate that an
  unconsumed or held issuing reply leaves the token available for the next
  accepted observation-only reply. On an ordinary focused float, check
  composed maximize/unmaximize back to the exact base and directional
  half-snap to the core-selected work-area half. Correlate operation ID
  and subject setter count with readback; later observations must add
  no duplicate setter.

# Focus

### Automatic focus guards
- Decision: core withholds automatic focus if newcomer focus would activate over a different focused floating/sticky window; automatic focus never targets fullscreen, sticky, floating or excluded rows except admission focus for ordinary float not fullscreen/maximized/sticky; newcomer focus never goes behind fullscreen/maximized sibling in same domain. Explicit focus and send-follow are exempt. Withheld effect is spent, not retried.
- Why: one cross-platform rule replaces KDE adapter guards; follows D01 and Gaming Compatibility.
- Rejected: KDE-only adapter guards; moving only D01 fence.
- Assumes / revisit if: hosts report maximize/fullscreen truthfully; Windows maximize reporting and Windows/macOS current behavior unchecked.
- Status: active, 2026-10-30
- Evidence: [review R5, F10](../changes/core-interface-review.md).

### Workspace active-window memory
- Decision: core remembers lifetime-bound floating, sticky and tiled identities in one workspace slot. Restore eligible remembered target first without float/tile preference, then existing tiled MRU/layout fallback. Accepted focus choices record before native effects; exception observations use tiled-observation safeguards. Sticky focus records only in workspace selected/observed focused, not every view where visible.
- Why: not recorded.
- Assumes / revisit if: unattributed foreground changes may replace intended memory even after unfinished arrival; this residual remains with unknown-foreground policy. Windows dispatching sticky away from home requires fresh lifetime/visibility proof and controlled home/shown scopes on same output; native homing/foreground vetoes remain authoritative.
- Status: active, 2026-10-28
- Evidence: [outcome](../changes/archive/windows-floating-return-focus.md).

### Command-owned linked-focus recording
- Decision: reuse existing session-binding gates; if recording returns false, preserve accepted effect without retry or promised repair. Observation/departure ingestion stays unchanged.
- Why: not recorded.
- Assumes / revisit if: existing binding gates remain sufficient.
- Status: active, 2026-10-28
- Evidence: [record](../changes/windows-workspace-return-focus.md#command-owned-memory-outcome-2026-10-28).

### Cross-output directional focus
- Decision: crossing output boundary selects a destination linked member nearest the crossed edge, not destination MRU; shared-core policy on all adapters, no setting. Candidates include nested leaves touching the canonical projected entry edge; most recently focused wins. Without history, greatest overlap against source perpendicular span, then DFS child order. Never use observed frames. Fullscreen and maximized destination members are eligible and retain their overlay state; they rank by retained allocation. Eligibility/safety gates are unchanged. Destination internal tiled last-active then focus stack newest-first; only currently linked/homed candidates; stale closed/floated/moved entries miss. Admission/convergence seed history. Preserve workspace-return lifetime slot.
- Why: output crossing should reach an eligible overlay member without unmaximizing or exiting it.
- Rejected: excluding fullscreen/maximized destination members or changing their overlay state on focus.
- Assumes / revisit if: no-focus origin uses source output usable-area span; all four crossing directions retained, focused tiled crossing horizontal-only. Float-origin stays local; empty/ineligible destination no-op without clearing state. Internal float-crossing remembered-target proposal is not newly enabled.
- Status: active, 2026-11-01
- Evidence: [offline evidence](../changes/archive/cross-output-edge-focus.md), [discriminator](../spec/reference-outcomes.md#cross-output-focus-edge-discriminator), [R-OUT-11](../spec/reference-outcomes/multi-output.md#r-out-11-directional-focus-between-fullscreen-or-maximized-outputs) sway destination-fullscreen target; Ours KDE/Windows selected outcome, [REQ-FOC-07](../spec/functional-spec.md#focus) and [macOS port note](../porting/macos-port-notes.md#approved-core-changes-and-deferred-questions).

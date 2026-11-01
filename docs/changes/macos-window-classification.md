# macOS normalized window policy and classification

Delivered offline: additive pure `tiler-core` window policy, actual Windows-copy
conformance and typed AX/CG classification through the macOS fake Engine path.

Tentative A, pending later user review: fullscreen returns report truthfully;
the wire-omission workaround is removed. Desired minimize/fullscreen slot
retention remains TBD, with a tentative P2 shared-core follow-up candidate.

Offline tests, strict clippy and formatting pass; all eight hosted
[CI jobs](https://github.com/beefsack/OmniTiler/actions/runs/38610743082) pass.
Native fact acquisition and live capability acceptance remain later Phase 1 work.

Scope, pinned reference comparison, decisions and evidence are in the
[archived change record](archive/macos-window-classification.md). Current port
contract: [macOS port notes](../porting/macos-port-notes.md).

## Backlog detail moved 2026-11-01

### Original backlog lines 1960-1966

- P2 | Shared minimize/fullscreen reference comparison |
   User-approved option A (2026-10-15): keep truthful adapter reporting for
   tile A/B/C -> minimize B -> restore B fullscreen -> exit; existing core
   forgets the saved D23 slot and fresh-admits. Keep this behavior; compare
   exact Tier 1/native outcomes before proposing any shared slot change
  ([discriminator](../spec/reference-outcomes.md#macos-classification-discriminators),
   [record](macos-window-classification.md)).

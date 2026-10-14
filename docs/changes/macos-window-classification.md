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
[archived change record](archive/macos-window-classification.md).

# Windows unsafe FFI consolidation

## Goal and scope

- Implement the selected [safe Win32 boundary](../../research/windows-port/safe-win32-wrappers.md)
  without changing product behavior: one minimal `windows-sys` boundary module
  in `tiler-windows`, safe callers, and workspace unsafe-lint enforcement.
- Preserve input/render handoffs and callback ordering; no unsafe Send/Sync,
  mutable statics, UnsafeCell, or cross-thread pointer dereferencing.
- Other platform implementations are unchanged; their existing approved
  boundaries remain accepted by the shared check.

## Acceptance

- No unsafe blocks, functions, or implementations outside the Windows boundary.
  Every other Windows module forbids unsafe.
- Workspace defaults to forbid; exception crates deny and only approved
  boundary roots allow unsafe. CI validates both source and manifest policy,
  with positive and negative regression fixtures.
- Windows build, tests, formatting, strict all-target clippy, policy check,
  and diff whitespace checks pass. No live desktop testing is included.
- Review buffer ownership, callback lifetime and no-unwind discipline,
  thread affinity, cleanup, and non-Windows cfg paths.

## Approach and bounded units

1. Inventory current native boundaries and safe handoffs.
2. Introduce `ffi/` with responsibility-split safe wrappers; migrate native
   calls and callback ownership without moving product policy into the boundary.
3. Add lint inheritance and the CI allowlist gate, retaining other platform
   boundary approvals as-is.
4. Independently review the boundary, verify offline gates, and update records.

## Inventory

- Verified baseline at `902da22a`: Windows `src/` contains 883 lines matching
  `unsafe\s*\{` in 71 files. Including integration tests: 890 matching lines,
  19 unsafe extern callbacks, two test-only unsafe functions, no unsafe
  implementations or traits. These are line-based counts, not AST counts.
- Final tree: 447 matching block lines, all inside `src/ffi/`; zero unsafe
  blocks/functions/implementations outside it. All 249 non-boundary Windows
  source/test files forbid unsafe; `src/lib.rs` denies it so the boundary can
  opt in. Earlier incomplete inventory counts were superseded by this
  full-tree comparison against HEAD.
- Only Win32 binding dependency is `windows-sys` 0.61.2; no `windows` or
  `winsafe` dependency is present.
- Exact CI allowlist:
  - `crates/tiler-windows/src/ffi/mod.rs` (children inherit its single allow).
  - `crates/tiler-macos/src/ffi.rs` (existing boundary, implementation unchanged).
  - `crates/tiler-kwin-effect-ffi/src/drag_oracle.rs`.
  - `crates/tiler-kwin-effect-ffi/src/group_highlight.rs`.
  - `crates/tiler-kwin-effect-ffi/src/visual.rs` (unsafe export attributes).
- KWin implementation bodies are unchanged; only source lint attributes and
  manifest posture were added. No crate-wide legacy bypass remains.
- Windows input/render handoffs already forbid unsafe and use safe standard
  synchronization. Keep these seams unchanged.

## Outcome

- Delivered offline 2026-10-17. One `ffi/` directory module, 22 files split by
  native responsibility; largest boundary file is `tray.rs`, 1,279 lines.
  The sole allow stays on `ffi/mod.rs`; no additional Win32 dependency or
  `windows-sys` feature change was needed.
- Safe facade: numeric window IDs for OS-mediated operations; owned buffers
  and enumeration results; opaque thread-confined GUI/hook owners; copied
  keyboard/mouse/WinEvent payloads; owned DIB/icon pixels and scoped painter
  slices. Policy and existing safe cross-thread handoffs remain outside.
- GUI callbacks validate state provenance and use snapshots or checked
  `RefCell` borrows before reentrant calls. Overlapping mutable access now
  fails closed instead of aliasing. Low-level hooks remain allocation-free;
  input-mask ordering and retain-on-unhook-failure paths are preserved.
- Review found hidden pointer obligations in the first pager facade and
  reentrant mutable references in the first settings facade. These were
  replaced by opaque owners and checked borrowing before acceptance. Final
  review fixes contain sink/menu panics, enforce test-window confinement,
  and clean up DIB/DC handles if a painter panics. Native call sequences on
  successful paths are preserved.
- Workspace forbid plus three explicit deny exception crates is enforced by
  `tiler-core/tests/unsafe_guardrail.rs` in Linux, Windows, and macOS CI.
  Comment/string-aware source scanning rejects stray unsafe lint weakening,
  including multiline/combined/cfg_attr spellings. Manifest checks reject
  missing inheritance and unapproved overrides; restricted TOML syntax fails
  closed. The gate has 32 live-tree, scanner, manifest, and fixture tests.

## Accepted verification

- All Rust commands use the declared toolchain. Applicable package set:
  `-p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows`.
- `cargo build --locked` with that set: pass.
- `cargo test --locked` with that set: 2,572 passed, 0 failed, 1 ignored,
  across 64 suite summaries, including four compile-fail confinement doctests.
  The ignored paint benchmark was not run.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --locked` with that set and `--all-targets -- -D warnings`:
  pass. Extraction lints and mechanical stable-toolchain suggestions were
  resolved without policy changes.
- `cargo test --locked -p tiler-core --test unsafe_guardrail`: 32/32 pass.
- `git diff --check`: pass.
- No live desktop tests were performed. Default tests use existing offline
  fixtures, event handles and headless GDI; no hooks/windows were installed.

## Limits and follow-up

- Linux/macOS target builds were unavailable; Windows-only FFI remains
  cfg-gated, portable stubs were inspected, and other-platform compile
  verification belongs to CI. Intra-doc references to Windows-only modules
  remain plain code text in portable modules.
- No runtime/performance measurements are claimed. Pager paint snapshots and
  tray menu snapshots add owned clones; cached icon artwork is Arc-shared,
  not re-buffered. Low-level hooks introduce no allocation.
- No live check is needed to prove the static unsafe guardrail. Before runtime
  regression acceptance, separately authorize normal owner start/stop;
  keyboard modifier pairs and mouse drag/release/Esc; tray and pager picks;
  settings apply/cancel and modal close; and border/underlay/preview rendering
  across display/DPI and Explorer changes. Follow the live Windows protocol.

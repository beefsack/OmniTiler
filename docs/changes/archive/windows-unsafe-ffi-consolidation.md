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
  and diff whitespace checks pass. Live regression checks use only identified
  disposable helpers and restore the tested resources.
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
- Initial consolidation: 447 matching block lines, all inside `src/ffi/`; zero unsafe
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
- Initial verification was offline. Default tests use existing offline
  fixtures, event handles and headless GDI; they install no hooks/windows.

## CI and live follow-through (2026-10-17)

- Implementation: `1fe27640`; shared-lint reconciliation: `bf99f3dc` makes
  the newly added KDE settings exporter inherit the workspace guardrail.
  CI confirms the allowlist gate on Linux, Windows, and macOS. An initial
  missing-posture failure was resolved without exempting the new crate.
- Live Settings testing found a consolidation regression: closing the window
  destroyed its HWND but left the process and singleton alive. `f42691af`
  restores the original `PostQuitMessage(0)` in `WM_DESTROY`; state cleanup
  remains in the opaque owner's Drop. A dedicated thread-queue regression
  test fails without the quit signal and passes with it, without creating
  a desktop window. Subsequent live clean Close, dirty discard, and posted
  Escape all exited without forced termination; reopening proved singleton
  release. No other product regression was established.
- `dc450af7` records the input hazard: a menu over the operator's terminal
  allowed injected input to interrupt the test. Foreground identity must be
  verified before each injected key; menus are dismissed without global
  keystrokes. Interrupted owners were identity-verified, stopped and restored.
- The final tray journey injected no keys: its popup was entirely over an
  owned helper, the enabled Stop row was verified by ID/text/position, and
  mouse hit-testing proved the owned popup before selection. The owner exited
  before cleanup requested any stop. An earlier rectangle-probe failure was
  a harness error (command ID passed instead of zero-based menu position),
  not spontaneous product exit.

| Live check | Observed outcome |
| --- | --- |
| Recovery prerequisites | 17-step owned-window proof passed: graceful and exact-identity forced exit, watcher reveal, independent/idempotent restore. |
| Scoped normal owner | Start/stop, helper admission, geometry/style/visibility readbacks and workspace hide/reveal passed; no foreign managed HWNDs. |
| Tray and pager | Owned tray popup opened/cancelled; actual Stop-row mouse selection exited the owner. Pager mouse selection changed workspace and hid the fixtures. |
| Settings transactions | Preview acceptance persisted the expected edit; Cancel/Revert and dirty-close discard did not persist the second edit. Modal owner disable/enable and ownership readbacks passed. Original settings bytes restored. |
| Border and preview | Visible owned carriers, geometry, checksums and renderer acknowledgements observed; preview hid after the drag settled. |
| Group underlay | Synthetic chord showed a two-member underlay with exact union geometry, below-member z-order and transparent/no-activate styles; release hid it; no failed render acknowledgements. |
| Mouse gesture | Synthetic drag start/end and preview/settlement observed. This does not establish physical keyboard/drag cancellation acceptance. |
| Final cleanup | No test actors, popups or owned overlays; ledger clean; settings bytes, session SPI values and taskbar state matched preimages. Final cursor/foreground restored. |

- Exact process/window identities, artifact hashes, launch arguments,
  preimages, incremental steps and machine readbacks remain in the ignored
  local journey evidence, not in repository records. Journeys used medium
  integrity, identified disposable helpers and explicit owner scope.
- Follow-up local gates at `dc450af7`: build, format, strict all-target clippy,
  and whitespace checks pass for the package set above; tests are 2,596 passed,
  0 failed, 1 ignored across 65 summaries (including concurrent shared tests).
  The allowlist suite remains 32/32.

## Limits and follow-up

- Linux/macOS builds were verified in CI rather than locally. Windows-only
  FFI remains cfg-gated; portable modules retain plain-code intra-doc
  references to Windows-only modules.
- No runtime/performance measurements are claimed. Pager paint snapshots and
  tray menu snapshots add owned clones; cached icon artwork is Arc-shared,
  not re-buffered. Low-level hooks introduce no allocation.
- Physical keyboard modifier pairing, normal-owner Esc cancellation and OS
  shortcuts remain user-physical pending: the normal owner intentionally
  ignores injected keys. Synthetic chord rendering and posted Settings
  Escape are not substitutes. Physical DPI/display changes and Explorer
  restart acceptance also remain pending; unrelated desktop resources were
  not changed to manufacture those outcomes.

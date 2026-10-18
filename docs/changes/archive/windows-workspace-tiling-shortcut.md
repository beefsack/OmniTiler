# Windows workspace-tiling shortcut default

## Goal and acceptance

- Deliver default Win+Y (user 2026-10-10), matching COSMIC ToggleTiling Super+Y, through the existing Windows binding catalog and conflict model.
- Authentic takes the recorded Mixed Reality conflict; Compatible disables it. Other currently unbound actions stay unbound.
- Shortcut and tray checkbox toggle the same existing per-workspace state and behavior. Apply/Revert, presets, rebind/disable and Snap-takeover-off release follow the existing binding paths.
- Offline evidence is dated; native runtime and physical acceptance remain pending.

## Scope and approach

- Windows catalog, classifier/intents, settings UI, tray presentation and owner dispatch; reuse the workspace-mode path delivered in [Windows workspace tiling](windows-workspace-tiling.md).
- Bounded units: implementation and regression coverage; offline gates; independent shortcut/owner-routing review; records and integration.
- No new workspace state, focus/window policy, unrelated shortcuts, shared-core changes or dependency changes.
- Align decisions, functional-spec shortcut rows, backlog and current Windows shortcut tables with delivery.

## Verification

- Native MSVC toolchain identity; locked four-package build/test; all-package format check; strict four-package all-target clippy; whitespace check; pushed-head CI.
- Portable classifier/catalog/settings/preset coverage and offline owner-routing coverage where feasible, following [launcher shortcuts](windows-launcher-shortcuts.md).
- User checks: physical Win+Y tiled-to-floating and floating-to-tiled; tray checkbox sync; Compatible inert with Mixed Reality behavior recorded; rebind/disable; takeover-off release.

## Evidence and outcome

- 2026-10-13: baseline deac1b7 CI completed successfully (run 38368414040); initial working tree clean.
- 2026-10-13: delivered offline. Catalog action `toggle-workspace-tiling`, `ToggleKind::WorkspaceTiling`, default Win+Y, `ChordAction::WorkspaceTiling` and down-only queued intent route on the owner thread to the existing `toggle_workspace_tiling` used by the tray checkbox. Current workspace resolves from the active output at dispatch. Chord origin does not add focus/window policy; saved default and other workspace modes use the existing path unchanged.
- Existing `chord_conflict("Win+Y")` already records Mixed Reality ownership; reused without changing its evidence status. Authentic keeps the chord; Compatible disables it (40 -> 41 rows). Catalog count 86 -> 87; settings UI and tray preset count text aligned. Other unbound defaults unchanged.
- Eleven new regressions: one catalog/settings/preset/rebind/disable case, five classifier/queue/modifier/mask/held-pair cases, five deterministic owner refusal/settled-routing cases. Owner suspension/elevation fence is injected from the production call site, following the launcher headless-CI fix. Positive mode flips use native observation and are not claimed by the offline owner tests.
- Native offline gates passed with Rust 1.99.0, `x86_64-pc-windows-msvc`: locked build/test for `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`; 1,803 passed, 0 failed, 1 ignored (+11 from baseline). `cargo fmt --all -- --check`, strict locked four-package all-target clippy and `git diff --check` passed.
- Independent shortcut/owner-routing review accepted the shared toggle path, conflict/preset behavior and regression scope. One comment-only count breakdown corrected; no semantic failed approach or new product ambiguity.
- Apply/Revert and live binding adoption reuse the existing settings machinery. Native runtime and all physical checks above remain user-owned, including actual Mixed Reality behavior and tray synchronization. Held repeats are consumed without another toggle, following the existing Windows toggle convention; verify hold/release/repress physically.

## Verification follow-up (2026-10-14)

- The scoped lifecycle run confirmed native input/tray startup and teardown,
  but did not exercise the current-workspace tray checkbox or Win+Y state flip.
  This is not toggle-state or tray-synchronization acceptance.
- Inspection confirms the current-workspace tray checkbox is session-only;
  default-choice radios and Settings Apply persist settings. Presets stage an
  unsaved draft and Revert reloads it. Live Apply/Revert/preset adoption was not
  exercised; saved settings remained byte-identical.
- Native runtime toggle and all physical checks above remain pending.

- Continuation opened the owned native menu through the production mouse-up
  callback, but menu-handle/accessibility queries did not establish item-level
  access. Item 1005 was not selected; checked-state changes and shared toggle
  behavior remain unverified. No exact-owner workspace CLI toggle exists.
- Open-menu graceful stop was independently fixed and verified in
  [tray-menu graceful stop](windows-tray-menu-stop.md). That verifies menu
  lifecycle, not Win+Y or checkbox synchronization. Saved settings remained
  byte-identical; Apply/Revert/preset adoption and physical keyboard paths remain
  pending.

## Scoped follow-up (2026-10-18)

- Native tray-toggle leg not run: the scoped sweep did not establish a verified
  tray-icon/menu-item target, and the normal exact-owner CLI has no workspace-
  tiling toggle. No speculative click, injected Win+Y or new test-control path.
- Existing checkbox/state-sync and physical Win+Y/preset/rebind acceptance stay
  pending. External evidence `wu-sweep-20261018-164250-30608`, `report.json`
  (`check20-tray-toggle`); unrelated workspace select success is not toggle proof.

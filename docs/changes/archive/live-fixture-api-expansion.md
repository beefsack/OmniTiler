# Live fixture API expansion

- Status: delivered; KDE Wayland/XWayland isolation validated. Unchanged
  macOS-crate Linux errors block full-workspace test/clippy only.

## Goal and scope

- Expand the development-only winit fixture with cheap, useful window controls,
  honest backend limits, and less awkward asynchronous readback.
- Include level, attention/focus, useful hints/input/chrome controls, compact
  snapshots and bounded event-driven matching where practical.
- No change-history ledger, elevated instances, exclusive-mode selection,
  production tiler integration, new unsafe code or foreign-window control.

## Acceptance and approach

- Sweep winit 0.30.13 common and platform APIs; document selected controls,
  skipped APIs and Windows/macOS follow-ups with reasons.
- Keep requested, configured and observed semantics distinct; no-op host APIs
  must not masquerade as supported effects or native observations.
- Preserve sentinel identity when allowing title variation.
- Settlement uses host events and a real deadline, not repeated reads or sleeps;
  deadline bounds scripts when hosts ignore or negotiate requests.
- Tests, formatting, clippy and repository dependency gates pass; KDE native
  Wayland and X11/XWayland isolation runs retain correlated evidence and clean
  only owned windows/processes/sockets. OmniTiler remains stopped.

## Bounded units

1. Audit local winit APIs and choose useful low-complexity controls.
2. Implement controls and targeted protocol/state tests.
3. Implement compact output and bounded event-driven observation matching.
4. Review contracts, verify offline gates, exercise both KDE backends and record
   honest host limits and cleanup.

## Material decisions and evidence

- Baseline fixture and Phase 1 KDE evidence are in the
  [original change](controllable-live-test-window.md).
- Wayland non-normal window level and `focus_window` are winit no-ops; expose unsupported
  rather than accepting them. Attention may request activation but cannot promise
  focus without compositor support and a valid activation context.
- No public winit level/stacking getter exists. A native stacking adapter is
  not justified for this set-only fixture; query remains explicitly unsupported
  without adding a connection/dependency or unsafe boundary.

## Outcome

- Added level, focus/attention, title suffixes, cursor visibility/passthrough,
  enabled buttons, resize increments, theme, transparency/blur hints and
  creation-only X11 override-redirect. Backend no-ops are refused except explicit
  default/no-action values; macOS attention-clear remains a documented winit no-op.
- Safe common APIs and cfg-gated X11 extensions only; no dependencies changed.
  Full selected/skipped API inventory and native-platform follow-ups are in the
  [README](../../../crates/tiler-live-fixture/README.md#api-sweep-exclusions-and-platform-follow-ups-unchanged).
- `send --compact` removes optional null snapshot fields only. `--wait-ms` plus
  `--expect` subscribes before requests, reads one baseline, then consumes events
  until latest observed predicates match or one monotonic deadline expires.
  Unknown is never a match; request/close/error/EOF failures are explicit.
  Deadline-aware partial reads/writes prevent chatty or trickling peers from
  resetting the bound. No polling or main-loop blocking added.
- Independent review found and resolved spawn action persistence, unsupported
  intent tags disappearing after unrelated changes, and partial-frame deadline
  reset. Regressions cover all three; follow-up review accepted corrections.

## Verification (2026-10-20)

- `cargo test --locked -p tiler-live-fixture`: 116 passed.
- `cargo fmt --all -- --check`: passed.
- Full-workspace test and all-target strict clippy remain blocked by unchanged
  `tiler-macos`: missing Linux `AX_ATTRIBUTE_UNSUPPORTED` in `textedit_post.rs`
  and missing `take_trailing_proof` in `select_owner.rs`. Tests and strict
  all-target clippy pass with `--exclude tiler-macos`.
- Repository CI license/bans/sources entry points (`cargo deny --locked check`)
  passed; duplicate-version bans warnings remain nonfatal.
  `just check-third-party-notices` passed; unsafe guardrail passed (32 tests).
- KDE native Wayland and forced X11/XWayland: compact snapshots, event-driven
  resize matching, multi-field observation and bounded mismatch timeout worked.
  Corrected live capture retained actual sender exit 0 on match and exit 1 on
  timeout on both backends; earlier tee-pipeline exit annotations were discarded.
- Owned X11 properties proved ABOVE/BELOW and their removal on normal, title/class
  identity, urgency set/clear, resize-increment hints and dark-theme preference.
  Focus readback matched for managed owned windows. Override-redirect creation,
  identity and close were exercised, but the redirect attribute was not queried;
  its focus request was accepted without observed focus and timed out honestly.
- Wayland non-normal level/focus/buttons/increments were explicitly unsupported.
  Attention/blur and cursor/transparency controls remain accepted hints, not
  visual or independently observed effects. The README distinguishes these limits.
- Runtime libraries already declared in devenv were exposed to live processes;
  an unrefreshed environment initially failed display-library loading. No new
  dependency or installation was required. No native Windows/macOS build/live
  or visual rendering claim is made.
- Five successful isolated server runs shut down and exited 0, all owned windows
  closed and sockets/run directories removed; the initial failed server also
  exited without retained resources. Controller status stayed DOWN and compositor
  config hash/mtime unchanged. No foreign windows were targeted; foreign-window
  geometry/restoration was not independently measured. Raw correlated responses
  and native-property checks were retained outside the repository.

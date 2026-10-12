# Windows launcher shortcuts (Win+B browser, Win+T Terminal)

## Goal and acceptance

- Default-on Win+B opens the user's Windows default web browser via the
  system association (never a named browser); Win+T opens Windows Terminal.
- Both follow the existing conflict model (Authentic takes over, Compatible
  disables) with conflict-list entries for native holders (Win+B notification
  area, Win+T taskbar cycling).
- Bindings defined, classified, presented, and dispatched through the same
  paths as existing Windows bindings; rebindable/disable-able per existing
  patterns; shared core/protocol untouched unless strictly required.
- Launch happens off the hook callback (owner or non-blocking spawn); hook
  stays synchronous consume/pass. Launched processes do not inherit owner
  handles or elevation; missing Terminal logs narrowly and keeps functioning.
- Offline gates pass; no live browser/Terminal launch by agents; user
  physical check listed.

## Scope and non-goals

- Windows only: `tiler-windows` binding catalog, input pump/handoff
  classification, settings schema/UI, conflict lists, owner dispatch, launch
  mechanism, offline tests.
- Non-goals: kwin/ edits, KDE/macOS counterparts, registry/policy changes,
  preset/first-run redesign, Snap/gaming changes.

## Approach

- Bounded units: (1) investigate existing binding paths and report exact
  files/enums/tests; (2) implement catalog + pump + handoff + settings/UI +
  conflict entries + owner dispatch + launch; (3) offline tests; (4) review,
  gates, records.
- Launch mechanism default: ShellExecuteW "open" of an `https:` URL for the
  browser (honours system association); `wt.exe` App Execution Alias spawn
  for Terminal. Tentative, pending user review.

## Verification

- Locked four-package build/test, `cargo fmt --check`, strict clippy
  `--all-targets -- -D warnings`, `git diff --check`; post-push CI.
- New offline regression tests for classification, settings/preset behavior,
  and launch-path selection (mocked spawn, no real browser/Terminal).
- No agent live launch of browser/Terminal; synthetic keys ignored by owner.
  User physical check: press Win+B / Win+T under Authentic and Compatible.

## Material decisions

- Decision source: docs/decisions.md "Launcher shortcuts (user 2026-10-11,
  Windows)" + "Conflict model".
- Tentative, pending user review: browser target URL choice, Terminal
  missing-binary behavior (log and keep functioning, no fallback).

## Outcome and evidence (2026-10-12)

- Offline-delivered; user physical check pending. Win+B opens the default
  browser via `ShellExecuteW` "open" of `https://` (system association,
  never a named browser); Win+T spawns `wt.exe` with nulled stdio, no
  handle inheritance or elevation. Missing `wt.exe` logs a narrow
  `launch-failed` error and keeps functioning.
- New `BindingFamily::Launch` rows (`launch-browser` Win+B,
  `launch-terminal` Win+T), `ChordAction::LaunchBrowser/LaunchTerminal`,
  down-only pump classification, owner `keyboard_tick` dispatch on the
  owner thread (never the hook callback), Compatible disables both (38->40).
  No shared core/protocol change; no `kwin/` edits.
- Gates: locked four-package build pass; tests 1,588 passed, 0 failed,
  1 ignored; `cargo fmt --check` clean; strict clippy
  `--all-targets -- -D warnings` clean; `git diff --check` clean.
  Independent spawn-path review: pass for merging (one cosmetic
  `SW_SHOWNORMAL` comment applied).
- Rejected: named-browser launch and Terminal fallback (decision requires
  system association / narrow failure).
- User physical check: under Authentic press Win+B (default browser opens)
  and Win+T (Terminal opens); under Compatible both stay disabled; rebind
  and disable per binding still work.

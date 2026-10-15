# Windows Tray Menu Pager Stopgap

## Scope (decided 2026-10-15)

- No separate popup and no click rebinding: left-click and right-click keep
  opening the existing tray menu (`tray_sys.rs` click arms unchanged).
- The existing menu gains plain rows, one per focused-output workspace in
  workspace order, with the active workspace radio-checked. Clicking a row
  selects that exact workspace.
- The tray icon carries the focused-output active 1-based ordinal over the
  existing tiling glyph; the warning badge is preserved underneath.
- Deferred: app icons, urgency marks, and the in-taskbar visual pager (which
  remains separate research, including the Win8+/PMv2 manifest question).
- Tentative, pending later user review: the focused-output choice (genuine
  foreground monitor first, not the cached last-switch output), the
  `Workspace N` wording, and the pager placement (after the toggle/default
  rows, before Settings).

## Acceptance (single-output native, 2026-10-15)

- Direct left/right icon-rect clicks opened the pid-bound menu in every
  session, using GUID-aware `Shell_NotifyIconGetRect` (bare HWND+id `E_FAIL`
  is harness identity usage, not overflow).
- Native rows exact: exactly the focused output's workspaces in order, with
  exactly the active one radio-checked; toggle/default/Settings/Stop rows
  intact.
- Menu picks switch workspaces both ways (`edge:tray outcome:ok` hide then
  reveal in the applied log); the tray number bitmap differs per workspace.
- Already-active row is an observed visibility no-op (unchanged); native
  geometry/history invariants were not measured.
- Esc and outside-click on an approved app close the menu with no select.
- Toggle off/on verified by native menu readback plus applied log; initial
  mode restored.
- Settings row opens the exact-owned settings UI (inventory class match,
  visible, responding); `WM_CLOSE` without Apply leaves the settings file
  unchanged with the owner alive; final cleanup exits owner, helper, and
  settings child with clean ledger and restore.
- Stop row ends owner, tray, and menu; stop with the menu open cancels the
  menu without modal block; independent restore is clean.
- Offline gates unchanged: locked four-package build/test plus strict
  all-target clippy, 2,115 passed / 0 failed / 1 ignored; targeted tray
  suite 36 passed.
- Remaining, not blockers: human visual legibility/feel of the number and
  rows; user-owned multi-output journeys.

## Harness notes (no product change)

- Earlier settings legs failed on harness identity: the first-child latch
  grabbed the preexisting watch-owner helper, and class-only `FindWindow`
  missed pid binding. Corrected to executable/creation-time latch plus
  inventory class+pid enumeration. One report's `status:FAIL` is that
  harness verdict only; the passing legs above stand on the passing
  reports.
- `Shell_NotifyIconGetRect` with bare HWND+id is not a usable locator for
  this icon; the harness identity error is not an overflow outcome.

## Corrections (shipped in this change)

- Native radio bullet: `AppendMenuW` with `MF_CHECKED` draws checks only,
  so the active pager row had no radio bullet. `show_menu` now skips the
  check flag for pager rows and applies `CheckMenuRadioItem` over the pager
  id range by command; toggle/default checks are untouched
  (`tray_sys.rs`).
- Transient owned-UI focus: after the menu closes, the foreground can still
  be the hidden owner, which dropped genuine focus to the pointer (tray
  click) monitor and refused legitimate picks. The per-pump focus now
  treats exactly the tray owner hwnd as transient owned UI and preserves
  the last genuine (foreground-monitor derived) output in
  `TileLoop::last_genuine_output`; any other real foreground recomputes
  and still refuses stale picks. Pure regression covers last-switch A,
  real focus B, menu on A, pick B succeeds, then real focus on A refuses
  stale B (`tray_pager.rs` `focused_output_transient`).

## Approach

- New portable `tray_pager.rs`: focused-output resolution (foreground, then
  pointer, then cache, then ordering), 1-based ordinals, menu ids from
  `MENU_ID_WORKSPACE_FIRST`, `SelectScope` plus `verify_select_scope`.
- `tray.rs`: `menu_items_with_pager` (legacy `menu_items` delegates with an
  empty pager, so existing order is unchanged), `SelectWorkspace` pick
  outcome with the same stop precedence, and `tray_icon_bgra_with_number`
  (doubled 3x5 digits over the hot pane, clear of the warning corner;
  `None` keeps the plain glyph byte-for-byte).
- `tray_sys.rs`: cached pager rows plus a scoped select slot beside the
  toggle slot; `sync` takes the ordinal and rebuilds both glyph variants on
  number/warning change (failure keeps the old pair); `sync_pager`,
  `take_select_request`/`has_select_request`.
- `tiling_sys`: per-pump focused-output/ordinal/pager sync and select wake
  in `run_loop_intake.rs`; new `tray_select.rs` dispatch beside the CLI
  request poll in `run_loop_tick.rs` (suspend/elevated fences, fresh
  observation, departure memory, `workspace_do_select`, edge `tray`);
  gesture-held turns leave the pick queued. `mod.rs` gains one module line.

## Behavioral discriminator

- Clicking the already-active pager row is an `already-active` visibility
  no-op: visibility unchanged; no geometry/history claim
  ([row](../../spec/reference-outcomes.md#windows-tray-menu-pager-discriminator-2026-10-15)).

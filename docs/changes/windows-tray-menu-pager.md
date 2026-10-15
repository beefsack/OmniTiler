# Windows Tray Menu Pager Stopgap

## Revised scope (decided 2026-10-15)

- No separate popup and no click rebinding: left-click and right-click keep
  opening the existing tray menu (`tray_sys.rs` click arms unchanged).
- The existing menu gains plain rows, one per focused-output workspace in
  workspace order, with tentative `Workspace N` 1-based labels and the active
  workspace radio-checked. Clicking a row selects that exact workspace.
- The tray icon carries the focused-output active 1-based ordinal over the
  existing tiling glyph; the warning badge is preserved underneath.
- Deferred: app icons, urgency marks, and the in-taskbar visual pager (which
  remains separate research, including the Win8+/PMv2 manifest question).
- Tentative, pending later user review: the focused-output choice (genuine
  foreground monitor first, not the cached last-switch output), the
  `Workspace N` wording, and the pager placement (after the toggle/default
  rows, before Settings).

## Acceptance

- Menu lists exactly the focused output's workspaces in order with exactly
  the active one checked; a pick selects that exact workspace id, including
  past the Win1..9 range.
- The tray number follows workspace switches and output focus changes.
- Existing tray settings/stop/toggle semantics and stop/reentrancy behavior
  (modal-track guard, stop-timer cancel, stop-during discards) are unchanged;
  proof owners still get no tray.
- Stale picks refuse once with an opaque log line and no native effect: the
  rendered output must still be focused and the workspace still live there.
- No owner-drawing, pager rectangles, framework, polling timer,
  embedding/manifest, or speculative changes.

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

## Evidence

- Final required gates pass on formatted source after one causal
  test-lint repair: locked build/test plus strict all-target clippy for
  `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`;
  workspace `fmt`; staged/unstaged diff checks. Full suite 2,115 passed /
  0 failed / 1 ignored (baseline 2,106, +9 new); targeted tray suite
  36 passed / 0 failed.
- First clippy attempt failed on a test-only `chunks_exact` lint; causal
  repair to `as_chunks`; final full pass is clean.
- Native single-output checks are callback/menu readback, not physical
  clicks: scoped brokered owner; CLI workspace switch hid approved apps
  then revealed them; native real HMENU readback after the right-click
  callback showed Workspace1 radio plus checked, Workspace2 unchecked,
  and the settings/toggle/default/stop rows intact.
- Native selection via the menu is not yet proven: validation paused
  before input because the permitted foreground precondition was not met.
  `Shell_NotifyIconGetRect` returns `E_FAIL`, so direct physical tray
  clicks and digit captures are unproven. Multi-output hardware is
  unavailable; stale/focus retention is unit-covered only.
- Final stop left owner/helper/Notepad counts at zero with a clean ledger;
  no stray test windows or processes.
- Menu-close and already-active-row native legs remain unproven; the
  reference discriminator retains native TBD.

## Corrections (review follow-up, offline)

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

## Behavioral discriminator

- Clicking the already-active pager row is an `already-active` no-op: no
  hide/reveal, focus, geometry, or history write
  ([row](../spec/reference-outcomes.md#windows-tray-menu-pager-discriminator-2026-10-15)).

## Future

- Remains ACTIVE: full native functional acceptance is blocked. Still
  pending are automated single-output native picks once a safe foreground
  is available (not punted to user physical acceptance), the
  multi-output hardware plus native focus leg, and human review of visual
  legibility/feel and the tentative focused-output/label/placement
  choices (stopped/suspended/elevated/gesture legs included).
- Reusable controls: existing `workspace.request` CLI select path and the
  toggle stale-scope fence/logs.

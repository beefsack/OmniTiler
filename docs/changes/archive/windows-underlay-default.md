# Windows underlay default

Status: delivered offline 2026-10-13; archived. User visual check pending.

## Scope and acceptance

- Deliver item 24: candidate C `#662a82da` (40% accent blue), selected for
  both platforms for now on 2026-10-13. Update Windows render/settings defaults,
  CLI/proof mirrors, any old-default UI text and affected tests.
- Preserve explicitly saved colors, including the original default; schema
  remains version 1 with no migration. The dogfood report that the user used
  `#50909090` does not establish whether it was saved or passed on the CLI.
- Keep underlay geometry, triggers and presentation unchanged.
- Verify omitted/default and explicit-color settings/CLI behavior, then run
  native locked four-package build/tests, fmt, strict clippy and whitespace
  gates. Black/white/grey visual checks at 100%/150% remain user-owned.

## Delivery and evidence

- Updated `group_underlay.rs` ARGB default, settings/serde default and invalid-
  color fallback, CLI help and the underlay proof script's expected default.
  Settings UI labels/tooltips use generic color-format text; no old-default
  wording needed changing. Rendering/owner/proof consumers inherit the default.
- Tests pin omitted-color defaults, explicit old/dogfood colors through serde,
  saved-settings-to-CLI precedence and live-apply CLI override retention.
  Premultiplied candidate C BGRA bytes are `[0x57, 0x34, 0x11, 0x66]`.
- Native offline gates: rustc 1.99.0, MSVC target; locked four-package build/
  tests, workspace fmt, strict all-target clippy and whitespace checks pass.
  **1,792 passed, zero failed, one ignored**: core 615, protocol 190, KWin FFI
  40, Windows 947 (581 library, 366 integration; one library test ignored).
- No live visual evidence. User black/white/grey check at 100%/150% remains
  pending: hold Win+Shift over a tiled group with extension -1 and confirm the
  fill/ring is legible on all three backgrounds. An explicit saved color still
  wins; select `#662a82da` deliberately for this comparison if overridden.

## Verification follow-up (2026-10-14)

- Read-only settings inspection found an explicit saved `#40808080`, with the
  underlay enabled and extension -1. It masks the new `#662a82da` default;
  the settings file stayed byte-identical throughout the scoped lifecycle run.
- The trace observed the underlay hidden/idle, not a displayed group underlay.
  No default-color appearance or screen-pixel sampling is claimed. A comparison
  can use `--group-underlay-color '#662a82da'` without changing saved settings.
- Black/white/grey appearance and physical 100%/150% legibility remain pending.

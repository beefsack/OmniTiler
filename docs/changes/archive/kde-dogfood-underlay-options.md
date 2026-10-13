# Group underlay default colour - options (research only)

Status: options research complete, decision resolved 2026-10-13; archived.
KDE default delivered offline 2026-10-13 (section 8); Windows implementation
and native visual acceptance remain queued separately.
No live visual checks run.

## 1. Research baseline (both platforms, before default delivery)

- Shipped default: #40808080 (ARGB: alpha 0x40 = 64/255 = 25.1%, grey 0x80 = 128).
  - KDE: `GroupUnderlayColor` default `#40808080` in kcfg; 1x1 QImage fill onto
    ImageItem composited rect; outer = union + gap + border width + resolved
    extension; KCM color control has alpha enabled.
  - Windows: `DEFAULT_COLOR_ARGB = (0x40, 0x80, 0x80, 0x80)`; straight-to-
    premultiplied BGRA fill (`paint_fill_argb`) presented via layered tool
    window below anchor; CLI `--group-underlay-color #aarrggbb`.
- Extension default both platforms: -1 sentinel = follow border width.
- Dogfood datapoint: user ran #50909090 (alpha 0x50 = 31.4%, grey 0x90 = 144)
  and found the shipped default too dim.

## 2. Why #40808080 reads dim (blend math)

Normal source-over: out = a*fg + (1-a)*bg, a = 0.251, fg = 128.

- Over black (0): out = 32 (#20). Delta 32 from background.
- Over white (255): out = 32 + 191 = 223 (#DF). Delta 32 from background.
- Over mid grey (128): out = 128. Delta 0 - invisible by construction.

Any neutral grey fill is invisible on a matching-grey background; raising
alpha alone only fixes black/white. Dogfood #50909090 (a = 0.314, fg = 144):
over black 45, over white 220, over grey-128 only 133 (delta 5). Still weak.

## 3. Candidates (same #aarrggbb usable on KDE and Windows)

A. #50909090 - dogfood status quo. 31% light grey. Marginally better than
   shipped on black/white; still near-invisible on grey. Zero-hue-change path.
B. #80808080 - 50% neutral grey. Doubles black/white delta to ~64
   (over black 64, over white 191) with no hue shift; still delta-0 on
   mid grey. Minimal-change option if hue must not change.
C. #662a82da - 40% accent blue (border-accent hue #2a82da, alpha 0x66).
   Over black (17,52,87); over white (170,205,240); over grey-128
   (94,129,164): luminance + chroma delta on all three, solves grey-on-grey.
   Matches existing border/drag-preview accent family.
D. #802a82da - 50% accent blue. Strongest: over grey-128 (85,129,173),
   unmissable on all backgrounds; risk of reading heavy in the extension
   ring over wallpaper or adjacent windows.

## 4. Extension / enabled considerations

- Keep default extension at -1 (follow border width); a wider explicit ring
  (e.g. 4-6 px) amplifies any colour without touching alpha.
- Keep default enabled = on on both platforms; visibility is already gated
  on Win+Shift chord / focused move, so a stronger fill cannot appear
  uninvited.

## 5. Recommendation and selection

Candidate C (#662a82da): hue breaks the
grey-on-grey degeneracy no grey can fix, 40% alpha is ~1.6x the shipped
luminance delta on black/white plus a blue-chroma delta on grey, it reuses
the product accent hue, and the identical hex applies to the KDE kcfg
default and the Windows CLI/constant.

## 6. User decision 2026-10-13

- Candidate C `#662a82da` (40% accent blue) selected on KDE and Windows, for now.
- KDE kcfg default delivered offline (section 8); Windows constant remains
  handoff item 24.
  Preserve explicit colors. Live visual check over black/white/grey wallpapers
  before 0.1; selection is not native evidence.
- Research decision is resolved; implementation/checks are in
  [backlog](../../backlog.md), [decision](../../decisions.md#visuals-border-underlay-and-grouping).

## 7. Pending live visual checks (not run)

- KDE: chord-held underlay over black, white, and mid-grey wallpapers at
  100%/150% scale; confirm extension ring reads at -1.
- Windows: same three backgrounds via tile-proof/loop trace; confirm
  premultiplied present matches KDE composited swatch.

## 8. KDE default delivery 2026-10-13

- Changed only `GroupUnderlayColor`'s default in
  `kwin/native-effect/activeborderconfig.kcfg` to `#662a82da`, plus its native
  QColor expectation and KWin static-contract assertion. KConfigXT supplies
  the effect and both Settings KCMs; no separate Rust/TypeScript/Nix default
  needs changing. Explicit saved colors remain intact; no migration.
- Format confirmed: generated KConfigXT uses `QColor("#662a82da")`; native
  missing-key and Defaults/readback checks expect
  `QColor(0x2a, 0x82, 0xda, 0x66)`, confirming alpha-first ARGB and exactly
  40% alpha.
- Accepted offline evidence: host-matched native effect/KCM build; CTest
  32/32 (including Rust drag-oracle 20/20 and group-highlight 19/19 tests);
  KWin static-contract suite 15/15; TypeScript typecheck and bundle pass;
  changed C++ file passes clang-format. No live visual evidence.
- Dev pickup: `just build-native-effect`, fully quit and reopen Settings for
  the new KCM default; logout/login loads the rebuilt effect binary. Saved
  overrides still win. Appearance on black/white/grey at 100%/150% remains
  user-owned before 0.1. Windows code is unchanged; item 24 specifies its
  default constants, mirrors and acceptance checks.

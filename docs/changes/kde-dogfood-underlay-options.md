# Group underlay default colour - options (research only)

Status: research note. No default changed. Decision pending by user.
No live visual checks run.

## 1. Current state (both platforms)

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

## 5. Recommendation

Candidate C (#662a82da) if a change is ever approved: hue breaks the
grey-on-grey degeneracy no grey can fix, 40% alpha is ~1.6x the shipped
luminance delta on black/white plus a blue-chroma delta on grey, it reuses
the product accent hue, and the identical hex applies to the KDE kcfg
default and the Windows CLI/constant.

## 6. Explicit non-change statement

No default was changed in this unit. KDE kcfg, Windows
`DEFAULT_COLOR_ARGB`, extension sentinel, and enabled flag are untouched.
Any adoption of A-D is a user decision after live visual checks.

## 7. Pending live visual checks (not run)

- KDE: chord-held underlay over black, white, and mid-grey wallpapers at
  100%/150% scale; confirm extension ring reads at -1.
- Windows: same three backgrounds via tile-proof/loop trace; confirm
  premultiplied present matches KDE composited swatch.

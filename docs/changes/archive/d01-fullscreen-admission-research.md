# D01 fullscreen admission research

- Goal: establish sourced defaults for new-window admission and explicit focus
  switches over fullscreen/maximized windows, then reassess INS-06's tentative
  recommendation against native gaming behavior.
- Scope: pinned reference WM source, native KWin and Microsoft documentation,
  current KDE adapter, D01 research and R-INS-06 evidence. No product changes
  or approved policy changes.
- Acceptance: distinguish focus, visibility and fullscreen state; separate
  ordinary windows from transients and explicit activation; identify defaults
  and options; leave unsupported/native runtime outcomes unverified; record
  alternatives and a recommendation without approval.
- Approach: inspect modern Wayland references, other reference profiles, then
  native host and current adapter; integrate evidence into one research note
  linked from triage and matrix.
- Verification: source-pin and citation checks, documentation diff and link
  review; no builds or live tests required.
- Accepted evidence: pinned reference focus tally C 6/8 versus B 2/8;
  fullscreen exit 2/8, retained 6/8; visible C 4/8, behind 3/8, qtile
  backend-qualified 1/8. niri remains contextual/non-voting.
- Material corrections: COSMIC front-to-back rendering puts unfocused
  fullscreen below C; awesome's shipped activation raises C above retained
  fullscreen; i3/sway explicit-target focus clears fullscreen despite their
  ordinary admission/directional fences. Current KDE convergence normally
  follows native focus rather than unconditionally focusing C.
- Native evidence: KWin v6.6.3 fallback confirms Low activates ordinary new
  Wayland windows; explicit selection raises without clearing fullscreen.
  X11 timestamp/FSP and Microsoft foreground rules remain distinct.
- Outcome: [research note](../../research/d01-fullscreen-admission.md) linked
  from D01; R-INS-06 evidence corrected and explicit-selection plus native
  activation discriminators added. Recommendation is native activation
  ownership, pending user review. Existing policy approval state preserved.
- Verification: targeted source/citation and tally review passed; local
  links reviewed; documentation whitespace checks passed. Runtime game,
  stacking/input settlement and overlay-close outcomes remain unverified.
- Next action: user reviews D01's revised recommendation before a product
  change; native parity sequences are recorded for later testing.

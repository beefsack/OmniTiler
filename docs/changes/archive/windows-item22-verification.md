# Windows item 22 verification-only follow-ups

## Scope and outcome (2026-10-13)

- D01, D03, D09, D10, D12, D13, D22, D25, D26, D27 and D28 require no new
  Windows adapter behavior in this delivery. Approved targets are unchanged;
  exact native outcomes remain user-owned and pending.
- D27 inspection found foreground observation via `focused_token` in
  `crates/tiler-windows/src/tiling_sys.rs`, but no dedicated activation-request
  classifier or urgency observer/marker in Windows source. This verifies the
  approved Mark urgent only/fullscreen-game guard policy is unchanged, not that
  it is implemented or native-proven. Explicit foreground selection is separate;
  KDE's native activation exception and no-guard choice are not Windows evidence.
- Existing shared defaults cover after-focused admission, middle nested entry,
  wrap/ancestor escape, local-before-cross and survivor-share preservation.
  Earlier Windows restart/float and fullscreen-send records remain authoritative;
  this verification adds no new physical acceptance to them.
- Offline allowlisted MSVC gates after D16/D23: Rust 1.99.0, locked build/tests
  passed (1,845 passed, 0 failed, 1 ignored), fmt, all-target strict clippy and
  diff check passed. D16/D23 have separate archived delivery records. No live
  testing performed; no shared core/protocol changes.

## Exact pending physical journeys

| Unit | Offline basis | User-owned physical journey; exact outcomes TBD |
| --- | --- | --- |
| D01 | Shared retained overlay/admission contract; Windows foreground rules remain host-specific | Compare tiler off/on: maximize/fullscreen B, open C with host-approved then denied activation; explicitly select C/B; close B. Check retained overlay/slot, focus, stacking and no fullscreen geometry write. |
| D03 | Shared after-focused admission and per-admission anchor in `session.rs` / `session/ops/lifecycle.rs` | From A/B with B focused and pointer fixed, open C, then D, then E with live focus updates. Inspect after-focused order and each leg's pointer/focus; legs 2-3 pending. |
| D09 | Shared middle-index nested entry in `directional.rs` / `session/ops/move.rs` | Build `H[A*,V[B,C]]`, remember B, move A right. Check geometric middle entry (`V[B,A*,C]`) and focus, rather than remembered-child entry. |
| D10 | Shared R1 orthogonal wrap | Build `V[H[A,B*],C]`, move B down. Check `V[V[A,B*],C]`, wrap and focus; no automatic output crossing. |
| D12 | Shared same-axis ancestor extraction | Reproduce R-MOV-04 nested tree and move its focused leaf across the same-axis ancestor boundary. Check ancestor-axis insertion, sibling shares and focus. |
| D13 | Shared local-before-output-cross ordering | With adjacent outputs, reproduce R-OUT-02's perpendicular edge move. Check local wrap wins and membership stays on source output before any R4 crossing. |
| D22 | Shared removal preserves survivor share ratios | Tile 50/25/25, float the middle 25. Verify remaining shares approximately 67/33 and the float remains independent. Repeat R-FLT-03's 60/40 survivor fixture. |
| D25 | Existing re-adoption and Windows restart-intent record | Record workspace set/membership/focus and intentional/sticky floats; orderly restart with apps alive. Verify no membership moves, only provable focus restore, rebuilt layout and preserved same-lifetime float intent. |
| D26 | Existing saved gaps/settings and re-observation; approved cross-login float-memory loss | Save owned gaps/settings, record host set/membership/maximize, logout/login with reopened apps. Verify saved settings reload and native state re-observation; record new identities and intentional-float-memory loss, without saved-layout restoration. |
| D27 | Policy unchanged; dedicated request/urgency observation not found | From hidden B, trigger an unsolicited request with ordinary A focused, then with fullscreen/game A focused in active and idle/paused play; compare tiler off/on. Inspect marker, selected workspace/focus, then explicitly Alt+Tab/taskbar-select B. Supported request classification and guard/marker behavior remain unproven. |
| D28 | Existing Windows fullscreen-send delivery, separate from D16 intentional-float combos | Retained tiled fullscreen and never-tiled born-fullscreen: numbered/relative x follow/stay, sole held source/empty target, real game. Inspect membership/view/focus/flags/frame/input; first exit of born hold admits on destination. Preserve earlier evidence; remaining physical/game/cross-output legs pending. |

## Remaining review

- D27 needs supported Windows request/urgency evidence before implementation or
  native behavior can be accepted; no urgency feature was inferred here.
- D23 source-slot lifetime after membership rehoming while minimized is the
  unchanged shared behavior and a separate TBD reference discriminator.

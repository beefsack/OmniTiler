# macOS batch-3 utility mapping: `AXFloatingWindow` floats

Status: implemented offline with native evidence recorded 2026-11-24; left
uncommitted in the phase-3 worktree. `AXSystemFloatingWindow` stays withheld
for lack of a truthful sample.

## Decision

- `AXWindow` + `AXFloatingWindow` maps to `WindowType::Utility`.
- `AXWindow` + `AXSystemFloatingWindow` stays `None` (withheld). The sibling
  subrole and the AppKit panel style are not evidence for it.
- `AXStandardWindow` -> `Normal`, `AXDialog`/`AXSystemDialog` -> `Dialog`,
  sheets, unknown reads and non-window roles are unchanged.

## Native measurement

Preflight on the signed probe bundle `io.github.omnitiler.macos.probe`
(Authority `OmniTiler Mac Dev`) reported `ax_trusted:true`, session unlocked,
secure input inactive. A bounded `caffeinate -d` child kept the display awake.
The helper was spawned directly from the signed bundle's CLI so the spawn
record named the bundle helper; AX was read through the LaunchServices route
(`just launch -- owned-frame ...`), which is the TCC-attributed process. Role
and subrole come from the native `AXRole`/`AXSubrole` attributes on the
CG-joined owned window, not from style or layer.

Run 2, 2026-11-24 10:06 local, helper pid 34730:

```json
{"pid":34730,"wid":2212,"ax_frame":{"x":300.0,"y":756.0,"w":320.0,"h":204.0},
 "cg_bounds":{"x":300.0,"y":756.0,"w":320.0,"h":204.0},
 "cg_onscreen":true,"cg_layer":3,"minimized":false,"fullscreen":false,
 "role":"AXWindow","subrole":"AXFloatingWindow"}
```

`cg-owned-windows` for the same helper showed the panel at layer 3, onscreen,
with the main window at layer 0. `owned-frame` for the main window reported
`AXWindow`/`AXStandardWindow`, layer 0. Run 3 (pid 34910, panel wid 2215)
repeated the identical role/subrole/layer/frame; at +9 s the self-closed panel
was layer 3 but offscreen while the main window stayed onscreen at layer 0.
`probe-stop` verified exact exit; no probe processes remained, the session
record directory was empty and the `caffeinate` child was reaped.

The helper's opt-in `NSPanel` (`Titled|Closable|UtilityWindow`,
`setFloatingPanel(true)`) is therefore natively a floating utility window. The
old fixture comment predicting a dialog subrole was stale and is corrected in
`crates/tiler-macos/src/ffi.rs` (comment only).

## Effect on admission and floating

`normalize_window_type` feeds `collect_window_facts` (`runtime_core.rs`) and
`build_report` (`world_builder.rs`). In core `window_classify.rs`,
`WindowType::Utility` is a float kind, so the panel becomes a float member;
the previous `None` held it. `Utility` cannot tile. The adapter's legacy
classifier already floated the subrole (`classifier.rs` float subroles).

## Withheld `AXSystemFloatingWindow`

No owned fixture emits `AXSystemFloatingWindow`, no repository record contains
an observed one, and reading a foreign app's palette was outside the
fixture-only authorization. Remaining options: (a) an opt-in fixture leg that
opens a genuine system palette and measures its subrole; (b) a user-run signed
read of a documented system palette; (c) keep withholding. Until one is
recorded, the system subrole stays `None`, so such a palette is held rather
than floated.

## Verification

- `cargo test -p tiler-macos --lib normalize_window_type_uses_factual_neutral_vocabulary`:
  1 passed.
- `cargo test -p tiler-macos --test integration`: 150 passed / 0 failed.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p tiler-macos --all-targets -- -D warnings`: clean.
- No `crates/tiler-core/` or `crates/tiler-protocol/` change.

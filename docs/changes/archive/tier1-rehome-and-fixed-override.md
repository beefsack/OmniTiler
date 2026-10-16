# Tier 1 minimized rehome and fixed override research

Research complete 2026-10-17. Both product decisions remain OPEN and parked.

## Goal and scope

- Research D23 item 6: remembered source-slot lifetime after minimized rehoming.
- Research 13f: durable fixed-size tile override adopted while maximized.
- Use pinned COSMIC, Hyprland, i3 and sway source; distinguish supported
  analogues from exact scenarios and leave unsupported outcomes TBD.
- Research only: shared policy affects all targets, including KDE. Both
  product choices remain open user decisions; no implementation or live tests.
- Separately reconcile the hide-proof source manifest only if mechanical.

## Acceptance and approach

- Two concise research documents with source paths/refs, options,
  consequences and a recommendation under the reference-tier rule.
- Minimal discriminating matrix rows and linked backlog status.
- Bounded units: D23 trace, fixed-override trace, mechanical manifest repair,
  then evidence review and documentation integration.
- Manifest validation must execute only the hashing/check logic, not hide
  actions, process control or native probes.
- Verify source claims, links, PowerShell parsing and `git diff --check`;
  verify pushed CI. No Rust changes or native behavioral claims.

## Decisions and outcome

- Research delivered 2026-10-17:
  [D23](../../research/d23-minimized-rehome.md) and
  [13f](../../research/fixed-override-born-maximized.md). Both choices remain
  OPEN user decisions, parked; existing behavior is unchanged.
- D23: COSMIC drops the source entry and anchor when moving a minimized
  window; Hyprland/i3/sway lack native minimize counterparts and do not vote.
  Recommend forget on confirmed window rehome, excluding mere omission and
  whole-workspace migration. Keep and explicit-send-only cleanup remain
  alternatives.
- 13f: no Tier 1 exact durable-override/restart/maximize counterpart. Proposed
  shared candidate: durable explicit tile keeps its reserved base under
  maximize under either born-maximized mode, including hint loss; automatic
  fixed D4 unchanged. This preserves native maximize and existing D3/D7 tile
  intent, not reference consensus; setting-led maximize clearing can also
  preserve the durable tiled base.
  Born-maximized-setting precedence and automatic-fixed precedence remain
  alternatives; no cross-target/KDE implementation selected.
- Existing matrix rows now distinguish a surviving former neighborhood from
  changed fresh-admission anchors and repeat the fixed override journey under
  both born-maximized settings. Unsupported/native outcomes stay TBD.
- Independent source review corrected one i3 citation: `tree_restore` returns
  false; `main.c` owns the `tree_init` fallback. Source claims and new relative
  links checked; changed research/matrix files contain ASCII only and
  `git diff --check` passes.
- Review distinguishes durable tiled-base intent from maximize overlay
  continuity: clearing maximize need not erase the tile override. Return
  frames are not inferred as proven from source-slot deletion; the matrix
  preserves surviving anchors and changes the fresh-admission anchor.
- Mechanical hide-proof repair committed separately as `3b84c717`: stale
  single-file entries replaced by stable recursive hashing of split module
  families. AST-only validation hashes 134 unique files, covers all three
  directories exactly, repeats a stable combined digest and preserves missing
  file/directory rejection. No proof stage or native action ran.
- Actual unmodified manifest hashing in PowerShell Core passes; a separate
  fallback verifier's different combined digest was traced to lowercase hex,
  not a file-set difference. The proof script retains its original uppercase
  per-file hashes and lowercase combined digest.
- Documentation verification covers pinned upstream claims, local links,
  minimal discriminators, whitespace and unchanged product behavior. Pushed
  CI results accompany the delivery report.

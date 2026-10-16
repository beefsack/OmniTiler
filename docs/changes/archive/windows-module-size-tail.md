# Windows module-size tail

COMPLETE (archived 2026-10-17). Tests-first row and all four required
production rows are verified; all five pushed refactor commits have green CI.

## Goal and scope

- Bring the remaining oversized Windows families within the approximate
  1,500-line production / 3,000-line test soft caps through responsibility
  moves only. Preserve behavior, item bodies, cfg gates, public paths and
  qualified test paths; allow only necessary imports, visibility and rustfmt.
- Baseline physical lines (including blanks): workspace 2,918,
  active_border_sys 1,986, product_hide 1,836, winarrow 1,774,
  render_handoff 1,515. Settings UI is already split: largest production
  file `settings_ui/mod.rs` is 1,184; no further extraction is needed there.
- No shared-core, KDE or macOS changes; no live testing or helper extraction.

## Approach and acceptance

1. Tests first: extract workspace, active_border_sys and render_handoff
   inline `tests` bodies into same-depth child modules. Preserve all 70 test
   attributes (47 / 8 / 15), including the ignored border benchmark.
2. Workspace: move whole method groups for output reassignment/topology
   away from ordinary workspace membership and selection.
3. Active border: separate shared native carrier/presentation work from
   stateful border, underlay and preview orchestration.
4. Product hide: file-back the cfg-gated `sys` module and separate watcher
   recovery from identity-safe visibility actuation.
5. Winarrow: file-back the cfg-gated native spike `sys` module, retaining
   portable classification, queue and reporting in the parent.
- Render handoff needs no additional production split if test extraction
  brings it within the cap; its transport contract remains together.
- One production row per family still oversized after tests; verify moved
  bodies and test inventories before accepting each row.
- Required gates per row: `mise exec -- cargo build --locked`, matching
  `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings`
  with packages `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`,
  `tiler-windows`; `mise exec -- cargo fmt --all -- --check`;
  `git diff --check`. Baseline 2,313 passed / 0 failed / 1 ignored.
- Each completed row must pass local gates and the pushed CI workflows.

## Evidence and outcome

- Initial tree clean. Existing Settings UI role files range from 211 to
  1,184 production lines; its two test files are 371 and 652 lines.
- Tests-first row complete: workspace parent 1,762 / tests 1,156;
  active_border_sys parent 1,748 / tests 238; render_handoff parent 1,057 /
  tests 458. All three extracted bodies match the original dedented bodies
  after edition-2024 rustfmt; production prefixes are byte-identical.
  All 70 moved test paths are unchanged. All five local gates pass:
  2,313 passed / 0 failed / 1 ignored.
- Render handoff is now under the cap; no production row is needed, keeping
  its coherent transport and lifecycle contract together.
- Workspace production row complete: `workspace.rs` 1,762 -> 1,144;
  `workspace/outputs.rs` 624. Seven output swap, displacement, reconnect and
  migration methods move byte-for-byte into an inherent impl in the child.
  State and private helpers stay in the parent with no visibility widening.
  All five local gates pass: 2,313 passed / 0 failed / 1 ignored.
- Active-border production row complete: parent 1,748 -> 1,107;
  `active_border_sys/carrier.rs` 654. Shared owned-window creation,
  liveness/z-order, positioning and DIB presentation move together. Twelve
  parent/test-facing functions and the test-facing `ZRelation` gain only
  `pub(super)`; three existing crate-visible paths remain re-exported.
  Parent and leaf compare exactly after those declarations, one blank seam
  collapse and rustfmt. Tests keep their original paths. All five local
  gates pass: 2,313 passed / 0 failed / 1 ignored.
- Tests-first CI `38904321670`: Windows, Rust, macOS and all other jobs
  except native passed. The unrelated KDE native test
  `native-effect-kcm-shortcut-selection` aborted with
  `malloc_consolidate(): unaligned fastbin chunk detected` (32/33 passed).
  No KDE source changed; this external failure is not repaired here.
- Product-hide production row complete: parent 1,836 -> 135;
  `product_hide/sys/mod.rs` 1,429, `sys/watcher.rs` 254. Native admission,
  identity, markers, durable hide/reveal stay together; watcher readiness,
  owner wait and dead-residue recovery move as whole items into a child.
  `sys` keeps its Windows cfg and module depth; six watcher APIs are explicitly
  re-exported at their old paths. No visibility changes. All bodies match
  exact dedent plus rustfmt and portable prefix is unchanged. All five local
  gates pass: 2,313 passed / 0 failed / 1 ignored.
- Winarrow production row complete: parent 1,774 -> 731;
  `winarrow/sys.rs` 1,026. The Windows-only native hook spike is file-backed
  at the same module depth, separate from portable classification, bounded
  queue and reporting vocabulary. No import or visibility changes; extracted
  body matches exact dedent plus rustfmt and portable prefix is unchanged.
  Existing integration tests remain in place. All five local gates pass:
  2,313 passed / 0 failed / 1 ignored.
- Workspace CI `38904671333` passed all eight jobs, including native, with
  no KDE changes after the earlier native abort.

## Commits and CI

| Row | Commit | CI run | Result |
| --- | --- | --- | --- |
| Tests first | `c9177cf2` | `38904321670` | Green after native-only retry |
| Workspace outputs | `4e8ade8d` | `38904671333` | Green |
| Overlay carrier | `94b79acb` | `38905175324` | Green |
| Product watcher | `e42de869` | `38905521146` | Green after native-only retry |
| Native WinArrow | `f0227d70` | `38905770131` | Green |

- All eight jobs pass on every listed run. The two initial native failures
  were the unchanged KDE `native-effect-kcm-shortcut-selection` heap abort:
  effect test 32/33 and settings test 25/26 respectively. Each failed job
  passed on one retry with no source changes.
- Final locked four-package build/test/strict all-target clippy, workspace
  fmt check and diff check pass: 2,313 passed / 0 failed / 1 ignored.
  Test count does not drop; no live testing was needed or performed.

## Final layout

Physical lines include blanks; nonblank lines contain non-whitespace text.
Paths below are relative to `crates/tiler-windows/src/`.

| File | Physical | Nonblank | Test attributes |
| --- | ---: | ---: | ---: |
| `workspace.rs` | 1,144 | 1,065 | 0 |
| `workspace/outputs.rs` | 624 | 617 | 0 |
| `workspace/tests.rs` | 1,156 | 1,103 | 47 |
| `active_border_sys.rs` | 1,107 | 1,042 | 0 |
| `active_border_sys/carrier.rs` | 654 | 628 | 0 |
| `active_border_sys/tests.rs` | 238 | 228 | 8 |
| `product_hide.rs` | 135 | 122 | 0 |
| `product_hide/sys/mod.rs` | 1,429 | 1,373 | 0 |
| `product_hide/sys/watcher.rs` | 254 | 246 | 0 |
| `winarrow.rs` | 731 | 681 | 0 |
| `winarrow/sys.rs` | 1,026 | 982 | 0 |
| `render_handoff.rs` | 1,057 | 986 | 0 |
| `render_handoff/tests.rs` | 458 | 439 | 15 |

- Largest changed production file 1,429; largest extracted test file 1,156.
  No cap exceptions. Settings UI is unchanged and already within caps.
- Final full-body comparison covers every changed source file: original
  slices plus exact dedent, necessary declarations/imports/visibility and
  edition-2024 rustfmt only. All 70 moved tests and their paths are preserved;
  product-hide and WinArrow integration test files are untouched.

## Out-of-scope follow-up

- `scripts/windows-hide-proof.ps1` has a fixed `$SOURCE_FILES` hash manifest
  that names `product_hide.rs` but not its new native children. A separate
  proof-harness maintenance change should include those children and reconcile
  its older split paths (including `tiling.rs`). The Rust-only scope leaves
  scripts unchanged; no live proof or source-manifest freshness is claimed.

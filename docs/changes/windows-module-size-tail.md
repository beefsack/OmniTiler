# Windows module-size tail

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

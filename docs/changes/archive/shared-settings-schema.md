# Shared settings schema research

- Goal: compare core-owned settings definitions and native tabbed Settings,
  with a recommendation and explicit pre-/post-0.1 decision for the user.
- Scope: source-backed research only; current KDE, Windows and macOS surfaces,
  duplicate metadata, existing tests, K2 interaction and packaging constraints.
- Acceptance: concise research with revision-bound file:line evidence; options
  cover complexity, layering, packaging, persisted-key compatibility, tests,
  effort and release risk; backlog links the delivered recommendation.
- Approach: inventory current surfaces; audit decisions and extraction history;
  compare build-time, runtime and conformance-only routes; verify citations and
  document a smallest first step. No implementation or architecture selection.
- Material findings: Settings ownership delivered 2026-10-11 is distro package
  ownership, not a Rust schema. macOS started 2026-10-14, making deferred K2
  due for consumer-driven review, not automatically approved extraction.
- Verification: source/commit inspection, research-link and locator checks,
  and `git diff --check`; all 40 cited source paths exist at the evidence
  snapshot and all cited ranges are in bounds. Substantive claims reviewed
  against source, including native conformance coverage and `ecd183e`.
- Outcome 2026-10-15: [options and recommendation](../../research/shared-settings-schema.md)
  delivered and linked from P1 backlog. Recommend a Rust catalog with checked-in
  generated descriptors after 0.1, plus independent KDE-only tabs before 0.1.
  Neither is selected architecture or implementation; user decides route,
  tab grouping and timing. Smallest schema slice is inner/outer gap metadata.
- Exact next action: user selects the route and timing from the research
  decision table before an implementation brief.

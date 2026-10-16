# Safe Win32 wrapper research

- Goal: resolve research item 13g with evidence against the
  [unsafe principle](../../principles.md#unsafe-code), leaving the dependency
  route an open decision.
- Scope: inventory `tiler-windows` unsafe sites and dependencies; assess safe
  wrapper coverage and adoption; compare minimal FFI boundaries using
  `windows` or `windows-sys`. No production changes or live testing.
- Acceptance: dated primary sources, reproducible inventory, coverage of all
  requested API families, recommendation and options/consequences, and the
  existing lint/module/CI follow-through plan.
- Approach: inspect current source and upstream metadata/source; publish one
  concise research document, link item 13g, and preserve pending implementation.
- Verification: source and citation review, document links, `git diff --check`,
  and repository CI.
- Decision: dependency route remains open; existing guardrail enforcement
  decision is unchanged.
- Outcome: [research delivered](../../research/windows-port/safe-win32-wrappers.md)
  2026-10-16; item 13g links the recommendation and parked options. Retaining
  `windows-sys` is recommended, not selected. No production changes.
- Evidence: inventory at `aa76633` verified (809 unsafe-block-opening lines
  in 67 files, two unsafe functions and 17 unsafe extern callbacks); direct
  signature checks across all requested families, upstream metadata and
  wrapper coverage reviewed. Documentation whitespace checks passed.
- Remaining: user route choice, then the existing Windows consolidation and
  Rust guardrail implementation items; no migration or live proof claimed.

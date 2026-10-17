# KWin 6.6 guard and offline window aliases

- Status: delivered offline 2026-10-17.
- Goal: restore the shell API-guard gate without admitting unsupported native
  KWin APIs or changing world-state behavior.
- Cause: full-state adapters used lowercase `fullscreen`, `maximized` and
  `sticky` aliases for offline fakes. The literal-read scanner correctly
  refused these unaudited names, despite the native-first fallback checks.
- Resolution: confine alias reads to a three-name typed seam. Leave the guard,
  its native API allowlist and 6.7 capability confinement unchanged. Make live
  safety's native reads explicit literals rather than a variable-name loop,
  keeping them visible to the guard. Preserve read order, lazy fallback,
  native precedence and whole-result failure on exceptions.
- Non-goals: new native APIs, changed host behavior, fixture-wide rewrites or
  live KWin testing.

## Evidence

- The guard passes on the current tree. Its shell suite passes 10 checks,
  including refusal of a direct lowercase alias read, an unconfined 6.7 API,
  a new unaudited API and a cast-hidden API name.
- All 15 headless shell suites pass; KWin tests pass 1,777/1,777 and typecheck
  passes. Four new native-safety tests pin canonical-property precedence,
  missing/type-invalid fallback and exception behavior.
- Earlier migration CI 39021538182 passed native and Rust jobs; its sole
  failure was the shell guard corrected here. No live evidence is claimed.

# Windows live harness curation

- Goal: retain reusable Windows test safety checks and operational findings
  in the repository without retaining raw run evidence or one-off probes.
- Scope: generic helper library, offline checks, point-of-use runbook lessons
  and dated decision annotations. No product/runtime behavior changes.
- Acceptance: no injected input or click fallback; exact helper identity and
  admission gates; restorable settings bytes and taskbar/SPI preimage;
  cleanup attempts survive phase failures; parser and offline mocks pass;
  documentation links, ASCII additions and whitespace checks pass.
- Approach: curate common library functions; parameterize resource paths;
  retain only reusable recovery, identity and observation checks. Review the
  script and documentation independently, then verify offline.
- Boundaries: library callers own journey setup and `finally` invocation;
  taskbar/SPI are captured and verified, not directly mutated by the library.
  Live API behavior and physical shortcut acceptance are not offline evidence.
- Backlog inspection: the existing P2 Windows shortcut-dialog presentation
  review already tracks a later nonblocking UX revisit; no duplicate item.
- Outcome: delivered `scripts/windows-live-lib.ps1`, offline parser/seam and
  fault-injection checks, plus `live-lib-check` and `live-lib-mock` recipes.
  Point-of-use Windows guides retain recovery, geometry, input and exhaustion
  lessons; affected records annotate the dated accepted choices.
- Verification: independent script/document review accepted the result after
  cleanup isolation, failed-read refusal, coordinate packing and preimage
  checks were corrected. Parser/seam checks and all 45 offline mocks pass;
  documentation path links, ASCII additions and `git diff --check` pass.
- Evidence boundary: no live journey was run. Real foreground activation,
  native API behavior and desktop timing remain unverified by these mocks.
  Raw evidence, one-off drivers, leg/probe scripts and settings preimages
  remain outside the repository; they are not reusable library code.
- Next action: none for curation; physical and other native acceptance gaps
  remain in their existing feature records.

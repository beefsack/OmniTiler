# macOS port kickoff

## Goal and acceptance

Start macOS preparation now (2026-10-14): reconcile the existing port guidance
with current source and read-only host evidence, then collect canonical reference
sources with reproducible upstream revision pins.

- Correct verified documentation drift; identify unresolved Phase 0 decisions.
- Collect Tier 1 and macOS interaction references outside this repository,
  preserving existing checkouts and recording clone provenance.
- Keep behavioral reference-profile pins unchanged.
- Recommend the first capability-first implementation slice and prerequisite
  install commands. No adapter implementation or live input/window probes.

## Approach

1. Review macOS environment, plan, prior art, development loop, CI, and shared-core
   extraction guidance against source and read-only prerequisite checks.
2. Resolve canonical upstreams, clone reference sources, and publish URL/revision
   pins in the prior-art documentation.
3. Review the combined diff, record the authorized scheduling decision, and
   verify documentation links, provenance, and scope.

## Decisions

- macOS work starts now; it no longer waits for KDE 0.1 to ship.
- The existing lower-level, SIP-enabled route remains the initial direction;
  Phase 0 capability evidence precedes implementation commitments.

## Outcome and evidence

- Reviewed the macOS setup, plan, prior art, non-native interaction research,
  development loop, recipes, tool declarations, CI, core audit and historical
  tier-2 plan against current source.
- Corrected stale public-only direction, install-governance status, root recipe
  dispatch and CI claims. Distinguished CLT-based builds/signing from the
  full-Xcode archive/export workflow. The setup runbook records only generic
  read-only host evidence; the provisional macOS 15 floor remains unverified.
- Collected 25 full reference clones; canonical upstreams, default branches,
  full revisions and committer dates are in the
  [source snapshot](../../research/macos-port/prior-art.md#source-checkout-snapshot-2026-10-14).
  Verified origins, default-branch refs, clean trees and non-shallow status.
- Replaced 190 plan links to sibling checkouts with canonical GitHub blob links
  at the original historical pins. All 85 unique revision/file pairs exist;
  all cited line ranges are in bounds. Behavioral reference pins were preserved.
- Portable baseline passed:
  `mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi`.
  Documentation diff and relative links checked.

## Follow-up

- Decide support floor, Intel scope, stable signer and Cmd/Meta mapping before
  the signed capability spike. Recommended defaults: provisional 15+, arm64
  first, persistent signing identity, Cmd-led candidate with conflict evidence.
- First implementation slice: a signed Rust macOS probe target and portable CI
  allowlist, with owned-window AX identity/observation/focus-readback and one
  registration plus one interception hotkey path. Establish the macOS live-test
  protocol and stable identity before consent/input/window probes.
- Reuse Engine, visual and workspace helpers first. Review consumed K2 settings
  and action descriptors, K3 differences and restart-store needs without
  transplanting native state machines. The core audit's first new candidate is
  normalized window policy (overlay refusal, born-fullscreen hold, admission
  clear), contingent on matching fixtures and unresolved behavior choices.
- Remaining risks: private-API version coupling, TCC identity stability,
  foreign-window authority, Space/parking shell effects and stranded restoration.
  Source availability and a build do not establish these capabilities.

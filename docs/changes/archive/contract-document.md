# Contract document

## Scope and method

- Make the [world contract](../../porting/world-contract.md) the normative
  current core/platform interface under the [record model](../../principles.md),
  addressing M2/M6 in the [interface review](../core-interface-review.md).
- Restore the pre-change guide and the contract, engine, and layout/admission
  decisions. Preserve normative sentences, except verified version corrections.
  Keep decision clauses in decisions; do not migrate approved v4 clauses.
- Add source-backed ownership, current message shapes, open questions, F31,
  and conformance references. Move only pure history/status paragraphs verbatim;
  keep mixed paragraphs unchanged. Change neither code nor contract shape.
- Acceptance: account for every removed original line, compare all three
  decisions exactly, resolve inbound guide/decision links, and verify whitespace
  and ASCII additions, including untracked files.

## Provenance

Pre-change originals are identified by Git blob id, not filesystem location:

| Original | Blob |
| --- | --- |
| Adapter guide, 672 lines | `0143f7cb3e894ae24ae4b94a4e0f6a4ea8f04f30` |
| Contract decisions | `cf7e8f467f71418541f083833bde8cdcbb30a33e` |
| Engine decisions | `e508a4db4afbfbd443d16dff69838e18e5c0d7ae` |
| Layout/admission decisions | `2572010c8cad78bfa6f6a39537a7f4fe9091bacd` |
| Documentation index | `dfed2addbe5204e7454f7f1c03d18baf6ab450dd` |
| Development loop | `9b41fe67066b7f548f31449311b451f2f6c38e64` |
| KDE shared-policy adoption record | `3a34cdfe30f93fd7f8715c0ef13c939a9aa27e63` |
| macOS world-port record | `fc44508a9b8c65a9d311517a3a8b2abc5b3d793d` |

## Additions and placement

- Ownership table distinguishes native fact normalization/projection, shared
  classification/policy, host limitations, actuation, settlement and rendering.
  Each row links existing rules or source definitions.
- `Approved, not yet implemented (v4)` is a title-and-link list only. Full
  clauses, rationale and rejected alternatives remain in the restored decisions.
- `Current message shapes (v3)` enumerates report/reply fields, command payloads,
  results, settings/capabilities and nested fact, attempt and presentation rows,
  with source-line citations. The nested `CoreCommand` vocabulary links its
  codec rather than duplicating that enum.
- `Open contract questions` separates current code/prose from approved targets.
- The border/motion section adds F31's reproduced-failure plus readable host
  completion-signal gate, links R20, and routes discrepancies to Shared:
  tolerate host-settled geometry. The existing G4 rule and decision link remain.
- `Conformance fixtures` lists world-wire, window-policy, world-actions/cursor,
  world-stress and stage-5 facts with source-backed consumers. World-wire
  consumption by Windows/macOS is explicitly not verified.

## Moved paragraphs

- Original lines 283-285: the `tests/world_minimized_native_pinning.rs`
  coverage/status paragraph, plus its following blank separator. Its three
  text lines are preserved verbatim in the
  [history archive](adapter-guide-history-2026-11-01.md), inside a
  Markdown fence. No normative requirement is moved.

## Mixed paragraphs, not split

Ranges below refer to the 672-line original. Whole paragraphs remain unchanged
except the listed codec-version correction; no rule/status sentence splitting
was attempted.

| Original lines | Mixed paragraph or block |
| --- | --- |
| 8-12 | KDE cutover evidence plus native-settlement prohibition |
| 27-32 | Stress connector evidence plus hidden-geometry/write fences |
| 146-154 | No-focus encoding approval/status plus command and refusal rules |
| 155-165 | Directional selection rules plus platform delivery status |
| 166-175 | Return-focus rules plus adapter scope/status |
| 180-184 | Birth-clear capability and disabled behavior plus opt-in status |
| 185-194 | Born-maximized tokens/proofs plus offline acceptance |
| 201-208 | Host-owned receipts plus amendment history |
| 209-218 | Nullable native current, membership rules and delivered status |
| 219-225 | Native-order/removal/sticky rules plus support qualifications |
| 237-243 | Creation correlation rules plus tentative review status |
| 253-259 | Host-hidden projection/safety plus deferred Activities testing |
| 260-267 | Minimized relocation rules plus parked sticky-slot claim |
| 269-281 | Dated minimized-path audit and existing-behavior rule table |
| 298-311 | Classification ordering/latches plus Windows test evidence |
| 313-320 | Membership classifier rules plus tentative/platform status |
| 322-334 | Retained-membership gates plus implementation status |
| 336-345 | Raw-ingress transport boundary plus partial-port status |
| 466-475 | Native-attempt proof rules plus delivered/tentative status |
| 518-532 | Adoption heading, launch rules and native-acceptance gap |
| 534-560 | Binding/migration rules plus parked/tentative provenance |
| 564-587 | Coordinates, size hints and clamp rules plus platform evidence |
| 612-620 | Fullscreen suspension opt-in plus adapter construction status |
| 621-629 | Suspension/staleness rules plus tentative technical vocabulary |
| 630-640 | Independent styling rules plus platform token examples |
| 649-659 | Core baseline evidence plus explicit acceptance scope |
| 660-662 | Logical receipt limitation and native-proof boundary |
| 663-668 | Native acceptance requirements plus trace-vocabulary history |
| 669-672 | Checker requirements plus connector scope/status |

## Open contract questions

- Per-report capabilities/settings versus connection handshake and revisioned
  settings; projected `WindowFact` versus raw fact ingress; retained frame and
  intent storage versus core-owned opaque persistence.
- Two `SendSource` fallback variants versus one approved action-context chain;
  per-screen `primary` boolean versus optional primary-output id; protocol
  rejection versus typed rejection in an accepted reply.
- Existing decoration freshness versus the shared revision rule; missing R10
  scale, R11 physical-display identity/duplicate fallback, and R14 fullscreen
  ownership/exitability and shared commands.
- R12 recoverable suppression advertisement, R18 external command routes, and
  R20 anchored border descriptors versus current absolute shapes.
- No transition is chosen. The options are retaining v3 until migration or
  implementing/reviewing the new shape separately. The guide links each
  mismatch to current source and the approved decision.

## Verification and outcome

- Original-to-final guide diff: 280 additions, 8 removals. Four removed lines
  are replaced only for version/title corrections (original 1, 4, 73, 337).
  Three removed text lines (283-285) are archived verbatim; one removed blank
  separator is original 286. No other removed line or normative rewrite.
  The final guide has 944 lines; 664 original lines remain verbatim.
- `cmp` confirms all three decision files exactly match their original blobs;
  there are no decision-file link exceptions.
- Archive extraction/`cmp` and `grep -F` confirm the moved paragraph's text.
- Independent review found a mixed paragraph incorrectly archived and an
  overstated classification-ownership row. The paragraph was restored whole;
  the table now separates normalization from shared policy.
- Restored all three classification inbound anchors to their original target,
  which exists again after restoration; the prior retargets no longer apply.
  The index/development-loop version labels now say v3. Unrelated existing
  decision-restructuring links are unchanged.
- Repository-wide targeted validation scanned 1,436 tracked/untracked text
  sources (627 Markdown, 809 code/text) and checked 1,088 links into the guide,
  decision index and decision topic files: zero unresolved. The check includes
  reference definitions, same-document fragments and explicit/heading anchors;
  fenced Markdown source is excluded.
- `git diff --check` is clean. Scoped tracked additions and the untracked
  decision, archive and change-record files contain zero non-ASCII added lines;
  the supplemental whole-file scan is also clean.
- Documentation acceptance is satisfied with the mixed-paragraph exceptions
  below. Native tests are outside this documentation-only scope.

## Deviations and remaining risks

- Discarded the previous rewrite and decision-clause migration: they lost
  normative detail (including R11 identity/fallback, R12 suppression, R14
  fullscreen facts and R20 descriptor fields). Exact restoration replaces that
  approach, not another paraphrased reconciliation.
- The no-history/status goal is constrained by the required preservation of
  mixed paragraphs. Those paragraphs remain as explicitly listed exceptions;
  removing their embedded status requires a separately reviewed text change.
- Python was used for local Markdown-link/anchor validation where no suitable
  CLI checker was available, and for ASCII validation. The first link-check
  slug implementation produced false positives; corrected results supersede it.
- Current v3 and approved future clauses still differ. This document records
  the gaps; it does not authorize a migration or imply native acceptance.

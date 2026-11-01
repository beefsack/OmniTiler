# Backlog trim

## Scope and acceptance

- Make the backlog pending work only: one short line per item, preserving sections, order, priorities, remaining actions, ownership, blockers and gates.
- Preserve removed detail in linked existing change records or the backlog archive; preserve live-check steps in linked records.
- Inventory and ledger cover all 2,924 original lines without gaps or overlaps.
- Verify moved text, account for every pending-marker sentence, check links, whitespace and ASCII-only additions. No builds or live tests are needed.
- Do not change principles, decisions or code, except necessary backlog-anchor link retargets in documentation.

## Method

- Partition the original by heading and top-level item; assign kept, done/superseded or merged, with evidence for removals.
- Cut detail verbatim to existing linked change records where available; otherwise use the backlog archive. Summaries name remaining dependencies and link to complete detail.
- Record narrow text-preservation exceptions by original line: section signs become "section"; the commit-metadata clause preserves its user-owned decision wording.
- Rendered inline-link targets may be retargeted from their new file, preserving labels and all other source text; record per-file counts in the ledger and verify with targets normalized. Fenced literal links stay unchanged; pre-existing bad fragments are retained.
- Record all dispositions, destinations, exceptions and verification in the dated trim ledger.

## Current evidence

- Inventory: 245 contiguous blocks, comprising 234 items and 11 heading blocks; lines 1-2924 covered with zero gaps or overlaps.
- The [dated ledger](backlog-trim-ledger-2026-11-01.md) now classifies all 245 ranges: 232 kept items (including three goals), two done items and 11 structural blocks. Done items are restart float-intent preservation (992-997) and Windows visibility diagnostics (1149-1155); both originals state delivery without a remaining action.
- Superseded Windows implementation routing, macOS FULL-display bounds, Ubuntu acceptance and Windows FFI consolidation remain kept because their originals explicitly name remaining native or physical work.
- Detail destinations include the backlog archive, linked platform/interface-review records and [pending live-check detail](../pending-live-checks.md) for 68 check groups. The 2026-10-31 R14, Rust/memory-safety, KDE effect Rust plan, license-scope and G4/R5 histories were trimmed with the same method.
- The active backlog is 285 lines: all original ranges have been migrated, all six sections and four live-check subsections remain, and 232 kept items pair in original order with 232 active bullets. All 134 priority labels match per item, including two conditional labels; the 132 unconditional labels comprise 17 P0, 69 P1, 35 P2 and 11 P3.
- Final independent current-file comparison found 234/234 original item blocks, including both done items, after only the recorded normalization, redaction and link-target substitutions below. Trailing blank range separators were excluded. Pending-marker accounting finds all 566 original marker-bearing source lines at their current mapped destinations, with zero unaccounted; matching complete blocks also preserves line-wrapped sentences.
- Four known issues whose latest original date is before 2026-10-25 have the requested not-reverified suffix. Three undated original issues remain undated; linked documents were not used to invent a current status date. No live status was re-tested.
- Final tracked whitespace and no-index whitespace checks for all 29 untracked Markdown files pass; added tracked Markdown lines and complete untracked Markdown files are ASCII-only. A trailing blank line in the new live-check record was repaired without changing source text.
- All 232 active detail links resolve. Current-file checks verify all 207 retargeted occurrences and all 24 ledger counts; 213/217 rendered moved links resolve fully, four retain only their original fragment defects and zero new rendered-link regressions remain.
- Final repo-wide scan read 630 Markdown files and 6,957 rendered inline/reference occurrences: all 1,181 scoped references to backlog, backlog archive and 123 touched change-record files resolve, including 41 reference-style occurrences. There are zero missing files or fragments in that scope.
- Semantic acceptance review covered all 234 original items against 232 summaries and two completed classifications. No misleading completion, lost user decision, unsupported action or omitted critical remaining headline was found. Detailed approval clauses, unresolved gates and TBD outcomes remain linked.

## Normalized and redacted text

- Original 1932-1933: U+00A7 before 5 or 6 becomes "section 5" or "section 6".
- Original 2112-2113: replaced with `  Fixing it rewrites published history (force-push); user-owned decision.` Original 2110-2111 are unchanged.
- `link-retargeted`: 207 rendered target changes across 24 destination files; labels, surrounding text and fenced literals are unchanged. Four changes retarget only paths while preserving known bad source fragments.

## Pre-existing fragment defects retained

- Original 1784: `research/macos-port/native-spaces-routes.md#user-manual-transfer-check--2026-10-16`.
- Original 1940: `research/macos-port/native-spaces-routes.md#live-verdict--2026-10-16`.
- Original 2187: `research/release-0.1-triage.md#proposal-b---pending-live-checks-53-top-level-m39--k12--w2`.
- Original 2394: `spec/reference-outcomes.md#r-max-04--r-flt-02-kde-held-key-discriminator`.
- Paths now resolve from the new records; these four fragment strings remain unchanged from the original, not newly introduced link defects.

## Deviations and outcome

- Earlier temporary Node scripts generated exact-range, multi-file patches and checked preservation; shell tools alone were not used for those transformations. Extraction, deletion prefixes and insertion context required repairs. The placement defect was repaired by moving the 75 summaries, unchanged, before the macOS-port item and removing the duplicate marker; subsequent order checks pass.
- Later migration used inspectable shell/awk/sed transformations rather than generated multi-file patches. One source-path typo left a range heading without its body; the body was restored and its exact comparison passed. Initially clipped live-check summaries were rewritten as complete remaining-work sentences. A missed original risk date was corrected to 2026-10-19.
- Independent verification used temporary Node scripts for Markdown link parsing and item-sequence checks. Six active detail-link defects were corrected: three missing `changes/` path prefixes, one omitted range fragment and two slash-containing heading slugs. The Windows input-thread record now targets the relocated common-definition range. A clipped Core/KDE-order label and an editorial sentence were also cleaned up without changing moved text.
- Earlier completion classifications incorrectly treated delivered subparts as completed whole items. The four remaining-work cases above were corrected in the disposition manifest and accepted by the completed source-to-summary review.
- The original link conflict involved 504 source-link occurrences: 287 fenced literals and 217 rendered Markdown links. Before repair, 202 rendered links had moved-path regressions, one had a moved-fragment regression and four already had missing source fragments. Earlier path-only counts incorrectly included fenced literals as rendered defects; that audit was corrected.
- The conflict is resolved by the link-target preservation exception. Target-only repair now covers 207 occurrences, including path-only correction for the four known bad fragments; all 217 rendered moved-link paths exist, 213 resolve fully and four retain their original fragment defects. Earlier fenced source blocks are unchanged.
- Repair used an inline Node operation driven by the rendered-link audit and range manifest; no repository script was added. The original-href audit was not reused as post-repair evidence because it does not inspect current targets.
- Normalized review found one extra leading space at original line 1784 in the relocated macOS block; the indentation was restored before the final 234/234 proof. Verification helpers with stale target bases or wrapper assumptions were corrected; their invalid findings were discarded, not used as acceptance evidence.
- Accepted outcome: pending-only, ordered, one-line backlog with preserved detail and zero new rendered-link regressions. The four pre-existing fragment defects above remain intentionally unchanged. The custom Markdown/GFM-style verifier covers inline, multiline and reference links, not raw HTML or external links; historical native-status claims were not re-tested.
- Next action: none for this trim. No build or live-test evidence is claimed; this work made no code changes and did not alter unrelated working-tree edits.

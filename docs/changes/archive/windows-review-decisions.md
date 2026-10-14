# Windows review decision promotion

## Goal and scope

- Promote User 2026-10-14 selections to topic sections in
  [decisions](../../decisions.md); clear matching labels in the spec, reference
  outcomes and active records. Keep the backlog limited to open follow-ups.
- D27 5b remains unresolved; D23 and fixed-size born-maximized policy remain
  research. Existing fullscreen/game guard and unrelated decisions stay intact.

## Acceptance and verification

- Accepted choices have one durable decision home and linked references;
  Windows float/maximized tie research is closed.
- `git diff --check` passed. No docs link/consistency gate exists in `just/`
  or CI; changed decision links were checked against their topic headings.

## Outcome

- Decisions 2, 3, 4, 5a, 7-12 and the accepted parts of 13 promoted. Decision
  13f explicitly stays open research; 13h closes the P2 tie-order research.
- Matching tentative labels cleared in the spec, reference outcomes and active
  records; unrelated tentatives retained. Backlog review block now holds only
  open questions and follow-ups, including the unresolved D27 guard question.
- [Dev entry point](../dev-entry-point.md) remains active for Linux KDE
  streaming/teardown acceptance; Windows acceptance is complete.

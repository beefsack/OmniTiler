# Custom Tile Runtime Acceptance

## Goal

Establish a stable Custom Tile runtime for the MVP, including drag/reflow,
floating, fullscreen, maximize, and workspace behavior on KWin.

## Scope And Non-Goals

- Static implementation is delivered. Live claims require the procedures in
  [the live testing guide](../live-kwin-testing.md) and the reviewed standing
  live-test boundary/protocol in [the project decisions](../decisions.md).
- Drag signal delivery, reflow, motion cleanup, cadence, and XWayland behavior
  remain unproven. The KWin client-realization drag gap is not an engine defect.
- Under the approved pragmatic risk/time tradeoff, exhaustive Custom Tile
  live-harness recovery is outside the immediate MVP critical path. Residual
  acceptance remains a separately authorized, user-performed manual/live check;
  this deferral is not runtime proof.
- Deferred window eligibility now attaches the same interaction handlers as the
  immediate path; its static evidence is accepted, while the generic Steam
  move/placement journey remains parked.
- True tabs, stacked windows, shared tiles (multiple windows per tile), and
  compositor-owned groups are outside this MVP record; nested split-tree
  structure remains in scope.

## Moved Evidence (from docs/decisions.md)

- `python3` was added intentionally to `devenv.nix`; the development session
  has been restarted and the committed dependency is available.
- The static carrier-only operational provenance harness is verified, but no
  successful carrier smoke occurred: bounded attempts either stopped before
  effect or were receipt-bound restored. A new smoke is blocked by two retained
  protected project runtime evidence records. Handling those records requires
  explicit user authorization under a race-safe recovery procedure; after that,
  one bounded carrier-only smoke must prove exact host baseline equality. A
  separately authorized Custom Tile journey remains a later gate.
- The inert checkout carrier establishes only operational lifecycle binding
  through its exact plugin/script identity, receipt, diagnostic, and unchanged
  KWin identity. Current public KWin APIs do not provide direct evaluated-memory
  source proof for the checkout controller, so `authoritative_ready` remains
  false; the carrier does not change that verdict.
- The preflight reports phased readiness: `setup_ready` proves only its
  read-only KWin, KGlobalAccel, shortcut, and persisted-state checks completed
  without drift; `journey_ready` and `authoritative_ready` remain false until
  the applicable acceptance gates are established. No readiness phase
  authorizes a Custom Tile lifecycle, live journey, or user physical or manual
  action on its own; carrier setup is limited to its bounded operational
  binding.

## Acceptance

- On disposable, project-owned topology with exact restoration of owned resources, Custom Tile
  structure remains stable through window add/remove, drag/reflow, floating,
  fullscreen, maximize, and workspace journeys.
- Live runtime acceptance requires user-performed physical or manual
  observations; session boundaries require user action; no unperformed live
  result is claimed.

## Approach And Dependencies

- Re-resolve tile handles after structural removal and reconstruct only through
  the accepted deferred path.
- The COSMIC movement and nested-placement records depend on this runtime's
  stable tree behavior.

## Material Safety Finding

- Never run `remove()` followed by `split()` against a changed tile tree, use a
  timer as a deferred-deletion barrier, or run structural probes in a persistent
  user scope. Re-resolve tile handles after removal.
- Isolated nested evidence supports deferred reconstruction that re-resolves
  after removal. It proved 4-to-3 and 3-to-2 rebuilds, but did not establish
  one-to-zero cases, detached-window close, physical shortcuts, or host live
  acceptance.

## Read-Only Preflight

- The accepted static harness is a current-session read-only preflight. It
  strictly diagnoses KWin and KGlobalAccel ownership, and fails closed on stale
  state, shortcut collision or drift, service drift, and provenance ambiguity.
- It establishes a prospective rollback and journal contract for a later
  authorized run only. It performs no lifecycle or mutation and persists no raw
  host evidence.
- `authoritative_ready` remains false because current public KWin APIs provide
  no direct evaluated-memory source proof for the checkout controller. The inert
  checkout carrier is a separate operational lifecycle binding and does not
  change that verdict.
- `setup_ready` means the read-only KWin, KGlobalAccel, shortcut, and
  persisted-state checks completed without drift. `journey_ready` remains false
  with `authoritative_ready`; neither phase nor the carrier setup permits a
  live journey or user manual action on its own.
- No Custom Tile runtime acceptance or journey occurred; the carrier
  setup/restore smoke is not claimed as passed and is not acceptance evidence.

## Current Evidence Assessment

- `devenv.nix` intentionally adds `python3`. The development session has been
  restarted, so the committed dependency and preflight are available. The
  static carrier-only operational provenance harness is verified.
- No successful carrier smoke occurred. Bounded carrier attempts either stopped
  before effect or were receipt-bound restored; no Custom Tile runtime lifecycle
  or mutation is claimed.
- An exhaustive harness smoke is blocked by two retained protected project
  runtime evidence records. Their handling requires explicit user authorization
  under a race-safe recovery procedure; that recovery is deferred outside the
  immediate MVP critical path, and no recovery action or successful carrier
  smoke is claimed.
- The residual gate remains a separately authorized, user-performed manual/live
  Custom Tile check with exact restoration of owned resources. No Custom Tile runtime journey has
  occurred.

## Verification

- Static carrier-only/read-only harness verification passed: 126 passed, 0
  failed. Bash syntax and diff checks passed; `shellcheck` was unavailable.
- Independent adversarial review found a post-enumeration KGlobalAccel shortcut
  drift gap. The harness now takes and compares a second exact shortcut
  contract snapshot before `setup_ready`; targeted regression coverage passed.
- This is harness/preflight verification only. No live Custom Tile, drag/reflow,
  or related journeys were run.

## Material Decisions And Accepted Evidence

- Stable nested split-tree behavior is an MVP dependency for COSMIC movement
  and nested placement.
- Accepted isolated evidence covers deferred reconstruction after removal, but
  does not establish one-to-zero cases, detached-window close, physical
  shortcuts, or host live acceptance.

## Next Action

- Keep exhaustive stale-harness recovery out of the immediate MVP critical path;
  do not invent recovery, remove ambiguous residue, or claim a carrier smoke.
- Retain the residual gate for a separately authorized, user-performed manual
  Custom Tile live check with exact restoration of owned resources; no journey has occurred.
- Continue to treat `authoritative_ready` as false unless a supported direct
  evaluated-memory source proof is established.

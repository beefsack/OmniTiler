# Windows floating fullscreen frame restoration

## Goal and acceptance

- Restore the exact pre-fullscreen floating frame when leaving fullscreen,
  including after a same-output workspace transfer.
- Preserve tiled reflow, maximized restoration, float intent and unrelated
  window geometry. Do not change shared core or KDE behavior.

## Reproduction and approach

- Agent-live 2026-10-17: a small ordinary float entered genuine fullscreen,
  transferred slotlessly to an empty workspace, then consumed fullscreen exit.
  The exit reported restoration but retained the fullscreen frame. Transfer
  and sibling-frame preservation passed; destination frame restoration failed.
- Investigate Windows-local fullscreen metadata and exit actuation; retain the
  pre-fullscreen frame and restore it through the existing guarded exit path.
- Add a regression for a non-degenerate floating frame and verify native exit
  both without a transfer and after a workspace send.

## Verification and bounded work

1. Implement the smallest Windows-local correction and targeted regressions.
2. Independently review native mutation, identity and restoration safety.
3. Repeat the discriminating native journey and the locked four-package build,
   test, whole-workspace formatting, strict all-target clippy and diff checks.
4. Record outcomes, publish and inspect cross-platform CI.

## Outcome and evidence (2026-10-17)

- Windows-local fullscreen metadata now retains the pre-fullscreen outer frame.
  Non-maximized exit restores it; maximized exit retains native placement
  restoration. Legacy style-only metadata remains exit-compatible. Failed
  restoration retains metadata for retry; verified cleanup removes all props.
- Four new regressions cover a small preimage, legacy metadata, failed geometry
  restoration and signed-coordinate encoding. Maximized coverage verifies that
  a carried frame does not override native maximized placement. Workspace
  transfer itself is proved by the native journey, not a duplicated exit test.
- Agent-live: a small ordinary float restored its exact pre-fullscreen frame
  both directly and after a slotless stay-send to an empty workspace. Each
  restored frame stayed stable through subsequent ticks; sibling frames were
  unchanged and the final tiled baseline had no fullscreen residue.
- Independent review accepted the correction after repairing incomplete
  property cleanup and removing a redundant test. No shared core/KDE change.
- Locked four-package build/test passed: 2,469 passed, zero failed, one ignored.
  Whole-workspace formatting, strict all-target clippy and diff check passed.
- Graceful stop and independent restore passed; saved settings and SPI state
  were unchanged. Publication and cross-platform CI are recorded below when
  available.

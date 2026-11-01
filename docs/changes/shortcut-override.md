# Shortcut Override

## Goal

Explicit KCM shortcut override for the MVP under the current contract in
[decisions](../decisions/reference-and-shortcuts.md#shared-shortcut-defaults-and-conflict-model): Apply assigns the five
project-required chords; confirmed Force clears any holder of those chords;
Revert restores KDE defaults for cleared non-project actions.

## Scope And Non-Goals

- Explicit KCM Apply, Force Apply (preview + confirm + revalidate), and Revert
  only. Installation, startup, and ordinary settings Save never mutate
  shortcuts. No per-component `cleanUp()`.
- Project-required chords: `Meta+L` (focus-right; relocates
  `ksmserver/Lock Session` `Meta+L` to `Meta+Esc`), `Meta+Alt+K`,
  `Meta+Alt+L`, `Meta+G`, `Meta+M`. Sole authorized target occupant:
  System Monitor `org.kde.plasma-systemmonitor.desktop` / `_launch` on
  `Meta+Esc` (displaced, never written).
- Force clears only the required keys on each listed holder; unrelated keys
  are preserved. Revert restores the full KDE default set for non-project
  cleared actions via `defaultShortcutKeys`/`setForeignShortcutKeys`;
  custom cleared bindings are lost (user-accepted). Project-owned IDs
  (`kwin/omnitiler-*`, current and legacy) stay cleared.
- Durable state is the minimal cleared component/action ID list at
  `~/.config/omnitiler/shortcut-clearedrc` (Components+Actions
  only, no cosmetic labels), union-persisted by ID BEFORE Force clearing
  and emptied only after successful Revert. Force preview transient
  labels are never persisted. Revert resolves each persisted ID to its
  fresh current tuple from `readAll` to supply the current friendly
  labels (empty allowed) for the 4-field `defaultShortcutKeys` /
  `setForeignShortcutKeys` actionId; absent or duplicate IDs fail closed
  without writing unrelated actions and retain the list for retry. No
  2-field daemon behavior is assumed. Existing journal files
  (`shortcut-override-journalrc`, including kcmshell6 legacy) are ignored
  and untouched: no migration, Finish Apply, or Restore path.
- Layout detection, omission, opt-in configuration, and complete
  keyboard-layout support remain deferred.

## Acceptance

- Apply refuses closed on any unexpected holder of a required chord, with
  zero writes.
- Force preview lists every active holder (known, unknown, legacy project
  IDs) with found keys, exact required keys removed, and unrelated keys
  kept; project actions, Lock Session, and the authorized System Monitor
  `Meta+Esc` holder are exempt. `.desktop`-default-only claimants with
  nothing to clear block Force until unbound manually.
- Confirmed Force revalidates owner, project/lock live images, and the full
  holder snapshot against fresh state; stale confirmations before persist
  fail with zero writes, including zero cleared-list writes.
  Forged/duplicate/unbounded confirmations refuse. After the union is
  persisted, each holder is re-read immediately before its foreign setter
  and aborts on active drift with zero further KGlobalAccel writes; the
  persisted union is then retained as an interruption-safe superset, so a
  later Revert may restore defaults for an action Force never cleared.
- Revert on an empty list is a no-op success; partial failure retains the
  list for resume, including absent/duplicate-ID resolution failures with
  zero unrelated writes. Owner drift fails closed.
- Bounded structured diagnostics on `omnitiler.shortcut` only
  (`op=`, `stage=`, `outcome=`, allowlisted identity, key images); foreign
  occupants redacted, logging never gates behavior. Query with
  `journalctl --user --no-pager -g "omnitiler.shortcut op="`.
- KCM: Force Apply/Cancel appear only for a pending preview; Cancel discards
  without writes; Revert confirms the recorded cleared count and the loss of
  custom bindings. No Finish/Restore controls.

## Verification (offline, uncommitted tree)

- Recorded interim verification per `docs/changes/archive/multi-output-failures.md`
  (Note, 2026-09-26): `just build-native-effect`, host-matched native
  CTest 29/29 (shortcut subset 15/15 at the transport green point), and
  `git diff --check` pass. Interim tree ~2500 insertions / ~5080 deletions
  vs HEAD, covering transport seam (`defaultShortcutKeys`,
  `setForeignShortcutKeys` with owner pinning, strict reply validation,
  void-setter readback), backend clear/revert rewrite with cleared-list
  regression tests, and KCM preview/cancel/confirm plus Save-isolation tests.
- This docs unit ran no live bus, KWin, Plasma, or config mutation; no
  tests, staging, or commit.

## Outstanding Live Checks (second multi-output test system, user-owned)

- Apply, Force (preview/confirm/revalidate/clear), and Revert on the
  multi-output test system, including the reported Apply refusal and unknown/legacy
  holders: unproven.
- Physical chords (`Meta+L`, `Meta+Esc` lock, `Meta+Alt+K/L`, `Meta+G`,
  `Meta+M`), exact Revert-to-default restoration, and interrupted-Force
  resume via the cleared list: unproven.

## Historic Evidence (compact, not current)

- Journal-era static coverage (closed allowlist, preimage journal, legacy
  migration, Finish/Restore) is superseded and removed; retained in Git
  history only.
- One live Finish Apply completed the three-row postimage with a physically
  verified `Meta+L` focus move (`kglobalshortcutsrc` sha256
  `e412626d...f4614`); Revert/Restore and resize/float/maximize physical
  checks from that era were never proven. Read-only captures (20 components,
  349 tuples) fixed empty-friendly validation and the out-first setter
  contract. No stateless-Revert or never-cleared-action mutation is claimed.

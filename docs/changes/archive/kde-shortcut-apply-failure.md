# KDE Shortcut Apply Failure

## Goal and acceptance

- Fix full-catalog Apply/Force failure and keep Settings responsive during
  shortcut discovery and writes.
- Preserve confirmed Force's owner, draft, full-snapshot and per-holder drift
  checks, exact-key clearing, and durable cleared-ID recovery.
- Explain unchanged versus relocated Lock Session bindings accurately; show
  complete holder identities without inventing ownership from an ID prefix.
- Add meaningful regressions, pass affected gates, and verify Apply/Force
  against live KGlobalAccel state with measured latency.

## Scope and approach

- KCM and shortcut transport/reconciler only. Existing presets, Keep semantics,
  System Monitor exception, and default-restoration semantics remain the contract.
- Investigation found a 64-write transport lifetime cap inconsistent with the
  130-write project bound, plus repeated blocking discovery on the UI thread.
- Bounded implementation: align write bounds with confirmed operations; remove
  redundant work where equivalent evidence is retained; keep expensive shortcut
  work responsive without weakening fresh-state checks; correct exemption text.
- Follow with independent mutation-boundary review, offline gates, and a KCM-only
  live reload. Rebuilt effect delivery requires a separate login boundary.

## Decisions

- Approved by user 2026-10-14: use operation-scoped bounded writes so repeated
  legitimate Apply/Force/Revert actions do not exhaust the Settings lifetime.
  Additional serialization/throttling was considered and rejected as unnecessary:
  writes are already serial and fast.
- Approved by user 2026-10-14: omit the Lock Session exempt line entirely when
  actual and post keys match; show it only when Apply moves the binding. Retain
  preserved-alternate wording and the approved System Monitor exception for moves.
- Approved by user 2026-10-14: remove previous-identity handling for
  `kwin/plasma-auto-tiler-*`; these are ordinary foreign holders. They were
  pre-release development residue, not a migration requirement. No pre-release
  migrations; prefer Simplicity.
- Approved by user 2026-10-14: run shortcut discovery/mutation on a serialized
  background thread with visible working status and disabled shortcut controls.
  Modal Force holds the same guard; reload evidence is bound to the draft epoch.
  An in-flight D-Bus hang can delay Settings close. Per-call deadlines are not
  adopted now; revisit only if a hang is observed.

## Root causes and fixes

- `shortcutreconciler.cpp`, `KGlobalAccelStore::writeKeys` and
  `setForeignShortcutKeys`: the transport refused after 64 lifetime writes,
  although the catalog-derived project budget is 130. Full-catalog Force with
  dozens of clears could never finish, and earlier operations consumed the same
  Settings-window counter. Removed the obsolete cap; reconciler checks bounded
  per-operation budgets before setters. Write totals remain monotonic for
  diagnostics. Existing owner pins and setter/readback validation remain.
- `unifiedsettings_module.cpp`, Apply/preview/refresh: full synchronous D-Bus
  discovery ran repeatedly on the UI thread. Serialized background execution
  now covers discovery, presets, Apply, Force and Revert. Fresh full Force
  revalidation and immediate per-holder checks remain intact; no stale snapshot
  is substituted for current evidence.
- The initial delivery added previous-identity labels and a project-owned
  Revert skip. User review rejected that special handling; both are removed.
  Old-name records now follow ordinary foreign-holder reporting and Revert.
- The lock text always said "will move", even when actual and intended keys
  matched. `Screensaver` is a real alternate key in Lock Session's key set,
  not another action. The dialog now omits unchanged bindings, describes preserved
  alternatives for actual moves, and describes System Monitor as retained/displaced
  without claiming a physical delivery observation.

## Verification and outcome (2026-10-14)

- Settings-only Qt/KF production and tests build with `-Wall -Wextra -Werror`;
  native CTest 26/26 pass. Full-catalog tests cover 94-write Apply, 137-write
  Force, repeated operations, foreign-holder clear/Revert and multi-key lock text.
- Async regressions cover event-loop responsiveness during delayed reads/writes,
  store-thread affinity, concurrent-request rejection, teardown, modal refresh
  followed by Force, and two loads with intervening live binding drift.
- Independent mutation-boundary review accepted owner/draft/full-snapshot checks,
  per-holder drift refusal, union-before-clear recovery, Keep and Save isolation.
  Review corrections fixed stale reload staging and modal refresh dropping an
  accepted Force. An initial reload correction needed a further epoch fence to
  handle two load-started discoveries; the discriminating regression now passes.
- `cargo fmt --all -- --check`, workspace tests (1680 passed), all-target
  Clippy with warnings denied, KWin build/tests (1491 passed), and
  `git diff --check` pass. A static-boundary test initially rejected a comment
  mentioning the transport class; rewording that comment restored the gate.
- Live KCM was loaded from a settings-only build, with its mapped library
  independently verified. No effect build or replacement was needed.
- Existing historical-holder chords were already cleared at baseline. A
  reversible fixture emptied 93 current project assignments and assigned
  their chords to 43 verified historical project records. The full 20-component,
  535-tuple live snapshot and configuration preimages were captured first.
- Apply refused with zero writes and displayed all 43 conflicts in about
  185 ms. Cancel preserved the complete fixture snapshot and recovery-file
  bytes. Confirmed Force succeeded with 136 writes in about 250 ms: 43 clears
  plus 93 assignments; Lock Session was already relocated, so no lock write.
- In the same Settings window, conflict-free full Apply then succeeded with
  93 writes in about 187 ms. Both result dialogs said "Shortcuts applied."
  Accessibility round trips remained about 3-13 ms during work. Post-operation
  live snapshots matched the baseline, with empty-key sentinels normalized.
- Shortcut and recovery configuration bytes were restored exactly; the KWin
  configuration was unchanged. The test KCM closed normally. Existing windows
  were preserved and KWin owner identity remained stable.
- Both rebuilt settings KCMs are staged for reopening; the effect binary is
  byte-identical to its pre-test image.
- Approved-review followup: deleted previous-identity helpers, matches, labels
  and their tests; unchanged Lock Session line omission is covered by the dialog
  regression. Fresh settings-only build with warnings denied and CTest 26/26 pass.
  Followup staging and live reopen remain pending because the staging recipe also
  replaces the effect payload and loaded-library identity was not verified.

## Remaining scope and risks

- Apply/Force Settings failure delivered. Broader physical-chord, restart,
  Revert-to-default and interruption acceptance stays with the existing
  [shortcut verification](../../live-shortcut-override-verification.md) work.
- Background operations still use the transport's existing D-Bus timeouts;
  closing the module waits for in-flight work to preserve store lifetime. An
  unavailable daemon can therefore delay close. User 2026-10-14 accepts this;
  per-call deadlines are deferred unless a hang is observed. Healthy-daemon
  latency measurements do not prove hung-daemon behavior.
- Next action: resume the separate KDE dual-output acceptance item.

# Windows shortcut settings dialogs

## Goal and scope

Completed the Windows shortcut-dialog handoff after the shared-catalog four-tab
Settings rebuild. Add modal conflict previews and concise Apply/Revert results
without changing KDE, the shared catalog, settings persistence, or runtime
adoption. Existing Windows Revert reloads saved settings, not shortcut defaults.

## Acceptance and approach

- Presets remain staged and write nothing until Apply.
- Apply validates first. Known unresolved draft conflicts use one owned,
  scrollable advisory preview with Apply/Cancel, never a fictitious Force action.
  Cancel writes nothing and opens no result dialog. Conflict-free Apply has no
  preliminary confirmation.
- Apply/Revert show concise success/error dialogs; full diagnostics remain
  available. Success says "Settings saved." / "Reverted to saved settings."
  without claiming runtime delivery or defaults restoration.
- Revert protects unsaved full-draft edits with one discard confirmation;
  cancellation retains the draft. Revert never writes or undoes an earlier Apply.
- Preserve stale-file refusal, validation, singleton/stop behavior, shared-catalog
  mappings, and existing timing notes. No KDE or core behavior changes.
- Add pure regression coverage, inspect the final diff independently, run the
  locked Windows package build/test and strict all-target clippy, formatting,
  and diff checks. Confirm non-Windows streams in CI.
- Live-check owned Settings from a scoped tiler: preset/preview Cancel and Apply,
  Apply/Revert success, discard Cancel, and a safe error path. Restore the exact
  settings preimage and verify system-input state is unchanged.

## Material decisions

- Tentative, pending user review: advisory Apply/Cancel adapts KDE's modal flow
  to Windows, where no foreign-binding Force operation exists. Preview uses the
  existing tray-warning conflict set, not every documented OS chord owner.
- Tentative, pending user review: no extra Apply confirmation; Revert asks
  "Discard unsaved edits?" only when unsaved state would be lost. Dialog titles
  use Apply Settings / Revert Settings. Existing saved-settings semantics remain
  authoritative.
- Shared catalog has no operation-result wording to reuse; adapter-owned result
  strings preserve the catalog/native-operation boundary.

## Plan and evidence

1. Read-only status investigation: clean tree at `4b61f120`; `bfc3ccbd` is an
   ancestor. Current UI has no modal Apply/Revert results or conflict gate.
   Windows Revert's saved-file semantics are recorded in decisions and code.
2. Implement bounded Windows-only dialog flow and regressions.
3. Independently review live/destructive boundaries; run gates and live checks.
4. Record evidence, archive on completion, and confirm integration CI.

## Outcome and accepted evidence

- Windows-only changes in `settings_ui/dialogs.rs`, `draft.rs`, and `mod.rs`;
  KDE and shared catalog unchanged. Ten added regressions cover warning-set
  filtering/order, result/error policy, full-draft discard state, stale images,
  dead-owner outcome, and exactly-once deferred close. Settings UI: 63 passed.
- Independent review found an owner-teardown lifetime hazard and an unbounded
  preview-to-save stale window. Both were corrected and the resolution accepted:
  owner close is deferred until the active UI borrow ends, and disk is rechecked
  after preview acceptance. Dead code and a stale lifetime comment were removed.
- Scripted live run opened Settings from the actual scoped tiler's tray menu;
  Settings' parent process confirmed the owner-dispatched route. Preset staging
  wrote nothing. Authentic showed a complete gaming-warning preview; Cancel
  retained draft/file with no second dialog; accepted Apply showed
  "Settings saved.". Compatible produced direct success without a preliminary
  dialog. The running owner observed the keyboard settings revision.
- Revert discard Cancel retained an edited core field; Discard restored the saved
  value, showed "Reverted to saved settings.", and wrote nothing. Invalid-field
  Apply showed a short error and wrote nothing. A file changed during preview
  was preserved and Apply refused/reloaded it with a short stale error.
- An initial Compatible probe targeted a hidden control and did not stage it;
  selecting the real Shortcuts tab and checking staged status corrected the
  automation, confirming conflict-free direct Apply. This was not a product bug.
- Exact original settings bytes/revision restored; scoped owner stopped and
  restore ledger clean; system-input preimages unchanged; no test tiler left
  running. The controlling terminal was untouched. Readable dialog/tray captures
  and diagnostic logs were retained outside the repository.
- Required locked build/test and strict all-target clippy passed for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, and `tiler-windows`; formatting and
  diff checks passed. Aggregate: 63 suites, 2,458 passed / 0 failed / 1 ignored.
- An additional five-package Windows test run including `tiler-macos` stopped
  with eight macOS tests unable to launch POSIX `sleep`/`true` helpers. Its build
  and clippy passed; the required four-package test gate passed separately.
  This unrelated cross-host test limitation was not changed.

Physical shortcut delivery and user acceptance of the tentative presentation
remain TBD. Synthetic input and owner revision logs do not establish either.
Integration CI is pending.

# Ubuntu KWin upgrade note

Status: delivered and verified, 2026-10-13; archived.

## Goal and scope

Deliver the installation-guide and package-description explanation selected
in the [Ubuntu exact-pin decision](../../decisions.md#kde-support-floor).
The Debian effect keeps its exact compiled-against `libkwin6` dependency;
RPM and Arch retain their deliberately unversioned KWin dependencies.

## Acceptance and approach

- Explain Ubuntu's possible held-back KWin update and the options to wait for
  a rebuilt effect or remove only the effect and reinstall its rebuild later.
- Confirm core tiling and Settings remain available without the effect,
  consistent with the split-package decision and existing removal guidance.
- Add `KWin upgrades wait for a rebuilt effect.` to the Debian effect
  description with normal continuation indentation and line length.
- Check existing Debian fixtures, offline source-archive contracts, ASCII
  additions and whitespace; retain all dependency constraints.

## Evidence and outcome

- `bash scripts/debian-packaging.test.sh`: PASS=30 FAIL=0.
- `bash scripts/build-source-archive.test.sh`: offline contract tests passed.
- Debian description continuation lines are at most 80 columns; ASCII
  additions and `git diff --check` passed.
- Installation note and Debian description delivered; all dependency
  constraints retained. [Prior apt solver evidence](ubuntu-apt-solver-acceptance.md)
  establishes the held-back update and effect-only removal behavior.
- Remaining Ubuntu live journeys and OBS provisioning/source handoff stay
  in the [backlog](../../backlog.md).

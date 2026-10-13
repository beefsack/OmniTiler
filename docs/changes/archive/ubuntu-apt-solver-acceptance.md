# Ubuntu apt solver acceptance

Status: solver validation and review complete, 2026-10-13; archived.
Doc/packaging follow-up and live journeys remain queued separately.

## Goal

Offline apt solver validation of the two Debian packages (`omnitiler` core, `omnitiler-native-effect` effect companion pinned to the exact `libkwin6` build incl. distro revision) in disposable containers: Ubuntu 26.04 release pocket (KWin 6.6.4), 26.04 with -updates (KWin 6.6.6), and 26.10 (KWin 6.7.5).

## Scope

- Build both packages using the existing scripted route (reuse what kwin-6.6-support used: source archive + `dpkg-buildpackage -us -uc` in disposable containers).
- Per target in a clean container: `apt install ./*.deb` (real solver), confirm effect pin resolves against archive libkwin6.
- On 26.04: KWin update transaction (install against release 6.6.4 with -updates disabled, then enable -updates and run `apt upgrade`/`full-upgrade`): record whether apt holds kwin back, removes the effect, or breaks.
- Core-only (`noeffect` profile) install and `apt remove omnitiler-native-effect` leaving core installed.
- Record results as evidence here; packaging policy changes need user decision.

Non-goals: no live desktop/KWin testing; no OBS/PPA/neon work; no publishing.

## Acceptance

- [x] Both debs build per target from the scripted route.
- [x] `apt install ./*.deb` passes per target with pin resolving against archive libkwin6.
- [x] 26.04 release-to-updates KWin transaction behavior recorded (hold back / remove effect / break).
- [x] `noeffect`/core-only install passes; effect removal leaves core installed.
- [x] Evidence recorded below; containers/images cleaned except shared base images.

## Approach

Disposable Docker containers with clean Ubuntu archives; builds reuse the
offline source-archive route. Network used during container preparation.

## Units

1. Build packages per target (26.04 release, 26.04 updates, 26.10, 26.04 noeffect).
2. Solver installs per target + pin verification.
3. 26.04 upgrade transaction + removal/core-only probes.
4. Record solver evidence and upgrade-policy review.

## Verification

- `scripts/debian-packaging.test.sh` passes.
- `apt install ./*.deb` in clean containers per target.
- `apt upgrade` / `apt full-upgrade` transaction on 26.04 recorded.
- `apt remove omnitiler-native-effect` leaves core installed.

## Decisions (user 2026-10-13)

- Option (b): keep the exact effect pin; queue an installation-guide note and
  effect package-description line explaining KWin upgrades wait until the
  rebuilt effect is installed. Small doc/packaging change, not delivered here.
- Future upgrade smoothness research: patch-release effect loading
  (IID/version binding), OBS rebuild latency and pin options. No pin relaxation.
- Test-condition note: clean base images ship with no KWin preinstalled, so the release-pocket install needed no downgrade flags; the preinstalled-updates downgrade path remains untested.

## Evidence

- `scripts/debian-packaging.test.sh`: PASS=30 FAIL=0.
- Source archive (one archive, all targets): `VERSION=0.1.0`, `SOURCE_REV=5c4bb16`, SHA-256 `218c1fb744d52dc8eaa05a06e1ee5d6f0fe2f31a257f460d9efc3acdf13f9caa`. Route: `scripts/build-source-archive.sh` + disposable-container `dpkg-buildpackage -us -uc` (`-Pnoeffect` for core-only).
- Base images: `ubuntu:26.04` and `ubuntu:26.10` digests reused from kwin-6.6-support, verified still valid.

### Solver installs (`apt install -y ./*.deb`, clean container per target, all exit 0)

| Target | libkwin6 | Core | Effect | Pin resolves |
| --- | --- | --- | --- | --- |
| 26.04 release (updates disabled) | 4:6.6.4-0ubuntu1 | 0.1.0-1 | 0.1.0-1 | yes (`= 4:6.6.4-0ubuntu1`) |
| 26.04 updates (stock) | 4:6.6.6-0ubuntu0.1 | 0.1.0-1 | 0.1.0-1 | yes (`= 4:6.6.6-0ubuntu0.1`) |
| 26.10 (stock) | 4:6.7.5-0ubuntu1 | 0.1.0-1 | 0.1.0-1 | yes (`= 4:6.7.5-0ubuntu1`) |
| 26.04 noeffect (core only, no kwin-dev) | 4:6.6.6-0ubuntu0.1 via kwin-wayland | 0.1.0-1 | n/a | core installs without KWin headers |

### Upgrade transaction (26.04 release debs installed, then -updates enabled)

- `apt upgrade -y`: exit 0. 32 upgraded, 0 removed. `kwin-wayland` + `libkwin6` listed as `Not upgrading` (held back by the effect pin, no hold marks set). No error, no removal proposed.
- `apt full-upgrade -y`: exit 0. Same 2 held back, 0 removed. Full-upgrade refuses to remove the manually-installed effect to satisfy the upgrade.
- Installing the rebuilt 6.6.6 debs then completes the KWin upgrade in one transaction (`Upgrading: 3`, `Not Upgrading: 0`); follow-up `full-upgrade` is clean.
- Stale-pin probe: installing the 6.6.4-pinned effect on updates sources fails fast with exit 100 (`Unable to satisfy dependencies`, no partial install).

### Removal

- `apt remove -y omnitiler-native-effect` (26.04 updates): exit 0, removes only the effect; core stays `install ok installed 0.1.0-1`; `kwin-wayland`/`libkwin6` remain installed. Settings/Revert stays available in core.

### Finding: KWin update behavior is safe but quiet

When KWin 6.6.6 lands in -updates before a rebuilt effect exists, both `upgrade` and `full-upgrade` succeed with KWin quietly held back (`Not Upgrading: 2`) and everything keeps working on 6.6.4. Nothing breaks and the effect is never silently removed. The cost is discoverability: the user must install the rebuilt effect deb before KWin advances, and a routine upgrade gives no pointer to that. Options for the user: (a) keep current behavior and document "install the rebuilt effect to unblock KWin upgrades"; (b) add an explicit upgrade hint (e.g. package description/NEWS/installation doc wording); (c) revisit only if a future KWin security update makes held-back KWin unacceptable. Recommendation: (a) plus a short installation-doc note; no packaging policy change.

## Outcome

- All solver acceptance passed. No packaging code changes were needed; no tentative fixes.
- All containers removed; shared base images kept. Scratch debs/logs remain under disposable scratch (not in repo).
- Remaining: user-owned live journeys on 6.6/6.7+ and OBS target provisioning/source handoff (backlog).
- Review resolved: exact pin retained, upgrade note/description queued; solver
  note has no remaining active work. [Queue](../../backlog.md),
  [decision](../../decisions.md#kde-support-floor).

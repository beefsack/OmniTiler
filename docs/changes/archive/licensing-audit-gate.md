# Licensing audit gate

Goal: pass tool-based license audit and add CI gate; record native closure per distro and vendored attribution (backlog P1 Licensing).

Scope:
- `cargo deny --locked check licenses`; fix `deny.toml` allow/clarify/exceptions consistent with decided licenses (MIT default, GPL-2.0-or-later native effect, MIT staticlib/KCMs).
- CI license gate following `.github/workflows/ci.yml` conventions; `bans`/`sources` only if trivially passing, else licenses only (tentative).
- Native closure per distro from `packaging/{debian,rpm,arch,obs}` + `kwin/native-effect` CMake; record in `docs/research/licensing.md` (source-based, no installs; offline container builds only if already scripted and cheap).
- Vendored attribution inventory; add THIRD-PARTY notice only if required and no equivalent exists (tentative).

Non-goals: changing decided licenses; swapping dependencies; `devenv.nix` edits (stop and report); live builds/installs.

Acceptance:
- `cargo deny --locked check licenses` passes; output summary recorded.
- CI gate present; YAML valid (`yq`), referenced commands exist.
- `docs/research/licensing.md` holds native per-distro table + vendored inventory.
- Backlog Licensing entry updated; this note archived on completion.

Approach: deny audit and deny.toml correction, CI gate, native closure
inventory and vendored attribution records.

Verification: `cargo deny --locked check licenses`; `yq` YAML check; `git status`/`git diff` review.

Material decisions (tentative, pending user review):
- `bans` and `sources` included in the CI gate alongside `licenses` (all three pass locally); licenses-only fallback not needed.
- No THIRD-PARTY notice file (see `docs/research/licensing.md` section 11); generated per-crate binary attribution (`cargo-about`) stays an optional follow-up, not a gate.

Blockers: none.

Evidence:
- `cargo deny --locked check licenses`: `licenses ok`; `check bans`: `bans ok`; `check sources`: `sources ok` (cargo-deny 0.20.2).
- CI gate: `licenses` job in `.github/workflows/ci.yml` runs all three checks via devenv; YAML valid (`yq`).
- Inventories: Rust/npm tables in `docs/research/licensing.md` sections 2-3; per-distro closure table section 4.4; vendored inventory section 11.

Outcome: complete; backlog P1 Licensing entry updated; archived on completion.

User review 2026-10-13: keep all three CI checks (`licenses`, `bans`, `sources`).
The tentative no-THIRD-PARTY/optional-`cargo-about` choice above is superseded:
tool-generated third-party notices with compiled dependency license texts
must ship in GitHub Release binaries and Debian/RPM/Arch packages before 0.1.
Candidate `cargo-about` needs a `devenv.nix` addition at implementation time.
Notice implementation is queued in [backlog](../../backlog.md);
[current decision](../../decisions.md#licensing). Audit delivery remains complete.

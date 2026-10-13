# OBS release handoff: tag to stable sources/checksums

- Research date: 2026-10-13. Status: **OPEN after user review; no route selected**.
- Prior recommendation (research only, not selected): GitHub Actions with a
  dedicated least-privilege OBS bot account to `osc commit` bumped recipes and
  real checksum, pinned `download_url` + `verify_file`, then `tag_push` webhook.
- User 2026-10-13 left this OPEN. Possibly not a 0.1 blocker and possibly a
  later-version item; neither decided. No account/credential/route selection.
- Scope: stable release tags (`vX.Y.Z`) delivering sources/checksums into OBS for packages `omnitiler` and `omnitiler-native-effect`. No accounts, tokens, projects, or packages were created; no authenticated OBS calls were made.
- Current pipeline state: `release.yml` builds the deterministic archive and publishes the GitHub Release, then `obs-trigger` fires a synthetic `tag_push` webhook that only re-runs `trigger_services` on already-committed OBS sources. Every release still needs a manual `osc` source update first. Details in [installation](../../installation.md#obs-wiring-provisioning-gated-nothing-created).

## Background: what must be delivered per release

- Per version: bumped recipes (spec, `debian/` files) plus `packaging/obs/_service` pointing at the immutable release asset `https://github.com/beefsack/OmniTiler/releases/download/vX.Y.Z/omnitiler-X.Y.Z.tar.gz` with the real SHA-256 from `dist/omnitiler-X.Y.Z.tar.gz.sha256` in `verify_file`. Placeholders fail closed (`REPLACE-WITH-RELEASE-SIDECAR-SHA256`).
- Archive contract (deterministic, offline): tracked `git archive` of the tag plus prebuilt `kwin/contents/code/main.js` (tag commit SHA baked as `SOURCE_REV`), `vendor/` (locked `cargo vendor`), `VERSION`, `SOURCE_REV`. Distro builds use no network: Cargo `--locked --offline` with vendor replacement, prebuilt bundle installed directly (no npm on builders). See `scripts/build-source-archive.sh:5-33`.
- Raw-checkout routes are excluded by prior decision: a raw git checkout lacks `vendor/` and the prebuilt bundle, and the source-service environment has no pinned Node 24 / cargo-vendor contract (`packaging/obs/_service:7-9`). `buildtime` services are excluded: build workers have no network (`_service:20-22`, confirmed by the [source-services guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-source-services)).

## Route 1: GitHub Actions runs full-credential `osc commit`

- Credential type: OBS user password via HTTP BASIC auth. OBS documents API auth as "BASIC AUTH against OBS user DB" ([authorization/token guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-authorization-token)); `osc` stores per-`apiurl` `user` + `pass`/`passx` in `~/.oscrc` ([osc guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-osc)). `osc` also has an `sshkey` option ([oscrc manual](https://manpages.opensuse.org/Leap-16.0/osc/oscrc.5.en.html)); SSH-signature authentication is documented for SUSE's internal build service ([osc wiki](https://github.com/openSUSE/open-build-service/wiki/osc)). Public `build.opensuse.org` acceptance is UNKNOWN; no scoped source-commit authorization verified.
- No scoped commit token exists. OBS token operations are exactly `Rebuild`, `Release`, `Service` (default), `Workflow` ([token.rb](https://github.com/openSUSE/open-build-service/blob/master/src/api/app/models/token.rb)). `Token::Service` authorizes only `POST /trigger/runservice` ([service.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/models/token/service.rb)); `Rebuild` only rebuilds ([rebuild.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/models/token/rebuild.rb)); `Release` only releases existing packages ([release.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/models/token/release.rb)); `Workflow` only runs `.obs/workflows.yml` steps ([workflow.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/models/token/workflow.rb)). None authorizes source upload. A package-bound `runservice` token narrows re-runs to one package ([token guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-authorization-token)) but cannot commit new sources.
- Secret storage: the GitHub secret holds full OBS user credentials (user + password, materialized as `~/.oscrc` content at job time). Mitigations: dedicated OBS bot account whose write rights cover only the stable project; credentials used only in the post-`source-release` job; secret never printed or passed on a command line (same hygiene as `scripts/trigger-obs-webhook.sh:28-29`).
- Blast radius if leaked: everything that OBS user can write (all owned/maintained projects/packages: source upload, submit, rebuild). It is NOT limitable to one package via any token type. Whether per-package maintainer roles can narrow a password-authenticated account below project scope is UNKNOWN and needs verification during provisioning.
- Integrity: strongest of the two routes. The committed checksum is the exact sidecar digest of the published release asset; `verify_file` (`sha256`) re-checks the download server-side; tag commit SHA is baked into the bundle and `SOURCE_REV`, and the tag/shape checks (`vX.Y.Z`, 40/64-hex SHA) fail closed in the archive script and webhook script.

## Route 2: OBS-side source services fetch the tag, narrow token triggers

- Narrow tokens verified: package-bound `runservice` (re-run `_service` on one package), `rebuild`, `release`, `workflow` (runs `.obs/workflows.yml` steps; requires stored SCM PAT). Webhook auth confirmed: token ID in URL query, secret as HMAC-SHA256 over the raw body ([SCM/CI workflow guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-scm-ci-workflow-integration), [token_extractor.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/services/trigger_controller_service/token_extractor.rb)).
- `trigger_services` does NOT deliver new sources: it re-runs services of an existing `project/package` and raises when no `_service` file exists ([guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-scm-ci-workflow-integration), [trigger_services.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/models/workflow/step/trigger_services.rb)). `branch_package` is the step that picks up new upstream state, but it creates tag-suffixed packages rather than updating the stable package. `link_package` does not run services. So the current post-release webhook can only rebuild what is already committed.
- `obs_scm` (preferred over deprecated `tar_scm`) checks out a git URL on the source server into `.obscpio`/`.obsinfo`; pinning via `revision` (SHA1, branch, explicit `refs/tags/vX`, `@PARENT_TAG@`), `match-tag` glob, `versionformat` ([tar_scm README](https://github.com/openSUSE/obs-service-tar_scm/blob/master/README.md), [tar_scm.service.in](https://raw.githubusercontent.com/openSUSE/obs-service-tar_scm/master/tar_scm.service.in)). Signed-input params (`latest-signed-commit`, `latest-signed-tag`, `maintainers-asc`) exist but their verification semantics are UNKNOWN beyond param names.
- `cargo_vendor` vendors Rust deps server-side (`vendor.tar.zst` via `cargo vendor`, typical `mode="manual"` with `download_files mode="manual"`; [README](https://raw.githubusercontent.com/openSUSE/obs-service-cargo_vendor/master/README.md)). `set_version` derives spec/dsc versions from `.obsinfo` ([concepts guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-concepts)). `download_files` fetches `Source:` URLs; `verify_file` compares against a literal checksum ([source-services guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-source-services)). `download_url` param-level specifics are UNKNOWN (dedicated README 404s).
- Why Route 2 alone does not close the handoff:
  1. Checksum delivery is unsolved: `verify_file` accepts a literal checksum, not a checksum-sidecar parameter (already investigated; [installation](../../installation.md#obs-wiring-provisioning-gated-nothing-created)). Downloading the `.sha256` sidecar alongside the tarball still leaves no documented service that verifies one download against the other (UNKNOWN; candidate for a POC, not a plan).
  2. Revision delivery is unsolved: `tag_push` has NO tag-name/version filter and reports nothing back to SCM ([guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-scm-ci-workflow-integration), [workflow.rb](https://raw.githubusercontent.com/openSUSE/open-build-service/master/src/api/app/models/workflow.rb)). Whether `obs_scm` + `match-tag` can float to the newest matching tag on a `trigger_services` re-run without a committed revision bump is UNKNOWN and needs a POC; the documented pattern pins `revision` explicitly.
  3. Reproducibility regresses: tag fetching reintroduces the raw-checkout problem (no `vendor/`, no prebuilt bundle). Closing it OBS-side needs `cargo_vendor` (fine, server-side) plus an npm/Node 24 bundle build in the service environment (no pinned contract; the release job already owns this toolchain). The release-tarball route avoids both.
  4. Execution security: service code (`_service` recipes, `obs_scm` helpers) runs on the OBS source server with the package's privileges. The pinned `download_url` + `verify_file` surface is minimal and auditable; a full `obs_scm` + `cargo_vendor` + `set_version` chain is more machinery with more failure modes. Per Simplicity, prefer the smaller chain.
- Route 2 strengths (retain regardless): least-privilege trigger credentials (package-bound `runservice` or `workflow` token; leak blast radius is re-runs/rebuilds of packages the token owner may already touch, versus full source write); no long-lived password in GitHub secrets; idempotent re-runs (pinned URL re-downloads the same bytes).

## Options table

| Dimension | Route 1: GHA `osc commit` (bot password) | Route 2: OBS-side fetch + narrow token |
| --- | --- | --- |
| Credential stored | OBS bot user + password as GitHub secret (BASIC auth, `~/.oscrc`) | Workflow/runservice token pair (`<id>:<secret>`); SCM PAT held by OBS, not GitHub |
| Blast radius if leaked | All projects/packages the bot user can write (tokens cannot scope commits) | Re-run/rebuild/trigger only, optionally bound to one package |
| Tag/source integrity | Exact: committed sidecar digest + server-side `verify_file`; tag SHA baked in bundle/`SOURCE_REV`; shape checks fail closed | Unsolved: no tag filter on `tag_push`; checksum-sidecar verification undocumented; signed-tag params unverified |
| Reproducibility (Rust vendoring, archive script) | Reuses the tested release pipeline unchanged (vendor + prebuilt bundle in tarball) | Must rebuild vendoring/bundling OBS-side (`cargo_vendor` OK; Node 24 bundle contract missing) |
| Execution security | Minimal OBS-side surface (pinned download + verify); GHA job is existing release infrastructure | More service machinery on the source server; each service is additional trusted code |
| Maintenance | Bot-account credential rotation; `bump-version.sh` already automates the bump | `_service`/workflow recipe ownership; still needs a checksum/revision delivery mechanism (the same unsolved problem) |
| Ubuntu 26.04/26.10 handling | Identical for both: handoff delivers the same archive+recipes; per-target work is in OBS project config (below) | Identical for both (same note) |

## Prior art reviewed 2026-10-13

- Review finding: CI `osc commit` exists but is uncommon, mostly small-project
  custom workflows, not an established official GitHub action pattern.
  Historical [wslu GHA](https://github.com/wslutilities/wslu/blob/master/.github/workflows/pkgdeploy.yaml)
  writes credentials and runs `osc addremove` / `osc commit` (OBS-era example,
  not a claim of current OBS delivery). Kubernetes uses bespoke release/Cloud
  Build tooling ([adoption](https://www.suse.com/blog/suse-open-build-service-now-adopted-by-the-kubernetes-to-generate-its-official-packages/),
  [release PR](https://github.com/kubernetes/release/pull/3071));
  [ungoogled-chromium-archlinux](https://github.com/ungoogled-software/ungoogled-chromium-archlinux/blob/master/.github/workflows/obs-upload.bash)
  uses curl source API uploads/commit rather than `osc`.
- Marketplace finding from review: one action,
  [beavailable/obs-upload](https://github.com/beavailable/obs-upload), third
  party, about 3 stars, user+password. No official openSUSE/SUSE action found;
  this is a search finding, not proof that none can exist. Other repository
  actions do not establish additional Marketplace listings.
- Higher-profile pattern: CI holds only a scoped trigger token and OBS does
  the source work. Examples:
  [rancher-desktop CI](https://github.com/rancher-sandbox/rancher-desktop/blob/main/.github/workflows/package.yaml),
  [zeromq/zyre workflows](https://github.com/zeromq/zyre/blob/master/.obs/workflows.yml),
  [logitune trigger](https://github.com/mmaher88/logitune/blob/master/.github/workflows/release.yml).
  Logitune also separately password-commits recipes; a scoped trigger alone
  does not solve recipe/checksum delivery.
- OBS guidance favors workflow tokens/source services
  ([SCM/CI guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-scm-ci-workflow-integration),
  [services](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-source-services))
  and the newer SCM bridge (`scmsync`), git-backed packages with `osc commit`
  disabled in favor of git commit/push
  ([OBS with Git](https://en.opensuse.org/openSUSE:OBS_with_Git),
  [bridge](https://github.com/openSUSE/obs-scm-bridge)).

### Citation conflicts still open

- Review reported rancher-desktop `trigger/runservice` for `isv:Rancher:stable`.
  The located CI source instead names `isv:Rancher:dev`;
  [stable service](https://api.opensuse.org/public/source/isv:Rancher:stable/rancher-desktop-1.24/_service)
  shows per-version assets without `verify_file`, not a version-free latest
  asset. Exact stable-trigger citation remains unresolved; do not substitute
  the dev trigger as proof of the reported stable trigger.
- Review reported CAF via Jenkins as OBS-upload prior art.
  [CAF Jenkinsfile](https://github.com/actor-framework/actor-framework/blob/0.19.5/Jenkinsfile)
  confirms Jenkins use, not the OBS-upload step. Exact OBS citation unresolved.
  These gaps select no route and remain for clarification.

## Additional candidate routes (not yet researched)

- SCM bridge: CI pushes recipes plus exact checksum to git, OBS syncs;
  no OBS password in CI. Git-backed package semantics replace `osc commit`;
  fit to the deterministic release-archive contract needs research.
- Rancher-desktop-style scoped trigger/OBS-side fetch, using a version-free
  `releases/latest/download` asset and no pinned checksum. This is a candidate,
  not a verified claim about the cited stable service or an integrity decision.
- Route 3: manual scripted `osc commit` per release, also raised in review.
  No selection; candidate automation/auth/acceptance tradeoffs remain OPEN.

## Ubuntu 26.04/26.10 requirements folded in

- Target existence does not prove a resolver/build/publish run; verify actual selected versions in OBS buildinfo. KWin/ECM need **universe**: stock `standard` alone is insufficient. Pocket ordering matters (first-repository-wins stacking per [OBS PR #19085](https://github.com/openSUSE/open-build-service/pull/19085), open/unmerged at research date): for the updated target place `universe-update` before `universe` and `update` before `standard`. DoD lets a project reference external repos with `<pubkey>` verification ([concepts guide](https://openbuildservice.org/help/manuals/obs-user-guide/cha-obs-concepts)); Ubuntu universe exposure via DoD is documented ([2016 DoD note](https://openbuildservice.org/2016/07/05/download-on-demand-feature/)). Ubuntu 26.04-specific pocket lists and ordering beyond the repo's prior research are UNKNOWN in official OBS sources and must be confirmed in buildinfo during provisioning.
- 26.04 ships KWin 6.6.x (release 6.6.4, updates 6.6.6): each needs its own matching effect binary (factory IID check refuses cross-version plugins); do not reuse one build across pockets. 26.10 builds against KWin 6.7.5 with per-output workspaces supported. Full version/KWin/ECM inventory is in [installation](../../installation.md#per-distro-notes) and [Ubuntu research](ubuntu-native-effect.md); not repeated here.
- Debian recipe notes for the handoff: `DEB_BUILD_PROFILES=noeffect` builds core-only without `kwin-dev` (useful while a KWin bump awaits its effect rebuild); Debian's exact KWin pin requires a matching rebuild or companion removal before the host upgrade flows. Rebuild/publish latency after a distro KWin release is unbounded (DoD polling/queue/build/publish), so a same-day KWin release can precede its rebuilt effect; the loader IID check plus independent core are the mitigation, not the handoff.
- KDE neon: no OBS target provisioning exists (upstream [open-build-service#19317](https://github.com/openSUSE/open-build-service/issues/19317)); out of scope for this handoff either way.

## Consequences if the prior recommendation were selected

- Accept: one high-value secret (bot password) in GitHub secrets, rotated manually; a dedicated OBS account to create and own.
- Keep: pinned `download_url` + `verify_file` service (fail-closed placeholders until provisioned), post-release `tag_push` webhook (now as a rebuild/verify trigger after the commit lands, ordering: release -> `osc commit` -> webhook), `OBS_TOKEN` unset until provisioning resolves and the end-to-end run is tested.
- Do not build: `obs_scm`/`cargo_vendor` tag-fetching chains, `buildtime` downloads, raw-checkout services, repo-level GitHub webhooks (race asset creation), `branch_package` for stable delivery (suffixed packages).
- Both packages (`omnitiler`, `omnitiler-native-effect`) ride the same commit: one archive, one checksum, bumped recipes covering both stanzas.

## Unknowns (need provisioning/POC, not guesses)

1. Whether a password-authenticated OBS account's write rights can be narrowed below project scope (per-package maintainership for a bot).
2. Any non-interactive `osc` auth beyond `~/.oscrc`/keyring (no env-var-only login found in official docs).
3. Whether `obs_scm` + `match-tag` can track the newest `v*` tag on `trigger_services` re-runs without a committed revision bump.
4. Whether any source service can verify a download against a downloaded checksum sidecar (vs literal `verify_file` checksum).
5. Signed-tag verification semantics (`latest-signed-tag` et al.).
6. `download_url` param-level behavior (dedicated README 404s).
7. Ubuntu:26.04 OBS-side pocket list/ordering and selected buildinfo versions (confirm at provisioning).
8. Token operations beyond the four verified in code.

## Tentative decisions (awaiting user review)

- (T1) Dedicated OBS bot account, write rights only on the stable project; human credentials never enter GitHub secrets.
- (T2) Keep the release-tarball contract as the single source-delivery mechanism; no OBS-side tag fetching for 0.1.
- (T3) `OBS_TOKEN` (workflow token) stays unset until the Route 1 handoff is implemented and tested end-to-end; manual `osc` fallback remains documented until then.
- (T4) Ubuntu 26.04/26.10 and Tumbleweed/Fedora targets share the handoff; neon stays out of scope for 0.1.

## Next action (OPEN)

- Clarify conflicting prior-art citations and research the additional candidates
  before the next route review. Release-blocker status, route and provisioning
  remain undecided; no implementation or end-to-end claim.

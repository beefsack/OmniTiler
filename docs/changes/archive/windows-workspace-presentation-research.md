# Windows workspace presentation research

## Goal and scope

- Compare hidden-workspace window presentation options on Windows and recommend a supported baseline.
- Cover shell presentation, activation and urgency, recovery, gaming, outputs, accessibility, performance, and API/security risks.
- Research and documentation only; no product-code changes. The presentation choice remains a parked product decision.

## Acceptance

- Concise option comparison grounded in existing decisions, official API documentation, and discriminating native evidence where practical.
- Unknown or untested outcomes remain explicit; reference outcomes distinguish observations from proposed behavior.
- Backlog links identify the completed research and remaining decision.
- Live-test state is restored; controlling terminal is untouched.
- Documentation review and `git diff --check` pass; committed changes are pushed and CI outcomes recorded.

## Approach and bounded units

1. Investigate existing inputs and documented APIs; perform narrowly scoped owned-window probes where useful; draft research and discriminating reference rows.
2. Independently review evidence, safety, completeness, and consistency with existing activation and reference-tier decisions.
3. Reconcile findings, update status records, verify documentation, and publish.

## Decisions and evidence

- Existing SW_HIDE behavior remains unchanged by this research.
- Native virtual desktops and private cloaking require separate API/product decisions; no undocumented product API use is introduced.
- Research now compares SW_HIDE, public off-screen parking, minimization,
  native desktops, and private cloaking across all requested dimensions.
- Recommendation: retain SW_HIDE; investigate public parking only if shell
  inclusion is required. Minimization is recommended against because it
  collides with allocation-release semantics, not finally rejected.
- Presentation mechanism plus switch-versus-pull selection remains PARKED.
- Owned-helper probes verified hide/show visibility, cloak and geometry
  readbacks and minimized/restored state. Shell listings and visuals were
  not observed and remain explicitly TBD, alongside accessibility, games,
  parking races and native-desktop integration evidence.
- Early ordinary-app probes failed window attribution. A residual empty,
  unmodified single-tab window was inspected before normal scoped closure;
  process exit and invalidated HWND were independently verified. Owned
  helpers were absent after cleanup; no remaining changed test state.
- Independent review resolved stale citations, overly final recommendation
  wording, recovery overclaims and unlabeled inference. Documentation scope,
  relative links, parked status and `git diff --check` passed after corrections
  and again after rebasing publication.

## Outcome

- Research delivered 2026-10-17; mechanism and switch-versus-pull selection
  remain PARKED. No product behavior or API policy changed.
- Research publication: `ade0da4c`; pushed after rebase.
- [CI run 38932773273](https://github.com/beefsack/OmniTiler/actions/runs/38932773273):
  Windows, macOS, native, Rust, KWin, notices and licenses passed. Shell failed
  in the known `kwin-66-api-guard` suite on unaudited
  `fullscreen|maximized|sticky` symbols and its missing clean marker. No other
  stream failed; the unrelated guard was not changed.
- Remaining product action: choose the presentation mechanism and
  switch-versus-pull policy; obtain native shell visual/accessibility and
  option-specific recovery evidence before adopting a different backend.

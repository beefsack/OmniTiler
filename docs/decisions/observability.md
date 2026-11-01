# Observability

### Cross-component diagnostics
- Decision: implementation and review include evidence needed to diagnose behavior/failure across every component. Project processes and Rust-path IPC emit bounded structured correlated lifecycle/terminal logs for requests, significant decisions, failures, recovery and outcomes to existing visible KWin console/stdout/stderr/journal; future troubleshooting checks these first. Carry trace/correlation IDs across components/services; distinguish dispatch, acceptance, completion and uncertainty. Normal operation has bounded summaries; opt-in trace supplies high-volume detail without frame/poll noise. Logging failures never block or change behavior.
- Why: not recorded.
- Assumes / revisit if: no unbounded pointer-step logging. Redact captions, application content, secrets, full paths, raw environment, native window handles/IDs and raw native D-Bus payloads.
- Status: active, 2026-10-14
- Evidence: [Observability](../principles.md#observability), [route diagnostic schema](../changes/core-interface-review.md).

### Normal-level window identity (R15)
- Decision: normal-level logs may carry application identity (executable basename, window class, bundle id) and process IDs so a window can be identified at baseline without title/content. Titles, content, full paths and native window handles/IDs stay redacted. "Native identifiers" means those handles/IDs, not app identity or process IDs.
- Why: identify windows in baseline logs without exposing titles or content.
- Rejected: restricting process IDs to verbose/trace; restricting both app identity and process IDs to verbose/trace.
- Assumes / revisit if: shared `omnitiler:route-diag` schema identifies component, direction/stage, correlation, authority generation, revision, event/action and outcome within redaction rule. `just dev verbose` keeps lifecycle summaries; `just dev trace` enables redacted high-volume per-window/hook/bounded structural request/reply detail, never raw native D-Bus payloads.
- Status: active, 2026-10-31
- Evidence: [review R15, F26](../changes/core-interface-review.md).

### Output-send and Windows tracing
- Decision: output-send diagnostics carry originating operation route, including late replies after newer opposite-kind flight starts. Windows trace outlier thresholds are 50 ms queue/transport/work and 10 ms dispatch; otherwise one summary per second while active. Reconcile `tick` emits only deduplicated skipped count. Windows visibility diagnostics use process-relative monotonic milliseconds, correlated per-window hide/reveal outcomes, nullable visibility/cloak samples and exception reasons; bounded one-second batches declare omitted/coalesced/lost history, never imply complete sampling during suspension or change behavior.
- Why: not recorded.
- Assumes / revisit if: logs remain bounded and avoid frame/poll noise.
- Status: active, 2026-10-16
- Evidence: [output-send diagnostics](../changes/kde-dual-output-live.md#product-defect-and-offline-fix), [trace volume](../changes/archive/windows-trace-volume.md), [visibility diagnostics](../changes/archive/windows-visibility-diagnostics.md).

### KDE world placement traces
- Decision: production KDE world placement traces include capped ordered source/target tree shapes and shares before and after send/R4 planning, without native or leaf identities.
- Why: distinguish nested whole-target wrapping from flat insertion.
- Assumes / revisit if: this does not prove exact named-leaf order; wire and actuation remain unchanged; user review remains pending.
- Status: tentative, 2026-10-19
- Evidence: [trace evidence and limits](../changes/kde-dual-output-live.md#current-continuation-2026-10-19).

### Shortcut diagnostics
- Decision: shortcut operations emit bounded structured `omnitiler.shortcut` diagnostics (operation, stage, outcome, shortcut component/action IDs, key images, cleared count/writes). Conflict records include every holder ID and chord at normal info/warning levels, with counts, row indexes and bounded chunks for long rows, enough to reconstruct list. Friendly labels/user content remain excluded; no permission for native window IDs/raw payloads. Operational warnings/info default on with debug-only records. Fixed unauthorized reply omits caller-supplied correlation to preserve caller authorization and strict adapter correlation fence; distinct unauthorized observability is not selected.
- Why: not recorded.
- Assumes / revisit if: no logging behavior changes; query via `journalctl --user --no-pager -g "omnitiler.shortcut op="`.
- Status: active, 2026-10-12
- Evidence: [shortcut diagnostics](../changes/archive/kde-post-windows-followups.md).

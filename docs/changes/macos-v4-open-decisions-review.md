# Review and Tentative Decisions: macOS v4 Port & Open Decisions

- **Status:** User-approved working decision record (2026-11-24). Decisions D0–D6 are approved to unblock macOS v4 work (D1 and D5 remain revisitable after dogfooding/diagnostic evidence without locking in irreversible architecture).
- **Scope:**
  1. Surface-level review of the macOS v4 port commits (`26c8d2a5..192aa88e`) and uncommitted worktree changes.
  2. Decision-ready analysis for Open Decisions 0–6 grounded in [VISION.md](../../VISION.md), [principles.md](../principles.md), [macos decisions](../decisions/macos.md), [platforms decisions](../decisions/platforms.md), [persistence decisions](../decisions/persistence.md), [contract decisions](../decisions/contract.md), [layout-and-admission decisions](../decisions/layout-and-admission.md), [macOS 0.1 gate proposal](macos-0.1-gate-proposal.md), and [retained-intent-desire.md](retained-intent-desire.md).
- **User disposition (2026-11-24):** All D0–D6 recommendations are accepted. D1 (Option 1A) and D5 (Option 1) are accepted as low-pivot defaults: D1 has zero code cost and leaves `resizable`/`AXSize` settability available for a later additive 1B pivot; D5 is adapter-local with the CG-alive guard as automatic fallback and is gated on the bounded fixture diagnostic before implementation.
- **Boundary:** No code, core, protocol, or committed decision records were modified by this review.

---

## Part 1: Surface-Level Review of Recent macOS Work (`26c8d2a5..192aa88e` + Uncommitted Worktree)

### Overall Assessment

The mechanical v4 port (`26c8d2a5`), pending-focus subject delegation (`a6451e41`), retained switch-settlement consumption (`d67b1733`), Mission Control close verification (`9e4d0303`), and the uncommitted worktree changes (`AXFloatingWindow -> Utility`, `focus_saturation` two-app `PositiveClose` fix, and post-only owned-Space placement in `fixture_leg.rs`) are generally disciplined:
- `tiler-core` and `tiler-protocol` are untouched.
- Assertions were ported honestly rather than weakened; fixture-only scoping and Space > n isolation were respected.
- Ambiguous product/contract points were surfaced instead of being worked around with synthetic proofs or local policy hacks.

However, **five concrete issues** were found in the committed and uncommitted changes that should be rectified before expanding native acceptance or committing the current worktree.

### Findings Requiring Rectification

| # | Severity | Location | Finding | Principle / Policy Impact | Required Rectification |
| --- | --- | --- | --- | --- | --- |
| **R1** | **High** | `crates/tiler-macos/src/tile.rs:2425`, `cursor_observer.rs:201-217`, `runtime_thread.rs:1596-1630` (`8b44a2fa`) | `cursor_observer` spawns unconditionally and polls `AXUIElementCopyElementAtPosition` at 250 ms on pointer motion and 1 s (`HIT_REFRESH_MS`) while resting, dispatching a full `WorldReportV4` through `codec_transact` (4 JSON codec passes + core eval) on every sample — **even when `focusFollowsCursor` is off, when idle, when the session is locked/Secure Input, and while a fullscreen game is active**. Also causes mid-flight observation-clock advances and stale-reply churn (`v4-phase3-macos-port.md:478`). | Violates [Gaming Compatibility](../principles.md#gaming-compatibility) (`absolute`: zero/provably minimal cost during games) and [Efficiency](../principles.md#efficiency) (prefer events over polling; idle/disabled work must cost little). | Gate/park `cursor_observer` when core effective `focusFollowsCursor` is `false` (wake only when enabled); suspend AX hit-testing when session is locked, Secure Input is active, or a fullscreen window is foreground; evaluate whether resting 1 s re-dispatch is needed when neither pointer nor window layout changed. |
| **R2** | **High** | `crates/tiler-macos/src/mc_probe.rs:45`, `mc_lifecycle.rs:180-200`, `space_recovery.rs:38` (`e066d73d`) | Mission Control AX traversal hard-switched exclusively to `com.apple.WindowManager` (direct `mc.display`) and deleted the `com.apple.dock` (`mc` → `mc.display`) path (`MC_ID_ROOT`). | Regresses the approved **macOS 15 minimum support floor** ([platforms](../decisions/platforms.md#macos-phase-0-and-host-foundations)): on macOS 15 (and potentially 26), Mission Control's AX tree is owned by `com.apple.dock` under `mc`, not `com.apple.WindowManager`. | Support both surfaces cleanly with exact bundle verification on each: probe `com.apple.WindowManager` (`mc.display`) and fall back to `com.apple.dock` (`mc` → `mc.display`) when the newer surface is absent or exposes no `mc.display`. Never guess PIDs or relax bundle gates. |
| **R3** | **Medium** | `crates/tiler-macos/src/world_dispatch.rs:180-241` (`26c8d2a5`) | `codec_transact` still executes four in-process JSON encode/decode legs (`encode_report_v4` → `decode_report_v4` → `handle_report_checked` → `encode_reply_v4` → `decode_reply_v4_for_host`) on every report. | Safe functionally (wire validation remains enforced), but compounds R1 latency/CPU cost and means the backlog item **C5 typed boundary adoption on macOS** (`docs/backlog.md`) is **not** complete yet. Commit/backlog wording ("typed boundary") should not be misread as codec-free dispatch. | Keep the C5 typed-boundary backlog item open until typed native ingress replaces the 4-leg JSON round-trip while preserving the exact D3=B semantic checks. |
| **R4** | **Medium** | `crates/tiler-macos/src/world_dispatch.rs:259-268` | `unpersisted_hold` latches `session.unpersisted_intent_desired = true` permanently on the first non-empty `intent_desired` reply, blocking all subsequent native apply (frames, focus, presentation) for the rest of the session. | Any `ToggleFloat` permanently freezes tiling for the session (tensions with [Resilience](../principles.md#resilience) and blocks dogfooding of floating). Confirms Decision 4 (M3 host store) cannot be deferred for 0.1. | Replace the permanent session latch with the contract's recoverable whole-reply hold once the M3 adapter-local atomic store lands (Decision 4). |
| **R5** | **Medium** | `docs/changes/v4-phase3-macos-port.md:294` (committed in `d41c17be`); `docs/changes/archive/macos-focus-saturation-close-cause.md:51-52,86` (untracked) | Two repository-record rule violations: (1) `v4-phase3-macos-port.md:294` recorded a specific development-host OS version, violating [live-macos-testing.md](../live-macos-testing.md#native-readbacks-and-acceptance) ("without the development host's OS version") and local-system info rules; (2) untracked `macos-focus-saturation-close-cause.md` recorded session-control/role wording instead of work and evidence only. | Violates repository-record and local-system-information rules. | Scrub the exact dev-host OS version from `v4-phase3-macos-port.md:294` (describe generically as the newer release family / major >= 27) and scrub session-control/role wording from `macos-focus-saturation-close-cause.md` **before** committing the worktree. |

### Rectification status (2026-11-24)

- **R1 fixed offline (follow-up gap F3 fixed):** `SamplingGate` now requires effective core `focusFollowsCursor` and no lock/unknown session, Secure Input, or focused-fullscreen game pause; while disallowed the observer takes no AX hit test and posts no sample, and the engine drops raced samples after ack. F3: the live cycle reads only the two gate atomics before short-circuiting, so disabled/paused states perform no session, Secure-Input, position, ack-mutex, hit-test, or post work; a disabled setting idles at 1 s instead of the 250 ms tick. Evidence: `cursor_observer` 14/14; real lock/Secure-Input/fullscreen transitions remain live-only.
- **R2 fixed offline (follow-up bugs F4 fixed):** dual Mission Control surfaces restored with exact per-surface bundle verification: WindowManager direct `mc.display`, automatic Dock `mc` → `mc.display` fallback. F4: the CG presentation check follows the last exact-verified surface that owned an open MC tree (Dock/not-yet-observed verify on fresh AX presence alone; WindowManager checks its own `layer >= 0` windows with a PID-reuse fence), WindowManager non-ownership `System` AX failures fall through to Dock, and exact bundle/ownership refusals never fall back. Evidence: `mc_probe` 44/44, `mc_lifecycle` 4/4; macOS 15 live surface remains unrun.
- **R3 open:** macOS `world_dispatch` still runs the four-leg JSON codec; C5 typed ingress remains open.
- **R4 fixed offline (follow-up bugs F1, F2, F5 fixed):** adapter-owned native intent store plus desired-vs-durable reconciler landed; the permanent `unpersisted_intent_desired` session latch is gone. F1: hydration/admission is per window (a late report still imports a newly observed app, an imported window never rides again), and the applied report's shipped records are the ones marked admitted. F2: a gone process keeps the recoverable hold, while an alive process with no verifiable LaunchServices lifetime is never persisted, is skipped without holding the reply, and diagnoses once per window; the store treats data as opaque and length-bounded. F5: `RuntimeConfig.intent_store` is explicitly `Disabled` by default, `Default` only in the production CLI, with an `Isolated(dir)` mode; the `cfg!(test)` open path is gone and the engine publishes the opened directory. Evidence: lib 1687 passed/0 failed/3 ignored, integration 151/0, including the staggered two-app hydration and alive/gone binding regressions. Native restart/failure proof remains required.
- **R5 fixed:** development-host OS version and session-control wording are scrubbed; repository records now use generic release-family wording.
- **D5 diagnostic run (2026-11-24): exact close identity confirmed, but the notification is not close-exclusive.** One bounded signed-fixture run registered both retained windows on `AXUIElementDestroyed` and drove controls: minimize/restore twice (zero events), first window hide (one false event for the still-alive window), later hide/show (zero), app hide/unhide (zero), close A (exactly one A match, zero B/unmatched), and owned-Space away/back with create/remove (zero). The close measurement proves `CFEqual` identity works; the window-hide event proves direct roster pruning would drop a hidden window. Production transport/pruning was not implemented. The CG-alive oracle remains authoritative; any future notification transport must revalidate liveness before pruning.
- **Verification (2026-11-24, F1–F5):** `cargo test -p tiler-core` clean; macOS lib 1687 passed/0 failed/3 ignored; integration 151 passed/0 failed; `cargo fmt --all -- --check` clean; strict `cargo clippy -p tiler-core -p tiler-macos --all-targets -- -D warnings` clean. Prior D5 targeted 7 and D4 hydration 1 records stand. Live lock/Secure-Input/fullscreen transitions, the macOS 15 Mission Control surface, and native restart/failure proof remain live-only gates.

### Uncommitted Worktree Disposition

- `crates/tiler-macos/src/runtime_types.rs` (`AXFloatingWindow` → `WindowType::Utility`, `AXSystemFloatingWindow` kept `None`): **Clean.** Matches native evidence (`macos-batch3-utility-mapping.md`) and [window-state decisions](../decisions/window-state.md#splash-utility-and-picture-in-picture-admission).
- `crates/tiler-macos/src/ffi.rs` (comment correction for `NSPanel` subrole): **Clean.**
- `crates/tiler-macos/tests/integration/runtime_phase1.rs` (`focus_saturation_drops_counted_and_survives`): **Clean.** Honest two-app `PositiveClose` cause, `focus_outbox_depth: 1` preserved, no assertion weakening.
- `crates/tiler-macos/src/fixture_leg.rs` (post-only owned-Space placement + `ensure_appkit_singleton` + `FOCUS_SETTLE_MS`): **Clean.** Note that `fixture_leg.rs` grew significantly; avoid growing it further without splitting test-harness helpers.
- `docs/backlog.md`, `docs/changes/v4-phase3-macos-port.md`, `docs/changes/macos-batch3-utility-mapping.md`, `docs/changes/archive/macos-focus-saturation-close-cause.md`: **Clean after R5 scrub** (scrub role wording from `macos-focus-saturation-close-cause.md` and the host OS version from `v4-phase3-macos-port.md` before committing).
- **Note on `dock_swipe_select.rs`:** `gesture_payload_allowed` refuses macOS major >= 27 (`space_switch_edges.rs:41,91`), so on newer release families Dock-swipe refuses and Mission Control AX is the active route (consistent with [macos decisions](../decisions/macos.md#managed-native-spaces-input-and-transfers)). Supporting the `>= 27` Dock-swipe payload (or accepting MC AX on >= 27) remains a separate native-Spaces acceptance item under MX1.

---

## Part 2: Prioritized Decision Table (Decisions 0–6)

| Priority | Decision | Status & Recommendation | Confidence | 0.1 Impact & Deferrability | Product / Contract Change? | Owner & Decision-Record Path |
| --- | --- | --- | --- | --- | --- | --- |
| **1 (Meta)** | **D0: macOS 0.1 native acceptance bar** | **APPROVED (2026-11-24) — Retain approved 2026-10-29 gate (43 M / 7 K / 7 X + MX1–MX8); apply narrow host-capability / ROI exceptions only** *(Rejects deferring lifecycle past 0.1)* | High | **Defines 0.1 gate.** Lifecycle, M3 restart persistence, M4 maximize, native Settings, underlay attempt, real games, dual-display, DMG/Homebrew remain hard 0.1 release gates (mostly user-run at release gate). Only genuine host-capability limits (D1/D3) or disproportionate harness proofs of pure core math (D2) may carry documented limits. | **No** (enforces existing 2026-10-29 approval + [ROI principle](../principles.md#focus-and-return-on-investment)). | Project owner; `docs/decisions/macos.md#macos-0-1-scope-and-gate` (no edit needed unless user narrows 0.1 scope). |
| **2 (P0/P1 Blocker)** | **D4: M3 `intentDesired` storage owner** | **APPROVED (2026-11-24) — Option 1: Adapter-owned host store in `tiler-macos`** *(Already selected by contract; Options 2–3 rejected; Option 4 unacceptable)* | High | **Hard 0.1 blocker; NOT deferrable.** Required by checks 8, 11, 48 and M3 amendment (2026-11-02); current session latch permanently freezes tiling on first `ToggleFloat` (R4). | **No** (implements approved [contract](../decisions/contract.md#intent-ownership-and-persistence) & [persistence](../decisions/persistence.md#orderly-restart-adoption)). | macOS adapter owner; confirm slot/namespace details in `docs/decisions/persistence.md` & `docs/changes/macos-v4-platform-fit.md`. |
| **3 (P1 UX / Jank)** | **D5: Close-gap exact destroyed-window identity** | **APPROVED (2026-11-24) — Option 1: Implement exact `kAXUIElementDestroyedNotification` identity before 0.1 (after R1/R2/R5 and D4; gate on small fixture diagnostic first)** | Medium-High | **Approved for 0.1 (revisitable if diagnostic surfaces unexpected OS behavior).** Measured 0.4–1.1 s (median ~1.0 s) freeze/hold on every window close directly impacts [VISION.md](../../VISION.md) "jank-free" and [Efficiency](../principles.md#efficiency). | **No** (adapter-only observation improvement; preserves core contract and CG guard). | Project owner (UX bar) + macOS adapter owner; `docs/decisions/macos.md#fixture-and-scoped-input-defaults` & `docs/changes/macos-responsiveness-investigation.md`. |
| **4 (P1 Product Parity)** | **D1: Fixed-float admission policy** | **APPROVED (2026-11-24) — Option 1A: Defer native fixed-float send proof; keep hints-only predicate; zero synthetic injection (revisit after real-app dogfooding if needed)** | High | **Documented host-capability limit for 0.1 checks 10 & 13.** AppKit exposes neither AX min nor max size bounds; fixed-size `AXStandardWindow`s tile unless explicitly floated, while dialogs/sheets/modals/`AXFloatingWindow` still float. Zero pivot cost if revisited later. | **No** (documented host limit; Option 1B would require a later product + core contract amendment). | Project owner; `docs/decisions/layout-and-admission.md#fixed-size-admission-predicate` and `docs/decisions/macos.md`. |
| **5 (P1/P2 Harness)** | **D2: Constrained-work-area route (no-space / best-effort)** | **APPROVED (2026-11-24) — Option 1: Defer native constrained-work-area exhaustion leg; rely on core geometric capacity coverage + adapter mapping tests** | High | **Deferrable without blocking 0.1.** Pure core geometry math; 19.7k–21.4k windows required natively; product override or flaky private `CGVirtualDisplay` fails [ROI](../principles.md#focus-and-return-on-investment). Same pragmatic posture as KDE (`docs/backlog.md`). | **No** (Option 2 would be an unwanted product change; Option 1 adds none). | macOS adapter owner; `docs/decisions/layout-and-admission.md#best-effort-d3-d4-no-space-result-and-focus` (note evidence limit) & `docs/changes/v4-phase3-macos-port.md`. |
| **6 (P1 Release Gate)** | **D6: Post-v4 session lifecycle authorization & sequencing** | **APPROVED (2026-11-24) — Option 2: User-run bounded lifecycle acceptance BEFORE 0.1 release, sequenced after adapter stabilization** *(Never agent-autonomous; NOT deferred past 0.1)* | High | **Hard 0.1 release gate; NOT deferrable past 0.1**, but sequenced **after** adapter work (R1–R5, M3, M4, slices 2–7). Does not block current phase-3 porting. | **No** (already required by [VISION.md](../../VISION.md) Reliability and checks 30, 50, N3, MX2, MX7, MX8). | Project owner (executes) + macOS adapter owner (prepares bounded procedure & recovery helpers); `docs/changes/macos-0.1-gate-proposal.md` & `docs/live-macos-testing.md`. |
| **7 (P3 Edge)** | **D3: `AXSystemFloatingWindow` mapping** | **APPROVED (2026-11-24) — Keep withholding as `None` (P3 documented limit)** | High | **Deferrable without blocking 0.1.** `AXFloatingWindow` (`NSPanel` utility palettes) already floats; `AXSystemFloatingWindow` has no truthful sample and holding is safe. | **No** (preserves truthful unknown policy). | macOS adapter owner; `docs/changes/macos-batch3-utility-mapping.md` (promote to `docs/decisions/macos.md` only if a real sample is later measured). |

---

## Part 3: Detailed Decision Records

### Decision 0: macOS 0.1 Native Acceptance Bar

- **Recommendation & Status:** **APPROVED (2026-11-24) — Keep the already-approved 2026-10-29 macOS 0.1 gate intact** (43 must-pass / 7 known-issue-allowed / 7 excluded + MX1–MX8 must-pass), with **explicit separation** between:
  1. *Incremental Phase-3 porting gates* (offline + fixture-scoped owned-Space proofs);
  2. *Release 0.1 must-pass acceptance* (including user-run physical input, real-game, dual-display, lifecycle, native Settings, M3 restart persistence, M4 composed maximize, underlay attempt, and DMG/Homebrew installed lifecycle); and
  3. *Narrow, explicitly documented host-capability or ROI-bounded evidence limits* (D1 if kept hints-only, D2 constrained-work-area exhaustion, D3 `AXSystemFloatingWindow`).
- **Confidence:** High.
- **Why the brief's Option 3 ("defer lifecycle until after 0.1") is rejected:**
  - [VISION.md](../../VISION.md#reliability) states *"Reliability is non-negotiable... including across: sleep and wake, plugging and unplugging outputs, resolution and scaling changes, full-screen applications, changes to underlying configuration"*.
  - [docs/decisions/macos.md#macos-0-1-scope-and-gate](../decisions/macos.md#macos-0-1-scope-and-gate) (2026-10-29) explicitly records the user's scope principle: *"I don't want to be cutting scope for things we have already put into windows - the only significant feature I've accepted pushing out of scope for 0.1 is the tabbed window groups and that is deferred for all platforms, not just one. Of course, if we hit problems during implementation, this needs to be revisited and some other items can become deferred due to platform capability restrictions."*
  - [docs/decisions/persistence.md](../decisions/persistence.md#orderly-restart-adoption) explicitly requires M3 within-session orderly restart persistence for 0.1.
- **User-visible consequences:** macOS 0.1 ships with the same core feature set and reliability guarantees as KDE/Windows (no silent feature cuts), while avoiding pointless harness over-engineering for pure core math or non-existent macOS AX APIs.
- **Risks, edge cases & cross-platform parity:**
  - Slices 5 (state targets / M4) and 7 (physical reconnect/rehome / M8/M9) remain gated on user-run KDE ST/RC passes and absorption of resulting core changes (including RC leg-2).
  - Held-Space recovery remains an unrun draft and must be natively proven before claiming slice 1–3 closure.
  - macOS 15 support floor remains unverified until tested on macOS 15 (already an accepted unverified risk under D4 of `macos-0.1-gate-proposal.md`, made acute by R2).
- **Evidence still needed:** User confirmation that the 2026-10-29 gate remains unchanged (no new 0.1 scope cuts beyond documented host-capability/ROI limits).
- **Owner & decision-record location:** Project owner; `docs/decisions/macos.md#macos-0-1-scope-and-gate` and `docs/changes/macos-0.1-gate-proposal.md`.
- **Implementation outline & acceptance tests:** Track progress directly against the 43 M + MX1–MX8 matrix in `macos-0.1-gate-proposal.md`; separate fixture-automated legs from user-run physical/lifecycle legs.
- **Rollback / containment:** If an approved 0.1 item hits an unsolvable macOS platform capability barrier (e.g., SkyLight underlay or Dock-swipe on >= 27), stop and report evidence to the user for an explicit deferral entry rather than silently downgrading it.
- **Assumptions & TBD:** Assumes user will execute the user-owned physical/game/dual-display/lifecycle legs once the adapter reaches release-candidate readiness.

---

### Decision 1: Fixed-Float Admission Policy

- **Recommendation & Status:** **APPROVED (2026-11-24) — Option 1A: Defer native fixed-float send proof; keep hints-only predicate and offline coverage; zero synthetic fact injection.** Revisitable after real-app dogfooding if non-resizable `AXStandardWindow`s prove disruptive (Option 1B remains a clean, additive future pivot via a real-app `AXSize`-settability survey and core capability amendment).
- **Confidence:** High that Option 3 (synthetic injection) must NOT count as native proof and Option 4 (AX max reader alone) is non-viable; Medium on 1A vs. 1B pending user preference and real-app `AXSize` settability data.
- **Rationale & user-visible consequences:**
  - Verified evidence (`v4-phase3-macos-port.md:648-713`): all four fixture windows (including `min=max=640x480, resizable=false` and `min=max=640x480, resizable=true`) returned the identical 28-name AX inventory with no `AXMinSize`, `AXMinimumSize`, or `AXMaxSize`; `ax_min_size` returned `None`; `size_settable` was `false` on the fixed non-resizable window and `true` on the resizable ones.
  - Under Option 1A, fixed-size `AXStandardWindow` apps (e.g., small utility apps that use `AXStandardWindow` with non-resizable style masks) will be forcibly tiled/resized (or reject `AXSize` writes and drift inside a tile slot) on macOS unless the user floats them explicitly.
  - Under Option 1B, non-resizable `AXStandardWindow`s would auto-float, matching macOS prior art (AeroSpace/yabai/rift check `AXSize` settability), **but** it directly reverses the existing rule in `docs/decisions/layout-and-admission.md#fixed-size-admission-predicate` (*"never infer from `resizeable`"*), `docs/decisions/macos.md:58`, and `docs/decisions/contract.md`.
- **Risks, edge cases & cross-platform parity:**
  - **Option 1A risk:** Poor UX with non-resizable standard windows (tiler repeatedly attempts `AXSize` writes that fail or leave tiny windows floating in huge tile gaps); diverges from KDE/Windows where fixed-size windows auto-float.
  - **Option 1B risk:** Some apps temporarily report `AXSize` non-settable during startup/sheet presentation, or intentionally lock one axis; auto-floating them could create surprising floats. Must NOT reinterpret `resizable` silently inside the macOS adapter — per [Layering](../principles.md#layering) and [Consistency](../principles.md#consistency), any such rule MUST live in `tiler-core` behind a neutral host capability or explicit fact rule with shared conformance tests and a `tier1-divergences.md` entry if applicable.
  - **Option 3 (rejected as acceptance):** Synthetic fact injection violates [Testing](../principles.md#testing) ("Test data looks like real data") and must never substitute for native proof.
- **Evidence still needed before Option 1B (none needed for Option 1A):**
  - Read-only `scope-inspect` sample across 6–10 real macOS apps (fixed-size standard apps, resizable apps, Electron/Chromium, browsers, native dialogs) checking `AXRole`, `AXSubrole`, `AXSize` settable, and whether any non-AppKit toolkit (Qt/GTK/Electron) exposes `AXMinSize`/`AXMaxSize`.
- **Owner & decision-record location:** Project owner; `docs/decisions/layout-and-admission.md#fixed-size-admission-predicate`, `docs/decisions/macos.md`, `docs/spec/reference-outcomes.md#macos-classification-discriminators`, and `docs/changes/macos-0.1-gate-proposal.md` (checks 10 & 13).
- **Implementation outline & acceptance tests:**
  - *If Option 1A approved:* No code change. Document the AppKit AX hint non-exposure limit in `docs/decisions/macos.md` and `macos-0.1-gate-proposal.md` checks 10/13; retain offline predicate & adapter mapping tests and check 13's real-game no-touch proof.
  - *If Option 1B approved:* (1) Add core contract amendment + capability/fact rule for `resizable == Some(false)` admission when size hints are unavailable (with `both/either` interaction defined and explicit tile-override preserved); (2) add core conformance fixtures & reference-outcome discriminator row; (3) plumb `resizable` honestly from `size_settable` (already carried); (4) run native fixture fixed-float admission + send-follow/stay proof on an owned Space.
- **Rollback / containment:** Option 1A has zero rollback risk. Option 1B would be a clean, isolated core predicate branch behind a capability flag, easily reverted without touching wire storage.
- **0.1 impact & deferrability:** Option 1A is immediately deferrable with a documented host-capability limit (consistent with the 2026-10-29 user scope caveat). Option 1B requires an explicit product/contract decision before implementation.
- **Assumptions & TBD:** Assumes winit/AppKit fixture behavior reflects standard AppKit windows; non-AppKit toolkits (Electron/Qt/Java) are unmeasured (TBD).

---

### Decision 2: Constrained-Work-Area Route for No-Space and Best-Effort Admission

- **Recommendation & Status:** **APPROVED (2026-11-24) — Option 1: Defer the native constrained-work-area exhaustion and shrink-repair leg; retain core geometric capacity tests and adapter frame-mapping tests; document the native exhaustion proof as unproven due to display scale.** Do NOT implement a product work-area override (Option 2) or a `CGVirtualDisplay` helper (Option 3). Option 4 (`float_origin`/`noSpace` provenance logging) may be added later as a low-priority P2 observability cleanup, not a 0.1 blocker.
- **Confidence:** High.
- **Rationale & user-visible consequences:**
  - Geometric capacity exhaustion (`no-space` float, largest-splittable fallback, newest-first shrink-repair eviction) is 100% core-owned pure geometry logic (`tiler-core`), already verified by core unit/integration fixtures.
  - On a real macOS display, exhausting a ~2224×1123 work area requires **19,688–21,400** default-hint windows (512 even at catalog max gap 64), completely outside safe fixture scale.
  - [Focus and Return on Investment](../principles.md#focus-and-return-on-investment) directly prescribes stopping here: *"Tooling, test harnesses and evidence collection are means, not ends. Size them to the decision they inform. When effort outgrows the value, stop and choose a simpler route, or drop the work, or accept weaker evidence and record it as weaker."*
  - Option 2 (product work-area override) violates [Simplicity](../principles.md#simplicity) by injecting a test-only geometry lie into production `TileRequest`/`RuntimeConfig`/`NativeContext::derive`, warping display-wide maximize/zoom/border/pager geometry while the OS screen stays full-size — it cannot truthfully prove native OS work-area behavior anyway.
  - Option 3 (`CGVirtualDisplay`) relies on private undocumented APIs with known removal flakiness (Chromium precedent), mutates live display/Space topology, and carries high risk/effort for negligible additional confidence over core tests.
  - Cross-platform precedent: KDE (`docs/backlog.md:101`) likewise has only offline proof + a qualified minimum-size sanity observation for no-space exhaustion.
  - **User-visible consequence:** None in production; macOS users still get full core best-effort admission and capacity-float behavior automatically.
- **Risks, edge cases & cross-platform parity:**
  - Residual risk: a macOS-specific adapter bug when applying a capacity-origin float (e.g., mistakenly writing a tile frame to a capacity float, or mishandling `FloatOrigin::Capacity` diagnostics) would only be caught by offline adapter integration tests rather than a live native exhaustion run. Mitigation: ensure offline `tests/integration` covers core-emitted `FloatOrigin::Capacity` rows passing through macOS `world_dispatch` with zero frame writes and preserved opening frame.
- **Evidence still needed:** None before adopting Option 1; verify offline adapter integration coverage of core capacity-float and shrink-eviction replies (no native work-area hack needed).
- **Owner & decision-record location:** macOS adapter owner; `docs/changes/v4-phase3-macos-port.md` and `docs/backlog.md` (downgrade native constrained-work-area proof from P1 blocker to documented offline-covered limit).
- **Implementation outline & acceptance tests:** No product change. Verify/add an offline `tiler-macos` integration test using a small synthetic `NativeDomain` work area (existing test-only `TopologyMode` seam, NOT production CLI) driving real `WorldEngine` + macOS `world_dispatch` through capacity-float admission and shrink-repair eviction, verifying zero tile-frame writes to capacity floats.
- **Rollback / containment:** Zero production footprint. If a real multi-display virtual-display harness is ever built later for M8/M9 two-display automation, constrained-work-area testing can ride it opportunistically.
- **0.1 impact & deferrability:** **Fully deferrable without blocking 0.1.**
- **Assumptions & TBD:** Assumes offline `tests/integration` tests the full macOS `world_dispatch` reply consumption path for capacity floats.

---

### Decision 3: `AXSystemFloatingWindow` Mapping

- **Recommendation & Status:** **APPROVED (2026-11-24) — Keep withholding `AXWindow` + `AXSystemFloatingWindow` as `None` (unknown) until a truthful native sample exists (P3 documented limit).**
- **Confidence:** High.
- **Rationale & user-visible consequences:**
  - `AXWindow` + `AXFloatingWindow` (the standard AppKit utility `NSPanel` subrole) is already natively measured and mapped to `WindowType::Utility`, so ordinary floating utility panels float properly.
  - `AXSystemFloatingWindow` has no truthful owned sample, and reading foreign system palettes was outside fixture-only scope. Per [Observability](../principles.md#observability) and [Facts versus classification](../decisions/contract.md#facts-versus-classification), never invent a mapping without evidence; holding an unknown rare system palette is strictly safer than floating/managing system chrome erroneously.
  - **User-visible consequence:** Extremely low; rare system-level floating palettes are conservatively held (untouched) rather than classified as managed floats.
- **Risks, edge cases & cross-platform parity:**
  - Risk: If a common user app emits `AXSystemFloatingWindow` during dogfooding, it will be held (unmanaged) rather than participating in float navigation/management. Because holding leaves the window untouched, it cannot corrupt tiling or break the app.
- **Evidence still needed (only if reopened):** One user-run signed `scope-inspect` read of a genuine macOS window reporting `AXSystemFloatingWindow` (including CG layer, onscreen, activation policy, and behavior).
- **Owner & decision-record location:** macOS adapter owner; `docs/changes/macos-batch3-utility-mapping.md` (already recorded) and `docs/backlog.md` (keep P3).
- **Implementation outline & acceptance tests:** No code change now. If a real sample is later captured, update `normalize_window_type` in `crates/tiler-macos/src/runtime_types.rs` and its unit test.
- **Rollback / containment:** None needed; one-line match arm if ever enabled.
- **0.1 impact & deferrability:** **Fully deferrable without blocking 0.1 (P3).**
- **Assumptions & TBD:** Assumes no standard 0.1 user-path application relies on `AXSystemFloatingWindow` being a managed float rather than held; revisit during user dogfooding.

---

### Decision 4: M3 `intentDesired` Storage Owner

- **Recommendation & Status:** **APPROVED (2026-11-24) — Option 1: Adapter-owned atomic host store in `tiler-macos`, REQUIRED for 0.1.**
  - *Note on prior authority:* The ownership question is **already decided** by [docs/decisions/contract.md#intent-ownership-and-persistence](../decisions/contract.md#intent-ownership-and-persistence) and [docs/decisions/persistence.md](../decisions/persistence.md#orderly-restart-adoption):
    - *"Platforms keep the opaque record in host storage (Windows window, KDE session runtime file, macOS where possible) and return unchanged. ... Rejected: core file store keyed by platform identity, losing intent on restart, KDE-only adapter store."*
    - *"Durability stays adapter-local: compare the latest accepted target with verified durable storage, writing only on difference; no storage report field, sequence, acknowledgement or attempt lane."*
    - *"M3 retains macOS intentional-float intent across orderly tiler restart within a session... not a product-scope cut."*
  - Furthermore, code review finding **R4** shows the current `unpersisted_intent_desired` session latch permanently freezes all tiling/focus on the first `ToggleFloat`. Deferring M3 (Option 4) is therefore **not viable** for 0.1.
- **Confidence:** High.
- **Rationale & user-visible consequences:**
  - Keeps core 100% platform-agnostic (no filesystem I/O or macOS audit-session/launch-date logic in `tiler-core`), directly mirroring KDE's `world-intent-store.ts` v2 opaque store design.
  - **User-visible consequences:** (1) `ToggleFloat` works natively without latching/freezing the session; (2) intentional floats (and fixed-tile overrides once applicable) survive orderly tiler restarts within the same macOS login session without losing float state; (3) logout/login starts clean with no stale cross-login adoption.
- **Risks, edge cases & cross-platform parity:**
  - **PID / CGWindowID reuse:** A window record must bind `(CGWindowID, pid, numeric process launch-date/birth, bundle_id)` and a validated login-session namespace (`getaudit_addr` `ai_asid` + UID). Unverifiable bindings must be discarded on import, never guessed by title or PID alone.
  - **Symlink / permission hazards:** Must enforce `0700` directory, `0600` file, UID match, no symlink following (`O_NOFOLLOW`), bounded file size/row count (10,000 rows / 1024-byte values), and atomic temp-file + fsync + rename.
  - **Whole-reply hold:** Matches approved Q3 in `retained-intent-desire.md` (narrower failure isolation is tracked separately as P2 `Intent save failure isolation` across all platforms).
  - **Stale/racing callbacks & skipped publications:** Store writes must be serialized; completion checks actual persisted snapshot against latest accepted `intentDesired` (not pending map or reply continuation), retrying on each later accepted reply while unequal with zero timers.
  - **Cross-platform parity:** Identical semantics to KDE's accepted `retained-intent-desire` implementation (pre-admission import, zero redundant writes on equality, canonical re-encoding, failure-then-recovery on later accept, clean re-login namespace invalidation).
- **Evidence still needed before claiming native M3 acceptance (not before starting offline implementation):**
  1. Native readback verifying numeric `NSRunningApplication.launchDate` stability across fresh signed observer launches on the live fixture (`macos-v4-platform-fit.md:82,1039-1042`).
  2. Native fixture proof of: toggle-float write → unrelated replies with zero disk writes (equal SHA-256 + mtime) → orderly tiler restart → exact lifetime re-adoption with float preserved and zero rewrite → isolated store failure (`0400`) hold and recovery (`0600`) on next accepted reply → skipped issuing publication recovery → re-login / mismatched-session namespace rejection.
- **Owner & decision-record location:** macOS adapter owner (implementation) + Project owner (confirm exact host directory slot); record final macOS store format/slot in `docs/decisions/persistence.md` (alongside KDE and Windows subsections) and `docs/changes/retained-intent-desire.md`.
- **Implementation outline & acceptance tests:**
  1. **Host store module (`crates/tiler-macos/src/intent_store.rs`, `#![forbid(unsafe_code)]`):** Private store under per-user `~/Library/Application Support/OmniTiler/intent-v1-<uid>-<asid>.json` (using `dirs`/standard macOS user-support resolution + private `0700` dir, `0600` file, `O_NOFOLLOW`, UID verification, atomic temp+rename, versioned schema storing `{ session_uid, session_asid, entries: [{ win, wid, pid, launch_date_ns, bundle_id, data }] }`). Corrupt/unreadable/mismatched-session files degrade cleanly to empty with bounded diagnostic (matching KDE).
  2. **Pre-admission hydration:** On startup / fresh connection before first admissible report, read store, verify each entry against live observed `(wid, pid, launch_date_ns, bundle_id)` and populate `WorldReportV4.intent_records` for verified live lifetimes only.
  3. **Reconciler in `world_dispatch.rs`:** Replace `unpersisted_hold` / `unpersisted_intent_desired` permanent latch with desired-vs-last-persisted reconciliation in both `Accepted` and `CommandConsumed` apply paths:
     - If `desired == verified_persisted`: zero disk writes, proceed to dependent native apply.
     - If unequal or unverified: perform serialized atomic persist before dependent native apply; on failure, retain whole-reply hold (no permanent latch!) and retry on the next accepted reply while unequal.
     - Explicit `null` clears delete the persisted entry while retaining `null` in durable-equality knowledge so subsequent publications cause zero writes.
  4. **Offline tests:** Equality/zero-write, skipped issuing publication, failure-then-later-accept recovery, corrupt/symlink/wrong-UID/wrong-session rejection, restart hydration timing (before admission vs. late import ignored), close/ID-reuse fencing, and `ToggleFloat` end-to-end integration test.
  5. **Native fixture tests:** Use an isolated `--intent-store-dir` (or fixture run dir) so user storage is never touched; prove the four KDE-equivalent native legs (restart, omission, failure/recovery, equality/zero-writes).
- **Rollback / containment:** If storage write fails at runtime, the recoverable whole-reply hold prevents diverging native state without bricking the session (recovers automatically once storage succeeds). Corrupt store degrades to empty (safe fail-open on read).
- **0.1 impact & deferrability:** **Hard 0.1 requirement; NOT deferrable.**
- **Assumptions & TBD:** Assumes `getaudit_addr` `ai_asid` + UID is a valid same-login namespace (corroborated by one physical logout/login observation in `macos-v4-platform-fit.md:123-129`) and that `NSRunningApplication.launchDate` is readable and stable for ordinary/fixture apps (fallback for processes with null launch date: do not persist/re-adopt unverifiable bindings, per M3 rule).

---

### Decision 5: Close-Gap Exact Destroyed-Window Identity

- **Recommendation & Status:** **APPROVED (2026-11-24) — Option 1: Implement exact `kAXUIElementDestroyedNotification` identity before 0.1, sequenced right after R1/R2/R5 and D4 (M3).** Gate implementation on the small bounded fixture diagnostic first (verifying `CFEqual` element matching on close and zero false fires on Space-switch / minimize / `Cmd+H`) so any unexpected macOS behavior is caught before investing in the adapter plumbing.
- **Confidence:** Medium-High (engineering route is clean and safe; whether 0.4–1.1 s close latency blocks 0.1 is explicitly a user subjective/jank call under [VISION.md](../../VISION.md#faithfulness-versus-jank)).
- **Rationale & user-visible consequences:**
  - Measured receipt-to-retile latency on window close is **0.4–1.1 s (median ~1.0–1.1 s)** (`macos-responsiveness-investigation.md:1004-1021`), during which the adapter holds the entire inventory (`held-incomplete-inventory`) waiting for CG to drop the lingering offscreen window entry on the 1 s backstop poll.
  - Root cause: `ffi.rs:3189-3205` already receives the AX notification callback from macOS, but **discards the notification name and `AXUIElementRef`**, forwarding only a coalesced wake counter!
  - **User-visible consequence of Option 3 (defer):** Every time a user closes a window, surviving tiles sit frozen for ~1 second and workspace/focus commands stall under an incomplete-inventory hold before reflow happens — unmistakably perceptible jank in day-to-day tiling.
  - **User-visible consequence of Option 1 (implement):** Window close reflows immediately (~50–70 ms end-to-end based on measured prune-to-batch timings of 58–68 ms) without waiting for CG's offscreen entry to expire, while keeping the CG-alive guard intact for all non-destroyed AX absences.
- **Risks, edge cases & cross-platform parity:**
  - **Critical macOS subtlety:** Calling `_AXUIElementGetWindow(element)` inside a `kAXUIElementDestroyedNotification` callback can fail once the window element is already torn down. Therefore, identity resolution MUST match the callback `AXUIElementRef` (`CFEqual`) against the per-app AX owner's retained `(wid, AxElement)` subscription map (with `_AXUIElementGetWindow` as secondary corroboration only while still readable). Unmatched/unknown destroyed elements must NEVER guess a `wid` — they safely fall back to the existing CG-alive guard!
  - **Hidden-Space / minimized / `Cmd+H` safety:** Hidden-Space switches, `AXMinimized`, and `Cmd+H` hide must NEVER emit `kAXUIElementDestroyedNotification` (and even if an app tears down a non-window UI element, only registered managed window elements in the `(wid, AxElement)` map match). If an app destroys and recreates a window with a reused `wid`, generation/element identity fences prevent pruning the replacement.
  - **Option 2 (bounded reread) rejected:** Adds AX/CG churn and still cannot prune until CG drops the lingering entry (~1 s), solving neither latency nor truth.
  - **Cross-platform parity:** KDE (`windowClosed`) and Windows (`EVENT_OBJECT_DESTROY`) both deliver exact window destruction identity and reflow immediately; matching them eliminates a glaring macOS-only UX regression.
- **Evidence still needed before implementation:**
  - Verify on the signed live fixture (bounded diagnostic check):
    1. Does closing one fixture window while the fixture process remains alive deliver `kAXUIElementDestroyedNotification` with an `AXUIElementRef` that `CFEqual`-matches the subscribed window element?
    2. Do Space-switch away, minimize, and `Cmd+H` app-hide emit **zero** `kAXUIElementDestroyedNotification` events for live windows?
- **Owner & decision-record location:** Project owner (confirm 0.1 latency bar) + macOS adapter owner (implementation); `docs/decisions/macos.md#fixture-and-scoped-input-defaults` & `docs/changes/macos-responsiveness-investigation.md`.
- **Implementation outline & acceptance tests (if Option 1 approved):**
  1. In `ffi.rs` / `ax_session.rs`: pass notification name + element pointer (matched via `CFEqual` against the per-app owner's subscribed `(wid, AxElement)` table on the AX owner thread — no cross-thread raw AX refs!) and collect exact `destroyed_wids: Vec<u32>` fenced by `(pid, generation)`.
  2. Transport `destroyed_wids` on `ScopedObservation` / snapshot into `runtime_core.rs` roster reconciliation: an AX-absent `wid` that is in `destroyed_wids` (and absent from the current AX window list) is positively pruned even if CG still lists a lingering offscreen row. Any AX-absent `wid` NOT in `destroyed_wids` retains the exact existing CG-alive / unreadable-CG guard!
  3. **Offline tests:** Exact destroyed `wid` prunes while CG row lingers; non-destroyed AX-absent `wid` with live CG row (hidden-Space / minimized / `Cmd+H`) stays retained/held; unmatched destroyed element degrades to CG guard; stale-generation destroyed event ignored; close-then-recreate same `wid` in current AX list is NOT pruned.
  4. **Native fixture test:** Close one of two tiled fixture windows while fixture stays alive; measure receipt-to-retile latency (target < 150 ms vs. prior 432–1104 ms) and verify zero false prunes across Space-switch away/back, minimize/restore, and `Cmd+H` hide/unhide.
- **Rollback / containment:** Cleanly contained behind the `destroyed_wids` match check; if native verification reveals false destruction notifications on any lifecycle transition, returning empty `destroyed_wids` instantaneously restores the existing CG-alive guard with zero core or protocol changes.
- **0.1 impact & deferrability:** Deferrable **only** if user explicitly accepts ~1.0 s close-to-retile jank for 0.1. Recommended for 0.1.
- **Assumptions & TBD:** Assumes AppKit delivers `kAXUIElementDestroyedNotification` on the subscribed window element before CG removes the lingering entry (to be confirmed by the quick fixture check above).

---

### Decision 6: Post-v4 Session Lifecycle Authorization and Sequencing

- **Recommendation & Status:** **APPROVED (2026-11-24) — Option 2: User-run bounded lifecycle acceptance BEFORE 0.1 release, sequenced AFTER adapter stabilization (R1–R5, M3, M4, slices 2–7). Never autonomous agent execution; NOT deferred past 0.1.**
- **Confidence:** High.
- **Rationale & user-visible consequences:**
  - [VISION.md](../../VISION.md#reliability) makes reliability across sleep/wake, output hotplug, scaling, fullscreen, and config changes **non-negotiable**, and [macos-0.1-gate-proposal.md](macos-0.1-gate-proposal.md) includes checks 30, 50, N3, MX2, MX7, MX8 as 0.1 must-pass gates.
  - [docs/live-macos-testing.md](../live-macos-testing.md#safety-boundary) prohibits automated agents from locking, sleeping, logging out, restarting, or running unscoped sessions.
  - Option 3 (fixture-only proxy) cannot observe real OS lock/sleep/logout kernel/WindowServer/TCC transitions.
  - Observed stale desktop access (`macos-v4-platform-fit.md:371-387`) was attributed to a surviving pre-login dev-tool context, not a demonstrated tiler defect — it does **not** justify derailing current phase-3 porting to run lifecycle tests early.
- **Risks, edge cases & cross-platform parity:**
  - Risk of running too early: testing before M3 store, R1 cursor-observer gating, R2 MC dual-surface, and M4 maximize land would test an incomplete runtime and require repeating the user-run pass anyway.
  - Risk of skipping before 0.1: silent CGEventTap death on wake, stranded window holds after unlock, or broken TCC/WindowServer handles after logout/login would violate Reliability.
  - Cross-platform parity: matches KDE and Windows 0.1 release gates (where sleep/wake and session transitions are user-run physical acceptance checks).
- **Evidence still needed:**
  - Offline verification that lock/unlock, Secure Input, and session-unavailable signals cleanly release the event tap, pause `cursor_observer` (per R1), emit no stale writes, and recover on unlock.
  - At release-candidate readiness: one bounded user-run physical lifecycle sequence (lock/unlock → sleep/wake → logout/login + relaunch) with explicit recovery steps and log/readback capture.
- **Owner & decision-record location:** Project owner (executes bounded physical sequence) + macOS adapter owner (maintains offline fences and prepares copy-pasteable procedure); `docs/changes/macos-0.1-gate-proposal.md` (checks 30, 50, N3, MX2, MX7, MX8) & `docs/changes/macos-v4-platform-fit.md`.
- **Implementation outline & acceptance tests:**
  1. Ensure lock/Secure-Input release covers `cursor_observer` (part of R1) as well as `CGEventTap` and AX/Space actuators.
  2. When slices 1–7 + M3 + M4 + Settings + Underlay are ready for release acceptance, publish a copy-pasteable, bounded user-run procedure covering:
     - Pre-check: signed bundle verified, `preflight` `ax_trusted:true`, exact product PID recorded, out-of-tap recovery (Activity Monitor / exact PID stop) confirmed.
     - Leg A: Lock screen (10 s) → unlock → verify tap active, tiling/focus responsive, no stranded holds.
     - Leg B: Sleep (30 s) → wake (with tiled + fullscreen window, covering check 30/N3) → verify topology re-observation, no replayed writes, fullscreen untouched.
     - Leg C: Logout → login → normal relaunch → verify fresh M3 namespace rejects old-login intent store, TCC grant survives, ordinary tiling works cleanly.
     - Stop conditions: any crash, stuck input consumption, unresponsive keyboard/mouse, or lost TCC grant stops the sequence immediately and uses the documented out-of-tap recovery ladder.
- **Rollback / containment:** Out-of-tap recovery ladder (`live-macos-testing.md#recovery-ladder`): physical Force Quit / Activity Monitor exact-PID stop, no autostart/login-item enabled during the test leg.
- **0.1 impact & deferrability:** **Hard 0.1 release gate (cannot be deferred past 0.1)**, but **deferred in sequencing** until phase-3 implementation stabilizes.
- **Assumptions & TBD:** Assumes the packaged/installed launch path (MX8: DMG/Homebrew + signing/notarization) is settled before the final logout/login relaunch leg.

---

## Part 4: Recommended Decision Order & Smallest Next Actions

### Recommended Order

1. **Pre-worktree Cleanup & High-Severity Fixes (before committing or expanding native legs):**
   - **R5 (Doc hygiene):** Scrub the dev-host OS version in `docs/changes/v4-phase3-macos-port.md:294` and the role/routing terms in `docs/changes/archive/macos-focus-saturation-close-cause.md:51-52,86`. Then the current uncommitted worktree (`AXFloatingWindow -> Utility`, `focus_saturation`, `fixture_leg` owned-Space) is clean to commit when appropriate.
   - **R1 (`cursor_observer` gating):** Gate `cursor_observer` on effective core `focusFollowsCursor` and suspend AX hit-tests during lock / Secure Input / focused fullscreen. Add offline unit/integration tests proving zero AX hit-tests and zero `OP_CURSOR` reports when disabled or locked/fullscreen.
   - **R2 (Mission Control dual-surface):** Restore `com.apple.dock` (`mc` → `mc.display`) as automatic fallback beside `com.apple.WindowManager` (`mc.display`) so macOS 15 is not broken. Add offline tests for both surfaces.
2. **Confirm D0 (Meta-gate) & Lock D2 + D3 (Immediate — zero code):**
   - **D0:** Confirm 2026-10-29 0.1 scope stands (43 M + MX1–MX8) with narrow host-capability/ROI limits.
   - **D2:** Record constrained-work-area native exhaustion as deferred/offline-covered; verify offline `tiler-macos` integration coverage of core `FloatOrigin::Capacity` replies.
   - **D3:** Keep `AXSystemFloatingWindow` withheld (`None`) at P3.
3. **Execute D4 (M3 `intentDesired` adapter-owned host store) — Highest-value unblocker:**
   - **Smallest next action:** Run one bounded read-only `proc-identity` / numeric launch-date check on a signed fixture launch (confirming fresh-observer equality), then implement `intent_store.rs` + `world_dispatch.rs` desired-vs-durable reconciler, removing the permanent `unpersisted_intent_desired` session latch and proving `ToggleFloat` + restart persistence offline and on the signed fixture.
4. **Execute D5 (Close-gap exact destroyed identity) & Record D1 (Option 1A):**
   - **D1:** Record Option 1A (documented AppKit AX hint non-exposure limit; zero code change; revisit after real-app dogfooding if needed).
   - **D5:** Run the small bounded signed-fixture diagnostic verifying `kAXUIElementDestroyedNotification` `CFEqual` element matching on close vs. zero false fires on Space-switch / minimize / `Cmd+H`. If clean, implement exact `destroyed_wids` transport and roster pruning while keeping the CG-alive guard as fallback for all unmatched absences.
5. **Continue Phase-3 Slices & Remaining 0.1 Scope (under D0):**
   - Complete **R3** (C5 typed boundary dispatch without 4-leg JSON codecs), native held-Space recovery proof, remaining MX1/MX4/MX6 legs, M4 composed maximize (once KDE ST/RC gates clear for slice 5), M8/M9 two-display/reconnect (slice 7, after KDE RC leg-2), native Settings window (check 12), and SkyLight group-underlay attempt (checks 23/44/N4).
6. **Execute D6 (User-run session lifecycle & real-game/physical acceptance) last, at 0.1 RC:**
   - **Smallest next action (now):** None beyond R1 lock/Secure-Input observer gating. Prepare the bounded copy-pasteable user procedure once slices 1–7 + M3/M4 stabilize.

---

## Part 5: Follow-Up Code Review and Tentative Decisions (2026-11-24)

### Follow-Up Review Findings (Uncommitted Worktree)

Follow-up review of the uncommitted worktree after offline R1/R2/R4/R5 rectifications and the D5 diagnostic (`ax_destroyed_probe.rs` + `ffi.rs` unsafe audit is clean: runloop source is thread-confined, invalidated and notifications removed before `Box::from_raw`, callback element is borrowed-only, and `ax_destroyed_probe.rs` uses `#![forbid(unsafe_code)]`).

Six concrete items should be resolved before committing the worktree or running the D4 native restart proof:

| # | Severity | Location | Finding | Required Fix |
| --- | --- | --- | --- | --- |
| **F1** | **High** | `crates/tiler-macos/src/world_runtime.rs:214,379-410`; `world_dispatch.rs:568,659,1013,1162` | `intent_admitted` is a single session-wide `bool`, latched `true` on the first `ReplyVerdict::Applied`. Because per-app AX workers report asynchronously on macOS, report #1 after restart commonly has 0 windows or only App A; when App B's windows arrive in report #2, `intent_records_for_admission()` returns empty and App B's persisted float intent is silently dropped. | Track import/admission **per window** (matching core's per-window `self.intent_admitted.contains(win)` in `crates/tiler-core/src/world_intent_state.rs:174-177`), omitting a window from `intent_records` only once it has been admitted rather than cutting off the whole session after report #1. |
| **F2** | **High** | `crates/tiler-macos/src/world_runtime.rs:475-479` | In `reconcile_intent()`, if `live_intent_binding(pid)` returns `None` (e.g., an unbundled/CLI-launched GUI window or any process with `launchDate == nil`), it returns `true` (whole-reply hold). Because core retains `intent_desired` on every later reply while that window stays alive, toggling float on an unverifiable process permanently freezes tiling, focus, and presentation for every window until it closes. | Distinguish exiting/transient failures from an alive process with no verifiable LaunchServices `(bundle_id, launchDate)`: per [persistence](../decisions/persistence.md#orderly-restart-adoption) ("Discard only genuinely unverifiable bindings") and [Resilience](../principles.md#resilience), log a bounded diagnostic and skip persisting that unverifiable row rather than wedging the whole session. Consider keeping `intent_store.rs` purely opaque + length-bounded rather than calling `tiler_core::intent_data::decode_intent_data` at `intent_store.rs:531-533`. |
| **F3** | **High** | `crates/tiler-macos/src/cursor_observer.rs:270-305`; `synth.rs:464-475`; `runtime.rs:867-869` | `observe()` evaluates `synth::cursor_position()` (`CGEvent::new(None)` + `CGEvent::location`) and `handle.cursor_sample_ack()` (`lock_shared` mutex) eagerly before `if !gate.allowed()`, running both 4×/second even when `focusFollowsCursor` is off or `game_paused` is true. When `focusFollowsCursor` is on and `game_paused` is true, lines 277–283 also call `ffi::session_state()` (`CGSessionCopyCurrentDictionary`) and `ffi::secure_input_known()` every 250 ms during fullscreen games. | Short-circuit before calling `session_state()`, `secure_input_known()`, `cursor_position()`, or `cursor_sample_ack()` whenever `!focus_follows_cursor || game_paused` (or pass lazy closures into `tick()`), and avoid waking 4×/s when `focusFollowsCursor` is off. |
| **F4** | **High** | `crates/tiler-macos/src/mc_lifecycle.rs:264-274,355-386,444-465` | Two macOS 15 dual-surface bugs: (1) `mc_presentation_windows_present()` calls `mc_surface_identity()`, which picks `WindowManager` whenever `com.apple.WindowManager` is running — it **is** running on macOS 15 (Stage Manager) even though `Dock` owns Mission Control's AX tree, so inspecting `WindowManager`'s `layer >= 0` CG windows can cause `mc_closed_verified()` to timeout forever on macOS 15; (2) `mc_display_children()` propagates `?` on `WindowManager` AX errors (`surface_element` / `children_with_id`) instead of falling through to `Dock`. | Key the CG presentation check on which surface actually owned the open Mission Control tree (not merely whether `WindowManager` is running), and let non-ownership AX unavailability on `WindowManager` fall through to `Dock` while still refusing protected/mismatched bundles. |
| **F5** | **Medium** | `crates/tiler-macos/src/world_runtime.rs:240-242`; `intent_store.rs:491-507` | `if !cfg!(test)` is `false` when Cargo builds `tiler-macos` for integration tests (`tests/integration/runtime_phase1.rs`), so `Runtime::start()` opens the real per-user `~/Library/Application Support/OmniTiler/intent-v1-<uid>-<asid>.json`. | Pass an explicit store option/path on `RuntimeConfig` (disabled or isolated temp dir in tests; `open_default()` in production CLI) instead of `cfg!(test)`. Reuse `desktop_shortcuts::recovery_dir()` instead of duplicating `default_dir()`. |
| **F6** | **Low** | `crates/tiler-core/src/world_cursor.rs:23-37`; `world_engine.rs:793-800` | Read-only `pub fn cursor_game_paused(&self) -> bool` added to `tiler-core`. Clean, platform-neutral, and matches adjacent `focus_follows_cursor()`/`cursor_warp()`. | Acceptable to keep; run `tiler-core` tests and strict clippy alongside `tiler-macos` before committing. |
| **F7** | **Low** | `docs/changes/macos-v4-open-decisions-review.md`; `v4-phase3-macos-port.md`; `fixture_leg.rs` (2,230 lines) | Leftover R5 wording in this review file and stale `intentDesired` table rows in `v4-phase3-macos-port.md` (fixed in this doc pass). `fixture_leg.rs` exceeds the ~1,500-line soft cap ([File Size](../principles.md#file-size)) and `ax_destroyed_probe.rs` is at 1,482 lines. | Avoid growing `fixture_leg.rs` further with new diagnostic harnesses without splitting helpers. |

### Tentative Follow-Up Decisions (TD1–TD7, 2026-11-24 — Pending Later User Confirmation)

All seven entries below are **tentative** working directions to unblock progress and will be reviewed later before promotion to active policy:

| ID | Topic | Tentative Direction | Key Constraints & Caveats |
| --- | --- | --- | --- |
| **TD1** | **D5 close-gap (`AXUIElementDestroyed`)** | **Option B — Authorize the second bounded diagnostic.** | Record at event time: notification identity, fresh `AXWindows` presence, and CG state across close, window hide, app hide, minimize, and Space switch. Explicitly check whether `orderOut:` window-hide is also absent from `AXWindows` while CG-present; if no safe close-only discriminator distinguishes close from window-hide, fall back cleanly to Option A (retain CG-alive guard, document the limit, revisit after dogfooding) — never risk false-pruning hidden windows. |
| **TD2** | **KDE reconnect same-numbered merge** | **Keep displaced same-numbered units separate (never merge).** | Matches Tier 1 evidence (i3/sway/Hyprland/COSMIC move whole workspace objects with no merge/later-unmerge semantics) and the offline KDE leg-2 fix ([reconnect discriminator](v4-phase2-kde-physical-reconnect.md#2026-11-10-offline-leg-2-discriminator)). Final macOS slice-7 acceptance still waits on physical KDE RC. |
| **TD3** | **M4 maximize sequencing** | **Implement and offline-test macOS composed maximize now; hold final slice-5 native acceptance until KDE ST/RC clears.** | Approved core mechanism is live (`maximize=false`, native `maximized:null`, no adapter maximize policy, no M3 extension). Uses waiting time without claiming slice-5 acceptance prematurely. |
| **TD4** | **Shared host-settled geometry** | **Do not block R3/D4/D5/M4; defer core design and implementation post-0.1 unless dogfooding reproduces visible resize shortfalls or tile oscillation.** | If dogfooding surfaces shortfalls (e.g., Ghostty cell-grid or DPI offsets), bring one core-owned, platform-neutral `FrameAttempt` settlement design for explicit user approval before any core change ([timing decision](../decisions/layout-and-admission.md#host-settled-geometry-design-timing)). |
| **TD5** | **MX8 distribution (signing, updates, App Store)** | **Reject Mac App Store; target Developer ID signing + Apple notarization for DMG + Homebrew Cask; 0.1 update channel is Homebrew Cask + GitHub Releases DMG only (no in-app updater in 0.1).** | Mac App Store requires App Sandbox, which is technically incompatible with `AXUIElement` foreign-window control, global `CGEventTap`, and Tier 2 private APIs. |
| **TD6** | **macOS 15 support floor** | **Fix F4 offline and keep the macOS 15 floor with the recorded unverified risk.** | Optionally validate Mission Control Dock fallback and basic tiling in an Apple Silicon macOS 15 VM at RC if desired. |
| **TD7** | **D4 store slot** | **Confirm `~/Library/Application Support/OmniTiler/intent-v1-<uid>-<asid>.json`.** | Matches existing `0700` private Application Support directory (`desktop_shortcuts.rs` / `dev_selection.rs`). Opportunistic cleanup of stale prior-ASID files can be handled later as P2 hygiene. |

### Recommended Next Execution Order

1. **Fix F1–F5** in the uncommitted worktree and re-run `tiler-core` + `tiler-macos` tests, `cargo fmt --all -- --check`, and strict clippy.
2. **Run D4 native restart/store proof** under the isolated fixture scope (including a two-app staggered startup check covering F1).
3. **Complete R3 (C5 typed ingress)** in `crates/tiler-macos/src/world_dispatch.rs`, replacing the four-leg JSON codec while preserving the D3=B semantic gates.
4. **Run the second bounded D5 diagnostic** (TD1), explicitly testing whether window-hide is also AX-absent and falling back to Option A if no safe close-only discriminator exists.
5. **Implement and offline-test macOS M4** (TD3, holding final acceptance for KDE ST/RC) and continue MX1/MX4/MX6 fixture legs.

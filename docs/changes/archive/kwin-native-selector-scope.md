# KWin native selector scope

Advertise a per-output native selector only when the workspace read method exists and KWin's per-output desktop option is strictly true. Otherwise core retains the requested setting, diagnoses unsupported separate modes, and uses shared mode. Keep KWin 6.6 shared behavior and do not write native settings or change shared-roster slot policy.

- Native adapter: gate discovery, renegotiate on the optional KWin notification, and fail closed immediately before one-output actuation if the option is no longer true.
- Fake-host acceptance: strict true, false/missing/unreadable, explicit shared actuation, option changes after planning, and KWin 6.6. Verify connection-level capabilities and no stale per-output write.
- Explain the host-specific selector rule in the wire contract. Verify with KWin typecheck and affected fake-host tests.

Before the fix, the focused offline regressions observed the wrong `per-output` handshake with the option off and a switch from `ws1` to `ws2` after the option changed during planning. The pinned KWin setter delegates to the virtual desktop manager, which updates all outputs when the option is off. The existing broker/engine handshake remints the generation; no core change is needed.

The adapter fences a conflicting bound scope before any discovery offer (including an existing wait-mismatch recovery). On modern KWin an unreadable option advertises the conservative all-output scope but cannot authorize a shared setter either: KWin could still switch only the active output. Verified-false and 6.6 hosts can use the shared setter. Tests use the real broker for the settings diagnostic and shared selection; its two output projections share a logical ID, and the harness drains the acknowledged command before checking native setters.

The standalone KWin 6.6 API guard already reports the same ten unrelated unaudited symbols at the baseline; the selector change introduces no new guard findings.

Verified: `npm --prefix kwin run typecheck` passed. The production bundle built with `npm run build` from `kwin`; the 82 affected test bundles passed under `node --test --test-concurrency=4` (1,049 passed, existing native gesture/presentation check skipped, zero failures). The new fake-host tests cover log-and-refuse before notify, stale reply and overlapping rediscovery fencing, rebind, legacy shape, and the real broker's core-degraded shared selection with zero per-output calls. Full gate: Rust 3,580 passed, KWin 1,456 passed, frozen fixtures unchanged.

Live smoke (real Plasma session, two outputs, KWin per-output desktops enabled, `just dev-on --scope-class OmniTilerTestWindow`): `ReadWorldPresentation` showed one native desktop bound to distinct logical workspaces per output (for example `ws-3` on one output and `ws-1` on the other, both bound to the same native ID), so separate mode stayed in effect under the strict-true gate. No setting changes and no input.

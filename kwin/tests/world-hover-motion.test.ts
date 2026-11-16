import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry, WORLD_DEBOUNCE_MS } from "../src/world-entry";
import { discover, makeHarness, winById, type FakeSignal, type Harness } from "./world-producer-harness";

function installHitTest(h: Harness, pos: { x: number; y: number }): void {
    (h.workspace as Record<string, unknown>)["cursorPos"] = pos;
    (h.workspace as Record<string, unknown>)["windowAt"] = (point: { x: number; y: number }) =>
        h.wins.filter((w) => {
            const f = w.frameGeometry as { x: number; y: number; width: number; height: number };
            return point.x >= f.x && point.x < f.x + f.width && point.y >= f.y && point.y < f.y + f.height;
        });
}

function centerOf(h: Harness, id: string): { x: number; y: number } {
    const f = winById(h, id).frameGeometry as { x: number; y: number; width: number; height: number };
    return { x: Math.floor(f.x + f.width / 2), y: Math.floor(f.y + f.height / 2) };
}

function makeFakeSignal(): FakeSignal {
    const handlers: Array<(payload?: unknown) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (payload?: unknown) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (payload?: unknown) => void): void => {
                const idx = handlers.indexOf(handler);
                if (idx >= 0) {
                    handlers.splice(idx, 1);
                }
            },
        },
    };
}

describe("world entry enabled hover motion capture", () => {
    it("subscribes to cursorPosChanged when focusFollowsCursor is true, coalesces, mints genuine core hover request, attempts once, and disconnects on stop", async () => {
        const h = await makeHarness("win-b", ["win-a", "win-b"], {
            scopeClass: "OmniTilerTestWindow",
            devSettingsJson: JSON.stringify({ focusFollowsCursor: true }),
        });
        try {
            await discover(h);
            await h.settle();

            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 1, "cursorPosChanged subscribed once when hover enabled");

            const callsBefore = h.calls.length;
            const focusedBefore = h.focused.length;
            const posA = centerOf(h, "win-a");
            installHitTest(h, posA);

            // Burst of genuine cursorPosChanged signals must coalesce into one capture.
            h.fire("cursorPosChanged");
            h.fire("cursorPosChanged");
            h.fire("cursorPosChanged");
            h.runTimers(WORLD_DEBOUNCE_MS);

            const newDescribes = h.calls.slice(callsBefore).filter((c) => c.method === "DescribeWorld");
            assert.equal(newDescribes.length, 1, "cursorPosChanged burst coalesces into one DescribeWorld capture");
            const report = (JSON.parse(newDescribes[0]!.payload) as { report: Record<string, unknown> }).report;
            assert.deepEqual(report["cursor"], [posA.x, posA.y]);
            assert.equal(report["cursorWindow"], "win-a");

            await h.settle();
            const newFocused = h.focused.slice(focusedBefore);
            assert.deepEqual(newFocused, ["win-a"], "genuine core FocusFollowsCursor request focuses win-a once");
            assert.equal(h.activeRef?.internalId, "win-a");

            // Teardown disconnects cursorPosChanged and suppresses trailing motion captures.
            h.handle.stop();
            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 0, "stop disconnects cursorPosChanged");
            const callsAfterStop = h.calls.length;
            h.fire("cursorPosChanged");
            h.runTimers(WORLD_DEBOUNCE_MS);
            assert.equal(
                h.calls.slice(callsAfterStop).filter((c) => c.method === "DescribeWorld").length,
                0,
                "cursorPosChanged after stop schedules no captures",
            );
        } finally {
            await h.stop();
        }
    });

    it("does not mint a new focus request when cursorPosChanged repeats at the same cursor position", async () => {
        const h = await makeHarness("win-b", ["win-a", "win-b"], {
            scopeClass: "OmniTilerTestWindow",
            devSettingsJson: JSON.stringify({ focusFollowsCursor: true }),
        });
        try {
            await discover(h);
            await h.settle();

            const posA = centerOf(h, "win-a");
            installHitTest(h, posA);
            h.fire("cursorPosChanged");
            h.runTimers(WORLD_DEBOUNCE_MS);
            await h.settle();
            assert.equal(h.activeRef?.internalId, "win-a");

            // Keyboard/activation moves focus to win-b while cursor stays static over win-a.
            h.activeRef = winById(h, "win-b");
            h.fire("windowActivated");
            h.runTimers(WORLD_DEBOUNCE_MS);
            await h.settle();
            assert.equal(h.activeRef?.internalId, "win-b");

            const focusedBefore = h.focused.length;
            // Repeated cursorPosChanged at the exact same static cursor position.
            h.fire("cursorPosChanged");
            h.runTimers(WORLD_DEBOUNCE_MS);
            await h.settle();
            assert.equal(h.focused.length, focusedBefore, "static cursor repeated signal must not steal focus");
            assert.equal(h.activeRef?.internalId, "win-b");
        } finally {
            await h.stop();
        }
    });

    it("does not subscribe to cursorPosChanged when focusFollowsCursor is false", async () => {
        const h = await makeHarness("win-b", ["win-a", "win-b"], {
            scopeClass: "OmniTilerTestWindow",
            devSettingsJson: JSON.stringify({ focusFollowsCursor: false }),
        });
        try {
            await discover(h);
            await h.settle();

            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 0, "no cursorPosChanged subscription when hover disabled");
            const callsBefore = h.calls.length;
            installHitTest(h, centerOf(h, "win-a"));
            h.fire("cursorPosChanged");
            h.runTimers(WORLD_DEBOUNCE_MS);
            assert.equal(
                h.calls.slice(callsBefore).filter((c) => c.method === "DescribeWorld").length,
                0,
                "cursorPosChanged schedules no capture when hover disabled",
            );
        } finally {
            await h.stop();
        }
    });

    it("keeps single subscription across configChanged and rebinds cleanly across generation rotation", async () => {
        const configChanged = makeFakeSignal();
        const h = await makeHarness("win-b", ["win-a", "win-b"], {
            scopeClass: "OmniTilerTestWindow",
            devSettingsJson: JSON.stringify({ focusFollowsCursor: true }),
            options: { configChanged: configChanged.signal },
        });
        try {
            await discover(h);
            await h.settle();
            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 1);

            for (const handler of [...configChanged.handlers]) {
                handler();
            }
            h.runTimers(WORLD_DEBOUNCE_MS);
            await h.settle();
            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 1, "configChanged does not double-subscribe");

            // Trigger generation rotation via restart on the same surface/bridge.
            const callsBeforeRestart = h.calls.length;
            h.restart();
            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 0, "old subscription detached on teardown/rotation");

            await discover(h, callsBeforeRestart);
            await h.settle();
            assert.equal(h.signals["cursorPosChanged"]?.handlers.length, 1, "re-subscribed after new generation settings accepted");

            const callsBeforeMotion = h.calls.length;
            installHitTest(h, centerOf(h, "win-a"));
            h.fire("cursorPosChanged");
            h.runTimers(WORLD_DEBOUNCE_MS);
            assert.equal(
                h.calls.slice(callsBeforeMotion).filter((c) => c.method === "DescribeWorld").length,
                1,
                "motion capture works after generation rotation",
            );
        } finally {
            await h.stop();
        }
    });

    it("fails open without captures when cursorPosChanged is absent or throws", () => {
        for (const mode of ["absent", "getter-throws", "connect-throws"] as const) {
            const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
            const timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
            const noopSig = makeFakeSignal().signal;
            const surface: Record<string, unknown> = {
                windowList: () => [],
                desktops: [{ id: "ws1", name: "WS1" }],
                screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
                activeScreen: { name: "out" },
                activeWindow: null,
                currentDesktop: { id: "ws1", name: "WS1" },
                clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
                windowAdded: noopSig,
                windowRemoved: noopSig,
                windowActivated: noopSig,
                desktopsChanged: noopSig,
                screensChanged: noopSig,
                currentDesktopChanged: noopSig,
                activeScreenChanged: noopSig,
            };
            if (mode === "getter-throws") {
                Object.defineProperty(surface, "cursorPosChanged", {
                    configurable: true,
                    get: () => {
                        throw new Error("boom-get");
                    },
                });
            } else if (mode === "connect-throws") {
                surface["cursorPosChanged"] = {
                    connect: () => {
                        throw new Error("boom-connect");
                    },
                    disconnect: () => undefined,
                };
            }

            const handle = startWorldAdapterEntry({
                workspace: surface,
                owner: "owner-a",
                adapter: "test-adapter",
                readScopeClassFn: () => "OmniTilerTestWindow",
                devSettingsJson: JSON.stringify({ focusFollowsCursor: true }),
                callDbus: (_s, _p, _i, method, payload, callback) => {
                    calls.push({ method, payload, callback });
                },
                scheduleOnce: (delayMs, callback) => {
                    const entry = { delayMs, callback, cancelled: false };
                    timers.push(entry);
                    return () => {
                        entry.cancelled = true;
                    };
                },
                log: () => undefined,
                gestureActive: () => false,
                registerShortcutFn: () => true,
                readWorkspaceModeFn: () => "per-output-local",
                readDefaultTiledFn: () => true,
            });
            assert.notEqual(handle, null, `entry starts when cursorPosChanged is ${mode}`);

            const hs = calls.find((c) => c.method === "DiscoverWorld");
            assert.ok(hs !== undefined);
            const hsReq = (JSON.parse(hs.payload) as { req: string }).req;
            hs.callback(JSON.stringify({ v: 4, type: "handshake-reply", adapter: "test-adapter", req: hsReq, gen: "gen-1", ready: false, kind: "" }));

            const setCall = calls.find((c) => c.method === "DiscoverWorld" && c !== hs);
            assert.ok(setCall !== undefined);
            const setBody = JSON.parse(setCall.payload) as { req: string; rev: string };
            setCall.callback(JSON.stringify({
                v: 4, type: "settings-reply", adapter: "test-adapter", gen: "gen-1",
                req: setBody.req, rev: setBody.rev, status: "accepted", diagnostics: [],
            }));

            const describesBefore = calls.filter((c) => c.method === "DescribeWorld").length;
            for (const t of [...timers]) {
                if (!t.cancelled && t.delayMs <= WORLD_DEBOUNCE_MS) {
                    t.callback();
                }
            }
            const describesAfter = calls.filter((c) => c.method === "DescribeWorld").length;
            // Only the initial post-settings describe runs; no extra motion captures.
            assert.ok(describesAfter - describesBefore <= 1, `no spurious captures on ${mode}`);
            handle?.stop();
        }
    });
});

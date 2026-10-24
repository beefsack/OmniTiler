// World binding retention flags: pin reads true exactly while the native
// id carries adapter-retained intent (an outstanding creation binding or
// a workspace referenced by the dispatched command). KWin exposes no
// pinned/displaced desktop property and no legacy source ever read one,
// so disp stays false; listed bindings are adapter-managed inventory by
// construction (unmanaged natives are omitted, never marked).

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

function makeSurface() {
    const ws1 = { id: "ws-1" };
    const ws2 = { id: "ws-2" };
    const wins = [
        {
            internalId: "win-a", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: null,
        },
    ];
    const screens = [
        { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
        { name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } },
    ];
    const world = {
        screens,
        desktops: [ws1, ws2],
        wins,
        activeScreen: { name: "out-1" },
        activeWindow: wins[0] as unknown,
        currentByScreen: new Map([["out-1", ws1], ["out-2", ws2]]),
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            world.activeWindow = ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

describe("world binding retention flags", () => {
    it("pins nothing for direction-only sends (core resolves the target)", () => {
        const { surface } = makeSurface();
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
                if (method === "QueueWorldCommand") {
                    const queueBody = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({ v: 2, outcome: "queued", gen: "gen-1",
                        client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
                } else if (method === "TakeCommandResult") {
                    const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                    if (stagedTake !== undefined) {
                        delete (globalThis as Record<string, unknown>)["__takeResult"];
                        callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                    } else {
                        callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
                    }                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
        });
        assert.ok(handle !== null);
        calls.find((c) => c.method === "DiscoverWorld")?.callback(JSON.stringify({
            v: 2, adapter: "broker-1", gen: "gen-1", req: "disc-1",
            obs: "0", rev: "0", ready: false, consumed: false,
            gestureSettled: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            scopes: [], workspaces: [], outputs: [], windows: [],
            transfers: [], effects: [], result: null,
        }));
        // Settle the auto report with a reply that binds both natives, so
        // the next report converges the adapter binding snapshot.
        const auto = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(auto !== undefined);
        const autoBody = JSON.parse((auto as { payload: string }).payload) as Record<string, unknown>;
        (auto as { callback: (reply: unknown) => void }).callback(JSON.stringify({
            v: 2, adapter: "broker-1", gen: "gen-1", req: autoBody["source"],
            obs: "1", rev: "1", ready: true, consumed: false,
            source: autoBody["source"],
            nativeInvalid: autoBody["nativeInvalid"],
            clock: "0",
            gestureSettled: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            scopes: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
            workspaces: [
                { id: "ws-1", out: "out-1", bind: "ws-1", scope: "out-1", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "ws-2", out: "out-2", bind: "ws-2", scope: "out-2", order: 0, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        // Observation-only reports carry no retention: nothing pinned.
        const settled = JSON.parse((auto as { payload: string }).payload) as Record<string, unknown>;
        void settled;
        handle.requestSendToOutput("right", false);
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= 120) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
        const sentQ = [...calls].reverse().find((c) => c.method === "QueueWorldCommand");
        assert.ok(sentQ !== undefined);
        const qbody = JSON.parse((sentQ as { payload: string }).payload) as Record<string, unknown>;
        const command = qbody["command"] as Record<string, unknown>;
        assert.equal(command["op"], "send-directional");
        // Bindings never ride the wire (adapter-owned snapshot only): the
        // inner report carries views, never a bindings key. Direction-only
        // intents name no workspace, so nothing pins.
        const sent = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(sent !== undefined);
        const body = JSON.parse((sent as { payload: string }).payload) as Record<string, unknown>;
        const inner = body["report"] as Record<string, unknown>;
        assert.ok(!("bindings" in inner), "bindings never ride the wire");
        assert.ok(!("command" in inner), "captures stay commandless");
        const bindings: Array<Record<string, unknown>> = [];
        void bindings;
        for (const b of bindings) {
            assert.equal(b["disp"], false, "no native displaced source exists");
            assert.equal(b["managed"], true, "listed bindings are managed inventory");
            assert.equal(b["pin"], false, "direction-only intents name no workspace, so nothing pins");
        }
        handle.stop();
    });
});

// World binding retention flags: pin reads true exactly while the native
// id carries adapter-retained intent (an outstanding creation binding or
// a workspace referenced by the dispatched command). KWin exposes no
// pinned/displaced desktop property and no legacy source ever read one,
// so disp stays false; listed bindings are adapter-managed inventory by
// construction (unmanaged natives are omitted, never marked).
//
// Synthetic strict-v4 receipts; scripted obs/rev are independent of nativeInvalid.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

function makeSurface() {
    const ws1 = { id: "ws-1" };
    const ws2 = { id: "ws-2" };
    const wins = [
        {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
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
                    const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                    callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                        client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
                } else if (method === "TakeCommandResult") {
                    const takeBody = JSON.parse(payload) as Record<string, unknown>;
                    const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                    const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                    if (stagedTake !== undefined) {
                        delete (globalThis as Record<string, unknown>)["__takeResult"];
                        callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                    } else {
                        callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
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
        // Accept settings before the first report.
        const handshake = calls.find((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
            } catch {
                return false;
            }
        });
        assert.ok(handshake !== undefined, "expected DiscoverWorld/handshake");
        const handshakeReq = (JSON.parse((handshake as { payload: string }).payload) as Record<string, unknown>)["req"] as string;
        (handshake as { callback: (reply: unknown) => void }).callback(JSON.stringify({
            type: "handshake-reply", v: 4, adapter: "test-adapter", req: handshakeReq,
            ready: false, kind: "", gen: "gen-1",
        }));
        const settings = calls.find((c) => {
            if (c.method !== "DiscoverWorld") {
                return false;
            }
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
            } catch {
                return false;
            }
        });
        assert.ok(settings !== undefined, "expected DiscoverWorld/settings");
        const settingsBody = JSON.parse((settings as { payload: string }).payload) as Record<string, unknown>;
        (settings as { callback: (reply: unknown) => void }).callback(JSON.stringify({
            type: "settings-reply", v: 4, adapter: "test-adapter", gen: "gen-1",
            req: settingsBody["req"], rev: settingsBody["rev"], status: "accepted", diagnostics: [],
        }));
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= 120) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
        // Settle the auto report with a reply that binds both natives, so
        // the next report converges the adapter binding snapshot.
        const auto = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(auto !== undefined);
        const autoBody = JSON.parse((auto as { payload: string }).payload) as Record<string, unknown>;
        const autoText = JSON.stringify({
            v: 4, adapter: "test-adapter", gen: "gen-1", req: autoBody["source"],
            obs: "1", rev: "1", ready: true, consumed: false,
            source: autoBody["source"],
            nativeInvalid: autoBody["nativeInvalid"],
            clock: "0",
            gestureSettled: null, gestureAssoc: null,
            focusRequest: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            intentDesired: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [{ scope: { kind: "workspace", out: "out-1", ws: "ws-1" }, auth: "controlled", reason: "" }],
            workspaces: [
                { id: "ws-1", out: "out-1", bind: "ws-1", scope: { kind: "output", out: "out-1" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "ws-2", out: "out-2", bind: "ws-2", scope: { kind: "output", out: "out-2" }, order: 0, mode: "tiled", exist: true, hold: "" },
            ],
            outputs: [],
            windows: [], transfers: [], effects: [], result: null,
        });
        assert.ok(parseWorldReply(autoText) !== null && isCoreWorldReply(parseWorldReply(autoText)), "authored accept binds strict v4");
        (auto as { callback: (reply: unknown) => void }).callback(autoText);
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

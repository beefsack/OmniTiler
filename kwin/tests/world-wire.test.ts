// Integrated wire-compatibility coverage (D1-D5): the ACTUAL production
// entry's sent DescribeWorld payload must carry the exact Rust-required key
// set (including minimized + predicate), and an ACTUAL Rust-encoded reply
// shape (golden bytes from tiler-protocol world_codec, including outputs
// mode) must parse through the TS parser and apply. Offline only: scripted
// in-memory transport, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, isWorldRejection } from "../src/world-report";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

function makeSurface() {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const wins = [
        {
            internalId: "win-a", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        },
        {
            internalId: "win-b", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws2],
            minimized: true, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
        },
    ];
    const screens = [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out" },
        activeWindow: wins[0],
        currentDesktop: ws1,
        geometryWrites: [] as string[],
    };
    for (const w of wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
    }
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
            world.activeWindow = ref as typeof world.activeWindow;
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            world.currentDesktop = ref as FakeDesktop;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef as FakeDesktop;
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

const RUST_REQUIRED_KEYS = [
    "v", "owner", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq",
    "command", "mode", "topology", "bindings", "domains", "minimized",
    "predicate", "refill", "anchorFallback", "noSpaceResult", "defaultTiled", "focused", "complete", "gesture", "feedback",
];

function isCanonicalU64(text: unknown): boolean {
    if (typeof text !== "string" || text.length === 0 || text.length > 20) {
        return false;
    }
    if (text === "0") {
        return true;
    }
    if (!/^[1-9][0-9]*$/.test(text)) {
        return false;
    }
    return !(text.length === 20 && text > "18446744073709551615");
}

describe("world wire fixture (TS construction <-> Rust codec)", () => {
    it("production entry payload carries the exact Rust-required key set", () => {
        const { surface } = makeSurface();
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
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
        const discover = calls.find((c) => c.method === "DiscoverWorld");
        assert.ok(discover !== undefined);
        const bootReq = JSON.parse(discover.payload).req as string;
        discover.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: bootReq,
            obs: "0", rev: "0", ready: false, consumed: false,
            scopes: [], workspaces: [], outputs: [], windows: [],
            transfers: [], effects: [],
        }));
        const auto = calls.find((c) => c.method === "DescribeWorld");
        assert.ok(auto !== undefined);
        // The auto observation-only report is the ACTUAL TS construction.
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        assert.deepEqual([...Object.keys(body)].sort(), [...RUST_REQUIRED_KEYS].sort());
        assert.ok(isCanonicalU64(body["obs"]));
        assert.ok(isCanonicalU64(body["reqSeq"]));
        assert.equal(body["predicate"], "both-axes-fixed");
        assert.equal(body["refill"], "last-remaining-workspace", "validated refill rides every report (default)");
        assert.equal(body["anchorFallback"], "largest-splittable-window", "validated D2 fallback rides every report (default)");
        assert.equal(body["noSpaceResult"], "float", "validated D3 no-space rides every report (default)");
        assert.equal(body["defaultTiled"], true, "saved default rides every report (backfill true)");
        const topo = body["topology"] as Array<Record<string, unknown>>;
        assert.equal(topo.length, 1);
        assert.deepEqual(topo[0]?.["rect"], [0, 0, 1920, 1080]);
        assert.equal(topo[0]?.["primary"], true);
        const minimized = body["minimized"] as Array<Record<string, unknown>>;
        assert.ok(minimized.some((m) => m["win"] === "win-b" && m["out"] === "out" && m["ws"] === "ws2"));
        const domains = body["domains"] as Array<Record<string, unknown>>;
        const wins = domains.flatMap((d) => d["windows"] as Array<Record<string, unknown>>);
        assert.ok(!wins.some((w) => w["win"] === "win-b"));
        assert.ok(wins.some((w) => w["win"] === "win-a"));
        handle?.stop();
    });

    it("Rust golden reply bytes parse through the TS parser with outputs mode", () => {
        // Byte shape copied from tiler-protocol world_codec
        // golden_reply_bytes_lock_wire_shape (key order and string-quoted
        // counters frozen there); adapter/gen values are opaque to the
        // parser beyond shape.
        const golden = "{\"v\":1,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\"req\":\"req-1\"," +
            "\"obs\":\"1\",\"rev\":\"1\",\"ready\":true,\"consumed\":false," +
            "\"scopes\":[{\"scope\":\"out/ws\",\"auth\":\"controlled\",\"reason\":\"\"}]," +
            "\"workspaces\":[{\"id\":\"ws\",\"out\":\"out\",\"bind\":\"ws\",\"logical\":\"\"," +
            "\"scope\":\"out\",\"order\":0,\"mode\":\"tiled\",\"exist\":true,\"hold\":\"\"}," +
            "{\"id\":\"wl-1\",\"out\":\"out\",\"bind\":\"\",\"logical\":\"wl-1\"," +
            "\"scope\":\"out\",\"order\":1,\"mode\":\"tiled\",\"exist\":true,\"hold\":\"\"}]," +
            "\"outputs\":[{\"out\":\"out\",\"view\":\"\",\"mode\":\"tiled\",\"hold\":\"\"}]," +
            "\"windows\":[{\"win\":\"win-a\",\"out\":\"out\",\"ws\":\"ws\"," +
            "\"alloc\":[0,0,800,600],\"frame\":[0,0,800,600]," +
            "\"frame_auth\":\"controlled\",\"frame_hold\":\"\"," +
            "\"member_auth\":\"controlled\",\"member_hold\":\"\",\"frame_op\":\"\"," +
            "\"floating\":false,\"float_origin\":\"\"}]," +
            "\"transfers\":[],\"effects\":[]}";
        const parsed = parseWorldReply(golden);
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        if (parsed !== null && !isWorldRejection(parsed)) {
            assert.equal(parsed.outputs.length, 1);
            assert.equal(parsed.outputs[0]?.mode, "tiled");
            assert.equal(parsed.windows.length, 1);
            assert.equal(parsed.rev, "1");
        }
        // Outputs without mode (pre-contract shape) refuse.
        const legacy = golden.replace("\"view\":\"\",\"mode\":\"tiled\",", "\"view\":\"\",");
        assert.equal(parseWorldReply(legacy), null);
    });

    it("Rust reply shape applies geometry through the production entry", () => {
        const { surface, world } = makeSurface();
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
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
        assert.ok(handle !== undefined && handle !== null);
        const discover = calls.find((c) => c.method === "DiscoverWorld");
        assert.ok(discover !== undefined);
        discover.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: JSON.parse(discover.payload).req,
            obs: "0", rev: "0", ready: false, consumed: false,
            scopes: [], workspaces: [], outputs: [], windows: [],
            transfers: [], effects: [],
        }));
        const auto = calls.find((c) => c.method === "DescribeWorld");
        assert.ok(auto !== undefined);
        const autoBody = JSON.parse(auto.payload) as Record<string, unknown>;
        // Full reconcile then a Rust-shaped command route: the reply below
        // uses the exact Rust key vocabulary including outputs mode.
        auto.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: autoBody["req"],
            obs: autoBody["obs"], rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-a", out: "out", ws: "ws1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                floating: false, float_origin: "",
            }],
            transfers: [], effects: [],
        }));
        assert.ok(world.geometryWrites.some((w) => w.startsWith("win-a@10,10")));
        handle?.stop();
    });

    it("numbered moves submit core-resolved send-numbered 0..9", () => {
        const { surface } = makeSurface();
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
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
            v: 1, adapter: "test-adapter", gen: "gen-1", req: "disc-1",
            obs: "0", rev: "0", ready: false, consumed: false,
            scopes: [], workspaces: [], outputs: [], windows: [],
            transfers: [], effects: [],
        }));
        const runTimers = (): void => {
            for (let i = 0; i < timers.length; i += 1) {
                const t = timers[i] as { delay: number; cb: () => void };
                if (t.delay <= 120) {
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        };
        // Settle the auto report so the command flight is idle.
        {
            const auto = [...calls].reverse().find((c) => c.method === "DescribeWorld");
            assert.ok(auto !== undefined);
            const b = JSON.parse((auto as { payload: string }).payload) as Record<string, unknown>;
            (auto as { callback: (reply: unknown) => void }).callback(JSON.stringify({
                v: 1, adapter: "test-adapter", gen: "gen-1", req: b["req"],
                obs: b["obs"], rev: "0", ready: true, consumed: false,
                scopes: [], workspaces: [], outputs: [], windows: [],
                transfers: [], effects: [],
            }));
        }
        handle?.requestWorkspaceMove(0, false);
        runTimers();
        const sent = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(sent !== undefined);
        const cmd = (JSON.parse((sent as { payload: string }).payload) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "send-numbered");
        assert.equal(cmd["index"], 0);
        assert.equal(cmd["follow"], false);
        handle?.stop();
    });
});

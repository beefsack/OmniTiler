// World overlay safety (legacy exclusion parity): native KWin property
// names (`fullScreen`, `maximizeMode`, `onAllDesktops`) classify overlays
// exactly like the legacy plan sources; a fullscreen/maximized/minimized
// flip between observation and actuation fails closed; automatic
// focus/raise/stack never touches game/excluded/overlay rows.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

function makeSurface() {
    const ws1 = { id: "ws-1" };
    const ws2 = { id: "ws-2" };
    const wins = [
        {
            internalId: "win-tiled", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-game", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 1920, height: 1080 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullScreen: true, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-sticky", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws2],
            minimized: false, fullScreen: false, maximizeMode: 0, onAllDesktops: true,
            minSize: null, maxSize: null,
        },
    ];
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out-1" },
        activeWindow: wins[0] as unknown,
        currentDesktop: ws1 as { id: string },
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
            world.activeWindow = ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef as { id: string };
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

function start(surface: object) {
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
    (discover as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        v: 1, adapter: "test-adapter", gen: "gen-1", req: JSON.parse(discover.payload).req,
        obs: "0", rev: "0", ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [],
    }));
    const auto = calls.find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined);
    return { handle: handle as NonNullable<typeof handle>, calls, auto: auto as { payload: string; callback: (reply: unknown) => void } };
}

describe("world overlay safety", () => {
    it("classifies native fullScreen/maximizeMode/onAllDesktops like legacy exclusions", () => {
        const { surface, world } = makeSurface();
        const { handle, auto } = start(surface);
        const body = JSON.parse(auto.payload) as {
            domains: Array<{ windows: Array<Record<string, unknown>> }>;
        };
        const wins = body.domains.flatMap((d) => d.windows);
        const game = wins.find((w) => w["win"] === "win-game");
        assert.ok(game !== undefined, "fullscreen game window is observed");
        assert.equal(game["fullscreen"], true);
        assert.equal(game["fitExcluded"], true);
        const sticky = wins.find((w) => w["win"] === "win-sticky");
        assert.ok(sticky !== undefined);
        assert.equal(sticky["sticky"], true);
        assert.equal(sticky["floating"], true, "sticky rides floating like legacy");
        const tiled = wins.find((w) => w["win"] === "win-tiled");
        assert.ok(tiled !== undefined);
        assert.equal(tiled["fitExcluded"], false);
        assert.equal(world.geometryWrites.length, 0);
        handle.stop();
    });

    it("refuses geometry when fullscreen flips between observation and actuation", () => {
        const { surface, world } = makeSurface();
        const { handle, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        // Native flip after the observation was captured, before the reply
        // applies: the setter must fail closed.
        (world.wins[0] as { fullScreen: boolean }).fullScreen = true;
        auto.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: body["req"],
            obs: body["obs"], rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-tiled", out: "out-1", ws: "ws-1",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
            }],
            transfers: [], effects: [],
        }));
        assert.equal(world.geometryWrites.length, 0, "flipped overlay takes no frame write");
        handle.stop();
    });

    it("refuses membership when minimized flips between observation and actuation", () => {
        const { surface, world } = makeSurface();
        const { handle, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        (world.wins[0] as { minimized: boolean }).minimized = true;
        auto.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: body["req"],
            obs: body["obs"], rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [{
                win: "win-tiled", out: "out-1", ws: "ws-2",
                alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
            }],
            transfers: [], effects: [],
        }));
        assert.equal((world.wins[0] as { desktops: Array<{ id: string }> }).desktops[0]?.id, "ws-1", "minimized flip blocks the move");
        handle.stop();
    });

    it("never steers automatic focus onto a game/excluded row", () => {
        const { surface, world } = makeSurface();
        const { handle, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        const before = world.activeWindow;
        auto.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: body["req"],
            obs: body["obs"], rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [],
            transfers: [], effects: [{ id: "fx-1", kind: "focus", target: "win-game", rev: "1" }],
        }));
        assert.equal(world.activeWindow, before, "overlay focus effect held, never stolen");
        handle.stop();
    });

    it("still applies automatic focus onto a tiled row", () => {
        const { surface, world } = makeSurface();
        world.activeWindow = world.wins[1] as unknown;
        const { handle, auto } = start(surface);
        const body = JSON.parse(auto.payload) as Record<string, unknown>;
        auto.callback(JSON.stringify({
            v: 1, adapter: "test-adapter", gen: "gen-1", req: body["req"],
            obs: body["obs"], rev: "1", ready: true, consumed: false,
            scopes: [{ scope: "out-1/ws-1", auth: "controlled", reason: "" }],
            workspaces: [], outputs: [],
            windows: [],
            transfers: [], effects: [{ id: "fx-2", kind: "focus", target: "win-tiled", rev: "1" }],
        }));
        assert.equal(
            ((world.activeWindow as unknown as { internalId: string }).internalId),
            "win-tiled",
            "tiled focus effect applies",
        );
        handle.stop();
    });
});

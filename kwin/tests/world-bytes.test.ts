// Cross-language byte gate (D1-D5): the ACTUAL production entry's
// DescribeWorld/DiscoverWorld payloads must equal the checked-in artifact
// bytes, and the same artifacts must decode fully in Rust with the
// structural estimator bounding the exact length. UPDATE_WORLD_FIXTURES=1
// regenerates the request artifacts from the live entry (the reply golden
// is produced by the Rust canonical encoder); otherwise bytes assert
// exactly. No node runtime is needed on the Rust side.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";

const FIXTURE_DIR = join("..", "test-fixtures", "world-wire");
const UPDATE = process.env["UPDATE_WORLD_FIXTURES"] === "1";

function makeSignal() {
    const handlers = new Set<(payload?: unknown) => void>();
    return {
        signal: {
            connect: (h: (payload?: unknown) => void): void => {
                handlers.add(h);
            },
            disconnect: (h: (payload?: unknown) => void): void => {
                handlers.delete(h);
            },
        },
        emit: (payload?: unknown): void => {
            for (const h of [...handlers]) {
                h(payload);
            }
        },
    };
}

const LONG_ID = "w".repeat(128);

function makeFixtureSurface() {
    const ws1 = { id: "ws-1", name: "Workspace 1" };
    const ws2 = { id: "ws-2", name: "Workspace 2" };
    const ws3 = { id: "ws-3", name: "Workspace 3" };
    const started = makeSignal();
    const stepped = makeSignal();
    const finished = makeSignal();
    const windowAdded = makeSignal();
    const windowRemoved = makeSignal();
    const wins: Array<Record<string, unknown>> = [
        {
            internalId: "win-a", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: { width: 100, height: 100 }, maxSize: { width: 800, height: 600 },
            move: true, resize: false,
            interactiveMoveResizeStarted: started.signal,
            interactiveMoveResizeStepped: stepped.signal,
            interactiveMoveResizeFinished: finished.signal,
        },
        {
            internalId: LONG_ID, normalWindow: true,
            frameGeometry: { x: 210, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-extreme", normalWindow: true,
            frameGeometry: { x: -16384, y: -16384, width: 16384, height: 16384 },
            output: { name: "out-2" }, desktops: [ws2],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-sticky", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-2" }, desktops: [ws2],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            onAllDesktops: true,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-fixed", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 400, height: 300 },
            output: { name: "out-2" }, desktops: [ws2],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: { width: 400, height: 300 }, maxSize: { width: 400, height: 300 },
        },
        {
            internalId: "win-game", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 1920, height: 1080 },
            output: { name: "out-1" }, desktops: [ws3],
            minimized: false, fullScreen: true, maximizeMode: 0,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-min", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: true, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: null,
        },
        {
            // Live-then-closed window: cached while live, removed from the
            // inventory before the removal event so the event-gated
            // difference (never the payload) reports the explicit close.
            internalId: "win-gone", normalWindow: true,
            frameGeometry: { x: 420, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: null,
        },
    ];
    const world = {
        wins,
        screens: [
            { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
            { name: "out-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } },
        ],
        desktops: [ws1, ws2, ws3],
        activeScreen: { name: "out-1" },
        activeWindow: wins[0] as unknown,
        currentByScreen: new Map([["out-1", ws1], ["out-2", ws2]]),
        geometryWrites: [] as string[],
    };
    for (const w of wins) {
        let frame = w["frameGeometry"] as { x: number; y: number; width: number; height: number };
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                world.geometryWrites.push(`${w["internalId"]}@${rect.x},${rect.y},${rect.width},${rect.height}`);
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
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: (_position: number, _name: string): void => undefined,
        removeDesktop: () => undefined,
        windowAdded: windowAdded.signal,
        windowRemoved: windowRemoved.signal,
    };
    return { surface, world, started, stepped, finished, windowAdded, windowRemoved };
}

// Native Qt QSize shapes: real clients report explicit minimum/maximum
// extents. The Qt defaults are a 0x0 minimum (no minimum) and a
// QWIDGETSIZE_MAX 16777215 maximum (no maximum); the i32::MAX sentinel is
// also unbounded. Generic fake ids, real field shapes.
function makeSizeHintsSurface() {
    const ws1 = { id: "ws-1" };
    const ws2 = { id: "ws-2" };
    const wins: Array<Record<string, unknown>> = [
        {
            internalId: "win-default", normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: { width: 0, height: 0 }, maxSize: { width: 16777215, height: 16777215 },
        },
        {
            internalId: "win-sentinel", normalWindow: true,
            frameGeometry: { x: 210, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: null, maxSize: { width: 2147483647, height: 2147483647 },
        },
        {
            internalId: "win-wide", normalWindow: true,
            frameGeometry: { x: 420, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: { width: 640, height: 0 }, maxSize: { width: 640, height: 0 },
        },
        {
            internalId: "win-fixed", normalWindow: true,
            frameGeometry: { x: 630, y: 0, width: 400, height: 300 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, maximized: false, sticky: false,
            minSize: { width: 400, height: 300 }, maxSize: { width: 400, height: 300 },
        },
    ];
    const world = {
        wins,
        screens: [
            { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
        ],
        desktops: [ws1, ws2],
        activeScreen: { name: "out-1" },
        activeWindow: wins[0] as unknown,
        currentByScreen: new Map([["out-1", ws1]]),
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            world.activeWindow = ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
            world.currentByScreen.set(screen.name, deskRef as { id: string });
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

interface Flow {
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    oracleCalls: Array<(reply: unknown) => void>;
}

function startFlow(surface: object): Flow {
    const calls: Flow["calls"] = [];
    const timers: Flow["timers"] = [];
    const logs: string[] = [];
    const oracleCalls: Array<(reply: unknown) => void> = [];
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
                }            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => { logs.push(m); },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        oracleCallDbus: (_s, _p, _i, _m, callback) => {
            oracleCalls.push(callback);
        },
    });
    assert.ok(handle !== null);
    return { handle: handle as NonNullable<typeof handle>, calls, timers, logs, oracleCalls };
}

function runTimers(flow: Flow): void {
    for (let i = 0; i < flow.timers.length; i += 1) {
        const t = flow.timers[i] as { delay: number; cb: () => void };
        if (t.delay <= 120) {
            flow.timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    }
}

function lastPayload(flow: Flow, method: string): string {
    const call = [...flow.calls].reverse().find((c) => c.method === method);
    assert.ok(call !== undefined, `expected ${method}`);
    return (call as { payload: string }).payload;
}

function replyCall(flow: Flow, method: string): (reply: unknown) => void {
    const call = [...flow.calls].reverse().find((c) => c.method === method);
    assert.ok(call !== undefined, `expected ${method}`);
    return (call as { callback: (reply: unknown) => void }).callback;
}

function seedBinds(source: unknown, nativeInvalid: string, rev: string): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen: "gen-1", req: source,
        obs: rev, rev, ready: true, consumed: false,
        source,
        nativeInvalid,
        clock: "1",
        gestureSettled: null,
        operationSettled: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
        workspaces: [
            { id: "ws-1", out: "out-1", bind: "ws-1", scope: "out-1", order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "ws-2", out: "out-2", bind: "ws-2", scope: "out-2", order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "ws-3", out: "out-1", bind: "ws-3", scope: "out-1", order: 1, mode: "tiled", exist: true, hold: "" },
        ],
    });
}

function bootstrap(gen: string, req: string): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen, req, obs: "0", rev: "0",
        ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
        gestureSettled: null, operationSettled: [], warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
    });
}

function acceptEmpty(source: unknown, nativeInvalid: string, rev: string): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen: "gen-1", req: source,
        obs: rev, rev, ready: true, consumed: false,
        source,
        nativeInvalid,
        clock: "0",
        gestureSettled: null,
        operationSettled: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
    });
}

function checkOrWrite(name: string, actual: string): void {
    if (UPDATE) {
        mkdirSync(FIXTURE_DIR, { recursive: true });
        writeFileSync(join(FIXTURE_DIR, name), actual);
        return;
    }
    const expected = readFileSync(join(FIXTURE_DIR, name), "utf8");
    assert.equal(actual, expected, `${name} byte gate: production payload must equal the shared artifact`);
}

describe("world shared byte fixtures", () => {
    it("captures discovery plus full/command/drag reports byte-identically", () => {
        const { surface, world, started, finished, windowRemoved } = makeFixtureSurface();
        const flow = startFlow(surface);
        // Discovery bytes are fully deterministic on a fresh entry.
        checkOrWrite("discover.json", lastPayload(flow, "DiscoverWorld"));
        // Settle discovery with the binding seed (rev 0 leaves room for
        // the Rust-produced golden rev 1 below), then settle the auto
        // report with the same binds to converge the mapping snapshot.
        replyCall(flow, "DiscoverWorld")(bootstrap("gen-1", "disc-1"));
        const autoSeed = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
        replyCall(flow, "DescribeWorld")(seedBinds(autoSeed["source"], "1", "0"));
        // A native close rides the next full report explicitly: the window
        // leaves the live inventory first, then the payload-free removal
        // event gates the cached-id difference (the payload is never read).
        world.wins = world.wins.filter((w) => w["internalId"] !== "win-gone");
        windowRemoved.emit();
        runTimers(flow);
        const observePayload = lastPayload(flow, "DescribeWorld");
        // The per-capture source correlation is a fresh nonce per
        // dispatch: mask it for the byte gate (everything else matches
        // exactly, including the broker envelope shape).
        const observeMasked = JSON.stringify({ ...JSON.parse(observePayload), source: "src-masked" });
        checkOrWrite("report-observe.json", observeMasked);
        const observeBody = JSON.parse(observePayload) as Record<string, unknown>;
        const observeInner = observeBody["report"] as Record<string, unknown>;
        // The fixture covers the contract surface the byte gate locks:
        // minimized lifetimes, native order, hints, long ids, extrema.
        const facts = observeInner["windows"] as Array<Record<string, unknown>>;
        assert.ok(facts.some((m) => m["win"] === "win-min" && m["minimized"] === true));
        assert.ok(((observeInner["native"] as Array<Record<string, unknown>>)[0]?.["order"] as Array<unknown>).length >= 3);
        assert.ok(observePayload.includes(LONG_ID), "128-char id rides verbatim");
        assert.ok(observePayload.includes("-16384"), "extrema ride verbatim");
        const game = facts.find((w) => w["win"] === "win-game");
        assert.ok(game !== undefined && game["fullscreen"] === true && game["fitExcluded"] === true);
        // The Rust golden reply for this exact report parses and applies
        // through the production entry (proves byte consumption in-flow).
        // During fixture regeneration the golden may not exist yet (Rust
        // produces it from report-observe.json); then a minimal accept
        // keeps the same revision sequence.
        let golden: string | null = null;
        try {
            golden = readFileSync(join(FIXTURE_DIR, "reply-golden.json"), "utf8");
        } catch (error) {
            void error;
            golden = null;
        }
        if (golden !== null) {
            // Rebind the golden to this capture's transport correlation:
            // the fixture locks the core shape, the envelope binds the
            // live source watermark.
            const goldenBound = JSON.stringify({
                ...JSON.parse(golden),
                req: observeBody["source"], source: observeBody["source"],
                nativeInvalid: observeBody["nativeInvalid"],
            });
            const parsed = parseWorldReply(goldenBound);
            assert.ok(parsed !== null && isCoreWorldReply(parsed), "golden reply parses");
            replyCall(flow, "DescribeWorld")(goldenBound);
            assert.ok(flow.logs.some((l) => l.includes("omnitiler:world:applied rev=1")), "golden reply applied");
        } else {
            assert.ok(UPDATE, "reply-golden.json must exist for verification runs");
            replyCall(flow, "DescribeWorld")(acceptEmpty(observeBody["source"], "1", "1"));
        }
        // A directed send is direction-only (core resolves the target):
        // no endpoint is named, so no binding pins for the target.
        flow.handle.requestSendToOutput("right", false);
        runTimers(flow);
        const sendPayload = lastPayload(flow, "DescribeWorld");
        const sendMasked = JSON.stringify({ ...JSON.parse(sendPayload), source: "src-masked" });
        checkOrWrite("report-send.json", sendMasked);
        const sendBody = JSON.parse(sendPayload) as Record<string, unknown>;
        const queued = JSON.parse(lastPayload(flow, "QueueWorldCommand")) as Record<string, unknown>;
        const sendCmd = queued["command"] as Record<string, unknown>;
        assert.equal(sendCmd["op"], "send-directional");
        assert.equal(sendCmd["direction"], "right");
        assert.equal(sendCmd["follow"], false);
        // Direction-only sends name no endpoint on any lane: the inner
        // report carries no command key at all.
        assert.ok(!("command" in (sendBody["report"] as Record<string, unknown>)), "drained capture stays commandless");
        replyCall(flow, "DescribeWorld")(acceptEmpty(sendBody["source"], "1", "2"));
        // A finished gesture routes an exact drop with source and null hover.
        // The final rect keeps the live start size (same-size move) at a
        // pointer far outside any plausible tiled layout.
        (surface as { cursorPos: { x: number; y: number } }).cursorPos = { x: 1500, y: 900 };
        started.emit();
        finished.emit();
        assert.ok(flow.oracleCalls.length >= 1, "finish pulls LastVerdict");
        const pull = flow.oracleCalls[flow.oracleCalls.length - 1] as (reply: unknown) => void;
        const startFrame = world.wins[0]?.["frameGeometry"] as Record<string, unknown>;
        const startW = startFrame["w"] ?? startFrame["width"];
        const startH = startFrame["h"] ?? startFrame["height"];
        assert.ok(typeof startW === "number" && typeof startH === "number", "live start size known");
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: 1500, y: 900, w: startW, h: startH },
            windowIdentity: "win-a", correlation: "drag-9", reason: "ok-moved",
        }));
        runTimers(flow);
        const dragPayload = lastPayload(flow, "DescribeWorld");
        const dragMasked = JSON.stringify({ ...JSON.parse(dragPayload), source: "src-masked" });
        checkOrWrite("report-dragdrop.json", dragMasked);
        const dragQueued = JSON.parse(lastPayload(flow, "QueueWorldCommand")) as Record<string, unknown>;
        const dragCmd = (dragQueued["command"] as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.equal(dragCmd["op"], "drag-drop");
        assert.equal(dragCmd["x"], 1500);
        assert.equal(dragCmd["y"], 900);
        assert.equal(dragCmd["hover"], null);
        assert.deepEqual(dragCmd["source"], { out: "out-1", ws: "ws-1" });
        flow.handle.stop();
    });

    it("captures real Qt size-hint defaults byte-identically with a select follow-up", () => {
        const { surface } = makeSizeHintsSurface();
        const flow = startFlow(surface);
        replyCall(flow, "DiscoverWorld")(bootstrap("gen-1", "disc-1"));
        const observePayload = lastPayload(flow, "DescribeWorld");
        const hintsMasked = JSON.stringify({ ...JSON.parse(observePayload), source: "src-masked" });
        checkOrWrite("report-kwin-size-hints.json", hintsMasked);
        const body = JSON.parse(observePayload) as Record<string, unknown>;
        // Real Qt QSize field shapes ride verbatim on the wire.
        assert.ok(observePayload.includes('"minW":0,"minH":0,"maxW":16777215,"maxH":16777215}'), "Qt default hints ride");
        assert.ok(observePayload.includes('"maxW":2147483647'), "unbounded sentinel rides");
        assert.ok(observePayload.includes('"minW":640,"minH":0,"maxW":640,"maxH":0}'), "partial-zero vector rides");
        assert.ok(observePayload.includes('"minW":400,"minH":300,"maxW":400,"maxH":300}'), "fixed pin rides");
        // fixedAuto classification stays meaningful under the default
        // both-axes predicate: only the equal nonzero pin floats.
        const innerHints = body["report"] as Record<string, unknown>;
        const rows = innerHints["windows"] as Array<Record<string, unknown>>;
        const flagOf = (id: string): boolean => {
            const row = rows.find((w) => w["win"] === id);
            assert.ok(row !== undefined, `expected row ${id}`);
            return row["fixedAuto"] === true;
        };
        assert.equal(flagOf("win-fixed"), true);
        assert.equal(flagOf("win-default"), false);
        assert.equal(flagOf("win-sentinel"), false);
        assert.equal(flagOf("win-wide"), false);
        // Accepted, never catchup-held: the explicit select rides the next
        // dispatch instead of being dropped to observation-only.
        replyCall(flow, "DescribeWorld")(acceptEmpty(body["source"], "1", "1"));
        flow.handle.requestWorkspaceSelect(2);
        runTimers(flow);
        assert.ok(!flow.logs.some((l) => l.includes("shortcut-failed")), "select dispatched with its command");
        const selectPayload = lastPayload(flow, "DescribeWorld");
        const selectMasked = JSON.stringify({ ...JSON.parse(selectPayload), source: "src-masked" });
        checkOrWrite("report-kwin-size-hints-select.json", selectMasked);
        const selectQueued = JSON.parse(lastPayload(flow, "QueueWorldCommand")) as Record<string, unknown>;
        const cmd = selectQueued["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "select-numbered");
        assert.equal(cmd["index"], 2);
        assert.equal(cmd["out"], "out-1");
        flow.handle.stop();
    });

    it("refuses oversize reports at the 4MiB gate without truncation", () => {
        // Fat rows (128-char window, output, and workspace ids) so the
        // real builder crosses 4MiB well inside the 10000-window cap.
        const longOut = "o".repeat(128);
        const longWs = "s".repeat(128);
        const mkWins = (count: number) =>
            Array.from({ length: count }, (_, i) => ({
                internalId: `w${"q".repeat(120)}-${i}`,
                normalWindow: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: longOut },
                desktops: [{ id: `${longWs.slice(0, 120)}-${i % 64}` }],
                minimized: false, fullscreen: false, maximized: false, sticky: false,
                minSize: { width: 100, height: 100 },
                maxSize: { width: 800, height: 600 },
            }));
        const mkSurface = (count: number) => {
            const wins = mkWins(count);
            const firstDesk = { id: `${longWs.slice(0, 120)}-0` };
            const desks = Array.from({ length: 64 }, (_, i) => ({ id: `${longWs.slice(0, 120)}-${i}` }));
            return {
                windowList: () => wins,
                screens: [{ name: longOut, geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
                desktops: desks,
                activeScreen: { name: longOut },
                activeWindow: wins[0] as unknown,
                clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
                currentDesktopForScreen: () => firstDesk,
                setCurrentDesktopForScreen: () => undefined,
                sendClientToScreen: () => undefined,
                createDesktop: () => null as unknown as object,
                removeDesktop: () => undefined,
            };
        };
        // Under: ~2.7MB of long-id windows dispatches through the real builder.
        {
            const flow = startFlow(mkSurface(4000));
            replyCall(flow, "DiscoverWorld")(bootstrap("gen-1", "disc-1"));
            const payload = lastPayload(flow, "DescribeWorld");
            assert.ok(payload.length < 4 * 1024 * 1024, `under-cap dispatches (${payload.length} bytes)`);
            assert.ok(!flow.logs.some((l) => l.includes("request-oversize")));
            flow.handle.stop();
        }
        // Over: the same builder past 4MiB refuses with no partial page.
        {
            const flow = startFlow(mkSurface(8000));
            const before = flow.calls.filter((c) => c.method === "DescribeWorld").length;
            replyCall(flow, "DiscoverWorld")(bootstrap("gen-1", "disc-1"));
            const after = flow.calls.filter((c) => c.method === "DescribeWorld").length;
            assert.equal(after, before, "oversize report never dispatches");
            assert.ok(flow.logs.some((l) => l.includes("request-oversize")), "one bounded oversize line");
            flow.handle.stop();
        }
    });
});

describe("world rejection log vocabulary", () => {
    function startRejectedFlow(message: unknown): string[] {
        const { surface } = makeSizeHintsSurface();
        const flow = startFlow(surface);
        replyCall(flow, "DiscoverWorld")(bootstrap("gen-1", "disc-1"));
        replyCall(flow, "DescribeWorld")(JSON.stringify({
            v: 2, outcome: "rejected", kind: "snapshot-invalid", message, gen: "gen-1",
            source: "", nativeInvalid: "0", clock: "0",
        }));
        const logs = [...flow.logs];
        flow.handle.stop();
        return logs;
    }

    it("carries only the fixed msg token on rejections", () => {
        const logs = startRejectedFlow("duplicate-window");
        assert.ok(logs.some((l) =>
            l === "omnitiler:world:rejected kind=snapshot-invalid msg=duplicate-window recovery=on-next-event"));
    });

    it("sanitizes non-vocabulary message content without echo", () => {
        const secret = "win-secret-1 payload {\"x\":1}";
        const logs = startRejectedFlow(secret);
        assert.ok(logs.some((l) =>
            l === "omnitiler:world:rejected kind=snapshot-invalid msg=unknown recovery=on-next-event"));
        assert.ok(!logs.some((l) => l.includes("secret")), "no content echoes into the log");
    });
});

describe("world T05 unbounded-maximum producer shape", () => {
    it("carries a real minimum with Qt INT_MAX maximum as an ordinary tiled client", () => {
        // Live T05 probe shape on the production DescribeWorld route: an
        // ordinary resizeable client reports a real minimum (176x150) with
        // Qt's unconstrained maximum (2147483647 x 2147483647, INT_MAX,
        // meaning "no maximum"). The world report carries the raw
        // QSize bytes verbatim; the core sanitizes the sentinel to absent
        // at every use site, never refuses the report, and never
        // classifies the client as an automatic float.
        const ws1 = { id: "ws-1" };
        const ws2 = { id: "ws-2" };
        const wins: Array<Record<string, unknown>> = [
            {
                internalId: "win-t05", normalWindow: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: "out-1" }, desktops: [ws1],
                minimized: false, fullscreen: false, maximized: false, sticky: false,
                minSize: { width: 176, height: 150 },
                maxSize: { width: 2147483647, height: 2147483647 },
            },
            {
                internalId: "win-fixed", normalWindow: true,
                frameGeometry: { x: 210, y: 0, width: 400, height: 300 },
                output: { name: "out-1" }, desktops: [ws1],
                minimized: false, fullscreen: false, maximized: false, sticky: false,
                minSize: { width: 400, height: 300 }, maxSize: { width: 400, height: 300 },
            },
            {
                internalId: "win-neg", normalWindow: true,
                frameGeometry: { x: 420, y: 0, width: 200, height: 200 },
                output: { name: "out-1" }, desktops: [ws1],
                minimized: false, fullscreen: false, maximized: false, sticky: false,
                minSize: { width: -5, height: 100 }, maxSize: null,
            },
            {
                internalId: "win-t", normalWindow: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: "out-1" }, desktops: [ws2],
                minimized: false, fullscreen: false, maximized: false, sticky: false,
                minSize: null, maxSize: null,
            },
        ];
        const surface = {
            windowList: () => wins,
            screens: [
                { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
            ],
            desktops: [ws1, ws2],
            activeScreen: { name: "out-1" },
            activeWindow: wins[0] as unknown,
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null as unknown as object,
            removeDesktop: () => undefined,
        };
        const flow = startFlow(surface);
        try {
            replyCall(flow, "DiscoverWorld")(bootstrap("gen-1", "disc-1"));
            const payload = lastPayload(flow, "DescribeWorld");
            const body = JSON.parse(payload) as Record<string, unknown>;
            const inner = body["report"] as Record<string, unknown>;
            const rows = inner["windows"] as Array<Record<string, unknown>>;
            const rowOf = (id: string): Record<string, unknown> => {
                const row = rows.find((w) => w["win"] === id);
                assert.ok(row !== undefined, `expected row ${id}`);
                return row as Record<string, unknown>;
            };
            const t05 = rowOf("win-t05");
            // Raw Qt bytes ride verbatim; the unbounded max never promotes
            // an ordinary client to an automatic float.
            assert.deepEqual(
                t05["hints"],
                { minW: 176, minH: 150, maxW: 2147483647, maxH: 2147483647 },
            );
            assert.equal(t05["fixedAuto"], false);
            assert.equal(t05["floating"], false);
            // In-bound finite pairs stay exact and still pin fixed-size.
            const fixed = rowOf("win-fixed");
            assert.deepEqual(
                fixed["hints"],
                { minW: 400, minH: 300, maxW: 400, maxH: 300 },
            );
            assert.equal(fixed["fixedAuto"], true);
            // Negative extents stay malformed-absent, never a bound.
            const neg = rowOf("win-neg");
            assert.deepEqual(
                neg["hints"],
                { minW: null, minH: null, maxW: null, maxH: null },
            );
            assert.equal(neg["fixedAuto"], false);
        } finally {
            flow.handle.stop();
        }
    });
});

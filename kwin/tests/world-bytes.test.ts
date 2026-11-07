// KDE-v4 byte gate: the ACTUAL production entry's handshake/settings and
// DescribeWorld payloads must equal the checked-in artifacts under
// test-fixtures/kde-v4/bytes, and the recorded Observe reply must strict-
// parse and apply through the production entry via the genuine Rust engine
// (raw_world_eval). UPDATE_KDE_V4_BYTE_FIXTURES=1 writes ONLY the named new
// files below; normal runs assert exactly. Legacy test-fixtures/world-wire
// artifacts stay frozen and untouched.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
import { createMiniBroker } from "./world-wire-harness";

const FIXTURE_DIR = join("..", "test-fixtures", "kde-v4", "bytes");
const UPDATE = process.env["UPDATE_KDE_V4_BYTE_FIXTURES"] === "1";
const ALLOWED_FIXTURES = new Set([
    "handshake.json",
    "settings.json",
    "report-startup.json",
    "report-observe.json",
    "reply-observe.json",
    "report-send.json",
    "report-size-hints.json",
    "report-size-hints-select.json",
]);

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
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: { width: 100, height: 100 }, maxSize: { width: 800, height: 600 },
            // Live title carries C0 controls 0x08 and 0x1F: the canonical
            // writer must spell them lowercase \u0008 and \u001f, never the
            // JSON.stringify short escapes.
            pid: 1234, resourceClass: "Example", caption: "A\bB\x1fC", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
            interactiveMoveResizeStarted: started.signal,
            interactiveMoveResizeStepped: stepped.signal,
            interactiveMoveResizeFinished: finished.signal,
        },
        {
            internalId: LONG_ID, normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 210, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 210, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window long", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-extreme", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: -16384, y: -16384, width: 16384, height: 16384 },
            output: { name: "out-2" }, desktops: [ws2],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window extreme", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-sticky", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-2" }, desktops: [ws2],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: true,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window sticky", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-fixed", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 400, height: 300 },
            clientGeometry: { x: 0, y: 0, width: 400, height: 300 },
            output: { name: "out-2" }, desktops: [ws2],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: { width: 400, height: 300 }, maxSize: { width: 400, height: 300 },
            pid: 1234, resourceClass: "Example", caption: "Window fixed", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-game", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 1920, height: 1080 },
            clientGeometry: { x: 0, y: 0, width: 1920, height: 1080 },
            output: { name: "out-1" }, desktops: [ws3],
            minimized: false, hidden: false, fullScreen: true, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window game", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-min", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: true, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window min", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            // Live-then-closed window: cached while live, removed from the
            // inventory after the startup capture settles so the event-gated
            // difference (never the payload) reports the explicit close.
            internalId: "win-gone", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 420, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 420, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: null,
            pid: 1234, resourceClass: "Example", caption: "Window gone", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
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
        // Borderless test doubles keep a zero decoration delta across
        // native moves/resizes: the client rect tracks the live frame size
        // exactly like a decoration-free production window, so converted
        // hints stay stable while frames move.
        if (w["clientGeometry"] !== undefined) {
            Object.defineProperty(w, "clientGeometry", {
                configurable: true,
                get: () => ({ ...frame }),
            });
        }
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
            internalId: "win-default", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: { width: 0, height: 0 }, maxSize: { width: 16777215, height: 16777215 },
            pid: 1234, resourceClass: "Example", caption: "Window default", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-sentinel", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 210, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 210, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: null, maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234, resourceClass: "Example", caption: "Window sentinel", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-wide", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 420, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 420, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: { width: 640, height: 0 }, maxSize: { width: 640, height: 0 },
            pid: 1234, resourceClass: "Example", caption: "Window wide", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
        },
        {
            internalId: "win-fixed", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 630, y: 0, width: 400, height: 300 },
            clientGeometry: { x: 630, y: 0, width: 400, height: 300 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
            minSize: { width: 400, height: 300 }, maxSize: { width: 400, height: 300 },
            pid: 1234, resourceClass: "Example", caption: "Window fixed", windowRole: "normal",
            desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
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
}

function startFlow(surface: object, opts: { gap?: number; outer?: number } = {}): Flow {
    const calls: Flow["calls"] = [];
    const timers: Flow["timers"] = [];
    const logs: string[] = [];
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
        log: (m) => { logs.push(m); },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        ...(opts.gap !== undefined ? { readInnerGapFn: () => opts.gap as number } : {}),
        ...(opts.outer !== undefined ? { readOuterGapFn: () => opts.outer as number } : {}),
    });
    assert.ok(handle !== null);
    return { handle: handle as NonNullable<typeof handle>, calls, timers, logs };
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

// Narrow env: only the named kde-v4 byte counterparts may be written.
function checkOrWrite(name: string, actual: string): void {
    if (UPDATE) {
        assert.ok(ALLOWED_FIXTURES.has(name), `refusing to write unlisted fixture ${name}`);
        mkdirSync(FIXTURE_DIR, { recursive: true });
        writeFileSync(join(FIXTURE_DIR, name), actual);
        return;
    }
    const expected = readFileSync(join(FIXTURE_DIR, name), "utf8");
    assert.equal(actual, expected, `${name} byte gate: production payload must equal the shared artifact`);
}

// Preserve canonical inner bytes: mask only the volatile envelope
// bindings (source plus the bound-reply req echo, both mint per capture)
// via exact-value replacement, never JSON re-serialization (which would
// re-escape C0 controls via JSON.stringify short escapes).
function maskSource(actual: string): string {
    const body = JSON.parse(actual) as Record<string, unknown>;
    const src = body["source"];
    assert.ok(typeof src === "string" && src !== "");
    let out = actual.replace(`"source":${JSON.stringify(src)}`, '"source":"src-masked"');
    if ("req" in body) {
        assert.equal(body["req"], src, "bound reply request echoes the capture source");
        out = out.replace(`"req":${JSON.stringify(src)}`, '"req":"src-masked"');
    }
    return out;
}

function repoRoot(): string {
    let dir = resolve(process.cwd());
    for (let depth = 0; depth < 4; depth += 1) {
        if (existsSync(join(dir, "Cargo.toml"))) {
            return dir;
        }
        dir = dirname(dir);
    }
    throw new Error("fixture root not found");
}

class WorldBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{ resolve: (reply: string) => void; reject: (error: Error) => void }> = [];

    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("raw_world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.on("error", (error) => {
            const failure = error instanceof Error ? error : new Error(String(error));
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(failure);
            }
        });
        proc.on("close", (code, signal) => {
            const failure = new Error(`raw_world_eval exited code=${String(code)} signal=${String(signal)}`);
            while (this.waiters.length > 0) {
                this.waiters.shift()?.reject(failure);
            }
        });
    }

    static start(): WorldBridge {
        return new WorldBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
                cwd: repoRoot(),
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    send(method: string, payload: string): Promise<string> {
        const line = JSON.stringify({ method, body: payload });
        return new Promise<string>((resolvePromise, rejectPromise) => {
            this.waiters.push({ resolve: resolvePromise, reject: rejectPromise });
            try {
                this.proc.stdin?.write(line + "\n");
            } catch (error) {
                rejectPromise(error instanceof Error ? error : new Error(String(error)));
            }
        });
    }

    async close(): Promise<void> {
        try {
            this.proc.stdin?.end();
        } catch (error) {
            void error;
        }
        await new Promise<void>((resolvePromise) => {
            const timer = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch (error) {
                    void error;
                }
                resolvePromise();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(timer);
                resolvePromise();
            });
        });
    }
}

// Scripted v4 connection receipts (concise, not claiming the genuine
// engine): handshake-reply + accepted settings-reply bound to the live
// request correlation. Used only for supporting fakes that assert producer
// bytes, oversize gating, rejection vocabulary, or hint shapes.
function answerHandshake(flow: Flow): string {
    const call = [...flow.calls].reverse().find((c) => c.method === "DiscoverWorld");
    assert.ok(call !== undefined, "expected DiscoverWorld");
    const body = JSON.parse((call as { payload: string }).payload) as Record<string, unknown>;
    const reply = JSON.stringify({
        v: 4, type: "handshake-reply", adapter: "test-adapter",
        req: body["req"], gen: "gen-1", ready: false, kind: "",
    });
    (call as { callback: (reply: unknown) => void }).callback(reply);
    return "gen-1";
}

function answerSettings(flow: Flow, gen: string): void {
    const call = [...flow.calls].reverse().find((c) => c.method === "DiscoverWorld");
    assert.ok(call !== undefined, "expected DiscoverWorld settings");
    const body = JSON.parse((call as { payload: string }).payload) as Record<string, unknown>;
    assert.equal(body["type"], "settings");
    const reply = JSON.stringify({
        v: 4, type: "settings-reply", adapter: "test-adapter",
        gen, req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    });
    (call as { callback: (reply: unknown) => void }).callback(reply);
}

function connectScripted(flow: Flow): string {
    const gen = answerHandshake(flow);
    answerSettings(flow, gen);
    return gen;
}

describe("world shared byte fixtures", () => {
    it("captures discovery plus full/command reports byte-identically", { timeout: 120000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const { surface, world, windowRemoved } = makeFixtureSurface();
            const flow = startFlow(surface);
            // Discovery bytes are fully deterministic on a fresh entry.
            const hsCall = flow.calls.find((c) => c.method === "DiscoverWorld");
            assert.ok(hsCall !== undefined, "entry discovers on start");
            checkOrWrite("handshake.json", hsCall.payload);
            const hsReplyText = await bridge.send("DiscoverWorld", hsCall.payload);
            const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
            assert.equal(hsReply["type"], "handshake-reply");
            const gen = hsReply["gen"] as string;
            assert.ok(typeof gen === "string" && gen !== "");
            hsCall.callback(broker.bindDiscovery(hsReplyText));
            const setCall = flow.calls.find((c, i) => c.method === "DiscoverWorld" && i > 0);
            assert.ok(setCall !== undefined, "revisioned settings follow the handshake");
            checkOrWrite("settings.json", setCall.payload);
            const setReplyText = await bridge.send("DiscoverWorld", setCall.payload);
            const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
            assert.equal(setReply["status"], "accepted");
            setCall.callback(setReplyText);
            // Startup auto capture settles first: the live inventory seeds
            // the close-tracker cache, so the later removal gates an
            // explicit close instead of vanishing before any capture.
            runTimers(flow);
            const startupPayload = lastPayload(flow, "DescribeWorld");
            // The startup capture carries the raw initial native frame
            // before any genuine actuation: byte-gate it exactly and lock
            // the extrema oracle here, never by resetting live state.
            checkOrWrite("report-startup.json", maskSource(startupPayload));
            assert.ok(startupPayload.includes("-16384"), "extrema ride verbatim in the startup capture");
            const startupInner = (JSON.parse(startupPayload) as Record<string, unknown>)["report"] as Record<string, unknown>;
            const startupExtreme = (startupInner["windows"] as Array<Record<string, unknown>>).find((w) => w["win"] === "win-extreme");
            assert.deepEqual(startupExtreme?.["rect"], [-16384, -16384, 16384, 16384], "raw initial extreme rect rides verbatim");
            const startupStamped = broker.stampCapture(startupPayload);
            const startupRaw = await bridge.send("DescribeWorld", startupStamped);
            assert.notEqual((JSON.parse(startupRaw) as Record<string, unknown>)["outcome"], "rejected", "genuine engine accepts the startup capture");
            replyCall(flow, "DescribeWorld")(broker.bindReply(startupRaw, startupPayload));
            // A native close rides the next full report explicitly: the
            // window leaves the live inventory first, then the payload-free
            // removal event gates the cached-id difference.
            world.wins = world.wins.filter((w) => w["internalId"] !== "win-gone");
            windowRemoved.emit();
            runTimers(flow);
            const observePayload = lastPayload(flow, "DescribeWorld");
            assert.notEqual(observePayload, startupPayload, "close rides a fresh capture, never a replay");
            checkOrWrite("report-observe.json", maskSource(observePayload));
            const observeBody = JSON.parse(observePayload) as Record<string, unknown>;
            const observeInner = observeBody["report"] as Record<string, unknown>;
            // Byte gate locks: minimized lifetimes, native order, raw hints,
            // long ids, close lane, native facts. Extrema lock at the
            // startup capture above: the genuine startup apply actuates the
            // initial extreme frame before the close capture.
            const facts = observeInner["windows"] as Array<Record<string, unknown>>;
            assert.ok(facts.some((m) => m["win"] === "win-min" && m["minimized"] === true));
            assert.ok(((observeInner["native"] as Array<Record<string, unknown>>)[0]?.["order"] as Array<unknown>).length >= 3);
            assert.ok(observePayload.includes(LONG_ID), "128-char id rides verbatim");
            assert.ok((observeInner["closed"] as Array<unknown>).includes("win-gone"), "explicit close rides the closed lane");
            const game = facts.find((w) => w["win"] === "win-game");
            assert.ok(game !== undefined && game["fullscreen"] === true && game["maximized"] === false);
            for (const row of facts) {
                for (const legacy of ["floating", "fitExcluded", "fixedAuto", "fixedSuppress", "frameGeometry", "clientGeometry", "desktops", "minSize", "maxSize", "fullScreen", "maximizeMode", "observedWs"]) {
                    assert.ok(!(legacy in row), `v4 row carries no legacy ${legacy}`);
                }
                assert.ok("rect" in row && "hints" in row && "visibility" in row, "v4 row carries native facts");
            }
            const winA = facts.find((w) => w["win"] === "win-a");
            assert.equal(winA?.["nativeWs"], "ws-1");
            // Canonical C0 escaping rides the byte fixture: the live 0x08
            // and 0x1F spell lowercase \u0008 and \u001f, never short escapes.
            assert.ok(observePayload.includes('"title":"A\\u0008B\\u001fC"'), "control title rides canonical escapes");
            assert.ok(!observePayload.includes('"title":"A\\b'), "no short-escape reencoding");
            // Truthful native facts ride the wire (not aliases).
            assert.equal(winA?.["processId"], "1234");
            assert.equal(winA?.["class"], "Example");
            assert.equal(winA?.["resizable"], true);
            // Genuine Rust reply for this exact report parses and applies
            // through the production entry (proves byte consumption).
            const stamped = broker.stampCapture(observePayload);
            const raw = await bridge.send("DescribeWorld", stamped);
            assert.notEqual((JSON.parse(raw) as Record<string, unknown>)["outcome"], "rejected", "genuine engine accepts the producer capture");
            const bound = broker.bindReply(raw, observePayload);
            checkOrWrite("reply-observe.json", maskSource(bound));
            const parsed = parseWorldReply(bound);
            assert.ok(parsed !== null && isCoreWorldReply(parsed), "genuine reply parses");
            replyCall(flow, "DescribeWorld")(bound);
            assert.ok(flow.logs.some((l) => l.includes("omnitiler:world:applied rev=")), "genuine reply applied");
            // True native-shaped apply: the ordinary eligible client takes
            // actual geometry writes (the applied log alone never proves it).
            assert.ok(world.geometryWrites.some((w) => w.startsWith("win-a@")), `ordinary client takes geometry writes, got ${JSON.stringify(world.geometryWrites.slice(0, 4))}`);
            // A directed send is direction-only (core resolves the target):
            // no endpoint is named on any lane. The queue files first; the
            // draining capture is sampled only after the real broker ack, so
            // the fixture never replays the already-settled capture.
            flow.handle.requestSendToOutput("right", false);
            runTimers(flow);
            const sendQueueCall = [...flow.calls].reverse().find((c) => c.method === "QueueWorldCommand");
            assert.ok(sendQueueCall !== undefined, "send files a queue command");
            const sendQueued = JSON.parse((sendQueueCall as { payload: string }).payload) as Record<string, unknown>;
            const sendCmd = sendQueued["command"] as Record<string, unknown>;
            assert.equal(sendCmd["op"], "send-directional");
            assert.equal(sendCmd["direction"], "right");
            assert.equal(sendCmd["follow"], false);
            const sendQueueReply = broker.answerQueue((sendQueueCall as { payload: string }).payload);
            assert.equal((JSON.parse(sendQueueReply) as Record<string, unknown>)["outcome"], "queued", "real broker files the send");
            (sendQueueCall as { callback: (reply: unknown) => void }).callback(sendQueueReply);
            runTimers(flow);
            const sendPayload = lastPayload(flow, "DescribeWorld");
            assert.notEqual(sendPayload, observePayload, "draining capture is new, never the settled capture");
            checkOrWrite("report-send.json", maskSource(sendPayload));
            const sendBody = JSON.parse(sendPayload) as Record<string, unknown>;
            assert.ok(!("command" in (sendBody["report"] as Record<string, unknown>)), "drained capture stays commandless");
            const sendDrainedBefore = broker.drained.length;
            const sendStamped = broker.stampCapture(sendPayload);
            assert.equal(broker.drained.length, sendDrainedBefore + 1, "real queue drains exactly once");
            assert.equal((broker.drained[broker.drained.length - 1] as { globalCmd: string }).globalCmd, (JSON.parse(sendQueueReply) as Record<string, unknown>)["globalCmd"], "drain binds the filed command");
            const sendRaw = await bridge.send("DescribeWorld", sendStamped);
            assert.notEqual((JSON.parse(sendRaw) as Record<string, unknown>)["outcome"], "rejected", "genuine engine accepts the draining capture");
            const sendBound = broker.bindReply(sendRaw, sendPayload);
            const sendParsed = parseWorldReply(sendBound);
            assert.ok(sendParsed !== null && isCoreWorldReply(sendParsed), "draining reply parses");
            replyCall(flow, "DescribeWorld")(sendBound);
            flow.handle.stop();
        } finally {
            await bridge.close();
        }
    });

    it("captures real Qt size-hint defaults byte-identically with a select follow-up", { timeout: 120000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        try {
            const { surface } = makeSizeHintsSurface();
            const flow = startFlow(surface);
            const hsCall = flow.calls.find((c) => c.method === "DiscoverWorld");
            assert.ok(hsCall !== undefined);
            hsCall.callback(await bridge.send("DiscoverWorld", hsCall.payload).then((t) => broker.bindDiscovery(t)));
            const setCall = flow.calls.find((c, i) => c.method === "DiscoverWorld" && i > 0);
            assert.ok(setCall !== undefined);
            setCall.callback(await bridge.send("DiscoverWorld", setCall.payload));
            runTimers(flow);
            const observePayload = lastPayload(flow, "DescribeWorld");
            checkOrWrite("report-size-hints.json", maskSource(observePayload));
            const body = JSON.parse(observePayload) as Record<string, unknown>;
            // Converted decorated-frame hints ride verbatim on the v4 wire
            // (zero decoration delta here, so decorated equals client).
            assert.ok(observePayload.includes('"hints":{"minW":0,"minH":0,"maxW":16777215,"maxH":16777215}'), "Qt default hints ride");
            assert.ok(observePayload.includes('2147483647'), "unbounded sentinel rides");
            assert.ok(observePayload.includes('"hints":{"minW":640,"minH":0,"maxW":640,"maxH":0}'), "partial-zero vector rides");
            assert.ok(observePayload.includes('"hints":{"minW":400,"minH":300,"maxW":400,"maxH":300}'), "fixed pin rides");
            const innerHints = body["report"] as Record<string, unknown>;
            const rows = innerHints["windows"] as Array<Record<string, unknown>>;
            assert.equal(rows.length, 4);
            for (const row of rows) {
                assert.ok("hints" in row && "rect" in row, "v4 row carries native facts");
                for (const legacy of ["fixedAuto", "minSize", "maxSize", "frameGeometry", "clientGeometry", "desktops", "floating"]) {
                    assert.ok(!(legacy in row), `v4 row carries no legacy ${legacy}`);
                }
            }
            // Accepted, never catchup-held: the neutral select rides the
            // next dispatch instead of being dropped to observation-only.
            const stamped = broker.stampCapture(observePayload);
            const raw = await bridge.send("DescribeWorld", stamped);
            assert.notEqual((JSON.parse(raw) as Record<string, unknown>)["outcome"], "rejected", "genuine engine accepts the size-hints capture");
            replyCall(flow, "DescribeWorld")(broker.bindReply(raw, observePayload));
            flow.handle.requestWorkspaceSelect(2);
            runTimers(flow);
            assert.ok(!flow.logs.some((l) => l.includes("shortcut-failed")), "select dispatched with its command");
            const selQueueCall = [...flow.calls].reverse().find((c) => c.method === "QueueWorldCommand");
            assert.ok(selQueueCall !== undefined, "select files a queue command");
            const selectQueued = JSON.parse((selQueueCall as { payload: string }).payload) as Record<string, unknown>;
            const cmd = selectQueued["command"] as Record<string, unknown>;
            assert.equal(cmd["op"], "select-numbered");
            assert.equal(cmd["index"], 2);
            assert.equal(cmd["out"], null);
            const selQueueReply = broker.answerQueue((selQueueCall as { payload: string }).payload);
            assert.equal((JSON.parse(selQueueReply) as Record<string, unknown>)["outcome"], "queued", "real broker files the select");
            (selQueueCall as { callback: (reply: unknown) => void }).callback(selQueueReply);
            runTimers(flow);
            const selectPayload = lastPayload(flow, "DescribeWorld");
            assert.notEqual(selectPayload, observePayload, "select drains a fresh capture, never a replay");
            checkOrWrite("report-size-hints-select.json", maskSource(selectPayload));
            const selDrainedBefore = broker.drained.length;
            const selStamped = broker.stampCapture(selectPayload);
            assert.equal(broker.drained.length, selDrainedBefore + 1, "real select drains exactly once");
            const selRaw = await bridge.send("DescribeWorld", selStamped);
            assert.notEqual((JSON.parse(selRaw) as Record<string, unknown>)["outcome"], "rejected", "genuine engine accepts the select drain");
            const selBound = broker.bindReply(selRaw, selectPayload);
            const selParsed = parseWorldReply(selBound);
            assert.ok(selParsed !== null && isCoreWorldReply(selParsed), "select drain parses");
            replyCall(flow, "DescribeWorld")(selBound);
            flow.handle.stop();
        } finally {
            await bridge.close();
        }
    });

    it("refuses oversize reports at the 4MiB gate without truncation", () => {
        // Fat rows (128-char window, output, and workspace ids) so the
        // real builder crosses 4MiB well inside the 10000-window cap.
        const longOut = "o".repeat(128);
        const longWs = "s".repeat(128);
        const mkWins = (count: number) =>
            Array.from({ length: count }, (_, i) => ({
                internalId: `w${"q".repeat(120)}-${i}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                normalWindow: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: longOut },
                desktops: [{ id: `${longWs.slice(0, 120)}-${i % 64}` }],
                minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
                minSize: { width: 100, height: 100 },
                maxSize: { width: 800, height: 600 },
                pid: 1234, resourceClass: "Example", caption: `Window ${i}`, windowRole: "normal",
                desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
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
        // A smaller complete inventory dispatches below the unchanged 4MiB cap.
        {
            const flow = startFlow(mkSurface(1000));
            connectScripted(flow);
            runTimers(flow);
            const payload = lastPayload(flow, "DescribeWorld");
            assert.ok(payload.length < 4 * 1024 * 1024, `under-cap dispatches (${payload.length} bytes)`);
            assert.ok(!flow.logs.some((l) => l.includes("request-oversize")));
            flow.handle.stop();
        }
        // Over: the same builder past 4MiB refuses with no partial page.
        {
            const flow = startFlow(mkSurface(8000));
            connectScripted(flow);
            const before = flow.calls.filter((c) => c.method === "DescribeWorld").length;
            runTimers(flow);
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
        // Synthetic correlated v4 rejection on the capture lane.
        const gen = connectScripted(flow);
        runTimers(flow);
        const envelope = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
        replyCall(flow, "DescribeWorld")(JSON.stringify({
            v: 4, outcome: "rejected", kind: "snapshot-invalid", message, gen,
            source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "0",
            gestureAssoc: null,
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
        // meaning "no maximum"). The world report carries the raw client
        // QSize values plus the live frame/client rects; the Rust broker
        // converts to decorated-frame sizes via the live delta (this
        // zero-decoration double keeps converted equal to raw) and the
        // sentinel rides for core-side sanitize (absent at every use site,
        // never a refusal, never an automatic float). Generic fake ids,
        // real field shapes.
        const ws1 = { id: "ws-1" };
        const ws2 = { id: "ws-2" };
        const wins: Array<Record<string, unknown>> = [
            {
                internalId: "win-t05", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: "out-1" }, desktops: [ws1],
                minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
                minSize: { width: 176, height: 150 },
                maxSize: { width: 2147483647, height: 2147483647 },
                pid: 1234, resourceClass: "Example", caption: "Window t05", windowRole: "normal",
                desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
            },
            {
                internalId: "win-fixed", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 210, y: 0, width: 400, height: 300 },
                clientGeometry: { x: 210, y: 0, width: 400, height: 300 },
                output: { name: "out-1" }, desktops: [ws1],
                minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
                minSize: { width: 400, height: 300 }, maxSize: { width: 400, height: 300 },
                pid: 1234, resourceClass: "Example", caption: "Window fixed", windowRole: "normal",
                desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
            },
            {
                internalId: "win-neg", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 420, y: 0, width: 200, height: 200 },
                clientGeometry: { x: 420, y: 0, width: 200, height: 200 },
                output: { name: "out-1" }, desktops: [ws1],
                minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
                minSize: { width: -5, height: 100 }, maxSize: null,
                pid: 1234, resourceClass: "Example", caption: "Window neg", windowRole: "normal",
                desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
            },
            {
                internalId: "win-t", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
                clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
                output: { name: "out-1" }, desktops: [ws2],
                minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
                minSize: null, maxSize: null,
                pid: 1234, resourceClass: "Example", caption: "Window t", windowRole: "normal",
                desktopFileName: "example.app", resizeable: true, keepAbove: false, keepBelow: false,
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
            connectScripted(flow);
            runTimers(flow);
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
            // Decorated-frame hints ride verbatim (zero decoration delta
            // here, so decorated equals client); the unbounded max never
            // promotes an ordinary client to an automatic float
            // (core-side sanitize, absent at use sites).
            assert.deepEqual(
                t05["hints"],
                { minW: 176, minH: 150, maxW: 2147483647, maxH: 2147483647 },
            );
            assert.ok(!("fixedAuto" in t05) && !("floating" in t05) && !("minSize" in t05) && !("maxSize" in t05), "no legacy verdicts");
            // In-bound finite pairs ride exact for broker-side pinning.
            const fixed = rowOf("win-fixed");
            assert.deepEqual(
                fixed["hints"],
                { minW: 400, minH: 300, maxW: 400, maxH: 300 },
            );
            // Negative client extents refuse upstream and null the bound
            // (readHintSize); the wire carries honest nulls, never invented
            // values or verdicts.
            const neg = rowOf("win-neg");
            assert.deepEqual(
                neg["hints"],
                { minW: null, minH: null, maxW: null, maxH: null },
            );
        } finally {
            flow.handle.stop();
        }
    });
});

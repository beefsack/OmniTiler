// K1 ordinary-frame carrier: genuine v4 producer bytes through the strict
// v4 broker transport into the retained real `WorldEngine`
// (`raw_world_eval`: handshake/settings via `route_connection_request` on
// DiscoverWorld, `decode_request` + `handle_report` + `encode_reply` on
// DescribeWorld), with the bound reply accepted by the production entry
// and one ordinary frame write converging against actual fake-native
// readback. Offline only: scripted in-memory fakes, no D-Bus, no KWin.
//
// Scope is deliberately one ordinary tiling frame: no selection, arrival,
// focus-change, state-target, or later-feature expectations, and no
// visible anchored-border assertions (K1 stays hidden-only). Intent
// persistence stays on the default disabled seam: the entry auto-seeds the
// (empty) intent snapshot before the first admissible report, so no opaque
// bytes ride. Included in `npm test`.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { createMiniBroker, type MiniBroker } from "./world-wire-harness";

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
    private readonly waiters: Array<{
        resolve: (reply: string) => void;
        reject: (error: Error) => void;
    }> = [];
    private dead: string | null = null;
    private stderrTail = "";

    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("raw_world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.on("data", (chunk: unknown) => {
            this.stderrTail = (this.stderrTail + String(chunk)).slice(-8192);
        });
        proc.on("error", (error) => {
            this.failAll(error instanceof Error ? error : new Error(String(error)));
        });
        proc.on("close", (code, signal) => {
            this.failAll(new Error(`raw_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
        });
    }

    private failAll(error: Error): void {
        if (this.dead === null) {
            this.dead = error.message;
        }
        while (this.waiters.length > 0) {
            this.waiters.shift()?.reject(error);
        }
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
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return Promise.reject(new Error("raw_world_eval process already exited"));
        }
        const line = JSON.stringify({ method, body: payload });
        return new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(line + "\n");
            } catch (error) {
                reject(error instanceof Error ? error : new Error(String(error)));
            }
        });
    }

    async close(): Promise<void> {
        try {
            this.proc.stdin?.end();
        } catch (error) {
            void error;
        }
        await new Promise<void>((resolve) => {
            const timer = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch (error) {
                    void error;
                }
                resolve();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(timer);
                resolve();
            });
        });
    }
}

interface FakeSignal {
    readonly handlers: Array<(payload?: unknown) => void>;
    readonly signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
}

function makeSignal(): FakeSignal {
    const handlers: Array<(payload?: unknown) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (payload?: unknown) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (payload?: unknown) => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

interface FakeDesktop {
    readonly id: string;
    readonly name: string;
}

interface FakeWindow {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    clientGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
    pid: number;
    resourceClass: string;
    caption: string;
    windowRole: string;
    desktopFileName: string;
    resizeable: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    readonly outputChanged: FakeSignal["signal"];
    readonly desktopsChanged: FakeSignal["signal"];
    readonly hiddenChanged: FakeSignal["signal"];
    readonly frameGeometryChanged: FakeSignal["signal"];
    readonly __signals: { readonly outputChanged: FakeSignal; readonly desktopsChanged: FakeSignal; readonly hiddenChanged: FakeSignal; readonly frameGeometryChanged: FakeSignal };
}

interface CapturedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

interface Timer {
    delayMs: number;
    callback: () => void;
    cancelled: boolean;
    fired: boolean;
}

function isCanonicalU64Text(text: unknown): text is string {
    return typeof text === "string" && (/^(0|[1-9][0-9]*)$/.test(text) &&
        !(text.length > 1 && text.startsWith("0")) &&
        (text.length < 20 || (text.length === 20 && text <= "18446744073709551615")));
}

function canonicalLess(a: string, b: string): boolean {
    if (a.length !== b.length) {
        return a.length < b.length;
    }
    return a < b;
}

describe("world ordinary frame engine carrier", () => {
    it("authentic handshake+settings then one ordinary frame converges with readback", { timeout: 240000 }, async () => {
        const ws: FakeDesktop = { id: "ws-1", name: "ws-1" };
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        let frame = { x: 0, y: 0, width: 200, height: 200 };
        // Mutable fake output: logical full bounds plus a physical ratio
        // the producer never reads (KDE reports logical units at scale:1).
        // A DPR-only change must not churn; a logical bounds change, as a
        // scale/mode reconfiguration can produce, must reflow (no exact
        // physical ratio proof).
        const outScreen = {
            name: "out",
            geometry: { x: 0, y: 0, width: 1920, height: 1080 },
            devicePixelRatio: 1,
        };
        let workArea = { x: 0, y: 0, width: 1920, height: 1040 };
        // Truthful native decoration: stable nonnegative insets mirroring
        // the v4 reference (frame 8.4,8.6,800,600 -> client 10,30,796,578:
        // left 2, top 21, right 2, bottom 1; deltaW 4, deltaH 22). The tiler
        // writes frameGeometry only; the native client tracks with the same
        // insets so every capture carries consistent frame/client facts.
        const DECOR = { left: 2, top: 21, right: 2, bottom: 1 };
        function truthfulClientFor(f: { x: number; y: number; width: number; height: number }): { x: number; y: number; width: number; height: number } {
            const w = Math.max(1, f.width - (DECOR.left + DECOR.right));
            const h = Math.max(1, f.height - (DECOR.top + DECOR.bottom));
            const x = f.x + Math.min(DECOR.left, Math.max(0, f.width - w));
            const y = f.y + Math.min(DECOR.top, Math.max(0, f.height - h));
            return { x, y, width: w, height: h };
        }
        let client = truthfulClientFor(frame);
        const geometryWrites: string[] = [];
        // Truthful native fake: positive pid, resizable, class identity,
        // and the unbounded INT_MAX maximum the KWin backends report (never
        // a (0,0) cap). These facts must ride the stamped bytes verbatim.
        const win: FakeWindow = {
            internalId: "win-a", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { ...frame },
            clientGeometry: { ...client },
            output: { name: "out" },
            desktops: [ws],
            minimized: false,
            hidden: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234,
            resourceClass: "Example",
            caption: "Window",
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
            keepAbove: false,
            keepBelow: false,
            outputChanged: outputChanged.signal,
            desktopsChanged: winDesktopsChanged.signal,
            hiddenChanged: winHiddenChanged.signal,
            frameGeometryChanged: winFrameChanged.signal,
            __signals: { outputChanged, desktopsChanged: winDesktopsChanged, hiddenChanged: winHiddenChanged, frameGeometryChanged: winFrameChanged },
        };
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            enumerable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                client = truthfulClientFor(rect);
                geometryWrites.push(`win-a@${rect.x},${rect.y},${rect.width},${rect.height}`);
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            enumerable: true,
            get: () => client,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                client = rect;
            },
        });
        const signals: Record<string, FakeSignal> = {};
        for (const name of ["windowAdded", "windowRemoved", "windowActivated", "desktopsChanged", "screensChanged", "currentDesktopChanged", "activeScreenChanged"] as const) {
            signals[name] = makeSignal();
        }
        const bridge = WorldBridge.start();
        const broker: MiniBroker = createMiniBroker();
        const calls: CapturedCall[] = [];
        const answered = new Set<number>();
        const replies = new Map<number, string>();
        const stamped: string[] = [];
        const timers: Timer[] = [];
        const logs: string[] = [];
        let activeRef: FakeWindow | null = win;
        const surface: Record<string, unknown> = {
            windowList: (): FakeWindow[] => [win],
            screens: [outScreen],
            desktops: [ws],
            activeScreen: { name: "out" },
            cursorPos: { x: 2, y: 100 },
            windowAdded: (signals["windowAdded"] as FakeSignal).signal,
            windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
            windowActivated: (signals["windowActivated"] as FakeSignal).signal,
            desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
            screensChanged: (signals["screensChanged"] as FakeSignal).signal,
            currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
            activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
            get activeWindow(): FakeWindow | null {
                return activeRef;
            },
            set activeWindow(ref: unknown) {
                if ((ref as FakeWindow) === win && activeRef !== win) {
                    activeRef = win;
                }
            },
            clientArea: (): Record<string, number> => ({ ...workArea }),
            currentDesktopForScreen: (): FakeDesktop => ws,
            setCurrentDesktopForScreen: (): void => undefined,
            sendClientToScreen: (): void => undefined,
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_service, _path, _iface, method, payload, callback): void => {
                if (method !== "DiscoverWorld" && method !== "DescribeWorld" && method !== "QueueWorldCommand" && method !== "TakeCommandResult") {
                    return;
                }
                calls.push({ method, payload, callback });
                if (method === "QueueWorldCommand") {
                    callback(broker.answerQueue(payload));
                } else if (method === "TakeCommandResult") {
                    callback(broker.answerTake(payload));
                }
            },
            scheduleOnce: (delayMs, callback): (() => void) => {
                const timer: Timer = { delayMs, callback, cancelled: false, fired: false };
                timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: (message: string): void => {
                logs.push(message);
            },
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readDefaultTiledFn: () => true,
        });
        assert.notEqual(handle, null, "world production entry starts on the fake surface");
        const runTimers = (maxDelay = WORLD_DEBOUNCE_MS): void => {
            for (let i = 0; i < timers.length; i += 1) {
                const timer = timers[i] as Timer;
                if (timer.delayMs <= maxDelay && !timer.cancelled && !timer.fired) {
                    timer.fired = true;
                    timers.splice(i, 1);
                    i -= 1;
                    timer.callback();
                }
            }
        };
        const nextUnanswered = (method: string, from = 0): number => {
            const at = calls.findIndex((call, index) => index >= from && call.method === method && !answered.has(index));
            assert.ok(at >= 0, `expected an unanswered ${method}; logs=${JSON.stringify(logs.slice(-8))}`);
            return at;
        };
        const flushDescribe = async (index: number): Promise<Record<string, unknown>> => {
            assert.ok(calls[index]?.method === "DescribeWorld" && !answered.has(index), "expected the targeted DescribeWorld to be unanswered");
            const envelope = (calls[index] as CapturedCall).payload;
            const stampedText = broker.stampCapture(envelope);
            stamped.push(stampedText);
            const coreReply = await bridge.send("DescribeWorld", stampedText);
            const parsed = JSON.parse(coreReply) as Record<string, unknown>;
            assert.notEqual(parsed["outcome"], "rejected", `real strict engine accepts the genuine producer capture, got ${coreReply.slice(0, 400)}`);
            const reply = broker.bindReply(coreReply, envelope);
            answered.add(index);
            replies.set(index, reply);
            (calls[index] as CapturedCall).callback(reply);
            return JSON.parse(reply) as Record<string, unknown>;
        };
        const flushAllDescribes = async (): Promise<void> => {
            for (let round = 0; round < 16; round += 1) {
                const at = calls.findIndex((call, index) => call.method === "DescribeWorld" && !answered.has(index));
                if (at < 0) {
                    return;
                }
                await flushDescribe(at);
            }
            throw new Error("flushAll did not converge: planner keeps dispatching");
        };
        const settle = async (): Promise<void> => {
            for (let round = 0; round < 30; round += 1) {
                runTimers();
                const before = calls.length;
                await flushAllDescribes();
                runTimers();
                await flushAllDescribes();
                if (calls.length === before) {
                    return;
                }
            }
            throw new Error("settle did not converge");
        };
        try {
            // Authentic connection pump: the handshake binds the producer
            // lifetime and mints the serving generation; the revisioned
            // settings snapshot (rev 1) precedes the first report on the
            // wire, and the first report waits for its accepted reply.
            const hsAt = nextUnanswered("DiscoverWorld");
            const hsBody = JSON.parse((calls[hsAt] as CapturedCall).payload) as Record<string, unknown>;
            assert.equal(hsBody["type"], "handshake", "first discovery is the handshake");
            assert.equal(hsBody["adapter"], "test-adapter", "handshake carries the producer adapter");
            const hsReplyText = await bridge.send("DiscoverWorld", (calls[hsAt] as CapturedCall).payload);
            const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
            assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
            assert.equal(hsReply["ready"], false, "accepted handshake carries ready:false");
            assert.equal(hsReply["kind"], "", "accepted handshake carries an empty kind");
            assert.equal(hsReply["adapter"], "test-adapter", "handshake retains the producer adapter, never a broker incarnation");
            assert.equal(hsReply["req"], hsBody["req"], "handshake echoes the request correlation");
            assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "", "handshake mints the serving generation");
            const gen = hsReply["gen"] as string;
            const boundHs = broker.bindDiscovery(hsReplyText);
            assert.equal((JSON.parse(boundHs) as Record<string, unknown>)["adapter"], "test-adapter", "harness passes the producer lifetime through");
            assert.equal(broker.gen, gen, "harness records the actual serving generation");
            (calls[hsAt] as CapturedCall).callback(boundHs);
            const setAt = nextUnanswered("DiscoverWorld", hsAt + 1);
            const setBody = JSON.parse((calls[setAt] as CapturedCall).payload) as Record<string, unknown>;
            assert.equal(setBody["type"], "settings", "second discovery is the revisioned settings snapshot");
            assert.equal(setBody["adapter"], "test-adapter", "settings carry the producer adapter");
            assert.equal(setBody["gen"], gen, "settings bind the serving generation");
            assert.equal(setBody["rev"], "1", "first settings snapshot carries rev 1");
            const setReplyText = await bridge.send("DiscoverWorld", (calls[setAt] as CapturedCall).payload);
            const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
            assert.equal(setReply["type"], "settings-reply", "engine answers settings");
            assert.equal(setReply["status"], "accepted", `settings accepted, got ${setReplyText.slice(0, 300)}`);
            assert.equal(setReply["adapter"], "test-adapter", "settings reply retains the producer adapter");
            assert.equal(setReply["gen"], gen, "settings reply binds the serving generation");
            assert.equal(setReply["req"], setBody["req"], "settings reply echoes the request correlation");
            assert.equal(setReply["rev"], "1", "settings reply binds the sent revision");
            (calls[setAt] as CapturedCall).callback(setReplyText);
            assert.deepEqual(
                calls.slice(0, setAt + 1).map((call) => call.method),
                ["DiscoverWorld", "DiscoverWorld"],
                "no report precedes the accepted settings",
            );

            await settle();
            assert.ok(stamped.length > 0, "at least one capture flowed to the real engine");
            // Strict transport truth on the first stamped report: the
            // producer lifetime and serving generation bind, the forbidden
            // owner key is absent, the capture-only lanes are stripped, and
            // every other producer byte rides verbatim.
            const firstStamped = JSON.parse(stamped[0] as string) as Record<string, unknown>;
            const firstEnvelope = JSON.parse((calls.find((call) => call.method === "DescribeWorld") as CapturedCall).payload) as Record<string, unknown>;
            assert.equal(firstStamped["v"], 4, "stamped report is v4");
            assert.equal(firstStamped["adapter"], "test-adapter", "stamped adapter is the producer lifetime");
            assert.equal(firstStamped["gen"], gen, "stamped gen is the serving generation");
            assert.ok(!("owner" in firstStamped), "stamped report carries no owner key");
            for (const stripped of ["nativeGesture", "cursorWindow", "nativeInvalidWitness"]) {
                assert.ok(!(stripped in firstStamped), `capture-only ${stripped} is stripped before the strict decode`);
            }
            for (const key of ["obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
                assert.ok(key in firstStamped, `broker-owned ${key} is stamped`);
            }
            assert.equal(firstStamped["req"], firstEnvelope["source"], "stamped req names the capture source");
            assert.equal(firstStamped["command"], null, "observation-only capture drains no command");
            assert.equal(firstStamped["cmdSeq"], null, "observation-only capture carries no cmdSeq");
            assert.ok(isCanonicalU64Text(firstStamped["obs"]), "obs is canonical");
            assert.ok(isCanonicalU64Text(firstStamped["reqSeq"]), "reqSeq is canonical");
            assert.ok(isCanonicalU64Text(firstStamped["monotonicMs"]), "monotonicMs is canonical");
            const report = firstEnvelope["report"] as Record<string, unknown>;
            const { nativeGesture: _droppedGesture, cursorWindow: _droppedCursor, nativeInvalidWitness: _droppedWitness, ...reportRest } =
                report as Record<string, unknown> & { nativeGesture?: unknown; cursorWindow?: unknown; nativeInvalidWitness?: unknown };
            const brokerKeys = new Set(["adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]);
            const stampedRest: Record<string, unknown> = {};
            for (const [key, value] of Object.entries(firstStamped)) {
                if (!brokerKeys.has(key)) {
                    stampedRest[key] = value;
                }
            }
            assert.deepEqual(stampedRest, reportRest, "every non-capture-only producer byte rides verbatim: no normalization, no reconstruction");
            const rows = (stampedRest["windows"] as Array<Record<string, unknown>>).filter((row) => row["win"] === "win-a");
            assert.equal(rows.length, 1, "the fake window rides exactly once");
            const row = rows[0] as Record<string, unknown>;
            assert.equal(row["processId"], "1234", "positive pid rides as decimal text");
            assert.equal(row["class"], "Example", "class rides verbatim");
            assert.equal(row["resizable"], true, "resizable rides true");
            assert.deepEqual((row["hints"] as Record<string, unknown>)["maxW"], 2147483647, "unbounded INT_MAX maximum rides verbatim");
            assert.deepEqual((row["hints"] as Record<string, unknown>)["maxH"], 2147483647, "unbounded INT_MAX maximum rides verbatim");
            assert.equal(row["nativeWs"], "ws-1", "single-homed native desktop rides");
            assert.equal(row["role"], "normal", "role rides verbatim");
            assert.equal(row["title"], "Window", "title rides verbatim");
            if (stamped.length > 1) {
                const later = JSON.parse(stamped[stamped.length - 1] as string) as Record<string, unknown>;
                assert.ok(
                    canonicalLess(firstStamped["monotonicMs"] as string, later["monotonicMs"] as string),
                    "the dispatch clock advances across captures, never a constant stamp",
                );
            }

            // Ordinary frame convergence: the accepted engine reply plans
            // one tiled frame, the entry writes it to the fake native, and
            // the live rect matches the reply frame with no further churn.
            const lastReplyIndex = Math.max(...[...replies.keys()].filter((at) => (calls[at] as CapturedCall).method === "DescribeWorld"));
            const lastReply = JSON.parse(replies.get(lastReplyIndex) as string) as Record<string, unknown>;
            assert.equal(lastReply["outcome"] ?? "accepted", "accepted", "engine reply accepted");
            assert.equal(lastReply["adapter"], "test-adapter", "bound reply retains the producer adapter");
            assert.equal(lastReply["gen"], gen, "bound reply binds the serving generation");
            assert.equal(lastReply["result"], null, "actuator-lane result stays nulled");
            assert.equal(lastReply["gestureAssoc"], null, "no fabricated gesture binding");
            const replyRows = (lastReply["windows"] as Array<Record<string, unknown>>).filter((entry) => entry["win"] === "win-a");
            assert.equal(replyRows.length, 1, "engine plans the fake window");
            const planned = (replyRows[0] as Record<string, unknown>)["frame"] as [number, number, number, number] | null;
            assert.ok(Array.isArray(planned) && planned.length === 4, `engine plans an ordinary frame, got ${JSON.stringify((replyRows[0] as Record<string, unknown>)["frame"])}`);
            assert.ok(geometryWrites.length > 0, `one ordinary frame write lands, got ${JSON.stringify(geometryWrites)}`);
            assert.deepEqual(frame, { x: planned[0], y: planned[1], width: planned[2], height: planned[3] }, "live fake rect matches the planned frame");
            const settledWrites = geometryWrites.length;
            await settle();
            assert.equal(geometryWrites.length, settledWrites, "converged frames stay silent: no churn, no replay");

            // The single ordinary tile insets the work area by the default
            // outer gap on every side: 1920x1040 at origin plans 8,8,1904,1024.
            assert.deepEqual(planned, [8, 8, 1904, 1024], "single ordinary tile carries the default outer gap");
            // Scaled-metric reconfiguration: a physical DPR change that
            // preserves logical bounds must not churn. The fake output
            // gains a ratio the producer never reads (encodeV4ScreenRow
            // hardcodes scale:1); the entry already subscribes to
            // screensChanged, so the signal refires capture.
            const discoverBefore = calls.filter((call) => call.method === "DiscoverWorld").length;
            const stampedBeforeDpr = stamped.length;
            const writesBeforeDpr = geometryWrites.length;
            outScreen.devicePixelRatio = 2;
            for (const handler of [...(signals["screensChanged"] as FakeSignal).handlers]) {
                handler();
            }
            await settle();
            assert.ok(stamped.length > stampedBeforeDpr, "DPR-only screensChanged refires a fresh capture");
            assert.equal(calls.filter((call) => call.method === "DiscoverWorld").length, discoverBefore, "unchanged logical bounds cause no rehandshake");
            const dprStamped = JSON.parse(stamped[stamped.length - 1] as string) as Record<string, unknown>;
            const dprScreens = dprStamped["screens"] as Array<Record<string, unknown>>;
            assert.equal(dprScreens.length, 1, "single output rides");
            assert.equal(dprScreens[0]?.["scale"], 1, "physical DPR never becomes wire scale");
            assert.deepEqual(dprScreens[0]?.["full"], [0, 0, 1920, 1080], "logical full bounds preserved across the DPR-only signal");
            assert.deepEqual(dprScreens[0]?.["work"], [0, 0, 1920, 1040], "logical work bounds preserved across the DPR-only signal");
            assert.equal(geometryWrites.length, writesBeforeDpr, "unchanged logical bounds cause no geometry churn");
            assert.deepEqual(frame, { x: planned[0], y: planned[1], width: planned[2], height: planned[3] }, "frame preserved across the DPR-only signal");
            await settle();
            assert.equal(geometryWrites.length, writesBeforeDpr, "DPR-only reflow settles silent");

            // Logical shrink (a bounds change as a scale/mode reconfiguration
            // can produce, with no exact physical ratio proof): the output
            // reports smaller logical bounds, capture reflows to the exact
            // default-gap tile with native-shaped readback, then silence.
            const writesBeforeShrink = geometryWrites.length;
            outScreen.geometry = { x: 0, y: 0, width: 1280, height: 720 };
            workArea = { x: 0, y: 0, width: 1280, height: 680 };
            for (const handler of [...(signals["screensChanged"] as FakeSignal).handlers]) {
                handler();
            }
            await settle();
            const shrinkStamped = JSON.parse(stamped[stamped.length - 1] as string) as Record<string, unknown>;
            const shrinkScreens = shrinkStamped["screens"] as Array<Record<string, unknown>>;
            assert.equal(shrinkScreens[0]?.["scale"], 1, "scale stays 1 across the logical shrink");
            assert.deepEqual(shrinkScreens[0]?.["full"], [0, 0, 1280, 720], "shrunk logical full bounds ride");
            assert.deepEqual(shrinkScreens[0]?.["work"], [0, 0, 1280, 680], "shrunk logical work bounds ride");
            const shrinkReplyIndex = Math.max(...[...replies.keys()].filter((at) => (calls[at] as CapturedCall).method === "DescribeWorld"));
            const shrinkReply = JSON.parse(replies.get(shrinkReplyIndex) as string) as Record<string, unknown>;
            assert.equal(shrinkReply["outcome"] ?? "accepted", "accepted", "shrunk topology reply accepted");
            const shrinkRows = (shrinkReply["windows"] as Array<Record<string, unknown>>).filter((entry) => entry["win"] === "win-a");
            assert.equal(shrinkRows.length, 1, "engine still plans the fake window");
            assert.deepEqual(shrinkRows[0]?.["frame"], [8, 8, 1264, 664], "shrunk ordinary tile insets the new work area by the default outer gap");
            assert.deepEqual(frame, { x: 8, y: 8, width: 1264, height: 664 }, "fake-native readback converges to the shrunk tile");
            assert.equal(calls.filter((call) => call.method === "DiscoverWorld").length, discoverBefore, "logical shrink causes no rehandshake");
            assert.ok(geometryWrites.length > writesBeforeShrink, "logical shrink reflows through a fresh write");
            await settle();
            const settledAfterShrink = geometryWrites.length;
            await settle();
            assert.equal(geometryWrites.length, settledAfterShrink, "shrunk tile holds silent");
            assert.deepEqual(frame, { x: 8, y: 8, width: 1264, height: 664 }, "shrunk frame holds after settle");

            // Change back: restoring the logical bounds replays the exact
            // original tile, proving the shrink reflow was pure geometry.
            outScreen.geometry = { x: 0, y: 0, width: 1920, height: 1080 };
            workArea = { x: 0, y: 0, width: 1920, height: 1040 };
            outScreen.devicePixelRatio = 1;
            for (const handler of [...(signals["screensChanged"] as FakeSignal).handlers]) {
                handler();
            }
            await settle();
            const backReplyIndex = Math.max(...[...replies.keys()].filter((at) => (calls[at] as CapturedCall).method === "DescribeWorld"));
            const backReply = JSON.parse(replies.get(backReplyIndex) as string) as Record<string, unknown>;
            const backRows = (backReply["windows"] as Array<Record<string, unknown>>).filter((entry) => entry["win"] === "win-a");
            assert.deepEqual(backRows[0]?.["frame"], [8, 8, 1904, 1024], "restored bounds replay the original tile");
            assert.deepEqual(frame, { x: 8, y: 8, width: 1904, height: 1024 }, "readback converges back to the original tile");
            assert.equal(calls.filter((call) => call.method === "DiscoverWorld").length, discoverBefore, "restoring bounds causes no rehandshake");
            await settle();

            // External perturbation reconverges through one restoring write:
            // the follow-up capture carries the diverged live fact and the
            // entry restores the planned frame with proven readback.
            frame = { x: 3, y: 3, width: 11, height: 11 };
            client = truthfulClientFor(frame);
            geometryWrites.push("external-perturbation");
            assert.ok(win.__signals.frameGeometryChanged.handlers.length > 0, "frame homing subscribed (entry attached)");
            for (const handler of [...win.__signals.frameGeometryChanged.handlers]) {
                handler();
            }
            runTimers();
            const disturbedAt = nextUnanswered("DescribeWorld");
            const beforeRestore = geometryWrites.length;
            await flushDescribe(disturbedAt);
            const restored = geometryWrites.slice(beforeRestore).filter((line) => line !== "external-perturbation");
            assert.equal(restored.length, 1, `one restoring write lands, got ${JSON.stringify(restored)}`);
            assert.deepEqual(frame, { x: planned[0], y: planned[1], width: planned[2], height: planned[3] }, "fake-native readback converges to the planned frame");
            await settle();
            assert.deepEqual(frame, { x: planned[0], y: planned[1], width: planned[2], height: planned[3] }, "restored frame holds after settle");
            assert.ok(!logs.some((line) => line.includes("command-busy")), "no busy loops");
        } finally {
            try {
                handle?.stop();
            } catch (error) {
                void error;
            }
            await bridge.close();
        }
    });
});

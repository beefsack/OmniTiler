// Outer-gap regression: genuine v4 producer bytes through the strict v4
// broker transport into the retained real `WorldEngine`, with settings
// gap 8 / outer 12 converging to native inset edges. 800x600 work area,
// two tiled windows. Host shape mirrors the live KWin boundary: QML
// array-likes (never JS arrays), renewed wrappers per enumeration, braced
// UUID live ids with bare ids on the wire, and QSize inputs with raw frame
// bounds. Gap/outer ride the separate revisioned settings snapshot, never
// the report lane. Included in `npm test`.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
import { createMiniBroker } from "./world-wire-harness";

const WS = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const OUT = "out-1";

const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

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

interface CapturedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

// Stable nonnegative decoration: the tiler writes frameGeometry only; the
// native client tracks with the same insets so captures carry consistent
// frame/client facts.
const DECOR = { left: 2, top: 21, right: 2, bottom: 1 };
function clientFor(f: { x: number; y: number; width: number; height: number }): { x: number; y: number; width: number; height: number } {
    const w = Math.max(1, f.width - (DECOR.left + DECOR.right));
    const h = Math.max(1, f.height - (DECOR.top + DECOR.bottom));
    return { x: f.x + Math.min(DECOR.left, Math.max(0, f.width - w)), y: f.y + Math.min(DECOR.top, Math.max(0, f.height - h)), width: w, height: h };
}

describe("world outer-gap regression", () => {
    it("settings gap 8 outer 12 insets all four edges with no replay", { timeout: 240000 }, async () => {
        const bridge = WorldBridge.start();
        const broker = createMiniBroker();
        const live = new Map<string, { x: number; y: number; width: number; height: number }>([
            [`{${WIN_A}}`, { x: 0, y: 0, width: 400, height: 600 }],
            [`{${WIN_B}}`, { x: 400, y: 0, width: 400, height: 600 }],
        ]);
        const geometryWrites: string[] = [];
        const calls: CapturedCall[] = [];
        const answered = new Set<number>();
        const replies = new Map<number, string>();
        const timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }> = [];

        // Renewed wrappers per enumeration: every windowList() call builds
        // fresh wrapper objects (QML array-like, never a JS array). Live ids
        // ride braced; the wire carries bare ids.
        const wrap = (braced: string): object => {
            const frame = live.get(braced) as { x: number; y: number; width: number; height: number };
            const w: Record<string, unknown> = {
                internalId: braced,
                managed: true,
                deleted: false,
                transient: false,
                transientFor: null,
                dialog: false,
                modal: false,
                wantsInput: true,
                normalWindow: true,
                output: { name: OUT },
                desktops: qmlLike([{ id: WS }]),
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                minSize: { width: 0, height: 0 },
                maxSize: { width: 16777215, height: 16777215 },
                pid: 1234,
                resourceClass: "Example",
                caption: `Window ${braced.slice(1, 9)}`,
                windowRole: "normal",
                desktopFileName: "example.app",
                resizeable: true,
                keepAbove: false,
                keepBelow: false,
                clientGeometry: { ...clientFor(frame) },
            };
            let cur = { ...frame };
            let client = clientFor(cur);
            Object.defineProperty(w, "frameGeometry", {
                configurable: true,
                get: () => ({ ...cur }),
                set: (rect: { x: number; y: number; width: number; height: number }) => {
                    cur = { ...rect };
                    client = clientFor(cur);
                    live.set(braced, { ...rect });
                    geometryWrites.push(`${braced}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                    return true;
                },
            });
            Object.defineProperty(w, "clientGeometry", {
                configurable: true,
                get: () => ({ ...client }),
                set: (rect: { x: number; y: number; width: number; height: number }) => {
                    client = { ...rect };
                    return true;
                },
            });
            return w;
        };
        const surface: Record<string, unknown> = {
            windowList: (): object => qmlLike([...live.keys()].map((b) => wrap(b))),
            get screens() {
                return qmlLike([{ name: OUT, geometry: { x: 0, y: 0, width: 800, height: 600 } }]);
            },
            get desktops() {
                return qmlLike([{ id: WS }]);
            },
            activeScreen: { name: OUT },
            get activeWindow() {
                return { internalId: `{${WIN_A}}` };
            },
            set activeWindow(_v: unknown) {},
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ({ id: WS }),
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
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
                const timer = { delayMs, callback, cancelled: false, fired: false };
                timers.push(timer);
                return (): void => {
                    timer.cancelled = true;
                };
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
            readInnerGapFn: () => 8,
            readOuterGapFn: () => 12,
        });
        assert.notEqual(handle, null, "world production entry starts on the fake surface");
        const runTimers = (maxDelay = WORLD_DEBOUNCE_MS): void => {
            for (let i = 0; i < timers.length; i += 1) {
                const timer = timers[i] as { delayMs: number; callback: () => void; cancelled: boolean; fired: boolean };
                if (timer.delayMs <= maxDelay && !timer.cancelled && !timer.fired) {
                    timer.fired = true;
                    timers.splice(i, 1);
                    i -= 1;
                    timer.callback();
                }
            }
        };
        const flushOne = async (): Promise<{ envelope: string; bound: string }> => {
            const index = calls.findIndex((call, at) => call.method === "DescribeWorld" && !answered.has(at));
            assert.ok(index >= 0, "expected a queued DescribeWorld dispatch");
            const envelope = (calls[index] as CapturedCall).payload;
            const stamped = broker.stampCapture(envelope);
            const raw = await bridge.send("DescribeWorld", stamped);
            assert.notEqual((JSON.parse(raw) as Record<string, unknown>)["outcome"], "rejected", `real strict engine accepts the genuine producer capture, got ${raw.slice(0, 400)}`);
            const bound = broker.bindReply(raw, envelope);
            answered.add(index);
            replies.set(index, bound);
            (calls[index] as CapturedCall).callback(bound);
            return { envelope, bound };
        };
        const settle = async (): Promise<void> => {
            for (let round = 0; round < 30; round += 1) {
                runTimers();
                const before = calls.length;
                for (let i = 0; i < 16; i += 1) {
                    const next = calls.findIndex((call, at) => call.method === "DescribeWorld" && !answered.has(at));
                    if (next < 0) {
                        break;
                    }
                    await flushOne();
                }
                runTimers();
                for (let i = 0; i < 16; i += 1) {
                    const next = calls.findIndex((call, at) => call.method === "DescribeWorld" && !answered.has(at));
                    if (next < 0) {
                        break;
                    }
                    await flushOne();
                }
                if (calls.length === before) {
                    return;
                }
            }
            throw new Error("settle did not converge");
        };
        try {
            // Authentic v4 connection pump: the handshake binds the producer
            // lifetime and mints the serving generation; the revisioned
            // settings snapshot (gap 8 / outer 12, rev 1) precedes the first
            // report on the wire.
            const hsIndex = calls.findIndex((call) => call.method === "DiscoverWorld");
            assert.ok(hsIndex >= 0, "entry discovers on start");
            const hsBody = JSON.parse((calls[hsIndex] as CapturedCall).payload) as Record<string, unknown>;
            assert.equal(hsBody["type"], "handshake");
            const hsReplyText = await bridge.send("DiscoverWorld", (calls[hsIndex] as CapturedCall).payload);
            const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
            assert.equal(hsReply["type"], "handshake-reply");
            assert.equal(hsReply["adapter"], "test-adapter");
            assert.equal(hsReply["req"], hsBody["req"], "handshake echoes the request correlation");
            assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "");
            const gen = hsReply["gen"] as string;
            (calls[hsIndex] as CapturedCall).callback(broker.bindDiscovery(hsReplyText));
            const setIndex = calls.findIndex((call, at) => at > hsIndex && call.method === "DiscoverWorld");
            assert.ok(setIndex >= 0, "revisioned settings follow the handshake");
            const setBody = JSON.parse((calls[setIndex] as CapturedCall).payload) as Record<string, unknown>;
            assert.equal(setBody["type"], "settings");
            assert.equal(setBody["gen"], gen);
            assert.equal(setBody["rev"], "1");
            assert.equal((setBody["values"] as Record<string, unknown>)["gap"], 8, "settings carry the inner gap");
            assert.equal((setBody["values"] as Record<string, unknown>)["outer"], 12, "settings carry the outer gap");
            const setReplyText = await bridge.send("DiscoverWorld", (calls[setIndex] as CapturedCall).payload);
            const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
            assert.equal(setReply["status"], "accepted");
            assert.equal(setReply["gen"], gen, "settings reply binds the serving generation");
            assert.equal(setReply["req"], setBody["req"], "settings reply echoes the request correlation");
            (calls[setIndex] as CapturedCall).callback(setReplyText);
            assert.deepEqual(
                calls.slice(0, setIndex + 1).map((call) => call.method),
                ["DiscoverWorld", "DiscoverWorld"],
                "no report precedes the accepted settings",
            );

            await settle();

            // Producer facts: 800x600 screen rows carry full/work geometry
            // with no gap keys (settings lane, never the report); window rows
            // ride the raw 400x600 halves with zero-minimum hints and the
            // truthful native facts; wire ids ride bare.
            const firstEnvelope = JSON.parse((calls.find((call) => call.method === "DescribeWorld") as CapturedCall).payload) as Record<string, unknown>;
            const report = (firstEnvelope["report"] ?? firstEnvelope) as Record<string, unknown>;
            const screens = report["screens"] as Array<Record<string, unknown>>;
            assert.equal(screens.length, 1);
            assert.deepEqual(screens[0]?.["full"], [0, 0, 800, 600]);
            assert.deepEqual(screens[0]?.["work"], [0, 0, 800, 600]);
            assert.ok(!("gap" in (screens[0] as Record<string, unknown>)), "v4 screen row carries no gap");
            assert.ok(!("outer" in (screens[0] as Record<string, unknown>)), "v4 screen row carries no outer");
            const rows = (report["windows"] as Array<Record<string, unknown>>).slice().sort(
                (a, b) => ((a["rect"] as number[])[0] ?? 0) - ((b["rect"] as number[])[0] ?? 0),
            );
            assert.equal(rows.length, 2, "both tiled windows ride exactly once");
            assert.deepEqual(rows[0]?.["rect"], [0, 0, 400, 600]);
            assert.deepEqual(rows[1]?.["rect"], [400, 0, 400, 600]);
            for (const row of rows) {
                assert.deepEqual(row["hints"], { minW: 0, minH: 0, maxW: 16777215, maxH: 16777215 });
                assert.equal(row["processId"], "1234");
                assert.equal(row["class"], "Example");
                assert.equal(row["resizable"], true);
                assert.equal(row["nativeWs"], WS);
            }
            assert.deepEqual(rows.map((r) => r["win"]).sort(), [WIN_A, WIN_B].sort(), "wire ids ride bare (braces stay live-side only)");

            // Genuine parse of the actual bound bytes, then the semantic edge
            // oracle (never a bare golden compare): inset 800x600 by 12 gives
            // 776x576 at (12,12); two leaves split 776 with one inner gap.
            const lastIndex = Math.max(...[...replies.keys()].filter((at) => (calls[at] as CapturedCall).method === "DescribeWorld"));
            const parsed = parseWorldReply(replies.get(lastIndex) as string);
            assert.ok(parsed !== null && isCoreWorldReply(parsed), "strict production decoder accepts the actual bound bytes");
            if (!isCoreWorldReply(parsed)) {
                return;
            }
            assert.equal(parsed.adapter, "test-adapter", "bound reply retains the producer adapter");
            assert.equal(parsed.gen, gen, "bound reply binds the serving generation");
            const frames = new Map(parsed.windows.map((w) => [w.win, w.frame] as const));
            const a = frames.get(WIN_A);
            const b = frames.get(WIN_B);
            assert.ok(a !== undefined && b !== undefined);
            const sorted = [a as [number, number, number, number], b as [number, number, number, number]].sort(
                (x, y) => x[0] - y[0],
            );
            assert.deepEqual(sorted[0], [12, 12, 384, 576]);
            assert.deepEqual(sorted[1], [404, 12, 384, 576]);
            assert.equal(sorted[1][0] - (sorted[0][0] + sorted[0][2]), 8);

            // Native-shaped apply/readback with the exact no-replay oracle.
            assert.ok(geometryWrites.some((w) => w.endsWith("@12,12,384,576")), `inset left edge written, got ${JSON.stringify(geometryWrites)}`);
            assert.ok(geometryWrites.some((w) => w.endsWith("@404,12,384,576")), `inset right edge written, got ${JSON.stringify(geometryWrites)}`);
            const want = [{ x: 12, y: 12, width: 384, height: 576 }, { x: 404, y: 12, width: 384, height: 576 }];
            assert.deepEqual([...live.values()].sort((x, y) => x.x - y.x), want, "live-native readback converges to the planned inset frames");
            const settledWrites = geometryWrites.length;
            await settle();
            assert.equal(geometryWrites.length, settledWrites, "converged frames stay silent: no churn, no replay");
            assert.deepEqual([...live.values()].sort((x, y) => x.x - y.x), want, "restored frames hold after settle with proven readback");
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

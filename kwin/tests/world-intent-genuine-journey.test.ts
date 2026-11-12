// Genuine retained intent journey at the KDE adapter boundary: the
// production `startWorldAdapterEntry` drives the real core through the
// test-only `raw_world_eval` engine and the `MiniBroker` transport shim (no
// hand-built replies anywhere on this path). A production `requestFloat`
// press files a real `toggle-float`, the draining capture publishes the
// genuine retained `intentDesired` save, the entry withholds every native
// apply until the exact durable write completes, later identical
// publications owe zero further writes, and the latest reply still carries
// the retained save. Offline only: scripted in-memory transport and
// storage, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { createWorldIntentStore } from "../src/world-intent-store";
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
            this.failAll(
                new Error(
                    `raw_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`,
                ),
            );
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
        } catch {
            // Held close: the bridge already exited.
        }
        await new Promise<void>((resolve) => {
            const timer = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch {
                    // Held close: the bridge already exited.
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

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

function makeSignal(): {
    handlers: Array<(payload?: unknown) => void>;
    signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
} {
    const handlers: Array<(payload?: unknown) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (payload?: unknown) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (payload?: unknown) => void): void => {
                const at = handlers.indexOf(handler);
                if (at >= 0) {
                    handlers.splice(at, 1);
                }
            },
        },
    };
}

function makeSurface(): {
    surface: Record<string, unknown>;
    wins: Array<Record<string, unknown>>;
    geometryWrites: string[];
    fireFrameChanged: (id: string) => void;
} {
    const ws1 = desk("ws1");
    const geometryWrites: string[] = [];
    const frameById = new Map<string, Array<(payload?: unknown) => void>>();
    const mkWin = (internalId: string, frame: { x: number; y: number; width: number; height: number }): Record<string, unknown> => {
        let current = { ...frame };
        const clientFor = (rect: { x: number; y: number; width: number; height: number }): unknown => ({
            x: rect.x + 2,
            y: rect.y + 21,
            width: Math.max(1, rect.width - 4),
            height: Math.max(1, rect.height - 22),
        });
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        frameById.set(internalId, winFrameChanged.handlers);
        return {
            internalId,
            normalWindow: true,
            managed: true,
            deleted: false,
            transient: false,
            transientFor: null,
            dialog: false,
            modal: false,
            wantsInput: true,
            output: { name: "out" },
            desktops: [ws1],
            minimized: false,
            hidden: false,
            fullscreen: false,
            fullScreenable: true,
            maximizeMode: 0,
            setMaximize: (): void => undefined,
            onAllDesktops: false,
            keepAbove: false,
            keepBelow: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234,
            resourceClass: "Example",
            caption: `Window ${internalId}`,
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
            outputChanged: outputChanged.signal,
            desktopsChanged: winDesktopsChanged.signal,
            hiddenChanged: winHiddenChanged.signal,
            frameGeometryChanged: winFrameChanged.signal,
            get frameGeometry(): unknown {
                return current;
            },
            set frameGeometry(rect: unknown) {
                const next = rect as { x: number; y: number; width: number; height: number };
                current = { ...next };
                geometryWrites.push(`${internalId}@${next.x},${next.y},${next.width},${next.height}`);
            },
            get clientGeometry(): unknown {
                return clientFor(current);
            },
            set clientGeometry(rect: unknown) {
                current = { ...(rect as { x: number; y: number; width: number; height: number }) };
            },
        };
    };
    const wins = [
        mkWin("win-a", { x: 0, y: 0, width: 200, height: 200 }),
        mkWin("win-b", { x: 300, y: 0, width: 200, height: 200 }),
    ];
    const world = {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out" },
        activeWindow: wins[0] as unknown,
    };
    const surface: Record<string, unknown> = {
        windowList: () => wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow(): unknown {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            world.activeWindow = ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => ws1,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return {
        surface,
        wins,
        geometryWrites,
        fireFrameChanged: (id: string): void => {
            // Genuine unchanged native observation: the geometry is
            // untouched, only the live KWin frame signal fires, exactly as
            // an external frame echo would. The entry invalidates and
            // re-observes; the real core republishes the retained save.
            const handlers = frameById.get(id);
            assert.ok(handlers !== undefined, `fake window ${id} exists`);
            for (const handler of [...(handlers as Array<(payload?: unknown) => void>)]) {
                handler();
            }
        },
    };
}

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
    done?: boolean;
}

interface Harness {
    bridge: WorldBridge;
    broker: MiniBroker;
    calls: Call[];
    timers: Array<{ delay: number; callback: () => void }>;
    logs: string[];
    writes: Array<{ payload: string; callback: (reply: unknown) => void }>;
    store: { records: Array<{ win: string; data: string }> };
    replies: string[];
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    runTimers: () => void;
    flushOne: () => Promise<{ bound: string; drained: boolean }>;
    observeAgain: () => Promise<string>;
    appliedCount: () => number;
    stop: () => Promise<void>;
}

async function makeHarness(): Promise<Harness> {
    const { surface, geometryWrites, fireFrameChanged } = makeSurface();
    void geometryWrites;
    const bridge = WorldBridge.start();
    const broker = createMiniBroker();
    const calls: Call[] = [];
    const timers: Array<{ delay: number; callback: () => void }> = [];
    const logs: string[] = [];
    const writes: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
    const store = { records: [] as Array<{ win: string; data: string }> };
    const replies: string[] = [];
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const timer = timers[i] as { delay: number; callback: () => void };
            if (timer.delay <= WORLD_DEBOUNCE_MS) {
                timers.splice(i, 1);
                i -= 1;
                timer.callback();
            }
        }
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            if (method === "ReadFloatIntent" || method === "WriteFloatIntent") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const corr = body["correlation_id"] as string;
                if (method === "ReadFloatIntent") {
                    callback(
                        JSON.stringify({
                            v: 2,
                            correlation_id: corr,
                            outcome: "ok",
                            records: store.records,
                            stored: store.records.length,
                            returned: store.records.length,
                        }),
                    );
                    return;
                }
                // Held by the test: the callback stays pending until the
                // journey explicitly answers it, so skip-issuing is visible.
                writes.push({ payload, callback });
                return;
            }
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                callback(broker.answerQueue(payload));
            } else if (method === "TakeCommandResult") {
                callback(broker.answerTake(payload));
            }
        },
        scheduleOnce: (delay, callback) => {
            timers.push({ delay, callback });
            return () => undefined;
        },
        log: (message) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
        floatIntent: true as const,
    });
    assert.ok(handle !== null, "world production entry starts on the fake surface");
    const flushOne = async (): Promise<{ bound: string; drained: boolean }> => {
        runTimers();
        const next = calls.find((call) => call.method === "DescribeWorld" && !call.done);
        assert.ok(next !== undefined, "expected a queued DescribeWorld dispatch to flush");
        next.done = true;
        const drainedBefore = broker.drained.length;
        const stamped = broker.stampCapture(next.payload);
        const raw = await bridge.send("DescribeWorld", stamped);
        assert.equal(
            (JSON.parse(raw) as Record<string, unknown>)["outcome"] ?? "accepted",
            "accepted",
            `engine accepts the capture: ${raw.slice(0, 300)}`,
        );
        const bound = broker.bindReply(raw, next.payload);
        replies.push(bound);
        next.callback(bound);
        runTimers();
        return { bound, drained: broker.drained.length > drainedBefore };
    };
    const observeAgain = async (): Promise<string> => {
        // Genuine unchanged re-observation while a persist flies (or after
        // convergence): a live frame echo invalidates, the entry
        // re-observes, and the real core republishes. Never hand-built.
        fireFrameChanged("win-a");
        return (await flushOne()).bound;
    };
    return {
        bridge,
        broker,
        calls,
        timers,
        logs,
        writes,
        store,
        replies,
        handle: handle as NonNullable<typeof handle>,
        runTimers,
        flushOne,
        observeAgain,
        appliedCount: () => logs.filter((line) => line.includes("omnitiler:world:applied")).length,
        stop: async () => {
            try {
                (handle as NonNullable<typeof handle>).stop();
            } catch {
                // Held stop: the entry already stopped.
            }
            await bridge.close();
        },
    };
}

function intentDesiredOf(bound: string): Array<{ win: string; data: string | null }> {
    const parsed = parseWorldReply(bound);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "bound reply parses as a core world reply");
    return parsed.intentDesired.map((row) => ({ win: row.win, data: row.data }));
}

describe("world genuine retained intent journey", () => {
    it("persists the exact engine save before native apply and owes nothing after", async () => {
        const h = await makeHarness();
        try {
            const handshake = h.calls.find((call) => call.method === "DiscoverWorld");
            assert.ok(handshake !== undefined, "entry discovers on start");
            const handshakeReply = await h.bridge.send("DiscoverWorld", handshake.payload);
            assert.equal(
                (JSON.parse(handshakeReply) as Record<string, unknown>)["type"],
                "handshake-reply",
                "engine answers the handshake",
            );
            handshake.callback(h.broker.bindDiscovery(handshakeReply));
            const settings = h.calls.find((call, index) => call.method === "DiscoverWorld" && index > 0);
            assert.ok(settings !== undefined, "settings follow the handshake");
            const settingsReply = await h.bridge.send("DiscoverWorld", settings.payload);
            assert.equal(
                (JSON.parse(settingsReply) as Record<string, unknown>)["status"],
                "accepted",
                "settings accepted",
            );
            settings.callback(settingsReply);
            // Seed capture: the genuine engine accepts with no transfers and
            // no retained desire on the untouched scene.
            const seedBound = (await h.flushOne()).bound;
            const seedParsed = parseWorldReply(seedBound);
            assert.ok(seedParsed !== null && isCoreWorldReply(seedParsed), "seed reply parses");
            assert.deepEqual(seedParsed.transfers, [], "seed capture is transfer-free");
            assert.deepEqual(intentDesiredOf(seedBound), [], "seed publishes no retained desire");
            // Settle the seed's follow-up observation flights before the
            // press: the trailing runTimers inside flushOne dispatches the
            // next commandless capture, which stays in flight until
            // answered and would fence the float press
            // (`world-apply-in-flight`). Flush every open capture until no
            // dispatch is outstanding, so the press files on a quiet wire.
            for (let round = 0; round < 8; round += 1) {
                h.runTimers();
                const open = h.calls.find((call) => call.method === "DescribeWorld" && !call.done);
                if (open === undefined) {
                    break;
                }
                const settledBound = (await h.flushOne()).bound;
                assert.deepEqual(intentDesiredOf(settledBound), [], "settle capture publishes no retained desire");
            }
            h.runTimers();
            assert.ok(
                h.calls.find((call) => call.method === "DescribeWorld" && !call.done) === undefined,
                "no capture in flight at the float press",
            );
            const appliedBefore = h.appliedCount();
            assert.equal(h.writes.length, 0, "seed owes no durable write");
            // Production float press files a real toggle-float through the
            // broker queue; the draining capture carries the genuine save.
            h.handle.requestFloat();
            h.runTimers();
            const queued = h.calls.filter((call) => call.method === "QueueWorldCommand");
            assert.equal(queued.length, 1, "float files exactly one queue command");
            assert.equal(
                ((JSON.parse(queued[0]?.payload as string) as Record<string, unknown>)["command"] as Record<string, unknown>)["op"],
                "toggle-float",
                "queued op is toggle-float",
            );
            // The draining capture carries the genuine save: flush through
            // any stale pre-queue observation until the broker drains the
            // filed toggle-float.
            let drainBound = "";
            for (let round = 0; round < 8; round += 1) {
                const flushed = await h.flushOne();
                if (flushed.drained) {
                    drainBound = flushed.bound;
                    break;
                }
            }
            assert.ok(drainBound !== "", "a capture drains the filed toggle-float");
            const drainDesired = intentDesiredOf(drainBound);
            assert.equal(drainDesired.length, 1, "drain publishes exactly one retained row");
            assert.equal(drainDesired[0]?.win, "win-a", "save targets the pressed window");
            const saved = drainDesired[0]?.data;
            assert.ok(typeof saved === "string" && saved.length > 0, "save carries opaque bytes");
            assert.equal(saved, '{"v":1,"float":true,"stickyPrior":null}', "genuine ordinary-float save spelling");
            // The exact durable write precedes every native apply: the write
            // payload carries the reply rows verbatim while the apply count
            // cannot move with the persist still unanswered.
            assert.equal(h.writes.length, 1, "drain issues exactly one durable write");
            const written = JSON.parse(h.writes[0]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(written["records"], [{ win: "win-a", data: saved }], "durable write equals the retained rows");
            assert.equal(h.appliedCount(), appliedBefore, "native apply waits for durable equality");
            // Two further observation publications keep the retained save
            // while the persist stays unanswered: still no native apply.
            // Each is a genuine unchanged re-observation: a live frame echo
            // invalidates, the entry re-observes, and the real core
            // republishes the retained save. The reconciler holds the reply
            // behind the flying write (never a second bus write).
            const secondBound = await h.observeAgain();
            assert.deepEqual(intentDesiredOf(secondBound), drainDesired, "second publication retains the save");
            assert.equal(h.appliedCount(), appliedBefore, "second publication still applies nothing");
            const thirdBound = await h.observeAgain();
            assert.deepEqual(intentDesiredOf(thirdBound), drainDesired, "third publication retains the save");
            assert.equal(h.appliedCount(), appliedBefore, "third publication still applies nothing");
            for (const pending of h.writes) {
                const body = JSON.parse(pending.payload) as Record<string, unknown>;
                assert.deepEqual(
                    body["records"],
                    [{ win: "win-a", data: saved }],
                    "every issued write spells the retained rows exactly",
                );
            }
            const writesOwed = h.writes.length;
            assert.ok(writesOwed >= 1, "at least the drain write is owed");
            // Answer every owed persist with the stored snapshot: the latest
            // retained desire converges and delivers exactly once.
            for (const pending of h.writes) {
                const body = JSON.parse(pending.payload) as Record<string, unknown>;
                const corr = body["correlation_id"] as string;
                pending.callback(
                    JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored", stored: 1 }),
                );
            }
            h.store.records = [{ win: "win-a", data: saved as string }];
            h.runTimers();
            assert.equal(h.appliedCount(), appliedBefore + 1, "durable equality delivers the latest reply once");
            // Later identical publications owe zero further writes.
            const writesSettled = h.writes.length;
            const fourthBound = await h.observeAgain();
            assert.deepEqual(intentDesiredOf(fourthBound), drainDesired, "post-converge publication retains the save");
            const fifthBound = await h.observeAgain();
            assert.deepEqual(intentDesiredOf(fifthBound), drainDesired, "next publication retains the save");
            assert.equal(h.writes.length, writesSettled, "converged desire owes zero further writes");
        } finally {
            await h.stop();
        }
    });

    it("withholds issuing plus two observations from a fresh consumer then saves exactly once", async () => {
        const h = await makeHarness();
        try {
            const handshake = h.calls.find((call) => call.method === "DiscoverWorld");
            assert.ok(handshake !== undefined, "entry discovers on start");
            const handshakeReply = await h.bridge.send("DiscoverWorld", handshake.payload);
            handshake.callback(h.broker.bindDiscovery(handshakeReply));
            const settings = h.calls.find((call, index) => call.method === "DiscoverWorld" && index > 0);
            assert.ok(settings !== undefined, "settings follow the handshake");
            const settingsReply = await h.bridge.send("DiscoverWorld", settings.payload);
            settings.callback(settingsReply);
            await h.flushOne();
            for (let round = 0; round < 8; round += 1) {
                h.runTimers();
                const open = h.calls.find((call) => call.method === "DescribeWorld" && !call.done);
                if (open === undefined) {
                    break;
                }
                await h.flushOne();
            }
            h.runTimers();
            assert.ok(
                h.calls.find((call) => call.method === "DescribeWorld" && !call.done) === undefined,
                "no capture in flight at the float press",
            );
            // Production float press through the broker queue; the raw
            // engine handles the original production captured report plus
            // the later commandless observations (no fabricated bytes).
            h.handle.requestFloat();
            h.runTimers();
            let issuing = "";
            for (let round = 0; round < 8; round += 1) {
                const flushed = await h.flushOne();
                if (flushed.drained) {
                    issuing = flushed.bound;
                    break;
                }
            }
            assert.ok(issuing !== "", "a capture drains the filed toggle-float");
            const issuingDesired = intentDesiredOf(issuing);
            assert.equal(issuingDesired.length, 1, "issuing publication carries the retained save");
            assert.ok(
                typeof issuingDesired[0]?.data === "string" && (issuingDesired[0]?.data as string).length > 0,
                "issuing save is a nonempty retained target",
            );
            const second = await h.observeAgain();
            const third = await h.observeAgain();
            assert.deepEqual(intentDesiredOf(second), issuingDesired, "second observation retains the save");
            assert.deepEqual(intentDesiredOf(third), issuingDesired, "third observation retains the save");
            // Skipped-publication proof setup: the fresh consumer sees
            // none of the three genuine publications yet.
            const withheld = [issuing, second, third];
            for (const bound of withheld) {
                const parsed = parseWorldReply(bound);
                assert.ok(parsed !== null && isCoreWorldReply(parsed), "withheld envelope stays genuine/strict-parsed");
            }
            const finalDesired = intentDesiredOf(third);
            assert.equal(finalDesired.length, 1, "final retained target is nonempty");
            // The fresh store binds the source final reply's real
            // generation as its transport/setDesired gen: actual binding,
            // never a remapped lifetime.
            const thirdParsed = parseWorldReply(third);
            assert.ok(thirdParsed !== null && isCoreWorldReply(thirdParsed), "final withheld envelope stays genuine/strict-parsed");
            const sourceGen = thirdParsed.gen;
            assert.ok(typeof sourceGen === "string" && sourceGen.length > 0, "final reply carries the source binding generation");
            const SKIPPED_BEFORE_FIRST_DELIVERY = withheld.length;
            assert.equal(SKIPPED_BEFORE_FIRST_DELIVERY, 3, "issuing plus two observations withheld");
            interface FreshCall { method: string; payload: string; callback: (reply: unknown) => void }
            const bus: FreshCall[] = [];
            const freshLogs: string[] = [];
            let freshGen: string | null = sourceGen;
            const fresh = createWorldIntentStore(
                {
                    route: { service: "svc", object: "/obj", iface: "iface" },
                    callDbus: (_s, _p, _i, method, payload, callback) => {
                        bus.push({ method, payload, callback });
                    },
                    scheduleOnce: () => () => undefined,
                    log: (line) => {
                        freshLogs.push(line);
                    },
                    isStopped: () => false,
                    getGeneration: () => freshGen,
                    onHydrated: () => undefined,
                },
                true,
            );
            fresh.seed(freshGen);
            const seedCall = bus.find((call) => call.method === "ReadFloatIntent");
            assert.ok(seedCall !== undefined, "fresh consumer seeds hydrated");
            const seedCorr = (JSON.parse(seedCall.payload) as Record<string, unknown>)["correlation_id"] as string;
            seedCall.callback(
                JSON.stringify({
                    v: 2, correlation_id: seedCorr, outcome: "ok", records: [], stored: 0, returned: 0,
                }),
            );
            assert.equal(fresh.isHydrated(freshGen), true, "fresh consumer hydrated empty");
            assert.equal(
                bus.filter((call) => call.method === "WriteFloatIntent").length,
                0,
                "zero store writes before the first delivery",
            );
            assert.equal(fresh.needsWrite(), false, "nothing owed before the first delivery");
            void freshLogs;
            // First delivery to the fresh consumer: ONLY the final
            // retained target. Never warmed with the issuing reply.
            const firstChanged = fresh.setDesired(finalDesired, freshGen);
            assert.equal(firstChanged, true, "first delivery of the final target registers");
            assert.equal(fresh.needsWrite(), true, "final target differs from empty durable");
            let settled: boolean | null = null;
            fresh.persist((ok) => {
                settled = ok;
            });
            const writes = bus.filter((call) => call.method === "WriteFloatIntent");
            assert.equal(writes.length, 1, "first delivery issues exactly one store write");
            const written = JSON.parse(writes[0]?.payload as string) as Record<string, unknown>;
            assert.deepEqual(
                written["records"],
                [{ win: "win-a", data: finalDesired[0]?.data }],
                "store write equals the final retained rows exactly",
            );
            const writeCorr = written["correlation_id"] as string;
            writes[0]?.callback(
                JSON.stringify({ v: 2, correlation_id: writeCorr, outcome: "stored", stored: 1 }),
            );
            assert.equal(settled, true, "covering write stores exactly once");
            assert.deepEqual(
                fresh.recordsForReport(new Set(["win-a"])),
                [{ win: "win-a", data: finalDesired[0]?.data }],
                "durable storage holds the final retained row",
            );
            assert.equal(fresh.needsWrite(), false, "durable equality after the single write");
            // Redelivery of the identical final target owes nothing further.
            assert.equal(fresh.setDesired(finalDesired, freshGen), false, "identical redelivery changes nothing");
            assert.equal(fresh.needsWrite(), false, "equality still owes zero writes");
            assert.equal(
                bus.filter((call) => call.method === "WriteFloatIntent").length,
                1,
                "no redundant second store write",
            );
        } finally {
            await h.stop();
        }
    });
});

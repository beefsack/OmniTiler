// Disjoint migration family recorder: replays the
// `test-fixtures/kde-v4-active/migration` input sequence through the
// production entry plus the genuine `raw_world_eval` engine and the
// test-only `MiniBroker` shim: handshake/settings, seed capture,
// migrate-directional queue, drain capture, take. Writes active equivalents
// to an explicit fresh OUTDIR outside the repo. Pure exported module with no
// main side effect (safe to import from test bundles); the standalone CLI
// entry is `world-migration-record-cli.ts`.
//
// Run through the declared development environment, with fresh output paths:
//   cargo build --offline -p omnitiler --example raw_world_eval
//   kwin/node_modules/.bin/esbuild kwin/tests/world-migration-record-cli.ts --bundle --platform=node --format=cjs --target=es2020 --outfile="$RECORDER_JS"
//   node "$RECORDER_JS" --out-dir "$OUT_DIR" [--bridge-binary "$BRIDGE_BIN"]
// BRIDGE_BIN follows Cargo's configured target directory. An existing output
// directory refuses instead of overwriting.
//
// Volatile normalization (mechanical, world-bytes precedent): only the exact
// per-run entry `source` nonce and its bound `req` echo are rewritten to the
// stable `src-a` token in the capture files via exact quoted-string
// replacement (no JSON re-serialization; inner report/reply bytes
// untouched). `gen` (`world-eval-0`), `nativeInvalid`, `obs`/`rev`/`clock`
// are deterministic across runs.

import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { startWorldAdapterEntry, WORLD_DEBOUNCE_MS } from "../src/world-entry";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
import { createMiniBroker } from "./world-wire-harness";

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

export function defaultMigrationBridgeBinary(): string {
    return join(repoRoot(), "target", "debug", "examples", "raw_world_eval");
}

export interface MigrationRecordOptions {
    readonly outDir: string;
    readonly bridgeBinary?: string;
}

export interface MigrationRecordResult {
    readonly files: number;
}

function makeSignal(): {
    signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
} {
    const handlers = new Set<(payload?: unknown) => void>();
    return {
        signal: {
            connect: (handler: (payload?: unknown) => void): void => {
                handlers.add(handler);
            },
            disconnect: (handler: (payload?: unknown) => void): void => {
                handlers.delete(handler);
            },
        },
    };
}

function makeSurface(): {
    windowList: () => unknown[];
    screens: unknown[];
    desktops: unknown[];
    activeScreen: { name: string };
    activeWindow: unknown;
    cursorPos: { x: number; y: number };
    clientArea: () => { x: number; y: number; width: number; height: number };
    currentDesktopForScreen: (screen: { name: string }) => unknown;
    setCurrentDesktopForScreen: () => undefined;
    sendClientToScreen: () => undefined;
    createDesktop: () => unknown;
    removeDesktop: () => undefined;
    windowAdded: { connect: (h: (p?: unknown) => void) => void; disconnect: (h: (p?: unknown) => void) => void };
    windowRemoved: { connect: (h: (p?: unknown) => void) => void; disconnect: (h: (p?: unknown) => void) => void };
} {
    // Synthetic offline producer mirroring the historical migration
    // topology: out-a/wa1 holds win-a, out-b/wb1 holds win-b, focused
    // win-a. Rects match the historical report facts.
    const wa1 = { id: "wa1", name: "Workspace 1" };
    const wb1 = { id: "wb1", name: "Workspace 2" };
    const windowAdded = makeSignal();
    const windowRemoved = makeSignal();
    const mkWin = (
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
        out: string,
        desk: object,
        caption: string,
    ): object => ({
        internalId,
        normalWindow: true,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        frameGeometry: { ...frame },
        clientGeometry: { ...frame },
        output: { name: out },
        desktops: [desk],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
        pid: 1234,
        resourceClass: "Example",
        caption,
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
    });
    const wins = [
        mkWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-a", wa1, "Window win-a"),
        mkWin("win-b", { x: 800, y: 0, width: 100, height: 100 }, "out-b", wb1, "Window win-b"),
    ];
    const currentByScreen = new Map([
        ["out-a", wa1],
        ["out-b", wb1],
    ]);
    return {
        windowList: () => wins,
        screens: [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ],
        desktops: [wa1, wb1],
        activeScreen: { name: "out-a" },
        activeWindow: wins[0] as unknown,
        cursorPos: { x: 2, y: 100 },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
        windowAdded: windowAdded.signal,
        windowRemoved: windowRemoved.signal,
    };
}

class Bridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{ resolve: (reply: string) => void; reject: (error: Error) => void }> = [];

    private constructor(proc: ChildProcess, binary: string) {
        this.proc = proc;
        createInterface({ input: proc.stdout as NodeJS.ReadableStream }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.on("error", (error: unknown) => {
            const failure = error instanceof Error ? error : new Error(String(error));
            while (this.waiters.length > 0) {
                const waiter = this.waiters.shift();
                waiter?.reject(failure);
            }
        });
        proc.on("close", (code, signal) => {
            const failure = new Error(`${binary} exited code=${String(code)} signal=${String(signal)}`);
            while (this.waiters.length > 0) {
                const waiter = this.waiters.shift();
                waiter?.reject(failure);
            }
        });
    }

    static start(binary: string): Bridge {
        return new Bridge(spawn(binary, [], { cwd: repoRoot(), stdio: ["pipe", "pipe", "pipe"] }), binary);
    }

    send(method: string, payload: string): Promise<string> {
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

function assertRecord(condition: unknown, message: string): asserts condition {
    if (!condition) {
        throw new Error(`producer assertion failed: ${message}`);
    }
}

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
    done?: boolean;
}

export async function recordMigrationFamily(options: MigrationRecordOptions): Promise<MigrationRecordResult> {
    if (existsSync(options.outDir)) {
        throw new Error(`refusing to overwrite existing output directory: ${options.outDir}`);
    }
    const bridge = Bridge.start(options.bridgeBinary ?? defaultMigrationBridgeBinary());
    const broker = createMiniBroker();
    const calls: Call[] = [];
    const timers: Array<{ delay: number; callback: () => void }> = [];
    const takePayloads: string[] = [];
    const takeReplies: string[] = [];
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
        workspace: makeSurface(),
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                callback(broker.answerQueue(payload));
            } else if (method === "TakeCommandResult") {
                takePayloads.push(payload);
                const reply = broker.answerTake(payload);
                takeReplies.push(reply);
                callback(reply);
            }
        },
        scheduleOnce: (delay, callback) => {
            timers.push({ delay, callback });
            return () => undefined;
        },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assertRecord(handle !== null, "entry starts on the synthetic surface");
    // Each flushed DescribeWorld retains its envelope plus the bound reply.
    const flushed: Array<{ envelope: string; bound: string }> = [];
    const flushDescribe = async (): Promise<void> => {
        for (let round = 0; round < 16; round += 1) {
            runTimers();
            const next = calls.find((call) => call.method === "DescribeWorld" && !call.done);
            if (next === undefined) {
                return;
            }
            next.done = true;
            const stamped = broker.stampCapture(next.payload);
            const raw = await bridge.send("DescribeWorld", stamped);
            assertRecord(
                (JSON.parse(raw) as Record<string, unknown>)["outcome"] !== "rejected",
                `engine accepts capture: ${raw.slice(0, 300)}`,
            );
            const bound = broker.bindReply(raw, next.payload);
            flushed.push({ envelope: next.payload, bound });
            next.callback(bound);
            runTimers();
        }
        throw new Error("flush did not converge");
    };
    try {
        const handshake = calls.find((call) => call.method === "DiscoverWorld");
        assertRecord(handshake !== undefined, "entry discovers on start");
        const handshakeRequest = (handshake as Call).payload;
        const handshakeReply = await bridge.send("DiscoverWorld", (handshake as Call).payload);
        assertRecord(
            (JSON.parse(handshakeReply) as Record<string, unknown>)["type"] === "handshake-reply",
            "engine answers handshake",
        );
        (handshake as Call).callback(broker.bindDiscovery(handshakeReply));
        const settings = calls.find((call, index) => call.method === "DiscoverWorld" && index > 0);
        assertRecord(settings !== undefined, "settings follow");
        const settingsRequest = (settings as Call).payload;
        const settingsReply = await bridge.send("DiscoverWorld", (settings as Call).payload);
        assertRecord(
            (JSON.parse(settingsReply) as Record<string, unknown>)["status"] === "accepted",
            "settings accepted",
        );
        (settings as Call).callback(settingsReply);
        runTimers();
        await flushDescribe();
        assertRecord(flushed.length >= 1, "seed capture emitted");
        const seed = flushed[0] as { envelope: string; bound: string };
        const seedParsed = parseWorldReply(seed.bound);
        assertRecord(seedParsed !== null && isCoreWorldReply(seedParsed), "seed reply parses");
        assertRecord(seedParsed.transfers.length === 0, "seed capture is transfer-free");
        handle.requestWorkspaceMigrate("right");
        runTimers();
        const queue = calls.find((call) => call.method === "QueueWorldCommand");
        assertRecord(queue !== undefined, "migration files a queue command");
        const queueRequest = (queue as Call).payload;
        const queued = JSON.parse(queueRequest) as Record<string, unknown>;
        assertRecord(
            (queued["command"] as Record<string, unknown>)["op"] === "migrate-directional",
            "queued op is migrate-directional",
        );
        await flushDescribe();
        assertRecord(flushed.length >= 2, "drain capture emitted");
        const drain = flushed[flushed.length - 1] as { envelope: string; bound: string };
        const drainParsed = parseWorldReply(drain.bound);
        assertRecord(drainParsed !== null && isCoreWorldReply(drainParsed), "drain reply parses");
        const kinds = drainParsed.transfers.map((transfer) => transfer.kind).sort();
        assertRecord(kinds.includes("migration"), `drain retains the migration transfer: ${JSON.stringify(kinds)}`);
        assertRecord(drainParsed.result === null, "actuator-lane result stays null");
        assertRecord(takePayloads.length >= 1, "entry takes the terminal result");
        const takeRequest = takePayloads[takePayloads.length - 1] as string;
        const takeReply = JSON.parse(takeReplies[takeReplies.length - 1] as string) as Record<string, unknown>;
        assertRecord(
            takeReply["outcome"] === "ready",
            `take collects the terminal result: ${JSON.stringify(takeReply).slice(0, 200)}`,
        );
        mkdirSync(options.outDir, { recursive: true });
        const pending: Array<[string, string]> = [
            ["handshake.request.json", handshakeRequest],
            ["settings.request.json", settingsRequest],
            ["capture-seed-1.envelope.json", seed.envelope],
            ["capture-seed-1.reply.json", seed.bound],
            ["queue.request.json", queueRequest],
            ["capture-drain.envelope.json", drain.envelope],
            ["capture-drain.reply.json", drain.bound],
            ["MIGRATE_DRAIN.reply.json", drain.bound],
            ["take.request.json", takeRequest],
            ["MIGRATE_TAKE.result.json", JSON.stringify((takeReply["result"] as unknown) ?? null)],
        ];
        // Volatile normalization: rewrite only the exact per-run entry
        // source nonce (quoted, so no substring of other content can match)
        // to the stable token. Inner report/reply bytes ride untouched.
        const nonces = new Set<string>();
        for (const [name, bytes] of pending) {
            if (name.endsWith(".envelope.json")) {
                const source = (JSON.parse(bytes) as Record<string, unknown>)["source"];
                if (typeof source === "string" && source !== "") {
                    nonces.add(JSON.stringify(source));
                }
            }
        }
        let count = 0;
        for (const [name, bytes] of pending) {
            let out = bytes;
            if (name.includes("capture-") || name === "MIGRATE_DRAIN.reply.json") {
                for (const nonce of nonces) {
                    out = out.split(nonce).join(JSON.stringify("src-a"));
                }
            }
            writeFileSync(join(options.outDir, name), out);
            count += 1;
        }
        return { files: count };
    } finally {
        handle.stop();
        await bridge.close();
    }
}

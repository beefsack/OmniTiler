// Stage 5 v4 recording harness: drives the real `startWorldAdapterEntry`
// facts/gesture lanes through `broker_world_eval` (`DiscoverWorld`
// handshake+settings, `DescribeWorld` captures, `ReadWorldPresentation`
// reads; exactly those three methods) and writes the emitted v4 envelopes
// plus the read projections to an explicit fresh OUTDIR outside the repo.
// Native fakes are scripted only: a KWin surface with dynamic
// frame/client readbacks (zero client inset, no fake policy), scripted
// effect-facts bytes, and scripted gesture waiter answers; every broker
// byte is authentic. Covers the full chain
// (p5,p6,cover,preview,cover2,a2,a3,preview-b) and the s2 chain (fresh
// service: p5,p6,cover,preview,s2). Allowed volatile normalization for
// later fixture comparison: gen, source, sourceSeq, nativeInvalid, req.
// No fact/revision/witness rewriting. Pure exported module with no main
// side effect (safe to import from test bundles); the standalone CLI entry
// is `world-stage5-v4-record-cli.ts`.
//
// Run through the declared development environment, with fresh output paths:
//   cargo build --locked -p omnitiler --example broker_world_eval
//   kwin/node_modules/.bin/esbuild kwin/tests/world-stage5-v4-record-cli.ts --bundle --platform=node --format=cjs --target=es2020 --define:__OMNITILER_TRACE__='"1"' --outfile="$RECORDER_JS"
//   node "$RECORDER_JS" --out-dir "$OUT_DIR" --chain both --bridge-binary "$BRIDGE_BIN"
// BRIDGE_BIN follows Cargo's configured target directory. Existing per-chain
// output directories refuse instead of overwriting. Scheduling-only seam: the
// first capture send waits one bounded 100ms so the broker-epoch creation
// receipt stamps solidly positive (see the pump seam note); every byte stays
// genuine.

import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { createWorldPresentationSubscriber } from "../src/world-presentation";
import { isCanonicalU64 } from "../src/world-seq";
import {
    isCoreWorldReply,
    isWorldPresentationReply,
    isWorldRejection,
    parseWorldReply,
} from "../src/world-report";

export const STAGE5_V4_B_INSTANCE = "9d4e6a2c-1b3f-4d8e-9f0a-2c5e7a1b3d4f";
export const STAGE5_V4_GESTURE_A = "gesture-1";
export const STAGE5_V4_ADAPTER = "kwin-stage5-v4";
// Allowed volatile normalization for later fixture comparison. Recorded
// bytes keep their raw values; only these tokens may be normalized.
export const STAGE5_V4_VOLATILE_TOKENS = ["gen", "source", "sourceSeq", "nativeInvalid", "req"] as const;

export type Stage5V4ChainName = "full" | "s2";

export type Stage5V4StepAction =
    | { readonly facts: "seed" | "cover" | "inv2" | "inv3" }
    | { readonly gesture: { readonly instance: string; readonly serial: string; readonly phase: "updated" | "idle" } }
    | { readonly fireSignal: { readonly win: string; readonly signal: "frameGeometryChanged" } };

export interface Stage5V4Step {
    readonly name: string;
    readonly actions: ReadonlyArray<Stage5V4StepAction>;
}

// Discriminator (source evidence, no core changes): the entry retains one
// gesture snapshot across captures (`gestureSnapshot` installs in
// `onGestureFactsReply` and only released/cancelled/idle/unavailable/
// malformed/host-timeout clear it), superseded waiter answers are dropped
// with no dispatch, and the broker treats idle/unavailable/superseded as
// omission lanes that never mutate the gesture map while the mapping
// persists (`kde_gesture.rs` normalize: omission returns None, map
// untouched). A released/cancelled phase instead commits a terminal that
// settles and can retire the identity, after which the identical
// instance/serial risks replay treatment. The legacy cover2 is the
// coverage/current A drain: witness 2 with no gesture while A stays live.
// Rendering it with released/idle-ack retires A and breaks the drain, so
// cover2 answers the waiter idle (omission: no commit, mapping persists,
// A recovers in a2 with the identical instance/serial and frozen start
// facts), and no step in either chain ever sends released/cancelled.
export const STAGE5_V4_FULL_STEPS: ReadonlyArray<Stage5V4Step> = [
    { name: "p5", actions: [{ facts: "seed" }] },
    // p5 vs p6 legacy envelopes carry byte-identical reports (only
    // source/sourceSeq/nativeInvalid advance): p6 re-fires the native
    // frame-geometry signal with unchanged facts for a genuine second
    // capture over the same witness, never a duplicate seed answer.
    { name: "p6", actions: [{ fireSignal: { win: "win-a", signal: "frameGeometryChanged" } }] },
    { name: "cover", actions: [{ facts: "cover" }] },
    { name: "preview", actions: [{ gesture: { instance: STAGE5_V4_GESTURE_A, serial: "1", phase: "updated" } }] },
    {
        name: "cover2",
        actions: [
            { gesture: { instance: STAGE5_V4_GESTURE_A, serial: "0", phase: "idle" } },
            { facts: "inv2" },
        ],
    },
    // a2 recovers the identical A/1 (same frozen start facts, same serial)
    // over witness 2: the coverage-current drain. No new facts answer: the
    // gesture update itself dispatches the drain capture.
    {
        name: "a2",
        actions: [
            { gesture: { instance: STAGE5_V4_GESTURE_A, serial: "1", phase: "updated" } },
        ],
    },
    { name: "a3", actions: [{ facts: "inv3" }] },
    { name: "preview-b", actions: [{ gesture: { instance: STAGE5_V4_B_INSTANCE, serial: "1", phase: "updated" } }] },
];

export const STAGE5_V4_S2_STEPS: ReadonlyArray<Stage5V4Step> = [
    { name: "p5", actions: [{ facts: "seed" }] },
    { name: "p6", actions: [{ fireSignal: { win: "win-a", signal: "frameGeometryChanged" } }] },
    { name: "cover", actions: [{ facts: "cover" }] },
    { name: "preview", actions: [{ gesture: { instance: STAGE5_V4_GESTURE_A, serial: "1", phase: "updated" } }] },
    // s2 advances the witness to inv2 at the same causal boundary that opens
    // same-source serial 2 (legacy envelope-s2: witness counter 2 with
    // gesture-1 serial 2).
    {
        name: "s2",
        actions: [
            { facts: "inv2" },
            { gesture: { instance: STAGE5_V4_GESTURE_A, serial: "2", phase: "updated" } },
        ],
    },
];

// Expected drop-preview producer per step (null: no preview must paint).
// Mirrors the legacy scenario: preview/a2/a3/preview-b/s2 paint with
// their producing identity; p5/p6/cover/cover2 paint none.
export const STAGE5_V4_PREVIEW_EXPECTATION: Readonly<Record<string, { readonly instance: string; readonly serial: string } | null>> = {
    p5: null,
    p6: null,
    cover: null,
    preview: { instance: STAGE5_V4_GESTURE_A, serial: "1" },
    cover2: null,
    a2: { instance: STAGE5_V4_GESTURE_A, serial: "1" },
    a3: { instance: STAGE5_V4_GESTURE_A, serial: "1" },
    "preview-b": { instance: STAGE5_V4_B_INSTANCE, serial: "1" },
    s2: { instance: STAGE5_V4_GESTURE_A, serial: "2" },
};

export interface Stage5V4RecordOptions {
    readonly chain: Stage5V4ChainName;
    readonly bridgeBinary?: string;
    readonly repoRoot?: string;
}

export interface Stage5V4RecordedExchange {
    readonly step: string;
    readonly file: string;
    readonly bytes: string;
}

export interface Stage5V4RecordedDispatch extends Stage5V4RecordedExchange {
    readonly gestureInstance: string | null;
    readonly gestureSerial: string | null;
}

// Live gesture identity for dispatch rendering (coreId never rides the
// dispatch bytes; the subscriber only compares instance/serial). Values
// match the fixture-test constants so regenerated dispatches stay
// comparable.
export function dispatchLiveGesture(
    step: string,
    variant: "normal" | "crossed-b" = "normal",
): { readonly instance: string; readonly serial: string; readonly coreId: string } | null {
    if (variant === "crossed-b") {
        return { instance: STAGE5_V4_B_INSTANCE, serial: "1", coreId: "2" };
    }
    const expected = STAGE5_V4_PREVIEW_EXPECTATION[step] ?? null;
    if (expected === null) {
        return null;
    }
    if (expected.instance === STAGE5_V4_B_INSTANCE) {
        return { instance: STAGE5_V4_B_INSTANCE, serial: "1", coreId: "2" };
    }
    if (expected.serial === "2") {
        return { instance: STAGE5_V4_GESTURE_A, serial: "2", coreId: "9" };
    }
    return { instance: STAGE5_V4_GESTURE_A, serial: "1", coreId: "1" };
}

// Shared production-subscriber dispatch renderer: replays one genuine
// broker projection through the real `createWorldPresentationSubscriber`
// (req re-keyed, no direct formatter calls) and returns the exact
// `SetPresentation` dispatch bytes. Used by the recorder and reused by the
// fixture tests so both prove the same route.
export function renderPresentationDispatch(
    projectionBytes: string,
    invalidation: { readonly instance: string; readonly counter: string },
    gesture: { readonly instance: string; readonly serial: string; readonly coreId: string } | null,
): string {
    const proj = JSON.parse(projectionBytes) as { gen: unknown };
    if (typeof proj.gen !== "string") {
        throw new Error("projection carries no gen");
    }
    const pending: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
    const paints: string[] = [];
    const logs: string[] = [];
    const sub = createWorldPresentationSubscriber({
        callDbus: (_s, _p, _i, _m, payload, callback) => {
            pending.push({ payload, callback });
        },
        getGeneration: () => proj.gen as string,
        getInvalidation: () => ({ instance: invalidation.instance, counter: invalidation.counter }),
        getGestureIdentity: () => gesture,
        isStopped: () => false,
        log: (m) => { logs.push(m); },
        scheduleOnce: () => () => undefined,
        renderer: {
            renderPresentation: (payload) => {
                paints.push(payload);
                return true;
            },
            hidePresentation: () => undefined,
            clearPresentation: () => undefined,
        },
    });
    sub.notifyGeneration(proj.gen as string);
    if (pending.length !== 1) {
        throw new Error(`dispatch render expected one read, got ${pending.length}`);
    }
    const call = pending.splice(0)[0] as { payload: string; callback: (reply: unknown) => void };
    const issued = (JSON.parse(call.payload) as { req: string }).req;
    const recorded = JSON.parse(projectionBytes) as Record<string, unknown>;
    recorded["req"] = issued;
    call.callback(JSON.stringify(recorded));
    sub.stop();
    if (paints.length !== 1) {
        throw new Error(`dispatch render expected one paint, got ${paints.length} logs=${logs.join("; ")}`);
    }
    return paints[0] as string;
}

export interface Stage5V4ChainResult {
    readonly chain: Stage5V4ChainName;
    readonly adapter: string;
    readonly gen: string;
    readonly discovery: { readonly handshakeRequest: string; readonly handshakeReply: string; readonly settingsRequest: string; readonly settingsReply: string };
    readonly envelopes: ReadonlyArray<Stage5V4RecordedExchange>;
    readonly replies: ReadonlyArray<Stage5V4RecordedExchange>;
    readonly projections: ReadonlyArray<Stage5V4RecordedExchange>;
    readonly dispatches: ReadonlyArray<Stage5V4RecordedDispatch>;
    readonly factsInputs: ReadonlyArray<Stage5V4RecordedExchange>;
    readonly gestureInputs: ReadonlyArray<Stage5V4RecordedExchange>;
    readonly internalReads: number;
    readonly parkedWaits: number;
    readonly volatile: ReadonlyArray<string>;
}

function repoRootOf(startDir: string): string {
    let dir = resolve(startDir);
    for (let depth = 0; depth < 6; depth += 1) {
        if (existsSync(join(dir, "Cargo.toml"))) {
            return dir;
        }
        dir = dirname(dir);
    }
    throw new Error("repo root (Cargo.toml) not found above " + startDir);
}

function fixtureBytes(root: string, name: string): string {
    return readFileSync(join(root, "test-fixtures", "stage5", name), "utf8");
}

function gestureBody(instance: string, serial: string, phase: "updated" | "idle"): string {
    if (phase === "idle") {
        return JSON.stringify({
            v: 1, instance, serial: "0", revision: "1", phase: "idle", win: "", kind: "",
            startRect: null, startCursor: null, cursor: null,
            modalMove: 0, changed: 1, superseded: 0, available: 1,
        });
    }
    return JSON.stringify({
        v: 1, instance, serial, revision: "1", phase, win: "win-a", kind: "move",
        startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [150, 50],
        modalMove: 1, changed: 1, superseded: 0, available: 1,
    });
}

function idleAckFor(requestPayload: string): string {
    let instance = STAGE5_V4_GESTURE_A;
    try {
        const parsed = JSON.parse(requestPayload) as Record<string, unknown>;
        if (typeof parsed["instance"] === "string" && (parsed["instance"] as string) !== "") {
            instance = parsed["instance"] as string;
        }
    } catch (error) {
        void error;
    }
    return gestureBody(instance, "0", "idle");
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

class BrokerBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{ resolve: (reply: string) => void; reject: (error: Error) => void }> = [];
    private dead: string | null = null;
    private stderrTail = "";

    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("broker_world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            const waiter = this.waiters.shift();
            if (waiter !== undefined) {
                waiter.resolve(line);
            }
        });
        proc.stderr?.on("data", (chunk: unknown) => {
            this.stderrTail = (this.stderrTail + String(chunk)).slice(-8192);
        });
        proc.on("error", (error) => {
            this.failAll(error instanceof Error ? error : new Error(String(error)));
        });
        proc.on("close", (code, signal) => {
            this.failAll(new Error(`broker_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
        });
    }

    static start(repoRoot: string, bridgeBinary?: string): BrokerBridge {
        if (bridgeBinary !== undefined && bridgeBinary !== "") {
            return new BrokerBridge(
                spawn(bridgeBinary, [], { cwd: repoRoot, stdio: ["pipe", "pipe", "pipe"] }),
            );
        }
        return new BrokerBridge(
            spawn("cargo", ["run", "--locked", "--quiet", "-p", "omnitiler", "--example", "broker_world_eval"], {
                cwd: repoRoot,
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    private failAll(error: Error): void {
        if (this.dead === null) {
            this.dead = error.message;
        }
        while (this.waiters.length > 0) {
            const waiter = this.waiters.shift();
            if (waiter !== undefined) {
                waiter.reject(error);
            }
        }
    }

    send(method: string, body: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return Promise.reject(new Error("broker_world_eval process already exited"));
        }
        const line = JSON.stringify({ method, body });
        const reply = new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(line + "\n");
            } catch (error) {
                reject(error instanceof Error ? error : new Error(String(error)));
            }
        });
        const timeout = new Promise<string>((_resolve, reject) => {
            const timer = setTimeout(() => {
                reject(new Error(`broker_world_eval send timed out method=${method}`));
            }, 120000);
            void reply.then(
                () => clearTimeout(timer),
                () => clearTimeout(timer),
            );
        });
        return Promise.race([reply, timeout]);
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

interface InterceptedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
    answered: boolean;
}

function asRecord(value: unknown): Record<string, unknown> {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        throw new Error(`expected a JSON object, got ${typeof value}`);
    }
    return value as Record<string, unknown>;
}

export async function recordStage5V4Chain(options: Stage5V4RecordOptions): Promise<Stage5V4ChainResult> {
    const root = options.repoRoot ?? repoRootOf(process.cwd());
    const steps = options.chain === "full" ? STAGE5_V4_FULL_STEPS : STAGE5_V4_S2_STEPS;
    const factsSeed = fixtureBytes(root, "facts-seed.json");
    const factsCover = fixtureBytes(root, "facts-cover.json");
    const factsInv2 = fixtureBytes(root, "facts-inv2.json");
    const factsInv3 = fixtureBytes(root, "facts-inv3.json");
    const factsFor = (key: "seed" | "cover" | "inv2" | "inv3"): string => {
        if (key === "seed") {
            return factsSeed;
        }
        if (key === "cover") {
            return factsCover;
        }
        if (key === "inv2") {
            return factsInv2;
        }
        return factsInv3;
    };

    const bridge = BrokerBridge.start(root, options.bridgeBinary);
    const desk = { id: "550e8400-e29b-41d4-a716-446655440001", name: "Workspace 1" };
    const outName = { name: "DP-1" };
    // Truthful dynamic native readbacks (zero client inset seam: the client
    // rect tracks the frame rect exactly, no fake policy). Entry frame
    // writes land through the setters so follow-up captures read back the
    // applied geometry instead of diverging static objects.
    const winSignals: Record<string, Record<string, FakeSignal>> = {};
    const mkWin = (
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
        pid: number,
        resourceClass: string,
    ): Record<string, unknown> => {
        let frameRect = { ...frame };
        let clientRect = { ...frame };
        const outputChanged = makeSignal();
        const desktopsChanged = makeSignal();
        const hiddenChanged = makeSignal();
        const frameGeometryChanged = makeSignal();
        winSignals[internalId] = { outputChanged, desktopsChanged, hiddenChanged, frameGeometryChanged };
        const win: Record<string, unknown> = {
            internalId,
            normalWindow: true,
            managed: true,
            deleted: false,
            transient: false,
            transientFor: null,
            dialog: false,
            modal: false,
            wantsInput: true,
            output: { ...outName },
            desktops: [{ ...desk }],
            minimized: false,
            hidden: false,
            fullScreen: false,
            fullscreen: false,
            maximizeMode: 0,
            maximized: false,
            sticky: false,
            onAllDesktops: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid,
            resourceClass,
            caption: `Window ${internalId}`,
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
            keepAbove: false,
            keepBelow: false,
            outputChanged: outputChanged.signal,
            desktopsChanged: desktopsChanged.signal,
            hiddenChanged: hiddenChanged.signal,
            frameGeometryChanged: frameGeometryChanged.signal,
        };
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            enumerable: true,
            get: () => ({ ...frameRect }),
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frameRect = { ...rect };
                clientRect = { ...rect };
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            enumerable: true,
            get: () => ({ ...clientRect }),
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                clientRect = { ...rect };
            },
        });
        return win;
    };
    const wins = [
        mkWin("win-a", { x: 0, y: 0, width: 200, height: 200 }, 1234, "Example"),
        mkWin("win-b", { x: 300, y: 0, width: 200, height: 200 }, 1235, "Example"),
    ];
    const winA = wins[0] as Record<string, unknown>;
    const signals: Record<string, FakeSignal> = {};
    for (const name of ["windowAdded", "windowRemoved", "windowActivated", "desktopsChanged", "screensChanged", "currentDesktopChanged", "activeScreenChanged"] as const) {
        signals[name] = makeSignal();
    }
    // Live native inventory: the entry's same-turn id-delta readback
    // proves creation against this exact list, so it must grow on a
    // fulfilled createDesktop (a static list would fake an ambiguous
    // no-delta receipt forever).
    const nativeDesktops: Array<{ id: string; name: string }> = [desk];

    const calls: InterceptedCall[] = [];
    const effectCalls: InterceptedCall[] = [];
    const gestureCalls: InterceptedCall[] = [];
    // Parked B1 waits: `WaitWorldWake` is the entry's legitimate held
    // native-event seam (one outstanding long-held wait per generation),
    // not broker traffic. The harness parks it without forwarding (the
    // bridge has no wait route; host-deadline timers never fire here), so
    // the entry never sees a transport failure for the expected wait.
    const parkedWaits: InterceptedCall[] = [];
    const timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }> = [];
    const logs: string[] = [];
    const envelopes: Stage5V4RecordedExchange[] = [];
    const replies: Stage5V4RecordedExchange[] = [];
    const projections: Stage5V4RecordedExchange[] = [];
    const dispatches: Stage5V4RecordedDispatch[] = [];
    const factsInputs: Stage5V4RecordedExchange[] = [];
    const gestureInputs: Stage5V4RecordedExchange[] = [];
    let internalReads = 0;
    let readCounter = 0;
    let activeStep = "genesis";
    let latestWitness: { instance: string; counter: string } | null = null;

    const surface: Record<string, unknown> = {
        windowList: (): unknown[] => wins,
        screens: [{ name: "DP-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: nativeDesktops,
        activeScreen: { ...outName },
        cursorPos: { x: 150, y: 50 },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        get activeWindow(): unknown {
            return winA;
        },
        clientArea: (): Record<string, number> => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktop: { ...desk },
        currentDesktopForScreen: (): unknown => ({ ...desk }),
        setCurrentDesktopForScreen: (): void => undefined,
        sendClientToScreen: (): void => undefined,
        // Truthful fulfilling fake (host fidelity, not product/oracle
        // behavior): KWin's void `createDesktop(position, name)` makes one
        // synchronously visible desktop carrying the passed name and fires
        // `desktopsChanged` in the same turn. The entry's post-call
        // id-delta readback then proves the created id (deterministic
        // `native-N` over the live list) and the later rename to the
        // friendly position name lands through the writable `name`
        // property, so the correctly issued eager spare desire settles
        // instead of riding forever ambiguous with epoch stamps.
        createDesktop: (...args: ReadonlyArray<unknown>): void => {
            if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string" || (args[1] as string) === "") {
                throw new Error("Insufficient arguments");
            }
            const at = Math.max(0, Math.min(args[0] as number, nativeDesktops.length));
            const id = `native-${nativeDesktops.length + 1}`;
            nativeDesktops.splice(at, 0, { id, name: args[1] as string });
            for (const handler of [...((signals["desktopsChanged"] as FakeSignal).handlers)]) {
                handler();
            }
        },
        removeDesktop: (): boolean => false,
    };

    const runTimers = (maxDelay = WORLD_DEBOUNCE_MS): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const timer = timers[i] as { delayMs: number; callback: () => void; cancelled: boolean; fired: boolean };
            if (timer.delayMs <= maxDelay && !timer.cancelled && !timer.fired) {
                const presentationWatchdog =
                    (timer.callback as unknown as Record<string, unknown>)["presentationReadWatchdog"] === true;
                if (presentationWatchdog) {
                    continue;
                }
                timer.fired = true;
                timers.splice(i, 1);
                i -= 1;
                timer.callback();
            }
        }
    };

    const assertAcceptedDescribe = (replyText: string, envelopeText: string): void => {
        const parsed = parseWorldReply(replyText);
        if (parsed === null) {
            throw new Error(`strict v4 DescribeWorld reply failed to parse: ${replyText.slice(0, 400)}`);
        }
        if (isWorldRejection(parsed)) {
            throw new Error(`DescribeWorld rejected kind=${parsed.kind} body=${replyText.slice(0, 500)}`);
        }
        if (!isCoreWorldReply(parsed)) {
            throw new Error(`DescribeWorld reply is not the accepted core lane: ${replyText.slice(0, 400)}`);
        }
        const envelope = asRecord(JSON.parse(envelopeText) as unknown);
        if (parsed.gen !== (envelope["gen"] as unknown) ||
            parsed.source !== (envelope["source"] as unknown) ||
            parsed.nativeInvalid !== (envelope["nativeInvalid"] as unknown)) {
            throw new Error(`DescribeWorld reply breaks the generation or watermark binding: ${replyText.slice(0, 300)}`);
        }
    };

    const assertPresentationProjection = (
        replyText: string,
        req: string,
        step: string,
        expected: { readonly instance: string; readonly serial: string } | null,
    ): void => {
        const parsed = parseWorldReply(replyText);
        if (parsed === null) {
            throw new Error(`strict v4 ReadWorldPresentation reply failed to parse at step ${step}: ${replyText.slice(0, 400)}`);
        }
        if (isWorldRejection(parsed)) {
            throw new Error(`ReadWorldPresentation rejected at step ${step} kind=${parsed.kind} body=${replyText.slice(0, 500)}`);
        }
        if (!isWorldPresentationReply(parsed)) {
            throw new Error(`ReadWorldPresentation reply is not the presentation lane at step ${step}: ${replyText.slice(0, 400)}`);
        }
        if (parsed.gen !== servingGen) {
            throw new Error(`presentation at step ${step} binds a stale generation`);
        }
        if (parsed.req !== req) {
            throw new Error(`presentation at step ${step} breaks req correlation`);
        }
        if (latestWitness === null) {
            throw new Error(`presentation at step ${step} read with no answered facts witness`);
        }
        if (parsed.coveredInstance !== latestWitness.instance || parsed.coveredInvalid !== latestWitness.counter) {
            throw new Error(`projection at step ${step} rev=${parsed.rev} obs=${parsed.obs} covers ${String(parsed.coveredInstance)}/${String(parsed.coveredInvalid)} but the latest witness is ${latestWitness.instance}/${latestWitness.counter}`);
        }
        // v2 producer: the adapter-local witness joins the same-capture
        // foreground lifetime, so the core publishes a genuine visible
        // anchored border (7 keys). Unknown/mismatch still hides narrowly.
        const border = parsed.presentation.border as unknown as Record<string, unknown>;
        const borderKeys = Object.keys(border).sort();
        if (JSON.stringify(borderKeys) !== JSON.stringify(["outsets", "radius", "reason", "subject", "visible", "width", "witness"])) {
            throw new Error(`projection at step ${step} border keys are not the anchored 7-key shape: ${JSON.stringify(borderKeys)}`);
        }
        if (border["visible"] !== true) {
            throw new Error(`projection at step ${step} hides the anchored border (v2 witness must join): ${JSON.stringify(border).slice(0, 500)}`);
        }
        if (border["subject"] !== "win-a") {
            throw new Error(`projection at step ${step} border subject is not win-a: ${JSON.stringify(border).slice(0, 300)}`);
        }
        if (border["reason"] !== "") {
            throw new Error(`projection at step ${step} visible border carries a hide reason`);
        }
        const bw = border["witness"] as { win?: unknown; obs?: unknown; frame?: unknown; foreground?: unknown } | null;
        if (bw === null || typeof bw !== "object" || Array.isArray(bw)) {
            throw new Error(`projection at step ${step} visible border carries no witness`);
        }
        if (bw["win"] !== "win-a" || bw["foreground"] !== "win-a") {
            throw new Error(`projection at step ${step} border witness does not join win-a`);
        }
        if (typeof bw["obs"] !== "string" || !isCanonicalU64(bw["obs"] as string)) {
            throw new Error(`projection at step ${step} border witness obs is not a canonical local watermark`);
        }
        if (!Array.isArray(bw["frame"]) || (bw["frame"] as Array<unknown>).length !== 4) {
            throw new Error(`projection at step ${step} border witness frame is not a rect`);
        }
        // Underlay is expected found for the tiled win-a group in every
        // step of this scenario (legacy projections all carry it).
        const underlay = parsed.presentation.underlay;
        if (underlay.status !== "found") {
            throw new Error(`projection at step ${step} hides the underlay (expected found): ${underlay.status}/${underlay.reason}`);
        }
        if (underlay.win !== "win-a" || underlay.members.length === 0) {
            throw new Error(`projection at step ${step} underlay misses the win-a group`);
        }
        // Preview existence is asserted per scenario, never merely
        // matched when present: quiet steps must paint none, live steps
        // must paint with their producing identity.
        if (expected === null) {
            if (parsed.preview !== null) {
                throw new Error(`projection at step ${step} rev=${parsed.rev} obs=${parsed.obs} paints a preview with no live gesture (fabricated ingress)`);
            }
        } else {
            if (parsed.preview === null) {
                throw new Error(`projection at step ${step} rev=${parsed.rev} obs=${parsed.obs} paints no preview (expected producer ${expected.instance}/${expected.serial})`);
            }
            if (parsed.preview.producerInstance !== expected.instance || parsed.preview.producerSerial !== expected.serial) {
                throw new Error(`projection at step ${step} rev=${parsed.rev} obs=${parsed.obs} preview producer does not match the live gesture`);
            }
        }
        // Frame-hold barrier is core-produced: every window row holds on
        // the gesture barrier exactly while a preview is live, else holds
        // nothing (legacy dispatch/projection pairs show the same split).
        const wantHold = expected === null ? "" : "gesture-barrier";
        for (const row of parsed.windows) {
            if (row.frame_hold !== wantHold) {
                throw new Error(`projection at step ${step} rev=${parsed.rev} obs=${parsed.obs} row ${row.win} frame_hold is ${JSON.stringify(row.frame_hold)} (expected ${JSON.stringify(wantHold)})`);
            }
        }
    };

    let servingGen = "";
    const discovery: { handshakeRequest: string; handshakeReply: string; settingsRequest: string; settingsReply: string } = {
        handshakeRequest: "",
        handshakeReply: "",
        settingsRequest: "",
        settingsReply: "",
    };
    // Scheduling-only seam (no byte changes, no synthetic clocks): the first
    // capture send waits one bounded 100ms so the serving broker clock is
    // safely positive when genesis-0 evaluates. The entry learns the broker
    // epoch from the reply clock and stamps its native-creation fulfillment
    // receipt with it, so the genesis-1 receipt carries a solidly positive
    // stamp that the mint-before-receipt replay order can rely on without
    // sub-ms luck. Genuine report ingestion, only later.
    let describedWorld = false;
    const pumpBridge = async (): Promise<boolean> => {
        let progressed = false;
        for (const call of calls) {
            if (call.answered) {
                continue;
            }
            if (call.method !== "DiscoverWorld" && call.method !== "DescribeWorld" && call.method !== "ReadWorldPresentation") {
                continue;
            }
            call.answered = true;
            if (call.method === "DescribeWorld" && !describedWorld) {
                describedWorld = true;
                await new Promise<void>((resolve) => setTimeout(resolve, 100));
            }
            const replyText = await bridge.send(call.method, call.payload);
            let replyValue: unknown;
            try {
                replyValue = JSON.parse(replyText) as unknown;
            } catch (error) {
                void error;
                throw new Error(`broker_world_eval stdout is not a JSON object for ${call.method}: ${replyText.slice(0, 200)}`);
            }
            if (typeof replyValue !== "object" || replyValue === null || Array.isArray(replyValue)) {
                throw new Error(`broker_world_eval stdout is not a JSON object for ${call.method}`);
            }
            if (call.method === "DescribeWorld") {
                const index = envelopes.filter((entry) => entry.step === activeStep).length;
                envelopes.push({ step: activeStep, file: `envelope-${activeStep}-${index}.json`, bytes: call.payload });
                assertAcceptedDescribe(replyText, call.payload);
                replies.push({ step: activeStep, file: `reply-${activeStep}-${index}.json`, bytes: replyText });
            } else if (call.method === "ReadWorldPresentation") {
                internalReads += 1;
            } else if (discovery.handshakeRequest === "") {
                discovery.handshakeRequest = call.payload;
                discovery.handshakeReply = replyText;
            } else if (discovery.settingsRequest === "") {
                discovery.settingsRequest = call.payload;
                discovery.settingsReply = replyText;
            } else {
                throw new Error(`unexpected third DiscoverWorld (rehandshake/rotation): ${call.payload.slice(0, 200)}`);
            }
            call.callback(replyText);
            progressed = true;
        }
        return progressed;
    };

    const settle = async (rounds = 40): Promise<void> => {
        for (let round = 0; round < rounds; round += 1) {
            runTimers();
            const before = calls.length;
            await pumpBridge();
            runTimers();
            await pumpBridge();
            const unanswered = calls.some((call) => !call.answered &&
                (call.method === "DiscoverWorld" || call.method === "DescribeWorld" || call.method === "ReadWorldPresentation"));
            if (calls.length === before && !unanswered) {
                // Swallowed throws surface as transport logs; the parked
                // B1 wait is expected held, never a failure.
                const transport = logs.filter((line) => /transport/i.test(line));
                if (transport.length > 0) {
                    throw new Error(`transport failure at step ${activeStep}: ${JSON.stringify(transport.slice(-4))}`);
                }
                return;
            }
        }
        throw new Error(`settle did not converge at step ${activeStep}; logs=${JSON.stringify(logs.slice(-8))}`);
    };

    const answerFacts = (bytes: string, step: string): void => {
        const waiter = effectCalls.find((call) => call.method === "WaitPresentationFacts" && !call.answered);
        if (waiter === undefined) {
            throw new Error(`no held WaitPresentationFacts waiter at step ${step}; logs=${JSON.stringify(logs.slice(-8))}`);
        }
        waiter.answered = true;
        const parsed = asRecord(JSON.parse(bytes) as unknown);
        if (typeof parsed["instance"] !== "string" || typeof parsed["invalidation"] !== "string") {
            throw new Error(`facts bytes at step ${step} carry no instance/invalidation witness`);
        }
        latestWitness = { instance: parsed["instance"] as string, counter: parsed["invalidation"] as string };
        const index = factsInputs.filter((entry) => entry.step === step).length;
        factsInputs.push({ step, file: `facts-${step}-${index}.json`, bytes });
        waiter.callback(bytes);
    };

    const answerGesture = (instance: string, serial: string, phase: "updated" | "idle", step: string): void => {
        const waiter = gestureCalls.find((call) => call.method === "WaitGesture" && !call.answered);
        if (waiter === undefined) {
            throw new Error(`no held WaitGesture waiter at step ${step}; logs=${JSON.stringify(logs.slice(-8))}`);
        }
        waiter.answered = true;
        const bytes = gestureBody(instance, serial, phase);
        const index = gestureInputs.filter((entry) => entry.step === step).length;
        gestureInputs.push({ step, file: `gesture-${step}-${index}.json`, bytes });
        waiter.callback(bytes);
    };

    const fireWindowSignal = (win: string, signal: "frameGeometryChanged", step: string): void => {
        const peer = winSignals[win]?.[signal];
        if (peer === undefined) {
            throw new Error(`no fake signal ${signal} on ${win} at step ${step}`);
        }
        for (const handler of [...peer.handlers]) {
            handler();
        }
    };

    const explicitRead = async (step: string): Promise<void> => {
        readCounter += 1;
        const req = `read-${options.chain}-${step}-${readCounter}`;
        const body = JSON.stringify({ v: 4, req });
        const replyText = await bridge.send("ReadWorldPresentation", body);
        const expected = STAGE5_V4_PREVIEW_EXPECTATION[step] ?? null;
        assertPresentationProjection(replyText, req, step, expected);
        projections.push({ step, file: `projection-${step}.json`, bytes: replyText });
        if (latestWitness === null) {
            throw new Error(`dispatch render at step ${step} has no answered facts witness`);
        }
        const invalidation = { instance: latestWitness.instance, counter: latestWitness.counter };
        const gesture = dispatchLiveGesture(step, "normal");
        const dispatchBytes = renderPresentationDispatch(replyText, invalidation, gesture);
        dispatches.push({
            step,
            file: `dispatch-${step}.json`,
            bytes: dispatchBytes,
            gestureInstance: gesture === null ? null : gesture.instance,
            gestureSerial: gesture === null ? null : gesture.serial,
        });
        if (options.chain === "full" && step === "a3") {
            const crossed = dispatchLiveGesture(step, "crossed-b");
            if (crossed === null) {
                throw new Error("crossed-b gesture missing for a3");
            }
            const crossedBytes = renderPresentationDispatch(replyText, invalidation, crossed);
            dispatches.push({
                step: "a3-b",
                file: "dispatch-a3-b.json",
                bytes: crossedBytes,
                gestureInstance: crossed.instance,
                gestureSerial: crossed.serial,
            });
        }
    };

    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: STAGE5_V4_ADAPTER,
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "QueueWorldCommand" || method === "TakeCommandResult") {
                throw new Error(`unsupported ${method} on the observation-only record route`);
            }
            if (method === "WaitWorldWake") {
                parkedWaits.push({ method, payload, callback, answered: false });
                return;
            }
            if (method !== "DiscoverWorld" && method !== "DescribeWorld" && method !== "ReadWorldPresentation") {
                throw new Error(`unsupported callDbus method ${method}`);
            }
            calls.push({ method, payload, callback, answered: false });
        },
        effectCallDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method !== "WaitPresentationFacts") {
                throw new Error(`unsupported effectCallDbus method ${method}`);
            }
            effectCalls.push({ method, payload, callback, answered: false });
        },
        gestureCallDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "AckGesture") {
                // Entry-armed terminal settlement proof: answered, never
                // swallowed. No step in either chain opens a terminal, so a
                // live run reaching here fails loudly at the reply oracle.
                gestureCalls.push({ method, payload, callback, answered: true });
                callback(idleAckFor(payload));
                return;
            }
            if (method !== "WaitGesture") {
                throw new Error(`unsupported gestureCallDbus method ${method}`);
            }
            gestureCalls.push({ method, payload, callback, answered: false });
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const timer = { delayMs, callback, cancelled: false, fired: false };
            timers.push(timer);
            return (): void => {
                timer.cancelled = true;
            };
        },
        log: (message: string): void => {
            logs.push(message);
        },
        // No gestureActive override: the production default derives the
        // barrier flag from the genuine waiter-delivered snapshot
        // (available + started/updated), matching legacy report.gesture.
        // A hardcoded false would lie about the native open state:
        // ingress still folds the preview while the core barrier stays
        // down (world_protocol.rs stages gesture_barrier from the report
        // gesture boolean; world_publish.rs publishes row frame_hold).
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    if (handle === null) {
        await bridge.close();
        throw new Error("startWorldAdapterEntry refused the fake surface");
    }

    try {
        runTimers();
        await settle();
        // Discovery replies are the exact bytes the bridge returned to the
        // entry (recorded in pump order, never redelivered: every successful
        // handshake resets broker state, so replaying one would destroy the
        // serving generation the entry just bound).
        if (discovery.handshakeRequest === "" || discovery.settingsRequest === "") {
            throw new Error(`handshake/settings discovery never completed; calls=${calls.length} logs=${JSON.stringify(logs.slice(-8))}`);
        }
        const handshakeRequest = discovery.handshakeRequest;
        const handshakeReply = discovery.handshakeReply;
        const settingsRequest = discovery.settingsRequest;
        const settingsReply = discovery.settingsReply;
        const handshakeBody = asRecord(JSON.parse(handshakeRequest) as unknown);
        if (handshakeBody["type"] !== "handshake" || handshakeBody["adapter"] !== STAGE5_V4_ADAPTER) {
            throw new Error(`first discovery is not the producer handshake: ${handshakeRequest.slice(0, 300)}`);
        }
        const settingsBody = asRecord(JSON.parse(settingsRequest) as unknown);
        if (settingsBody["type"] !== "settings") {
            throw new Error(`second discovery is not the revisioned settings snapshot: ${settingsRequest.slice(0, 300)}`);
        }
        const handshakeParsed = asRecord(JSON.parse(handshakeReply) as unknown);
        if (handshakeParsed["ready"] !== false) {
            throw new Error(`handshake reply is not not-ready: ${handshakeReply.slice(0, 300)}`);
        }
        if (typeof handshakeParsed["gen"] !== "string" || (handshakeParsed["gen"] as string) === "") {
            throw new Error(`handshake reply mints no serving generation: ${handshakeReply.slice(0, 300)}`);
        }
        if (handshakeParsed["adapter"] !== STAGE5_V4_ADAPTER || handshakeParsed["req"] !== handshakeBody["req"]) {
            throw new Error(`handshake reply breaks the producer lifetime or correlation: ${handshakeReply.slice(0, 300)}`);
        }
        servingGen = handshakeParsed["gen"] as string;
        const settingsParsed = asRecord(JSON.parse(settingsReply) as unknown);
        if (settingsParsed["status"] !== "accepted") {
            throw new Error(`settings not accepted: ${settingsReply.slice(0, 300)}`);
        }
        if (settingsParsed["gen"] !== servingGen || settingsParsed["req"] !== settingsBody["req"]) {
            throw new Error(`settings reply breaks the generation or correlation: ${settingsReply.slice(0, 300)}`);
        }

        // Genesis: whatever the entry emits before the first facts answer is
        // preserved verbatim (replay order stays truthful).
        activeStep = "genesis";
        await settle();

        for (const step of steps) {
            activeStep = step.name;
            for (const action of step.actions) {
                if ("facts" in action) {
                    answerFacts(factsFor(action.facts), step.name);
                } else if ("gesture" in action) {
                    answerGesture(action.gesture.instance, action.gesture.serial, action.gesture.phase, step.name);
                } else {
                    fireWindowSignal(action.fireSignal.win, action.fireSignal.signal, step.name);
                }
                await settle();
            }
            await explicitRead(step.name);
        }

        return {
            chain: options.chain,
            adapter: STAGE5_V4_ADAPTER,
            gen: servingGen,
            discovery: { handshakeRequest, handshakeReply, settingsRequest, settingsReply },
            envelopes,
            replies,
            projections,
            dispatches,
            factsInputs,
            gestureInputs,
            internalReads,
            parkedWaits: parkedWaits.length,
            volatile: [...STAGE5_V4_VOLATILE_TOKENS],
        };
    } catch (error) {
        // Retain captured bytes for diagnosis without a blind re-record.
        // No separate log files; the normal per-step layout is reused.
        const partial: Stage5V4ChainResult = {
            chain: options.chain,
            adapter: STAGE5_V4_ADAPTER,
            gen: servingGen,
            discovery: { ...discovery },
            envelopes,
            replies,
            projections,
            dispatches,
            factsInputs,
            gestureInputs,
            internalReads,
            parkedWaits: parkedWaits.length,
            volatile: [...STAGE5_V4_VOLATILE_TOKENS],
        };
        (error as { partialResult?: Stage5V4ChainResult }).partialResult = partial;
        throw error;
    } finally {
        try {
            handle.stop();
        } catch (error) {
            void error;
        }
        await bridge.close();
    }
}

export function chainDir(outDir: string, chain: Stage5V4ChainName): string {
    const root = repoRootOf(process.cwd());
    const resolved = resolve(outDir);
    if (resolved === root || resolved.startsWith(root + "/")) {
        throw new Error(`refusing to write inside the repo: ${resolved}`);
    }
    return join(resolved, chain);
}

export function writeStage5V4Chain(outDir: string, result: Stage5V4ChainResult, failure?: string): ReadonlyArray<string> {
    const dir = chainDir(outDir, result.chain);
    mkdirSync(resolve(outDir), { recursive: true });
    if (existsSync(dir)) {
        throw new Error(`refusing to overwrite existing chain directory: ${dir}`);
    }
    mkdirSync(dir);
    const written: string[] = [];
    const write = (name: string, bytes: string): void => {
        const path = join(dir, name);
        writeFileSync(path, bytes);
        written.push(path);
    };
    write("discovery-handshake.request.json", result.discovery.handshakeRequest);
    write("discovery-handshake.reply.json", result.discovery.handshakeReply);
    write("discovery-settings.request.json", result.discovery.settingsRequest);
    write("discovery-settings.reply.json", result.discovery.settingsReply);
    for (const entry of result.envelopes) {
        write(entry.file, entry.bytes);
    }
    for (const entry of result.replies) {
        write(entry.file, entry.bytes);
    }
    for (const entry of result.factsInputs) {
        write(entry.file, entry.bytes);
    }
    for (const entry of result.gestureInputs) {
        write(entry.file, entry.bytes);
    }
    for (const entry of result.projections) {
        write(entry.file, entry.bytes);
    }
    for (const entry of result.dispatches) {
        write(entry.file, entry.bytes);
    }
    const manifest = {
        tool: "world-stage5-v4-record",
        bridge: "broker_world_eval",
        dispatchProvenance: {
            tool: "world-stage5-v4-record+production-subscriber",
            route: "broker-projection->createWorldPresentationSubscriber->SetPresentation",
            reqRekeyed: true,
            crossed: "dispatch-a3-b.json renders projection-a3.json with B/1 live gesture",
        },
        chain: result.chain,
        adapter: result.adapter,
        gen: result.gen,
        volatile: result.volatile,
        internalReads: result.internalReads,
        parkedWaits: result.parkedWaits,
        steps: result.projections.map((projection) => ({
            name: projection.step,
            projection: projection.file,
            dispatches: result.dispatches.filter((entry) => entry.step === projection.step || (projection.step === "a3" && entry.step === "a3-b")).map((entry) => entry.file),
            envelopes: result.envelopes.filter((entry) => entry.step === projection.step).map((entry) => entry.file),
            replies: result.replies.filter((entry) => entry.step === projection.step).map((entry) => entry.file),
            facts: result.factsInputs.filter((entry) => entry.step === projection.step).map((entry) => entry.file),
            gestures: result.gestureInputs.filter((entry) => entry.step === projection.step).map((entry) => entry.file),
        })),
        genesisEnvelopes: result.envelopes.filter((entry) => entry.step === "genesis").map((entry) => entry.file),
        ...(failure === undefined ? {} : { failed: failure }),
    };
    write("manifest.json", JSON.stringify(manifest, null, 2) + "\n");
    return written;
}

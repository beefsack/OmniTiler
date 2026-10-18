import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";

import {
    PLAN_DEBOUNCE_MS,
    PLAN_TIMEOUT_MS,
    PlanAdapter,
    snapshotOf,
    type MaximizeClearOutcome,
    type PlanAdapterEnv,
    type PlanObserved,
    type PlanWindowConstraints,
    isFixedSize,
} from "../src/plan-adapter";

describe("fixed-size admission predicate", () => {
    it("floats only both-axes nonzero-equal usable vectors by default", () => {
        assert.equal(isFixedSize({ w: 640, h: 480 }, { w: 640, h: 480 }), true);
        // G-05: zero is unset per axis, so equal partial-zero vectors pin
        // nothing under both-axes-fixed (each tiles by default).
        assert.equal(isFixedSize({ w: 640, h: 0 }, { w: 640, h: 0 }), false);
        assert.equal(isFixedSize({ w: 0, h: 480 }, { w: 0, h: 480 }), false);
        assert.equal(isFixedSize({ w: 16384, h: 16384 }, { w: 16384, h: 16384 }), true);
        assert.equal(isFixedSize({ w: 640, h: 480 }, null), false);
        assert.equal(isFixedSize(null, { w: 640, h: 480 }), false);
        assert.equal(isFixedSize(undefined, undefined), false);
        assert.equal(isFixedSize({ w: 640, h: 100 }, { w: 640, h: 480 }), false);
        assert.equal(isFixedSize({ w: 640, h: 480 }, { w: 800, h: 600 }), false);
        assert.equal(isFixedSize({ w: 0, h: 0 }, { w: 0, h: 0 }), false);
        assert.equal(isFixedSize({ w: -1, h: 480 }, { w: -1, h: 480 }), false);
        assert.equal(
            isFixedSize({ w: 640, h: 480 }, { w: 2147483647, h: 2147483647 }),
            false,
        );
        assert.equal(isFixedSize({ w: 16385, h: 16385 }, { w: 16385, h: 16385 }), false);
        assert.equal(isFixedSize({ w: 1.5, h: 480 }, { w: 1.5, h: 480 }), false);
    });

    it("floats either-axis equal vectors under either-axis-fixed with guards preserved", () => {
        assert.equal(
            isFixedSize({ w: 640, h: 100 }, { w: 640, h: 480 }, "either-axis-fixed"),
            true,
        );
        assert.equal(
            isFixedSize({ w: 640, h: 480 }, { w: 800, h: 480 }, "either-axis-fixed"),
            true,
        );
        assert.equal(
            isFixedSize({ w: 640, h: 480 }, { w: 640, h: 480 }, "either-axis-fixed"),
            true,
        );
        assert.equal(
            isFixedSize({ w: 640, h: 100 }, { w: 640, h: 480 }, "both-axes-fixed"),
            false,
        );
        assert.equal(
            isFixedSize({ w: 640, h: 100 }, { w: 640, h: 480 }, "bogus"),
            false,
        );
        // Guards preserved: missing/sentinel still tile under either-axis.
        assert.equal(
            isFixedSize({ w: 640, h: 480 }, null, "either-axis-fixed"),
            false,
        );
        assert.equal(
            isFixedSize({ w: 640, h: 480 }, { w: 2147483647, h: 2147483647 }, "either-axis-fixed"),
            false,
        );
        assert.equal(
            isFixedSize({ w: 0, h: 0 }, { w: 0, h: 0 }, "either-axis-fixed"),
            false,
        );
        // G-05: both transposed partial-zero vectors pin via their nonzero
        // axis under either-axis-fixed; full-zero and sentinel stay tiled.
        assert.equal(
            isFixedSize({ w: 640, h: 0 }, { w: 640, h: 0 }, "either-axis-fixed"),
            true,
        );
        assert.equal(
            isFixedSize({ w: 0, h: 480 }, { w: 0, h: 480 }, "either-axis-fixed"),
            true,
        );
        assert.equal(
            isFixedSize({ w: 640, h: 0 }, { w: 640, h: 0 }, "both-axes-fixed"),
            false,
        );
        assert.equal(
            isFixedSize({ w: 0, h: 480 }, { w: 0, h: 480 }, "both-axes-fixed"),
            false,
        );
    });
});

// Real-Engine fixtures below: every row dispatches through the production
// observer path and flushes the payload to the real Planner
// (planner_eval: same validation and retained state as the shipped
// binary; only D-Bus is stubbed). No fabricated replies. Each row
// inspects the actual wire payload AND applies the real reply through
// the instrumented native setters, asserting the accepted terminal,
// the desired topology, and that the automatic target saw no writes.

function makeRefs(): { a: object; b: object } {
    return { a: {}, b: {} };
}

const FIXED = { w: 640, h: 480 };

interface ObservedOpts {
    focused?: object;
    refB?: object;
    floatingB?: boolean;
    stickyB?: boolean;
    fullscreenB?: boolean;
    maximizedB?: boolean;
    omitB?: boolean;
    workspaceB?: string;
    fingerprint?: string;
    rects?: Record<string, { x: number; y: number; w: number; h: number }>;
    domainOutput?: string;
    domainWorkspace?: string;
}

function makeObserved(refs: { a: object; b: object }, opts: ObservedOpts = {}): PlanObserved {
    const rectFor = (id: string): { x: number; y: number; w: number; h: number } =>
        opts.rects?.[id] ??
        (id === "win-a" ? { x: 0, y: 0, w: 600, h: 800 } : { x: 600, y: 0, w: 600, h: 800 });
    const domainOutput = opts.domainOutput ?? "out-1";
    const domainWorkspace = opts.domainWorkspace ?? "ws-1";
    const entries = [
        Object.freeze({
            id: "win-a",
            ref: refs.a,
            rect: rectFor("win-a"),
            output: domainOutput,
            workspace: domainWorkspace,
            fullscreen: false,
            maximized: false,
            floating: false,
            sticky: false,
            resourceClass: "unknown",
        }),
    ];
    if (opts.omitB !== true) {
        entries.push(
            Object.freeze({
                id: "win-b",
                ref: opts.refB ?? refs.b,
                rect: rectFor("win-b"),
                output: domainOutput,
                workspace: opts.workspaceB ?? domainWorkspace,
                fullscreen: opts.fullscreenB === true,
                maximized: opts.maximizedB === true,
                floating: opts.floatingB === true,
                sticky: opts.stickyB === true,
                resourceClass: "game-app",
            }) as never,
        );
    }
    return {
        domainOutput,
        domainWorkspace,
        domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
        domainGap: 0,
        domainOuterGap: 0,
        focusedId: opts.focused === undefined || opts.focused === refs.a ? "win-a" : "win-b",
        windows: Object.freeze(entries) as unknown as PlanObserved["windows"],
        activeRef: opts.focused ?? refs.a,
        fingerprint: opts.fingerprint ?? "fp-fixed-1",
        revalidate: () => true,
    };
}

function makeHidden(refs: { a: object; b: object }, hiddenRef: object): PlanObserved {
    return {
        domainOutput: "out-1",
        domainWorkspace: "ws-9",
        domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
        domainGap: 0,
        domainOuterGap: 0,
        focusedId: "win-h",
        windows: Object.freeze([
            Object.freeze({
                id: "win-h",
                ref: hiddenRef,
                rect: { x: 0, y: 0, w: 600, h: 800 },
                output: "out-1",
                workspace: "ws-9",
                fullscreen: false,
                maximized: false,
                floating: false,
                sticky: false,
                resourceClass: "unknown",
            }),
        ]) as unknown as PlanObserved["windows"],
        activeRef: refs.a,
        fingerprint: "fp-hidden-1",
        revalidate: () => true,
    };
}

interface Mocks {
    readonly dbusCalls: Array<{ payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly geometries: Array<{ target: object; rect: { x: number; y: number; w: number; h: number } }>;
    readonly keepAboveWrites: Array<{ target: object; above: boolean }>;
    readonly floatingWrites: Array<{ id: string; floating: boolean }>;
    readonly activeWrites: Array<object>;
    readonly maximizeClears: Array<object>;
    readonly desktopToggles: Array<{ target: object; allDesktops: boolean }>;
    readonly subscribes: Array<{ kind: string; handler: (target?: object) => void }>;
    observeImpl: () => PlanObserved | null;
    observeHiddenImpl: () => ReadonlyArray<PlanObserved>;
    constraintsImpl: (target: object) => PlanWindowConstraints | null;
    geometryImpl: (target: object, rect: { x: number; y: number; w: number; h: number }) => boolean;
    desktopToggleImpl: (target: object, allDesktops: boolean) => "invoked" | "missing" | "threw";
    desktopIdsImpl: (target: object) => ReadonlyArray<string> | null;
    activeImpl: () => object | null;
    setActiveImpl: (target: object) => boolean;
    env: PlanAdapterEnv;
}

function mockEnv(refs: { a: object; b: object }): Mocks {
    const state = {
        dbusCalls: [],
        callbacks: [],
        timers: [],
        logs: [],
        geometries: [],
        keepAboveWrites: [],
        floatingWrites: [],
        activeWrites: [],
        maximizeClears: [],
        desktopToggles: [],
        subscribes: [],
        observeImpl: (): PlanObserved | null => makeObserved(refs),
        observeHiddenImpl: (): ReadonlyArray<PlanObserved> => [],
        constraintsImpl: (_target: object): PlanWindowConstraints | null => null,
        geometryImpl: (_target: object, _rect: { x: number; y: number; w: number; h: number }): boolean => true,
        desktopToggleImpl: (_target: object, _allDesktops: boolean): "invoked" | "missing" | "threw" => "invoked",
        desktopIdsImpl: (_target: object): ReadonlyArray<string> | null => [],
        activeImpl: (): object | null => refs.a,
        setActiveImpl: (_target: object): boolean => true,
        env: null as unknown as PlanAdapterEnv,
    } as Mocks;
    const env: PlanAdapterEnv = {
        callDbus: (service, path, iface, method, payload, callback): void => {
            void service;
            void path;
            void iface;
            if (method === "NameHasOwner") {
                callback(true);
                return;
            }
            if (method === "GetNameOwner") {
                callback(":1.7");
                return;
            }
            if (method === "StartServiceByName") {
                callback(1);
                return;
            }
            state.dbusCalls.push({ payload });
            state.callbacks.push(callback);
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const entry = { delayMs, callback, cancelled: false };
            state.timers.push(entry);
            return (): void => {
                entry.cancelled = true;
            };
        },
        log: (message): void => {
            state.logs.push(message);
        },
        observe: (): PlanObserved | null => state.observeImpl(),
        observeHidden: (): ReadonlyArray<PlanObserved> => state.observeHiddenImpl(),
        clearMaximize: (target): "invoked" => {
            state.maximizeClears.push(target);
            return "invoked";
        },
        setGeometry: (target, rect): boolean => {
            state.geometries.push({ target, rect: { x: rect.x, y: rect.y, w: rect.w, h: rect.h } });
            return state.geometryImpl(target, rect);
        },
        setActive: (target): boolean => {
            state.activeWrites.push(target);
            return state.setActiveImpl(target);
        },
        active: (): object | null => state.activeImpl(),
        readKeepAbove: (): boolean => false,
        readKeepBelow: (): boolean => false,
        setKeepAbove: (target, above): "invoked" => {
            state.keepAboveWrites.push({ target, above });
            return "invoked";
        },
        setFloating: (id, floating): void => {
            state.floatingWrites.push({ id, floating });
        },
        setAllDesktops: (target, allDesktops): MaximizeClearOutcome => {
            state.desktopToggles.push({ target, allDesktops });
            return state.desktopToggleImpl(target, allDesktops);
        },
        subscribe: (kind, handler): (() => void) => {
            state.subscribes.push({ kind, handler });
            return (): void => {};
        },
        readWindowConstraints: (target: object): PlanWindowConstraints | null =>
            state.constraintsImpl(target),
        readDesktopIds: (target: object): ReadonlyArray<string> | null => state.desktopIdsImpl(target),
    };
    (state as { env: PlanAdapterEnv }).env = env;
    return state;
}

function enableAdapter(mocks: Mocks): PlanAdapter {
    const adapter = new PlanAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
}

function runDebounce(mocks: Mocks): void {
    const pending = [...mocks.timers];
    mocks.timers.length = 0;
    for (const timer of pending) {
        if (timer.cancelled) {
            continue;
        }
        if (timer.delayMs === PLAN_DEBOUNCE_MS) {
            timer.callback();
        } else {
            mocks.timers.push(timer);
        }
    }
}

function fire(mocks: Mocks, kind: string, target?: object): void {
    for (const sub of mocks.subscribes) {
        if (sub.kind === kind) {
            sub.handler(target);
        }
    }
}

// Fire the unanswered-request deadline for the in-flight plan: an actual
// failure terminal through the real route (no fabricated reply).
function fireTimeout(mocks: Mocks): void {
    const pending = [...mocks.timers];
    mocks.timers.length = 0;
    let fired = false;
    for (const timer of pending) {
        if (timer.cancelled) {
            continue;
        }
        if (!fired && timer.delayMs === PLAN_TIMEOUT_MS) {
            fired = true;
            timer.callback();
        } else {
            mocks.timers.push(timer);
        }
    }
    assert.ok(fired, "a request deadline must be armed");
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

class EngineBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{
        resolve: (reply: string) => void;
        reject: (error: Error) => void;
    }> = [];
    private dead: string | null = null;

    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("planner_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.resume();
        proc.on("error", (error) => {
            this.failAll(error instanceof Error ? error : new Error(String(error)));
        });
        proc.on("exit", (code) => {
            this.failAll(new Error(`planner_eval exited with code ${String(code)}`));
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

    private deathReason(): string | null {
        if (this.dead !== null) {
            return this.dead;
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return "planner_eval process already exited";
        }
        return null;
    }

    static start(): EngineBridge {
        return new EngineBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                cwd: repoRoot(),
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    send(request: string): Promise<string> {
        const dead = this.deathReason();
        if (dead !== null) {
            return Promise.reject(new Error(dead));
        }
        return new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(request + "\n");
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

async function flushPlan(mocks: Mocks, engine: EngineBridge, index: number): Promise<Record<string, unknown>> {
    const call = mocks.dbusCalls[index];
    assert.ok(call !== undefined, `dispatch ${index} exists before flushing to the real Engine`);
    const replyText = await engine.send(call.payload);
    mocks.callbacks[index]?.(replyText);
    return JSON.parse(replyText) as Record<string, unknown>;
}

// Flush every outstanding dispatch in order, including completion-chain
// follow-ups (a hidden admission after a foreground apply). Bounded: the
// chain settles once all domains converge.
async function drainOutstanding(
    mocks: Mocks,
    engine: EngineBridge,
    flushed: { count: number },
): Promise<void> {
    for (let round = 0; round < 8; round += 1) {
        if (flushed.count >= mocks.dbusCalls.length) {
            return;
        }
        const reply = await flushPlan(mocks, engine, flushed.count);
        flushed.count += 1;
        assert.equal(reply["outcome"], "planned", `chain reply plans, got ${JSON.stringify(reply)}`);
    }
    assert.ok(flushed.count >= mocks.dbusCalls.length, "completion chain settles");
}

function dispatchAdded(mocks: Mocks): Record<string, unknown> {
    const before = mocks.dbusCalls.length;
    fire(mocks, "added");
    runDebounce(mocks);
    assert.ok(mocks.dbusCalls.length > before, "an observer signal must dispatch");
    return JSON.parse(mocks.dbusCalls[mocks.dbusCalls.length - 1]?.payload as string) as Record<
        string,
        unknown
    >;
}

function wireWindows(payload: Record<string, unknown>): Array<Record<string, unknown>> {
    return payload["windows"] as Array<Record<string, unknown>>;
}

function wireOf(payload: Record<string, unknown>, id: string): Record<string, unknown> {
    const found = wireWindows(payload).find((entry) => entry["window"] === id);
    assert.ok(found !== undefined, `wire carries ${id}`);
    return found;
}

function desiredWindows(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["desired_geometry"] as Array<Record<string, unknown>> | undefined) ?? [];
}

function fixedMocks(): { refs: { a: object; b: object }; mocks: Mocks } {
    const refs = makeRefs();
    const mocks = mockEnv(refs);
    mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
        if (target === refs.b) {
            return { resizeable: false, minSize: { ...FIXED }, maxSize: { ...FIXED } };
        }
        return { resizeable: true, minSize: null, maxSize: null };
    };
    return { refs, mocks };
}

describe("fixed-size admission through the real Planner", () => {
    it("brand-new fixed floats with origin, plans, and never actuates the game", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-admit" });
            const payload = dispatchAdded(mocks);
            const fixed = wireOf(payload, "win-b");
            assert.equal(fixed["floating"], true);
            assert.equal(fixed["fixed_auto"], true);
            assert.equal(fixed["fit_excluded"], true);
            assert.deepEqual(fixed["min_size"], FIXED);
            assert.deepEqual(fixed["max_size"], FIXED);
            const tiled = wireOf(payload, "win-a");
            assert.ok(!("fixed_suppress" in tiled), "hintless rows stay byte-identical");
            assert.ok(!("fixed_auto" in tiled));
            const reply = await flushPlan(mocks, engine, 0);
            assert.equal(reply["outcome"], "planned", `Engine plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-a"),
                "tiled survivor keeps a slot",
            );
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "automatic float takes no slot",
            );
            // Apply proof: the tiled sibling takes its planned slot, so the
            // absence of automatic writes below is a proven no-touch, not
            // a dead apply path.
            const sibling = desiredWindows(reply).find((entry) => entry["window"] === "win-a");
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.a),
                [{ target: refs.a, rect: sibling?.["rect"] as { x: number; y: number; w: number; h: number } }],
                "sibling applied in its planned slot",
            );
            assert.ok(
                mocks.logs.some(
                    (entry) => entry.includes("kind=reconcile") && entry.includes("outcome=planned-applied"),
                ),
                "accepted apply terminal logged",
            );
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no automatic geometry write to the fixed client",
            );
            assert.deepEqual(mocks.keepAboveWrites, [], "no automatic keep-above write");
            assert.deepEqual(mocks.floatingWrites, [], "autoclassification never calls intentional setters");
            assert.deepEqual(mocks.maximizeClears, [], "no maximize interference");
            assert.deepEqual(mocks.activeWrites, [], "no automatic activation of any window");
            const line = mocks.logs.find((entry) => entry.includes("fixed-size-classification"));
            assert.ok(line !== undefined, "bounded classification diagnostic emitted");
            assert.ok(line.includes("phase=classify"), line);
            assert.ok(line.includes("reason=fixed-equal"), line);
            assert.ok(!line.includes("win-"), line);
        } finally {
            await engine.close();
        }
    });

    it("explicit unfloat of the automatic client tiles through the real Planner", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-u1" });
            dispatchAdded(mocks);
            const admit = await flushPlan(mocks, engine, 0);
            assert.equal(admit["outcome"], "planned");
            // Meta+G on the automatic float: native still reports tiled, so
            // the logical view must still unfloat instead of floating.
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-u2" });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "unfloat dispatches toggle-float");
            const payload = JSON.parse(mocks.dbusCalls[1]?.payload as string) as Record<string, unknown>;
            assert.deepEqual((payload["command"] as Record<string, unknown>)["op"], "toggle-float");
            const reply = await flushPlan(mocks, engine, 1);
            assert.equal(reply["outcome"], "planned", `unfloat plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "unfloated client rejoins the tiled topology",
            );
            // The same live client stays tiled with suppression afterwards.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-u3" });
            const after = dispatchAdded(mocks);
            const excused = wireOf(after, "win-b");
            assert.ok(!("floating" in excused), "explicit tile wins, no re-float");
            assert.equal(excused["fixed_suppress"], true);
            assert.ok(!("fixed_auto" in excused));
        } finally {
            await engine.close();
        }
    });

    it("maximized intentional unfloat clears before admission and applies the tiled slot through the real Planner", async () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            // Seed both windows tiled through the real Planner.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-b9-n1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Intentional float of win-b.
            let floatingB = false;
            let maximizedB = false;
            let rectB = { x: 600, y: 0, w: 600, h: 800 };
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    floatingB,
                    maximizedB,
                    rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": rectB },
                    fingerprint: "fp-b9-n2",
                });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "float dispatches toggle-float");
            // Native float takes effect in the apply below, so the
            // apply-time observation still shows the tiled frame.
            const floatReply = await flushPlan(mocks, engine, 1);
            assert.equal(floatReply["outcome"], "planned", `float plans, got ${JSON.stringify(floatReply)}`);
            assert.deepEqual(mocks.floatingWrites[mocks.floatingWrites.length - 1], { id: "win-b", floating: true });
            // Native maximize of the intentional float, then explicit unfloat:
            // the clear settles before any admission geometry. Settlement
            // is derived from the recorded clear itself: the pre-clear
            // observation still shows maximize, every later read shows the
            // restored frame.
            maximizedB = true;
            const maximizedFrame = { x: 0, y: 0, w: 1200, h: 800 };
            const restoredFrame = { x: 300, y: 200, w: 500, h: 400 };
            const clearsBeforeUnfloat = mocks.maximizeClears.length;
            mocks.observeImpl = () => {
                const cleared = mocks.maximizeClears.length > clearsBeforeUnfloat;
                return makeObserved(refs, {
                    focused: refs.b,
                    floatingB: true,
                    maximizedB: !cleared,
                    rects: { "win-a": { x: 0, y: 0, w: 600, h: 800 }, "win-b": cleared ? restoredFrame : maximizedFrame },
                    fingerprint: "fp-b9-n3",
                });
            };
            const callsBefore = mocks.dbusCalls.length;
            const clearsBefore = mocks.maximizeClears.length;
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.maximizeClears.length, clearsBefore + 1, "exactly one native clear before admission");
            assert.equal(mocks.dbusCalls.length, callsBefore + 1, "observed-clear fresh-admits");
            const command = JSON.parse(mocks.dbusCalls[callsBefore]?.payload as string)[
                "command"
            ] as Record<string, unknown>;
            assert.deepEqual(command, {
                op: "toggle-float",
                window: "win-b",
                float_rect: { x: 300, y: 200, w: 500, h: 400 },
            });
            assert.ok(
                mocks.logs.some((line) => line.includes("outcome=observed-cleared")),
                "settlement observed before admission",
            );
            const reply = await flushPlan(mocks, engine, callsBefore);
            assert.equal(reply["outcome"], "planned", `unfloat plans, got ${JSON.stringify(reply)}`);
            const slot = desiredWindows(reply).find((entry) => entry["window"] === "win-b");
            assert.ok(slot !== undefined, "cleared float rejoins the tiled topology");
            assert.notDeepEqual(
                slot["rect"],
                { x: 0, y: 0, w: 1200, h: 800 },
                "admission places the tile, never the maximized frame",
            );
            const writesB = mocks.geometries.filter((write) => write.target === refs.b);
            assert.deepEqual(
                writesB[writesB.length - 1],
                { target: refs.b, rect: slot["rect"] as { x: number; y: number; w: number; h: number } },
                "restored tile written exactly once at its planned slot",
            );
            assert.deepEqual(mocks.floatingWrites[mocks.floatingWrites.length - 1], { id: "win-b", floating: false });
            assert.ok(
                mocks.logs.some(
                    (entry) => entry.includes("kind=toggle-float") && entry.includes("outcome=planned-applied"),
                ),
                "applied terminal logged for the cleared unfloat",
            );
            // Settled tiling never re-clears.
            floatingB = false;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-b9-n4" });
            const settledCalls = mocks.dbusCalls.length;
            fire(mocks, "geometry");
            runDebounce(mocks);
            assert.equal(mocks.maximizeClears.length, clearsBefore + 1, "settled tiling never re-clears");
            if (mocks.dbusCalls.length > settledCalls) {
                const last = JSON.parse(
                    mocks.dbusCalls[mocks.dbusCalls.length - 1]?.payload as string,
                ) as Record<string, unknown>;
                assert.ok(!("floating" in wireOf(last, "win-b")), "settled member rides as a normal tile");
            }
        } finally {
            await engine.close();
        }
    });

    it("maximized automatic unfloat clears before admission and keeps the D3 tile through the real Planner", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            // Automatic float admission for the fixed client.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-b9-f1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Native maximize of the automatic float (still natively tiled).
            // Settlement derives from the recorded clear: pre-clear reads
            // show maximize, later reads show the restored frame.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    maximizedB: mocks.maximizeClears.length === 0,
                    fingerprint: "fp-b9-f2",
                });
            const callsBefore = mocks.dbusCalls.length;
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.maximizeClears.length, 1, "fixed clients still clear before admission");
            assert.equal(mocks.dbusCalls.length, callsBefore + 1, "explicit tile dispatches despite fixed hints");
            assert.ok(
                mocks.logs.some((line) => line.includes("outcome=observed-cleared")),
                "settlement observed before admission",
            );
            const reply = await flushPlan(mocks, engine, callsBefore);
            assert.equal(reply["outcome"], "planned", `unfloat plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "cleared fixed client rejoins the tiled topology",
            );
            const slot = desiredWindows(reply).find((entry) => entry["window"] === "win-b");
            const fixedWrites = mocks.geometries.filter((write) => write.target === refs.b);
            assert.deepEqual(
                fixedWrites[fixedWrites.length - 1],
                { target: refs.b, rect: slot?.["rect"] as { x: number; y: number; w: number; h: number } },
                "fixed tile written exactly once at its planned slot",
            );
            assert.ok(
                mocks.logs.some(
                    (entry) => entry.includes("kind=toggle-float") && entry.includes("outcome=planned-applied"),
                ),
                "applied terminal logged for the fixed unfloat",
            );
            // D3/D7: the same live client stays tiled with suppression and
            // survives the next observation through the real Planner.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-b9-f3" });
            const after = dispatchAdded(mocks);
            const excused = wireOf(after, "win-b");
            assert.ok(!("floating" in excused), "explicit tile wins, no re-float");
            assert.equal(excused["fixed_suppress"], true);
            assert.ok(!("fixed_auto" in excused));
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
        } finally {
            await engine.close();
        }
    });

    it("still-maximized settlement consults no Planner dispatch", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-b9-s1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // The native clear lands but the host keeps maximize: narrow
            // refusal, no Planner contact, no stuck operation.
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, maximizedB: true, fingerprint: "fp-b9-s2" });
            const callsBefore = mocks.dbusCalls.length;
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.maximizeClears.length, 1, "one clear attempt per press");
            assert.equal(mocks.dbusCalls.length, callsBefore, "raced clear never admits beneath retained maximize");
            assert.ok(mocks.logs.some((line) => line.includes("outcome=observed-maximized")));
            assert.ok(
                mocks.logs.some((line) => line.includes("float-refused-maximize-clear") && line.includes("cause=still-maximized")),
            );
            assert.equal(adapter.isInFlight, false, "operation released with no stuck state");
        } finally {
            await engine.close();
        }
    });

    it("a refused native sticky write preserves the automatic identity", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-k1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // The native sticky write refuses: no planner dispatch, and the
            // failed command must not destroy the automatic identity.
            mocks.desktopToggleImpl = (): MaximizeClearOutcome => "missing";
            const callsBefore = mocks.dbusCalls.length;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-k2" });
            adapter.requestSticky();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsBefore, "refused setter dispatches nothing");
            assert.equal(mocks.desktopToggles[mocks.desktopToggles.length - 1]?.allDesktops, true);
            assert.ok(
                mocks.logs.some((line) => line.includes("sticky-toggle") && line.includes("outcome=missing")),
                "refusal terminal logged",
            );
            // The next observation still floats with origin through the real
            // Planner: the failed intent owned no state change.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-k3" });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "failed sticky keeps automatic");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            assert.deepEqual(mocks.floatingWrites, [], "no intentional setters on the failed path");
            // The retry with a working setter adopts as intentional with a
            // floating prior, proving the preserved record still classifies.
            mocks.desktopToggleImpl = (): MaximizeClearOutcome => "invoked";
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-k4" });
            adapter.requestSticky();
            runDebounce(mocks);
            assert.equal(mocks.desktopToggles[mocks.desktopToggles.length - 1]?.allDesktops, true);
            mocks.observeImpl = () =>
                makeObserved(refs, { floatingB: true, stickyB: true, fingerprint: "fp-real-k5" });
            const sticky = dispatchAdded(mocks);
            assert.ok(!("fixed_auto" in wireOf(sticky, "win-b")), "retry adopts as intentional");
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
        } finally {
            await engine.close();
        }
    });

    it("a timed-out unfloat flight keeps the automatic until applied proof", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-t1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-t2" });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "unfloat dispatches toggle-float");
            // The reply never arrives: the timeout terminal fails the
            // flight with no application and no record change.
            const writesBefore = mocks.geometries.length;
            fireTimeout(mocks);
            assert.ok(
                mocks.logs.some((entry) => entry.includes("outcome=timeout")),
                `timeout terminal logged: ${mocks.logs.slice(-4).join(" | ")}`,
            );
            assert.equal(mocks.geometries.length, writesBefore, "dead flight writes nothing");
            assert.deepEqual(mocks.floatingWrites, [], "dead flight calls no setters");
            // The staged tile win died with the flight: the next
            // observation still floats with origin.
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-t3" });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "timeout keeps automatic");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            // The late reply is fenced and ignored: no dispatch, no writes.
            const lateReply = await engine.send(mocks.dbusCalls[1]?.payload as string);
            mocks.callbacks[1]?.(lateReply);
            assert.equal(mocks.dbusCalls.length, 3, "late reply dispatches nothing");
            assert.equal(mocks.geometries.length, writesBefore, "late reply writes nothing");
            // The retry tiles with full apply proof once the plan lands.
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-t4" });
            adapter.requestFloat();
            runDebounce(mocks);
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `retry plans, got ${JSON.stringify(reply)}`);
            const slot = desiredWindows(reply).find((entry) => entry["window"] === "win-b");
            assert.ok(slot !== undefined, "retry rejoins the topology");
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [{ target: refs.b, rect: slot["rect"] as { x: number; y: number; w: number; h: number } }],
                "retry places the tile exactly once",
            );
            assert.ok(
                mocks.logs.some(
                    (entry) => entry.includes("kind=toggle-float") && entry.includes("outcome=planned-applied"),
                ),
                "applied terminal logged for the retry",
            );
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-t5" });
            const after = dispatchAdded(mocks);
            assert.ok(!("floating" in wireOf(after, "win-b")), "committed tile wins");
            assert.equal(wireOf(after, "win-b")["fixed_suppress"], true);
        } finally {
            await engine.close();
        }
    });

    it("a drifted unfloat reply replans once and still tiles on proof", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-d1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-d2" });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "unfloat dispatches toggle-float");
            // The frame drifts before the reply lands: the boundary
            // replans exactly once with the staged intent carried over,
            // applying nothing yet.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    fingerprint: "fp-real-d3",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const writesBeforeReplan = mocks.geometries.length;
            await flushPlan(mocks, engine, 1);
            assert.equal(mocks.dbusCalls.length, 3, "one prewrite replan");
            assert.ok(
                mocks.logs.some((entry) => entry.includes("stale-replan")),
                "replan terminal logged",
            );
            assert.equal(mocks.geometries.length, writesBeforeReplan, "replan applies nothing");
            // The stable retry reply matches and applies: the carried stage
            // commits, tiling with suppression. The fingerprint stays
            // identical (same content), satisfying the reply fence.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    fingerprint: "fp-real-d3",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const reply = await flushPlan(mocks, engine, 2);
            assert.equal(reply["outcome"], "planned", `replan plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "replanned unfloat rejoins the topology",
            );
            assert.ok(
                mocks.logs.some(
                    (entry) => entry.includes("kind=toggle-float") && entry.includes("outcome=planned-applied"),
                ),
                "applied terminal logged for the replan",
            );
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-d5",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const after = dispatchAdded(mocks);
            assert.ok(!("floating" in wireOf(after, "win-b")), "carried stage committed the tile");
            assert.equal(wireOf(after, "win-b")["fixed_suppress"], true);
        } finally {
            await engine.close();
        }
    });

    it("a twice-drifted unfloat fails closed preserving the automatic", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-e1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-e2" });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, 2, "unfloat dispatches toggle-float");
            // First drift replans once; a second drift before that reply
            // lands exceeds the single retry and fails stale-scope with
            // nothing applied and no record change.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    fingerprint: "fp-real-e3",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const writesBeforeDrift = mocks.geometries.length;
            const floatsBeforeDrift = mocks.floatingWrites.length;
            await flushPlan(mocks, engine, 1);
            assert.equal(mocks.dbusCalls.length, 3, "one prewrite replan");
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    fingerprint: "fp-real-e4",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            await flushPlan(mocks, engine, 2);
            assert.ok(
                mocks.logs.some(
                    (entry) => entry.includes("kind=toggle-float") && entry.includes("stale-scope"),
                ),
                `stale-scope terminal logged: ${mocks.logs.slice(-6).join(" | ")}`,
            );
            assert.deepEqual(mocks.geometries.length, writesBeforeDrift, "failed flight writes nothing");
            assert.equal(mocks.floatingWrites.length, floatsBeforeDrift, "failed flight calls no setters");
            // The staged tile win died with the flight: still automatic,
            // and a stable retry tiles with proof.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    fingerprint: "fp-real-e5",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "failed terminal keeps automatic");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            adapter.requestFloat();
            runDebounce(mocks);
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `retry plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "retry rejoins the topology",
            );
        } finally {
            await engine.close();
        }
    });

    it("a pinned normal that later turns fixed stays tiled with suppression", async () => {        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.constraintsImpl = (): PlanWindowConstraints | null => null;
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-pin1" });
            const first = dispatchAdded(mocks);
            assert.ok(!("fixed_suppress" in wireOf(first, "win-b")), "hintless pin stays byte-identical");
            const reply1 = await flushPlan(mocks, engine, 0);
            assert.equal(reply1["outcome"], "planned");
            // Hints later report fixed for the same live client: admission-only
            // keeps the pinned tile, and the wire suppresses core reclassify.
            // Hint-only changes never dispatch alone, so the pin rides the
            // next drift dispatch (nudged frame below).
            mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
                if (target === refs.b) {
                    return { resizeable: false, minSize: { ...FIXED }, maxSize: { ...FIXED } };
                }
                return null;
            };
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-pin2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const second = dispatchAdded(mocks);
            const pinned = wireOf(second, "win-b");
            assert.ok(!("floating" in pinned), "become-fixed never re-floats a pinned tile");
            assert.equal(pinned["fixed_suppress"], true);
            assert.deepEqual(pinned["min_size"], FIXED);
            const reply2 = await flushPlan(mocks, engine, 1);
            assert.equal(reply2["outcome"], "planned", `pinned tile plans, got ${JSON.stringify(reply2)}`);
            assert.ok(
                desiredWindows(reply2).some((entry) => entry["window"] === "win-b"),
                "pinned client keeps its tiled slot",
            );
        } finally {
            await engine.close();
        }
    });

    it("hint loss on a retained automatic never re-tiles and never writes", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-h1" });
            dispatchAdded(mocks);
            const reply1 = await flushPlan(mocks, engine, 0);
            assert.equal(reply1["outcome"], "planned");
            mocks.constraintsImpl = (): PlanWindowConstraints | null => null;
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-h2" });
            const second = dispatchAdded(mocks);
            assert.equal(wireOf(second, "win-b")["floating"], true, "hint loss never re-tiles");
            assert.equal(wireOf(second, "win-b")["fixed_auto"], true);
            const reply2 = await flushPlan(mocks, engine, 1);
            assert.equal(reply2["outcome"], "planned", `retained float plans, got ${JSON.stringify(reply2)}`);
            assert.ok(
                !desiredWindows(reply2).some((entry) => entry["window"] === "win-b"),
                "retained automatic still takes no slot",
            );
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "still no writes after hint loss",
            );
            // A throwing constraint reader also never resets identity.
            mocks.constraintsImpl = (): PlanWindowConstraints | null => {
                throw new Error("constraints unavailable");
            };
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-h3" });
            const third = dispatchAdded(mocks);
            assert.equal(wireOf(third, "win-b")["floating"], true, "reader failure keeps identity");
        } finally {
            await engine.close();
        }
    });

    it("minimize omission and reappearance retain the same automatic identity", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-m1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Minimized: the client is omitted from this scoped snapshot.
            // Absence must not evict the record.
            mocks.observeImpl = () => makeObserved(refs, { omitB: true, fingerprint: "fp-real-m2" });
            const omitted = dispatchAdded(mocks);
            assert.ok(
                !wireWindows(omitted).some((entry) => entry["window"] === "win-b"),
                "omitted snapshot carries no row",
            );
            await flushPlan(mocks, engine, 1);
            // Reappear with the same live ref: identity retained.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-m3" });
            const back = dispatchAdded(mocks);
            assert.equal(wireOf(back, "win-b")["floating"], true);
            assert.equal(wireOf(back, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 2);
            assert.equal(reply["outcome"], "planned", `reappearance plans, got ${JSON.stringify(reply)}`);
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no writes across hide and reappear",
            );
        } finally {
            await engine.close();
        }
    });

    it("cross-domain and background snapshots keep foreign automatic identity", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const hiddenRef = {};
        mocks.observeHiddenImpl = () => [makeHidden(refs, hiddenRef)];
        const engine = EngineBridge.start();
        try {
            const flushed = { count: 0 };
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-x1" });
            dispatchAdded(mocks);
            await drainOutstanding(mocks, engine, flushed);
            // A background-domain snapshot carries no win-b row; the reset
            // keys on CURRENT sightings, so the foreign call preserves
            // the foreground automatic record.
            adapter.retileAutomaticFixed("out-1", "ws-9", ["win-h"]);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-x2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "foreign retile keeps identity");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            await drainOutstanding(mocks, engine, flushed);
            // D6: enable resets by sighting, so the homed-domain call
            // re-evaluates and fresh-classifies the still-fixed client.
            // An explicit suppress tiles (D3, covered in the unfloat rows).
            adapter.retileAutomaticFixed("out-1", "ws-1", ["win-a", "win-b"]);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-x4",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            const still = dispatchAdded(mocks);
            assert.equal(wireOf(still, "win-b")["floating"], true, "homed enable keeps floating");
            assert.equal(wireOf(still, "win-b")["fixed_auto"], true);
            await drainOutstanding(mocks, engine, flushed);
        } finally {
            await engine.close();
        }
    });

    it("a replaced native reference under a reused id classifies fresh", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, floatingB: true });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.ok(mocks.dbusCalls.length > 0, "unfloat must dispatch");
            await flushPlan(mocks, engine, 0);
            // Same id, replaced native reference: a new live client, so the
            // stale suppress never inherits and fixed hints float again.
            const freshB = {};
            const live = { a: refs.a, b: freshB };
            mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
                if (target === freshB) {
                    return { resizeable: false, minSize: { ...FIXED }, maxSize: { ...FIXED } };
                }
                return { resizeable: true, minSize: null, maxSize: null };
            };
            mocks.observeImpl = () => makeObserved(live, { fingerprint: "fp-real-r2" });
            const payload = dispatchAdded(mocks);
            assert.equal(wireOf(payload, "win-b")["floating"], true);
            assert.equal(wireOf(payload, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 1);
            assert.equal(reply["outcome"], "planned", `fresh client plans, got ${JSON.stringify(reply)}`);
            // Verified removal evicts: the id may classify again afterwards.
            adapter.noteNativeRemovedId("win-b");
            mocks.observeImpl = () => makeObserved(live, { fingerprint: "fp-real-r3" });
            const again = dispatchAdded(mocks);
            assert.equal(wireOf(again, "win-b")["floating"], true);
        } finally {
            await engine.close();
        }
    });

    it("sticky-on adopts the automatic and Meta+G tiles while prior-float restores", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-s1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Meta+Shift+G on the automatic float adopts it as intentional:
            // a native sticky write, never a planner dispatch or an
            // intentional float setter.
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-s2" });
            const callsBeforeSticky = mocks.dbusCalls.length;
            adapter.requestSticky();
            runDebounce(mocks);
            assert.equal(mocks.dbusCalls.length, callsBeforeSticky, "sticky-on writes natively, no dispatch");
            assert.equal(mocks.desktopToggles[mocks.desktopToggles.length - 1]?.allDesktops, true);
            assert.deepEqual(mocks.floatingWrites, [], "no intentional setters on sticky-on");
            // Native sticky observed (sticky floats observe floating): the
            // wire carries intentional float with no automatic origin.
            mocks.observeImpl = () =>
                makeObserved(refs, { floatingB: true, stickyB: true, fingerprint: "fp-real-s3" });
            const sticky = dispatchAdded(mocks);
            const row = wireOf(sticky, "win-b");
            assert.equal(row["floating"], true);
            assert.ok(!("fixed_auto" in row), "sticky adoption drops automatic origin");
            const stickyReply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(stickyReply["outcome"], "planned", `sticky plans, got ${JSON.stringify(stickyReply)}`);
            // Meta+G on the sticky window tiles with a suppress pin.
            mocks.observeImpl = () =>
                makeObserved(refs, { focused: refs.b, floatingB: true, stickyB: true, fingerprint: "fp-real-s4" });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.desktopToggles[mocks.desktopToggles.length - 1]?.allDesktops, false);
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-s5" });
            const tiled = dispatchAdded(mocks);
            assert.ok(!("floating" in wireOf(tiled, "win-b")), "Meta+G tiles the sticky client");
            assert.equal(wireOf(tiled, "win-b")["fixed_suppress"], true);
            const tiledReply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(tiledReply["outcome"], "planned", `sticky-off tile plans, got ${JSON.stringify(tiledReply)}`);
        } finally {
            await engine.close();
        }
    });

    it("externally observed sticky adopts as intentional without origin", async () => {
        // No requestSticky call: KWin (or a rule) made the automatic
        // float sticky externally. The verified sticky observation must
        // adopt it as intentional with no fixed_auto provenance, keep it
        // through workspace enable, and follow the existing
        // previous-float rules on sticky-off.
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-n1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // External sticky, never requested: adoption drops the origin.
            mocks.observeImpl = () =>
                makeObserved(refs, { floatingB: true, stickyB: true, fingerprint: "fp-real-n2" });
            const adopted = dispatchAdded(mocks);
            const row = wireOf(adopted, "win-b");
            assert.equal(row["floating"], true);
            assert.equal(row["sticky"], true);
            assert.ok(!("fixed_auto" in row), "external sticky adopts without origin");
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            // Hint churn on the adopted sticky never recreates an origin.
            mocks.constraintsImpl = (): PlanWindowConstraints | null => null;
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    floatingB: true,
                    stickyB: true,
                    fingerprint: "fp-real-n3",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const churned = dispatchAdded(mocks);
            assert.ok(!("fixed_auto" in wireOf(churned, "win-b")), "hint churn recreates no origin");
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            // Workspace enable keeps the adopted sticky: it is intentional,
            // never a reset candidate (recordless, and absent from the
            // sighted reset only if omitted - here sighted but sticky).
            adapter.retileAutomaticFixed("out-1", "ws-1", ["win-a", "win-b"]);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    floatingB: true,
                    stickyB: true,
                    fingerprint: "fp-real-n4",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "enable keeps adopted sticky");
            assert.ok(!("fixed_auto" in wireOf(kept, "win-b")), "enable assigns no origin");
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            // Meta+G with the adopted unknown-float origin restores the
            // floating prior per the existing sticky rules.
            mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
                if (target === refs.b) {
                    return { resizeable: false, minSize: { ...FIXED }, maxSize: { ...FIXED } };
                }
                return { resizeable: true, minSize: null, maxSize: null };
            };
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    focused: refs.b,
                    floatingB: true,
                    stickyB: true,
                    fingerprint: "fp-real-n5",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            adapter.requestFloat();
            runDebounce(mocks);
            assert.equal(mocks.desktopToggles[mocks.desktopToggles.length - 1]?.allDesktops, false);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    floatingB: true,
                    fingerprint: "fp-real-n6",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            const restored = dispatchAdded(mocks);
            assert.equal(wireOf(restored, "win-b")["floating"], true, "prior-float restores floating");
            assert.ok(!("fixed_auto" in wireOf(restored, "win-b")), "restored float stays intentional");
        } finally {
            await engine.close();
        }
    });

    it("born maximized fixed floats beneath the overlay without clearing it", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { maximizedB: true, fingerprint: "fp-real-max1" });
            const payload = dispatchAdded(mocks);
            assert.equal(wireOf(payload, "win-b")["floating"], true);
            assert.equal(wireOf(payload, "win-b")["fixed_auto"], true);
            assert.deepEqual(mocks.maximizeClears, [], "native maximize kept");
            const reply = await flushPlan(mocks, engine, 0);
            assert.equal(reply["outcome"], "planned", `maximized admission plans, got ${JSON.stringify(reply)}`);
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no geometry under the maximize overlay",
            );
            assert.deepEqual(mocks.floatingWrites, [], "no intentional setters under maximize");
        } finally {
            await engine.close();
        }
    });

    it("born fullscreen exits fresh with fixed floating untouched", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fs1" });
            const held = dispatchAdded(mocks);
            assert.ok(!("fixed_auto" in wireOf(held, "win-b")), "born-fullscreen bypass claims no origin");
            const heldReply = await flushPlan(mocks, engine, 0);
            assert.equal(heldReply["outcome"], "planned", `held plans, got ${JSON.stringify(heldReply)}`);
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no writes while fullscreen held",
            );
            // First normal observation is fresh admission (D5): fixed
            // floats automatic with no writes at all (no geometry,
            // keep-above, focus, stacking, or maximize clear).
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fs2" });
            const exit = dispatchAdded(mocks);
            assert.equal(wireOf(exit, "win-b")["floating"], true, "fixed exit floats");
            assert.equal(wireOf(exit, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 1);
            assert.equal(reply["outcome"], "planned", `exit plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "exited float takes no slot",
            );
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "exit writes no geometry to the fixed float",
            );
            assert.deepEqual(mocks.keepAboveWrites, [], "no keep-above around fullscreen exit");
            assert.deepEqual(mocks.floatingWrites, [], "no intentional setters around fullscreen exit");
            assert.deepEqual(mocks.maximizeClears, [], "no maximize clear for the fixed exit");
            assert.deepEqual(mocks.desktopToggles, [], "no sticky writes around fullscreen exit");
            const line = mocks.logs.find((entry) => entry.includes("fixed-size-classification"));
            assert.ok(line !== undefined, "bounded classification diagnostic emitted");
            assert.ok(!line.includes("win-"), line);
        } finally {
            await engine.close();
        }
    });

    it("born fullscreen nonfixed exits tiled with placement", async () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.constraintsImpl = (): PlanWindowConstraints | null => null;
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fsn1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fsn2" });
            const exit = dispatchAdded(mocks);
            assert.ok(!("floating" in wireOf(exit, "win-b")), "nonfixed exit tiles");
            const reply = await flushPlan(mocks, engine, 1);
            assert.equal(reply["outcome"], "planned", `exit plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "exited tile rejoins the topology",
            );
        } finally {
            await engine.close();
        }
    });

    it("held fullscreen exit with maximize and fixed hints skips the clear", async () => {
        // D5 maximize-clear gate: a newly fixed float on first exit gets
        // no writes including maximize clear.
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fsm1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Exit still maximized with fixed hints: floats, no clear.
            mocks.observeImpl = () =>
                makeObserved(refs, { maximizedB: true, fingerprint: "fp-real-fsm2" });
            const exit = dispatchAdded(mocks);
            assert.equal(wireOf(exit, "win-b")["floating"], true, "fixed exit floats");
            assert.equal(wireOf(exit, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 1);
            assert.equal(reply["outcome"], "planned", `exit plans, got ${JSON.stringify(reply)}`);
            assert.deepEqual(mocks.maximizeClears, [], "no maximize clear for fixed exit");
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no geometry for fixed exit",
            );
            assert.deepEqual(
                mocks.activeWrites.filter((target) => target === refs.b),
                [],
                "no refocus of the exiting float",
            );
            assert.deepEqual(mocks.keepAboveWrites, [], "no keep-above for fixed exit");
            assert.deepEqual(mocks.desktopToggles, [], "no sticky writes for fixed exit");
            assert.ok(
                mocks.logs.some((line) => line.includes("outcome=skipped-fixed")),
                "skip-fixed diagnostic logged",
            );
        } finally {
            await engine.close();
        }
    });

    it("fullscreen hint churn and predicate switch classify only at first exit", async () => {
        // D5: hints/predicate changing while held must not classify; the
        // first non-fullscreen observation for the same lifetime uses
        // CURRENT state. Born hintless under both-axes, fixed hints and
        // either-axis arrive while held: exit floats.
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let predicate: unknown = "both-axes-fixed";
        Object.assign(mocks.env, { readFixedSizePredicate: (): unknown => predicate });
        mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
            if (target === refs.b && (mocks as { fixedOn?: boolean }).fixedOn === true) {
                return { resizeable: false, minSize: { ...FIXED }, maxSize: { ...FIXED } };
            }
            return { resizeable: true, minSize: null, maxSize: null };
        };
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fc1" });
            const held = dispatchAdded(mocks);
            assert.ok(!("fixed_auto" in wireOf(held, "win-b")), "hintless hold claims nothing");
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Fixed hints arrive while still fullscreen: still held, still
            // no origin and no writes.
            (mocks as { fixedOn?: boolean }).fixedOn = true;
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fc2" });
            const churned = dispatchAdded(mocks);
            assert.ok(!("fixed_auto" in wireOf(churned, "win-b")), "no classification while fullscreen");
            assert.equal(await flushPlan(mocks, engine, 1).then((reply) => reply["outcome"]), "planned");
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no writes while held",
            );
            // Predicate switches to either-axis while held (one-axis would
            // also do, but fixed hints already discriminate): exit floats
            // with the current predicate on the wire.
            predicate = "either-axis-fixed";
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fc3" });
            const exit = dispatchAdded(mocks);
            assert.equal(exit["fixed_size_predicate"], "either-axis-fixed");
            assert.equal(wireOf(exit, "win-b")["floating"], true, "first exit floats");
            assert.equal(wireOf(exit, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 2);
            assert.equal(reply["outcome"], "planned", `exit plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "exited float takes no slot",
            );
        } finally {
            await engine.close();
        }
    });

    it("later fullscreen exits keep retained identity instead of re-admitting", async () => {
        // First exit floats the fixed born client; a second fullscreen
        // cycle with lost hints retains the automatic (D2) with no slot
        // and no writes: later exits are not fresh admissions.
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fl1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fl2" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 1).then((reply) => reply["outcome"]), "planned");
            // Second fullscreen: rides with origin, no actuation.
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fl3" });
            const under = dispatchAdded(mocks);
            assert.equal(wireOf(under, "win-b")["floating"], true);
            assert.equal(wireOf(under, "win-b")["fixed_auto"], true);
            assert.equal(await flushPlan(mocks, engine, 2).then((reply) => reply["outcome"]), "planned");
            // Hints are lost before the second exit: retention still
            // floats with origin (ordinary identity, not re-admission).
            mocks.constraintsImpl = (): PlanWindowConstraints | null => null;
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fl4" });
            const restored = dispatchAdded(mocks);
            assert.equal(wireOf(restored, "win-b")["floating"], true, "later exit retains float");
            assert.equal(wireOf(restored, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 3);
            assert.equal(reply["outcome"], "planned", `restore plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "retained float still takes no slot",
            );
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no writes across the second cycle",
            );
            assert.deepEqual(mocks.keepAboveWrites, [], "no keep-above across the second cycle");
            assert.deepEqual(
                mocks.activeWrites.filter((target) => target === refs.b),
                [],
                "no refocus across the second cycle",
            );
        } finally {
            await engine.close();
        }
    });

    it("a previously automatic float under fullscreen restores floating", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fr1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // The automatic float enters fullscreen: membership rides
            // through the overlay with its origin and no actuation.
            mocks.observeImpl = () => makeObserved(refs, { fullscreenB: true, fingerprint: "fp-real-fr2" });
            const under = dispatchAdded(mocks);
            assert.equal(wireOf(under, "win-b")["floating"], true);
            assert.equal(wireOf(under, "win-b")["fixed_auto"], true);
            const underReply = await flushPlan(mocks, engine, 1);
            assert.equal(underReply["outcome"], "planned", `overlay plans, got ${JSON.stringify(underReply)}`);
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no geometry under the fullscreen overlay",
            );
            // Exit restores the automatic float with no slot and no writes.
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-fr3" });
            const restored = dispatchAdded(mocks);
            assert.equal(wireOf(restored, "win-b")["floating"], true);
            assert.equal(wireOf(restored, "win-b")["fixed_auto"], true);
            const restoredReply = await flushPlan(mocks, engine, 2);
            assert.equal(restoredReply["outcome"], "planned", `restore plans, got ${JSON.stringify(restoredReply)}`);
            assert.ok(
                !desiredWindows(restoredReply).some((entry) => entry["window"] === "win-b"),
                "restored float still takes no slot",
            );
        } finally {
            await engine.close();
        }
    });

    it("workspace enable keeps automatic floating while explicit overrides tile", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-w1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // D6: enable resets by sighting; the resync re-evaluates and
            // fresh-classifies the still-fixed client. No writes to it.
            adapter.retileAutomaticFixed("out-1", "ws-1", ["win-a", "win-b"]);
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-w2" });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "enable keeps automatic floating");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 1);
            assert.equal(reply["outcome"], "planned", `enable plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "kept float takes no slot",
            );
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no geometry write to the kept float",
            );
            // Explicit same-live-client tile overrides still tile (D3):
            // unfloat the automatic, prove the slot, and confirm the
            // suppress pin survives.
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-w3" });
            adapter.requestFloat();
            runDebounce(mocks);
            const unfloat = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(unfloat["outcome"], "planned", `unfloat plans, got ${JSON.stringify(unfloat)}`);
            assert.ok(
                desiredWindows(unfloat).some((entry) => entry["window"] === "win-b"),
                "override rejoins the topology",
            );
        } finally {
            await engine.close();
        }
    });

    it("rejection and restart keep or recompute identity per D2 and D7", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        const appliedTerminal = (op: string): void => {
            const line = [...mocks.logs]
                .reverse()
                .find((entry) => entry.includes(`kind=${op}`) && entry.includes("outcome=planned-applied"));
            assert.ok(line !== undefined, `applied terminal for ${op}: ${mocks.logs.slice(-4).join(" | ")}`);
        };
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-j1" });
            dispatchAdded(mocks);
            const reply1 = await flushPlan(mocks, engine, 0);
            assert.equal(reply1["outcome"], "planned");
            appliedTerminal("reconcile");
            // An identical re-observation re-carries the same automatic
            // identity (constraint rereads never reset it): the wire still
            // floats with origin, the Engine still plans, and no fresh
            // classification diagnostic fires for the retained mark.
            const logsAfterAdmit = mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length;
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-j1" });
            const recarried = dispatchAdded(mocks);
            assert.equal(wireOf(recarried, "win-b")["floating"], true, "reread keeps identity");
            assert.equal(wireOf(recarried, "win-b")["fixed_auto"], true);
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            appliedTerminal("reconcile");
            assert.equal(
                mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length,
                logsAfterAdmit,
                "retained marks emit no fresh classification",
            );
            // An unanswered drift flight is an actual failure terminal
            // (timeout, no retry): the authoritative record never moves
            // without application, so the next observation still floats
            // with origin and no setter ever fired for the dead flight.
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-j2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            dispatchAdded(mocks);
            const writesBeforeTimeout = mocks.geometries.length;
            fireTimeout(mocks);
            assert.ok(
                mocks.logs.some((entry) => entry.includes("outcome=timeout")),
                `timeout terminal logged: ${mocks.logs.slice(-4).join(" | ")}`,
            );
            assert.equal(mocks.geometries.length, writesBeforeTimeout, "dead flight writes nothing");
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-j3",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "failed terminal keeps identity");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            appliedTerminal("reconcile");
            // Restart clears lifecycle state (D7): a fresh adapter and a
            // fresh planner recompute from live hints instead of
            // persisting pins.
            await engine.close();
            mocks.subscribes.length = 0;
            mocks.timers.length = 0;
            const engine2 = EngineBridge.start();
            try {
                enableAdapter(mocks);
                mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-j3" });
                const recomputed = dispatchAdded(mocks);
                assert.equal(wireOf(recomputed, "win-b")["floating"], true);
                assert.equal(wireOf(recomputed, "win-b")["fixed_auto"], true);
                assert.equal(
                    await flushPlan(mocks, engine2, mocks.dbusCalls.length - 1).then(
                        (reply) => reply["outcome"],
                    ),
                    "planned",
                );
                const before = mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length;
                mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-j3" });
                fire(mocks, "geometry");
                runDebounce(mocks);
                assert.equal(
                    mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length,
                    before,
                    "converged frames stay silent",
                );
            } finally {
                await engine2.close();
            }
        } finally {
            await engine.close();
        }
    });

    it("an explicit tile survives release and readmission on a fresh domain", async () => {
        // D3 across a real domain change: unfloat on ws-1 (suppress),
        // release the source domain, then readmit the same live client
        // on ws-2. The wire must carry fixed_suppress (never floating),
        // and the fresh session must tile it with no automatic origin.
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-v1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-real-v2" });
            adapter.requestFloat();
            runDebounce(mocks);
            const unfloat = await flushPlan(mocks, engine, 1);
            assert.equal(unfloat["outcome"], "planned", `unfloat plans, got ${JSON.stringify(unfloat)}`);
            // Release the source domain through the real route.
            let releaseOutcome: string | null = null;
            const releaseObserved = mocks.observeImpl();
            assert.ok(releaseObserved !== null, "source observation readable");
            adapter.requestDomainRelease(snapshotOf(releaseObserved), (outcome) => {
                releaseOutcome = outcome;
            });
            runDebounce(mocks);
            const releaseReply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(releaseReply["outcome"], "released", `release confirms, got ${JSON.stringify(releaseReply)}`);
            assert.equal(releaseOutcome, "released", "release callback confirms");
            // Same live client readmitted on ws-2: tiled with suppression.
            const writesBeforeMove = mocks.geometries.length;
            mocks.observeImpl = () => makeObserved(refs, { domainWorkspace: "ws-2", fingerprint: "fp-real-v3" });
            const moved = dispatchAdded(mocks);
            assert.equal((moved["domain"] as Record<string, unknown>)["workspace"], "ws-2");
            const row = wireOf(moved, "win-b");
            assert.ok(!("floating" in row), "moved tile never re-floats");
            assert.equal(row["fixed_suppress"], true, "suppress survives the domain change");
            assert.ok(!("fixed_auto" in row), "no automatic origin after explicit tile");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `readmit plans, got ${JSON.stringify(reply)}`);
            const slot = desiredWindows(reply).find((entry) => entry["window"] === "win-b");
            assert.ok(slot !== undefined, "moved tile keeps a slot on the fresh domain");
            assert.deepEqual(
                mocks.geometries.slice(writesBeforeMove).filter((write) => write.target === refs.b),
                [{ target: refs.b, rect: slot["rect"] as { x: number; y: number; w: number; h: number } }],
                "moved tile placed exactly once",
            );
        } finally {
            await engine.close();
        }
    });

    it("an automatic readmission stays floating on enable homing", async () => {
        // The same live client admitted automatic on ws-1, released, and
        // readmitted automatic on ws-2: enable calls keep it floating on
        // its current homing (D6); explicit suppress would tile (D3).
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-g1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            let releaseOutcome: string | null = null;
            const releaseObserved = mocks.observeImpl();
            assert.ok(releaseObserved !== null, "source observation readable");
            adapter.requestDomainRelease(snapshotOf(releaseObserved), (outcome) => {
                releaseOutcome = outcome;
            });
            runDebounce(mocks);
            const releaseReply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(releaseReply["outcome"], "released");
            assert.equal(releaseOutcome, "released");
            mocks.observeImpl = () => makeObserved(refs, { domainWorkspace: "ws-2", fingerprint: "fp-real-g2" });
            const moved = dispatchAdded(mocks);
            assert.equal(wireOf(moved, "win-b")["floating"], true, "moved client stays automatic");
            assert.equal(wireOf(moved, "win-b")["fixed_auto"], true);
            const admit = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(admit["outcome"], "planned", `readmit plans, got ${JSON.stringify(admit)}`);
            assert.ok(
                !desiredWindows(admit).some((entry) => entry["window"] === "win-b"),
                "readmitted automatic takes no slot",
            );
            // Stale-domain reset keys on sightings: ws-1 no longer sights
            // win-b, so the transferred record survives and keeps floating.
            adapter.retileAutomaticFixed("out-1", "ws-1", ["win-a"]);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    domainWorkspace: "ws-2",
                    fingerprint: "fp-real-g3",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const missed = dispatchAdded(mocks);
            assert.equal(wireOf(missed, "win-b")["floating"], true, "stale retile misses");
            assert.equal(
                await flushPlan(mocks, engine, mocks.dbusCalls.length - 1).then((reply) => reply["outcome"]),
                "planned",
            );
            // Current-domain enable resets by sighting and re-evaluates:
            // still fixed, so it stays floating (D6).
            adapter.retileAutomaticFixed("out-1", "ws-2", ["win-a", "win-b"]);
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    domainWorkspace: "ws-2",
                    fingerprint: "fp-real-g4",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "enable keeps floating");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `enable plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "kept float takes no slot",
            );
        } finally {
            await engine.close();
        }
    });

    it("enable reset preserves omitted automatic identity without fresh classification", async () => {
        // G3: the reset keys on CURRENT sightings. A minimized (omitted)
        // automatic is absent from the sighted set, so its record
        // survives and the next dispatch retains it silently. A
        // domain-keyed reset would erase it and re-emit classification.
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-real-om1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            const classified = mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length;
            assert.equal(classified, 1, "admission classifies once");
            // Omitted from the enable sightings (minimized): preserved.
            adapter.retileAutomaticFixed("out-1", "ws-1", ["win-a"]);
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), "auto", "omitted record kept");
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-om2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const kept = dispatchAdded(mocks);
            assert.equal(wireOf(kept, "win-b")["floating"], true, "omitted automatic retained");
            assert.equal(wireOf(kept, "win-b")["fixed_auto"], true);
            assert.equal(
                mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length,
                classified,
                "retained marks emit no fresh classification",
            );
            assert.equal(
                await flushPlan(mocks, engine, 1).then((reply) => reply["outcome"]),
                "planned",
            );
            // Sighted reset does re-evaluate: same call with win-b
            // included drops the record, and the resync fresh-classifies.
            adapter.retileAutomaticFixed("out-1", "ws-1", ["win-a", "win-b"]);
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), null, "sighted record reset");
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-real-om3",
                    rects: { "win-a": { x: 8, y: 0, w: 600, h: 800 } },
                });
            const fresh = dispatchAdded(mocks);
            assert.equal(wireOf(fresh, "win-b")["floating"], true, "sighted fixed floats fresh");
            assert.equal(wireOf(fresh, "win-b")["fixed_auto"], true);
            assert.equal(
                mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length,
                classified + 1,
                "fresh classification emits once",
            );
        } finally {
            await engine.close();
        }
    });
});

describe("automatic fixed-float focus preservation through the real Planner", () => {
    function replyFocusWindow(reply: Record<string, unknown>): string | undefined {
        const focus = reply["desired_focus"] as Record<string, unknown> | undefined;
        const geometry = reply["desired_geometry"] as Array<Record<string, unknown>> | undefined;
        if (focus === undefined || focus === null || !Array.isArray(geometry)) {
            return undefined;
        }
        const found = geometry.find((entry) => entry["leaf"] === focus["leaf"]);
        return found === undefined ? undefined : (found["window"] as string);
    }

    function commandOp(payload: Record<string, unknown>): unknown {
        return (payload["command"] as Record<string, unknown>)["op"];
    }

    function automaticAttempt(mocks: Mocks, target: string): boolean {
        return mocks.logs.some(
            (line) => line.includes("automatic-focus-attempt") && line.includes(`target=${target}`),
        );
    }

    function automaticResult(mocks: Mocks, target: string, outcome: string): boolean {
        return mocks.logs.some(
            (line) =>
                line.includes("automatic-focus-result") &&
                line.includes(`target=${target}`) &&
                line.includes(`outcome=${outcome}`),
        );
    }

    function focusSkipped(mocks: Mocks, reason: string, target: string): boolean {
        return mocks.logs.some(
            (line) =>
                line.includes("focus-skipped") &&
                line.includes("kind=reconcile") &&
                line.includes(`reason=${reason}`) &&
                line.includes(`target=${target}`),
        );
    }

    it("an active automatic float keeps native focus across a survivor-focus reconcile", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-focus-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Native focus moves to the admitted automatic float; the next
            // lifecycle reconcile still carries retained tiled-survivor focus.
            // The mock active window tracks native focus exactly, so the
            // re-read guard cannot mask the theft with a stale active value.
            mocks.activeImpl = (): object | null => refs.b;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-ff-focus-2" });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "second dispatch is a lifecycle reconcile");
            assert.equal(wireOf(payload, "win-b")["floating"], true);
            assert.equal(wireOf(payload, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.equal(replyFocusWindow(reply), "win-a", "planner retains tiled-survivor focus");
            assert.deepEqual(mocks.activeWrites, [], "zero activation writes to any window");
            assert.equal(mocks.activeImpl(), refs.b, "automatic float remains active");
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no geometry write to the float",
            );
            assert.deepEqual(mocks.keepAboveWrites, [], "no keep-above write for the automatic float");
            assert.ok(focusSkipped(mocks, "automatic-active", "win-a"), "effective-floating skip logged");
            assert.ok(!automaticAttempt(mocks, "win-a"), "protected skip attempts no setter");
        } finally {
            await engine.close();
        }
    });

    it("hint loss retains automatic focus protection on reconcile", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-hint-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Hints are lost but the retained record keeps identity; native
            // focus sits on the automatic float.
            mocks.constraintsImpl = (): PlanWindowConstraints | null => null;
            mocks.activeImpl = (): object | null => refs.b;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-ff-hint-2" });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "second dispatch is a lifecycle reconcile");
            assert.equal(wireOf(payload, "win-b")["floating"], true, "hint loss never re-tiles");
            assert.equal(wireOf(payload, "win-b")["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.equal(replyFocusWindow(reply), "win-a", "planner retains tiled-survivor focus");
            assert.deepEqual(mocks.activeWrites, [], "zero activation writes to any window");
            assert.ok(focusSkipped(mocks, "automatic-active", "win-a"), "retained skip logged");
            assert.ok(!automaticAttempt(mocks, "win-a"), "protected skip attempts no setter");
        } finally {
            await engine.close();
        }
    });

    it("a replaced native reference under a reused id classifies fresh and stays protected", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        // The replacement carries the same fixed hints on a new reference.
        const freshRef = {};
        const fixedFor = new Set<object>([refs.b, freshRef]);
        mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
            if (fixedFor.has(target)) {
                return { resizeable: false, minSize: { ...FIXED }, maxSize: { ...FIXED } };
            }
            return { resizeable: true, minSize: null, maxSize: null };
        };
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-ref-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), "auto", "setup: original automatic recorded");
            // The live window is replaced under the reused id and takes
            // native focus: the stale mark must not linger, the replacement
            // classifies fresh, and the fresh automatic keeps focus.
            mocks.activeImpl = (): object | null => freshRef;
            mocks.observeImpl = () =>
                makeObserved(refs, { focused: freshRef, refB: freshRef, fingerprint: "fp-ff-ref-2" });
            const classifiedBefore = mocks.logs.filter((entry) =>
                entry.includes("fixed-size-classification"),
            ).length;
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "second dispatch is a lifecycle reconcile");
            assert.equal(wireOf(payload, "win-b")["floating"], true, "replacement floats fresh");
            assert.equal(wireOf(payload, "win-b")["fixed_auto"], true);
            assert.equal(
                mocks.logs.filter((entry) => entry.includes("fixed-size-classification")).length,
                classifiedBefore + 1,
                "replacement classifies fresh instead of inheriting",
            );
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), null, "stale record gone");
            assert.equal(adapter.originOfFixedClient("win-b", freshRef), "auto", "fresh automatic recorded");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.equal(replyFocusWindow(reply), "win-a", "planner retains tiled-survivor focus");
            assert.deepEqual(mocks.activeWrites, [], "zero activation writes to any window");
            assert.ok(focusSkipped(mocks, "automatic-active", "win-a"), "fresh automatic skip logged");
            assert.ok(!automaticAttempt(mocks, "win-a"), "protected skip attempts no setter");
        } finally {
            await engine.close();
        }
    });

    it("a resizable replacement under a reused id tiles with the stale automatic evicted", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-resize-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), "auto", "setup: original automatic recorded");
            // The live window is replaced by a resizable one under the
            // reused id and takes native focus: the stale automatic must
            // be evicted, the wire stays plain, and the tiled replacement
            // keeps its slot with no float protection skip.
            const freshRef = {};
            mocks.activeImpl = (): object | null => freshRef;
            mocks.observeImpl = () =>
                makeObserved(refs, { focused: freshRef, refB: freshRef, fingerprint: "fp-ff-resize-2" });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "second dispatch is a lifecycle reconcile");
            const row = wireOf(payload, "win-b");
            assert.ok(!("floating" in row), "resizable replacement never floats");
            assert.ok(!("fixed_auto" in row), "no automatic origin for the resizable replacement");
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), null, "stale automatic evicted");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "replacement keeps its tiled slot",
            );
            assert.equal(replyFocusWindow(reply), "win-b", "planner retains the tiled replacement");
            assert.deepEqual(mocks.activeWrites, [], "no activation writes to any window");
            assert.ok(
                !mocks.logs.some((line) => line.includes("focus-skipped")),
                "no float protection skip for the tile",
            );
            assert.ok(
                mocks.logs.some(
                    (line) => line.includes("kind=reconcile") && line.includes("outcome=planned-applied"),
                ),
                "replacement applies normally",
            );
        } finally {
            await engine.close();
        }
    });

    it("an explicit tile override is not protected as a float", async () => {
        const { refs, mocks } = fixedMocks();
        const adapter = enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-tile-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // Explicit unfloat of the automatic client: it rejoins the tiled
            // topology with suppression instead of floating.
            mocks.activeImpl = (): object | null => refs.b;
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-ff-tile-2" });
            adapter.requestFloat();
            runDebounce(mocks);
            const unfloat = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(unfloat["outcome"], "planned", `unfloat plans, got ${JSON.stringify(unfloat)}`);
            assert.ok(
                desiredWindows(unfloat).some((entry) => entry["window"] === "win-b"),
                "override rejoins the topology",
            );
            // The overridden tile focused: the next reconcile applies
            // normally with no float protection and no focus skip.
            mocks.observeImpl = () => makeObserved(refs, { focused: refs.b, fingerprint: "fp-ff-tile-3" });
            const logsBefore = mocks.logs.length;
            const payload = dispatchAdded(mocks);
            const overridden = wireOf(payload, "win-b");
            assert.ok(!("floating" in overridden), "explicit tile wins, no re-float");
            assert.equal(overridden["fixed_suppress"], true);
            assert.ok(!("fixed_auto" in overridden));
            assert.equal(adapter.originOfFixedClient("win-b", refs.b), "suppress", "override pins suppress, not auto");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "override keeps its tiled slot",
            );
            const freshLogs = mocks.logs.slice(logsBefore);
            assert.ok(!freshLogs.some((line) => line.includes("focus-skipped")), "no focus skip for the tile");
            assert.ok(
                freshLogs.some(
                    (line) => line.includes("kind=reconcile") && line.includes("outcome=planned-applied"),
                ),
                "reconcile applies normally",
            );
            assert.deepEqual(mocks.activeWrites, [], "no activation writes across the override");
        } finally {
            await engine.close();
        }
    });

    it("close-survivor refocus actuates the survivor with correlated attempt and acceptance", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-close-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // The automatic float closes with the tiled survivor already
            // focused, but KWin reports no live active window: the survivor
            // refocus must actuate exactly once with correlated
            // attempt/result evidence and no float protection skip.
            mocks.activeImpl = (): object | null => null;
            mocks.observeImpl = () => makeObserved(refs, { omitB: true, fingerprint: "fp-ff-close-2" });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "close dispatches a lifecycle reconcile");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `close converges, got ${JSON.stringify(reply)}`);
            assert.equal(replyFocusWindow(reply), "win-a", "planner keeps the survivor focused");
            assert.deepEqual(mocks.activeWrites, [refs.a], "survivor actuated exactly once");
            assert.ok(automaticAttempt(mocks, "win-a"), "setter attempt logged");
            assert.ok(automaticResult(mocks, "win-a", "setter-accepted"), "setter acceptance logged");
            assert.ok(
                !mocks.logs.some((line) => line.includes("focus-skipped")),
                "no protection skip on the survivor path",
            );
        } finally {
            await engine.close();
        }
    });

    it("a stale live active enforces planner focus with correlated attempt and acceptance", async () => {
        const { refs, mocks } = fixedMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-ghost-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            // KWin reports no live active window while the observation still
            // focuses the tiled survivor: the adapter enforces planner focus
            // with correlated attempt/result evidence. Setter acceptance is
            // reported honestly, never proven native focus.
            mocks.activeImpl = (): object | null => null;
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-ff-ghost-2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "drift dispatches a lifecycle reconcile");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.equal(replyFocusWindow(reply), "win-a", "planner keeps the survivor focused");
            assert.deepEqual(mocks.activeWrites, [refs.a], "stale active enforced to planner focus");
            assert.ok(automaticAttempt(mocks, "win-a"), "setter attempt logged");
            assert.ok(automaticResult(mocks, "win-a", "setter-accepted"), "setter acceptance logged");
            assert.ok(
                mocks.logs.some(
                    (line) => line.includes("kind=reconcile") && line.includes("outcome=planned-applied"),
                ),
                "enforced focus still applies",
            );
        } finally {
            await engine.close();
        }
    });

    it("a refused automatic setter logs attempt and failure with a write-failed terminal", async () => {
        const { refs, mocks } = fixedMocks();
        mocks.setActiveImpl = (_target: object): boolean => false;
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-deny-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.activeImpl = (): object | null => null;
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-ff-deny-2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "drift dispatches a lifecycle reconcile");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.deepEqual(mocks.activeWrites, [refs.a], "attempt recorded before the refusal");
            assert.ok(automaticAttempt(mocks, "win-a"), "setter attempt logged");
            assert.ok(automaticResult(mocks, "win-a", "setter-refused"), "setter refusal logged");
            assert.ok(
                mocks.logs.some(
                    (line) => line.includes("kind=reconcile") && line.includes("outcome=write-failed"),
                ),
                "write-failed terminal logged",
            );
        } finally {
            await engine.close();
        }
    });

    it("a throwing automatic setter logs attempt and failure with a write-failed terminal", async () => {
        const { refs, mocks } = fixedMocks();
        mocks.setActiveImpl = (_target: object): boolean => {
            throw new Error("native focus unavailable");
        };
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-ff-throw-1" });
            dispatchAdded(mocks);
            assert.equal(await flushPlan(mocks, engine, 0).then((reply) => reply["outcome"]), "planned");
            mocks.activeImpl = (): object | null => null;
            mocks.observeImpl = () =>
                makeObserved(refs, {
                    fingerprint: "fp-ff-throw-2",
                    rects: { "win-a": { x: 4, y: 0, w: 600, h: 800 } },
                });
            const payload = dispatchAdded(mocks);
            assert.equal(commandOp(payload), "reconcile", "drift dispatches a lifecycle reconcile");
            const reply = await flushPlan(mocks, engine, mocks.dbusCalls.length - 1);
            assert.equal(reply["outcome"], "planned", `reconcile plans, got ${JSON.stringify(reply)}`);
            assert.deepEqual(mocks.activeWrites, [refs.a], "attempt recorded before the throw");
            assert.ok(automaticAttempt(mocks, "win-a"), "setter attempt logged");
            assert.ok(automaticResult(mocks, "win-a", "threw"), "setter throw logged");
            assert.ok(
                mocks.logs.some(
                    (line) => line.includes("kind=reconcile") && line.includes("outcome=write-failed"),
                ),
                "write-failed terminal logged",
            );
        } finally {
            await engine.close();
        }
    });
});

const PARTIAL_ZERO_VECTORS = [
    { min: { w: 640, h: 0 }, max: { w: 640, h: 0 } },
    { min: { w: 0, h: 480 }, max: { w: 0, h: 480 } },
] as const;

function partialZeroMocks(vector: { min: { w: number; h: number }; max: { w: number; h: number } }): {
    refs: { a: object; b: object };
    mocks: Mocks;
} {
    const refs = makeRefs();
    const mocks = mockEnv(refs);
    mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
        if (target === refs.b) {
            return { resizeable: false, minSize: { ...vector.min }, maxSize: { ...vector.max } };
        }
        return { resizeable: true, minSize: null, maxSize: null };
    };
    return { refs, mocks };
}

describe("G-05 partial-zero admission through the real Planner", () => {
    it("tiles both partial-zero vectors by default and floats each under either-axis with no writes", async () => {
        for (const vector of PARTIAL_ZERO_VECTORS) {
            const label = `min/max=(${vector.min.w},${vector.min.h})`;
            // Default both-axes-fixed: the partial-zero client tiles with a slot.
            {
                const { refs, mocks } = partialZeroMocks(vector);
                enableAdapter(mocks);
                const engine = EngineBridge.start();
                try {
                    mocks.observeImpl = () => makeObserved(refs, { fingerprint: `fp-g05-both-${vector.min.w}x${vector.min.h}` });
                    const payload = dispatchAdded(mocks);
                    assert.ok(!("floating" in wireOf(payload, "win-b")), `${label} tiles by default`);
                    const reply = await flushPlan(mocks, engine, 0);
                    assert.equal(reply["outcome"], "planned", `${label} plans, got ${JSON.stringify(reply)}`);
                    assert.ok(
                        desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                        `${label} keeps a tiled slot by default`,
                    );
                } finally {
                    await engine.close();
                }
            }
            // Either-axis-fixed: the same vector floats via its nonzero axis
            // with no automatic writes to the float.
            {
                const { refs, mocks } = partialZeroMocks(vector);
                Object.assign(mocks.env, { readFixedSizePredicate: () => "either-axis-fixed" });
                enableAdapter(mocks);
                const engine = EngineBridge.start();
                try {
                    mocks.observeImpl = () => makeObserved(refs, { fingerprint: `fp-g05-either-${vector.min.w}x${vector.min.h}` });
                    const payload = dispatchAdded(mocks);
                    assert.equal(payload["fixed_size_predicate"], "either-axis-fixed");
                    assert.equal(wireOf(payload, "win-b")["floating"], true, `${label} floats under either-axis`);
                    assert.equal(wireOf(payload, "win-b")["fixed_auto"], true, `${label} carries automatic identity`);
                    const reply = await flushPlan(mocks, engine, 0);
                    assert.equal(reply["outcome"], "planned", `${label} plans, got ${JSON.stringify(reply)}`);
                    assert.ok(
                        !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                        `${label} float takes no slot`,
                    );
                    assert.deepEqual(
                        mocks.geometries.filter((write) => write.target === refs.b),
                        [],
                        `${label} sees no geometry write`,
                    );
                    assert.deepEqual(mocks.keepAboveWrites, [], `${label} sees no keep-above write`);
                    assert.deepEqual(mocks.floatingWrites, [], `${label} calls no intentional setters`);
                } finally {
                    await engine.close();
                }
            }
        }
    });

    it("a live predicate switch never re-floats the retained partial-zero tile", () => {
        for (const vector of PARTIAL_ZERO_VECTORS) {
            const label = `min/max=(${vector.min.w},${vector.min.h})`;
            const { refs, mocks } = partialZeroMocks(vector);
            let predicate: unknown = "both-axes-fixed";
            Object.assign(mocks.env, { readFixedSizePredicate: (): unknown => predicate });
            enableAdapter(mocks);
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: `fp-g05-live-${vector.min.w}x${vector.min.h}-1` });
            const before = dispatchAdded(mocks);
            assert.ok(!("floating" in wireOf(before, "win-b")), `${label} tiles before the switch`);
            fireTimeout(mocks);
            predicate = "either-axis-fixed";
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: `fp-g05-live-${vector.min.w}x${vector.min.h}-2` });
            const after = dispatchAdded(mocks);
            assert.equal(after["fixed_size_predicate"], "either-axis-fixed");
            assert.ok(!("floating" in wireOf(after, "win-b")), `${label} switch never re-floats the retained client`);
            assert.equal(wireOf(after, "win-b")["fixed_suppress"], true, `${label} retained tile carries suppression`);
            assert.ok(!("fixed_auto" in wireOf(after, "win-b")), `${label} retained tile is not automatic`);
        }
    });
});
const ONE_AXIS_MIN = { w: 640, h: 100 };
const ONE_AXIS_MAX = { w: 640, h: 480 };

function oneAxisMocks(): { refs: { a: object; b: object }; mocks: Mocks } {
    const refs = makeRefs();
    const mocks = mockEnv(refs);
    mocks.constraintsImpl = (target): PlanWindowConstraints | null => {
        if (target === refs.b) {
            return { resizeable: false, minSize: { ...ONE_AXIS_MIN }, maxSize: { ...ONE_AXIS_MAX } };
        }
        return { resizeable: true, minSize: null, maxSize: null };
    };
    return { refs, mocks };
}

describe("fixed-size predicate wire and live switch through the real Planner", () => {
    it("omits fixed_size_predicate by default and tiles one-axis clients", async () => {
        const { refs, mocks } = oneAxisMocks();
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-pred-omit" });
            const payload = dispatchAdded(mocks);
            assert.ok(!("fixed_size_predicate" in payload), "default stays wire-omitted");
            assert.ok(!("floating" in wireOf(payload, "win-b")), "one-axis tiles by default");
            const reply = await flushPlan(mocks, engine, 0);
            assert.equal(reply["outcome"], "planned", `Engine plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "one-axis client keeps a tiled slot by default",
            );
        } finally {
            await engine.close();
        }
    });

    it("carries either-axis-fixed explicitly and never actuates the one-axis float", async () => {
        const { refs, mocks } = oneAxisMocks();
        Object.assign(mocks.env, { readFixedSizePredicate: () => "either-axis-fixed" });
        enableAdapter(mocks);
        const engine = EngineBridge.start();
        try {
            mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-pred-either" });
            const payload = dispatchAdded(mocks);
            assert.equal(payload["fixed_size_predicate"], "either-axis-fixed");
            const fixed = wireOf(payload, "win-b");
            assert.equal(fixed["floating"], true);
            assert.equal(fixed["fixed_auto"], true);
            const reply = await flushPlan(mocks, engine, 0);
            assert.equal(reply["outcome"], "planned", `Engine plans, got ${JSON.stringify(reply)}`);
            assert.ok(
                !desiredWindows(reply).some((entry) => entry["window"] === "win-b"),
                "one-axis float takes no slot",
            );
            assert.deepEqual(
                mocks.geometries.filter((write) => write.target === refs.b),
                [],
                "no geometry write to the one-axis float",
            );
            assert.deepEqual(mocks.keepAboveWrites, [], "no automatic keep-above write");
            assert.deepEqual(mocks.floatingWrites, [], "autoclassification never calls intentional setters");
        } finally {
            await engine.close();
        }
    });

    it("a live predicate switch keeps the retained one-axis client tiled with suppression", () => {
        const { refs, mocks } = oneAxisMocks();
        let predicate: unknown = "both-axes-fixed";
        Object.assign(mocks.env, { readFixedSizePredicate: (): unknown => predicate });
        enableAdapter(mocks);
        // First dispatch under the default: one-axis tiles and pins a
        // suppress record for the live client; the wire stays byte-identical.
        mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-pred-live-1" });
        const before = dispatchAdded(mocks);
        assert.ok(!("fixed_size_predicate" in before), "default stays wire-omitted");
        assert.ok(!("floating" in wireOf(before, "win-b")), "one-axis tiles by default");
        assert.ok(!("fixed_suppress" in wireOf(before, "win-b")), "non-fixed rows stay byte-identical");
        // The reply never arrives: the real timeout terminal fails the
        // flight with no application and no record change.
        fireTimeout(mocks);
        assert.ok(
            mocks.logs.some((entry) => entry.includes("outcome=timeout")),
            "timeout terminal logged",
        );
        // Switch to either-axis for subsequent admissions only: the retained
        // client stays tiled via its suppress pin, now carrying suppression
        // (the only shape the core could reclassify), and the payload
        // carries the new predicate.
        predicate = "either-axis-fixed";
        mocks.observeImpl = () => makeObserved(refs, { fingerprint: "fp-pred-live-2" });
        const after = dispatchAdded(mocks);
        assert.equal(after["fixed_size_predicate"], "either-axis-fixed");
        assert.ok(!("floating" in wireOf(after, "win-b")), "switch never re-floats the retained client");
        assert.equal(wireOf(after, "win-b")["fixed_suppress"], true);
        assert.ok(!("fixed_auto" in wireOf(after, "win-b")));
    });
});

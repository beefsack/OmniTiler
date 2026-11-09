import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";

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

export class WorldBridge {
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
            throw new Error("broker_world_eval stdout unavailable");
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
            this.failAll(new Error(`broker_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
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
            spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "broker_world_eval"], {
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
            return Promise.reject(new Error("broker_world_eval process already exited"));
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

export interface FakeSignal {
    readonly handlers: Array<(payload?: unknown) => void>;
    readonly signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
}

export function makeSignal(): FakeSignal {
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

export interface FakeDesktop {
    readonly id: string;
    readonly name: string;
}

export interface FakeWindow {
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
    fullScreenable: boolean;
    maximizeMode: number;
    setMaximize: (vertical: boolean, horizontal: boolean) => void;
    onAllDesktops: boolean;
    minSize: { width: number; height: number };
    maxSize: { width: number; height: number };
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

export interface CapturedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

export interface Timer {
    delayMs: number;
    callback: () => void;
    cancelled: boolean;
    fired: boolean;
}

export interface FakeOutputSpec {
    readonly name: string;
    readonly geometry: { x: number; y: number; width: number; height: number };
    readonly manufacturer?: string;
    readonly model?: string;
    readonly serialNumber?: string;
}

export interface Harness {
    readonly bridge: WorldBridge;
    workspace: Record<string, unknown>;
    readonly wins: FakeWindow[];
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly takeReplies: string[];
    // Presentation lane (recorded only, never auto-answered: the
    // production subscriber stays in flight until a test forwards a read
    // to the real broker, exactly like discover() does for DiscoverWorld).
    readonly reads: CapturedCall[];
    readonly readReplies: string[];
    readonly renders: Array<{ method: string; payload: string }>;
    readonly effectCalls: CapturedCall[];
    readonly timers: Timer[];
    readonly logs: string[];
    readonly geometryWrites: string[];
    readonly nativeStateWrites: string[];
    // Native output writes, one entry per adapter-driven cross-output move
    // (the `sendClientToScreen` echo below), plus the echo's own desktop
    // re-home. Recorded only; the shared single-output default never moves
    // a window across outputs, so both stay empty there.
    readonly outputWrites: string[];
    readonly echoWrites: string[];
    // Native view (current-desktop) writes, one entry per applied switch.
    // Recorded only; the shared single-desktop default never writes.
    readonly viewWrites: string[];
    readonly focused: string[];
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    // Retained Planner durability snapshots, mirroring server-side
    // persistence across an orderly adapter restart: native preimage
    // originals (exact-contract v1 rows) and opaque intent records
    // (exact-contract v2 rows). Empty until the entry writes; reads
    // serve the retained snapshot.
    readonly preimageRetained: Array<Record<string, unknown>>;
    readonly intentRetained: Array<{ win: string; data: string }>;
    // Orderly in-place restart: stops the live entry and starts a fresh
    // one on the SAME fake surface, bridge and retained snapshots. The
    // broker resets core state on the new handshake (same as a fresh
    // broker process); only native facts plus the retained lanes carry
    // over, exactly like production.
    restart: () => void;
    // Physical topology events (tier 3B): remove one fake screen and later
    // return it, modelling pinned KWin exactly (sibling `src/workspace.cpp`
    // `rearrange` plus geometry-driven `setOutput(outputAt(center))`:
    // windows keep desktop membership, only the output name moves, with a
    // per-window `outputChanged` and one workspace `screensChanged`).
    // Return re-adds the screen with no window moves of its own: KWin
    // never pulls windows back, the core reply drives the adapter apply.
    removeOutput: (name: string) => void;
    returnOutput: (spec: FakeOutputSpec) => void;
    readonly signals: Record<string, FakeSignal>;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    fireNextTimer: (maxDelay: number, from: number) => void;
    flush: () => Promise<number>;
    flushAt: (index: number) => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    commandPayloads: (op: string) => Array<{ index: number; body: Record<string, unknown> }>;
    stop: () => Promise<void>;
}

export function frameOf(win: FakeWindow): { x: number; y: number; w: number; h: number } {
    const frame = win.frameGeometry as { x: number; y: number; width: number; height: number };
    return { x: frame.x, y: frame.y, w: frame.width, h: frame.height };
}

// Truthful native decoration: stable nonnegative insets mirroring the v4
// reference (deltaW 4, deltaH 22). The tiler writes frameGeometry only; the
// native client tracks with the same insets so every capture carries
// consistent frame/client facts.
export const DECOR = { left: 2, top: 21, right: 2, bottom: 1 };
export function truthfulClientFor(f: { x: number; y: number; width: number; height: number }): { x: number; y: number; width: number; height: number } {
    const w = Math.max(1, f.width - (DECOR.left + DECOR.right));
    const h = Math.max(1, f.height - (DECOR.top + DECOR.bottom));
    const x = f.x + Math.min(DECOR.left, Math.max(0, f.width - w));
    const y = f.y + Math.min(DECOR.top, Math.max(0, f.height - h));
    return { x, y, width: w, height: h };
}

export async function makeHarness(focusId: string, ids: string[], opts?: { withEffectFacts?: boolean; withNativePreimage?: boolean; withIntentStore?: boolean; withNativeCreation?: boolean; desktops?: string[]; fractionalScale?: number; outputs?: FakeOutputSpec[]; homes?: Record<string, string>; currents?: Record<string, string> }): Promise<Harness> {
    // Multi-desktop selection support (tier 2A): the default stays exactly
    // one native desktop (`ws1`, the pre-existing shape); passing
    // `desktops` seeds that many natives in order, all but the first empty.
    // Every window still homes to the first desktop; tests re-home via the
    // live `desktops` setter before discovery.
    const desktopIds = opts?.desktops ?? ["ws1"];
    assert.ok(desktopIds.length > 0, "at least one native desktop");
    const liveDesktops: FakeDesktop[] = desktopIds.map((id) => ({ id, name: id === "ws1" ? "WS1" : id }));
    const ws: FakeDesktop = (liveDesktops[0] as FakeDesktop);
    // Multi-output topology support (tier 3B): the default stays exactly
    // one screen (`out`, the pre-existing shape); passing `outputs` seeds
    // that many live screens in order. Each entry carries the exact KWin
    // scripting shape the observer reads (`name`, `geometry`, plus the
    // neutral `manufacturer`/`model`/`serialNumber` identity arms, absent
    // unless the test supplies them). The `homes` map names each window's
    // seeding output; unlisted windows home to the first screen.
    const liveScreens: Array<Record<string, unknown>> = (opts?.outputs ?? [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }]).map((spec) => ({
        name: spec.name,
        geometry: { ...spec.geometry },
        ...(spec.manufacturer !== undefined ? { manufacturer: spec.manufacturer } : {}),
        ...(spec.model !== undefined ? { model: spec.model } : {}),
        ...(spec.serialNumber !== undefined ? { serialNumber: spec.serialNumber } : {}),
    }));
    assert.ok(liveScreens.length > 0, "at least one native screen");
    // Fractional-scale outputs (opt-in `fractionalScale`) carry the
    // physical/logical devicePixelRatio exactly as real KWin exposes it.
    // The producer never reads it (scaled-metrics note: wire scale stays
    // 1 in KWin logical space, so a DPR-only change must not churn);
    // logical bounds stay identical to the default so the same-logical
    // route is exercised. Default carries no DPR key, as before.
    if (opts?.fractionalScale !== undefined) {
        assert.equal(liveScreens.length, 1, "fractional scale rides the single-output shape only");
        (liveScreens[0] as Record<string, unknown>)["devicePixelRatio"] = opts.fractionalScale;
    }
    const screenGeometry = (name: string): { x: number; y: number; width: number; height: number } => {
        const found = liveScreens.find((s) => s["name"] === name);
        assert.ok(found !== undefined, `screen ${name} exists on the fake surface`);
        return (found["geometry"] as { x: number; y: number; width: number; height: number });
    };
    const homeOf = (id: string): string => {
        const named = opts?.homes?.[id] ?? (liveScreens[0] as Record<string, unknown>)["name"];
        assert.ok(liveScreens.some((s) => s["name"] === named), `home screen ${String(named)} exists`);
        return named as string;
    };
    // Native current per output (single-output fake: `out`). The legacy
    // shared getter/setter and the per-output pair stay consistent. The
    // `currents` map seeds each screen's current (tier 3B distinct-current
    // topologies); unlisted screens start on the first desktop.
    const currentByOutput = new Map<string, FakeDesktop>();
    for (const s of liveScreens) {
        const want = opts?.currents?.[s["name"] as string] ?? (liveDesktops[0] as FakeDesktop).id;
        const found = liveDesktops.find((desk) => desk.id === want);
        assert.ok(found !== undefined, `current desktop ${want} exists`);
        currentByOutput.set(s["name"] as string, found as FakeDesktop);
    }
    const perOutputCount = new Map<string, number>();
    const wins: FakeWindow[] = ids.map((id) => {
        const home = homeOf(id);
        const origin = screenGeometry(home);
        // Per-output synthetic cascade: the k-th window seeded on an
        // output starts at that output's origin, so every seed center is
        // contained in its home screen exactly like the single-output
        // cascade was contained in `out`. Default output and order give
        // observably identical seeds to before.
        const k = perOutputCount.get(home) ?? 0;
        perOutputCount.set(home, k + 1);
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        // Fractional-scale native readbacks (opt-in `fractionalScale`):
        // real KWin at a fractional output scale reports logical doubles
        // (v4-reference fractional shape 8.4/8.6 cited beside the carrier in
        // world-ordinary-frame-engine.test.ts; 1.25-grid helper evidence in
        // the scenario-runner fractional-scale note). Positions and sizes
        // stay in this harness's existing synthetic cascade; only the read
        // character turns fractional. Fractions straddle the half (0.4 rounds
        // to 0, 0.6 rounds to 1 under the ordinary-fact rule), so normalized
        // seeds equal the default integers on x/width/height; exact halves
        // ride the admit-journey newcomer seed instead. Default stays
        // integer, observably identical to before.
        const frame = opts?.fractionalScale !== undefined
            ? { x: origin.x + k * 300 + 0.4, y: origin.y + 0.6, width: 200, height: 200 }
            : { x: origin.x + k * 300, y: origin.y, width: 200, height: 200 };
        return {
            internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { ...frame },
            clientGeometry: truthfulClientFor(frame),
            output: { name: home },
            desktops: [ws],
            minimized: false,
            hidden: false,
            fullScreen: false,
            // Truthful static KWin shape: real clients expose `fullScreenable`
            // (neutral exitability path, K:src/window.h) and the sole base
            // Q_INVOKABLE `setMaximize(bool, bool)` driving `maximizeMode`.
            // Normal windows are fullscreenable; the echo below mirrors the
            // in-repo fullscreen-toggle fixture (both axes -> mode 3).
            fullScreenable: true,
            maximizeMode: 0,
            setMaximize: (): void => undefined,
            onAllDesktops: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234,
            resourceClass: "Example",
            caption: `Window ${id}`,
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
    });
    const geometryWrites: string[] = [];
    const focused: string[] = [];
    const sigNames = [
        "windowAdded",
        "windowRemoved",
        "windowActivated",
        "desktopsChanged",
        "screensChanged",
        "currentDesktopChanged",
        "activeScreenChanged",
    ] as const;
    const signals: Record<string, FakeSignal> = {};
    for (const name of sigNames) {
        signals[name] = makeSignal();
    }
    const harness: Harness = {
        bridge: WorldBridge.start(),
        workspace: {},
        wins,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        takeReplies: [],
        reads: [],
        readReplies: [],
        renders: [],
        effectCalls: [],
        timers: [],
        logs: [],
        geometryWrites,
        nativeStateWrites: [],
        outputWrites: [],
        echoWrites: [],
        viewWrites: [],
        focused,
        activeScreen: { name: (liveScreens[0] as Record<string, unknown>)["name"] as string },
        activeRef: null,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        signals,
        fire: (name: string, payload?: unknown): void => {
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(payload);
            }
        },
        runTimers: (maxDelay = WORLD_DEBOUNCE_MS): void => {
            for (let i = 0; i < harness.timers.length; i += 1) {
                const timer = harness.timers[i] as Timer;
                if (timer.delayMs <= maxDelay && !timer.cancelled && !timer.fired) {
                    timer.fired = true;
                    harness.timers.splice(i, 1);
                    i -= 1;
                    timer.callback();
                }
            }
        },
        fireNextTimer: (maxDelay: number, from: number): void => {
            let at = -1;
            for (let i = from; i < harness.timers.length; i += 1) {
                const timer = harness.timers[i] as Timer;
                if (!timer.cancelled && !timer.fired && timer.delayMs <= maxDelay &&
                    (at < 0 || timer.delayMs < (harness.timers[at] as Timer).delayMs)) {
                    at = i;
                }
            }
            assert.ok(at >= 0, "expected a due timer");
            const [timer] = harness.timers.splice(at, 1) as Timer[];
            (timer as Timer).callback();
        },
        flush: async () => {
            // The broker transport is async (process lines, like D-Bus):
            // a press files its Queue first and the entry dispatches the
            // DescribeWorld only once the filing answer returns. Yield to
            // transport I/O and pump due production timers boundedly
            // (debounce/drain only; timeouts stay armed) so the dispatch
            // lands before giving up.
            for (let round = 0; round < 200; round += 1) {
                const index = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (index >= 0) {
                    return harness.flushAt(index);
                }
                harness.runTimers();
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
            }
            assert.ok(false, "expected a queued DescribeWorld dispatch to flush through the production broker");
            throw new Error("unreachable");
        },
        flushAt: async (index: number): Promise<number> => {
            assert.ok(
                (harness.calls[index] as CapturedCall | undefined)?.method === "DescribeWorld" && !harness.answered.has(index),
                "expected the targeted DescribeWorld dispatch to be unanswered",
            );
            // Production broker drain: the envelope runs end to end through
            // the real `WorldService` (one fresh capture incorporates native
            // evidence once and drains ALL parked commands FIFO in core,
            // returning one final full publication with the terminal result
            // nulled for the take lane). No test-side stamping, sequencing,
            // or reply binding: the broker owns all of it.
            const envelope = (harness.calls[index] as CapturedCall).payload;
            const reply = await harness.bridge.send("DescribeWorld", envelope);
            harness.answered.add(index);
            harness.replies.set(index, reply);
            (harness.calls[index] as CapturedCall).callback(reply);
            // Pump to quiescence: post-accept native apply waits for the
            // async per-command take collection (production D-Bus is async
            // the same way; the old sync fake resolved takes inline), so
            // yield to transport I/O and due timers until no new lane call
            // appears and every take is answered. Follow-up DescribeWorld
            // dispatches stay recorded but unflushed for the test to drive.
            let calm = 0;
            let lastCalls = -1;
            let lastTakes = -1;
            for (let round = 0; round < 300 && calm < 3; round += 1) {
                harness.runTimers();
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
                const takeCalls = harness.calls.filter((call) => call.method === "TakeCommandResult").length;
                if (
                    harness.calls.length === lastCalls &&
                    harness.takeReplies.length === lastTakes &&
                    takeCalls <= harness.takeReplies.length
                ) {
                    calm += 1;
                } else {
                    calm = 0;
                    lastCalls = harness.calls.length;
                    lastTakes = harness.takeReplies.length;
                }
            }
            return index;
        },
        flushAll: async (): Promise<void> => {
            for (let round = 0; round < 16; round += 1) {
                const next = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (next < 0) {
                    return;
                }
                await harness.flush();
            }
            throw new Error("flushAll did not converge: planner keeps dispatching");
        },
        settle: async (): Promise<void> => {
            for (let round = 0; round < 10; round += 1) {
                harness.runTimers();
                const before = harness.calls.length;
                await harness.flushAll();
                harness.runTimers();
                await harness.flushAll();
                if (harness.calls.length === before) {
                    return;
                }
            }
            throw new Error("settle did not converge");
        },
        commandPayloads: (op: string): Array<{ index: number; body: Record<string, unknown> }> => {
            // v4 command lane: presses file QueueWorldCommand (the broker
            // drains the whole parked FIFO batch in the next fresh capture);
            // DescribeWorld envelopes carry no inline command.
            const out: Array<{ index: number; body: Record<string, unknown> }> = [];
            harness.calls.forEach((call, index) => {
                if (call.method !== "QueueWorldCommand") {
                    return;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    if ((body["command"] as Record<string, unknown> | null)?.["op"] === op) {
                        out.push({ index, body });
                    }
                } catch {
                    return;
                }
            });
            return out;
        },
        stop: async (): Promise<void> => {
            try {
                harness.handle?.stop();
            } catch (error) {
                void error;
            }
            await harness.bridge.close();
        },
        preimageRetained: [],
        intentRetained: [],
        restart: (): void => {
            harness.handle.stop();
            harness.handle = startEntry();
        },
        removeOutput: (name: string): void => {
            // Fidelity limits (reviewed): real KWin also runs
            // `finishInteractiveOutputChange` on removal-driven `setOutput`,
            // re-homing desktops when the survivor's current differs from its
            // first desktop; this fake never re-homes, so journeys here keep
            // the survivor's current equal to its first desktop. Shared-mode
            // currents and Activities are unmodelled, and the real drop of the
            // removed output's current precedes `setOutput` (the fake drops it
            // after).
            // Pinned KWin removal order (sibling `src/workspace.cpp`
            // `updateOutputs` tail: rearrange, then per-window
            // `setOutput(outputAt(center))`, then the outputs-changed
            // signal): membership never moves here, only the output name,
            // with a per-window `outputChanged` each and one workspace
            // `screensChanged`. The nearest survivor wins by the same
            // clamped-point rule KWin `outputAt` uses; with one survivor
            // that choice is forced. Unknown `desktopsChanged` never
            // fires: core rehome arrives through the reply apply instead.
            const at = liveScreens.findIndex((s) => s["name"] === name);
            assert.ok(at >= 0, `screen ${name} exists`);
            assert.ok(liveScreens.length > 1, "removal keeps at least one survivor");
            const [gone] = liveScreens.splice(at, 1);
            void gone;
            const survivors = liveScreens.map((s) => ({
                name: s["name"] as string,
                geometry: s["geometry"] as { x: number; y: number; width: number; height: number },
            }));
            const nearest = (center: { x: number; y: number }): string => {
                let best = survivors[0]?.name as string;
                let bestDist = Number.POSITIVE_INFINITY;
                for (const survivor of survivors) {
                    const geo = survivor.geometry;
                    const cx = Math.min(Math.max(center.x, geo.x), geo.x + geo.width - 1);
                    const cy = Math.min(Math.max(center.y, geo.y), geo.y + geo.height - 1);
                    const dist = (cx - center.x) * (cx - center.x) + (cy - center.y) * (cy - center.y);
                    if (dist < bestDist) {
                        bestDist = dist;
                        best = survivor.name;
                    }
                }
                return best;
            };
            for (const win of harness.wins) {
                if (win.output.name !== name) {
                    continue;
                }
                const live = frameOf(win);
                win.output = { name: nearest({ x: live.x + live.w / 2, y: live.y + live.h / 2 }) };
                for (const handler of [...win.__signals.outputChanged.handlers]) {
                    handler();
                }
            }
            if ((harness.activeScreen as { name: string }).name === name) {
                harness.activeScreen = { name: survivors[0]?.name as string };
            }
            // Pinned KWin `VirtualDesktopManager` (same pin:
            // `outputRemoved` drops the output's current from
            // `m_currentDesktops`; a later `outputAdded` re-inits through
            // `initialDesktopForNewOutput`, the first desktop in
            // per-output mode): the removed current is forgotten, and a
            // returned screen reads the first desktop until the core
            // binds and shows its own unit.
            currentByOutput.delete(name);
            harness.fire("screensChanged");
        },
        returnOutput: (spec: FakeOutputSpec): void => {
            // Pinned KWin addition order: the screen list gains the panel
            // and the outputs-changed signal fires; KWin moves no window
            // back on its own (the core return reply drives the adapter
            // apply through `sendClientToScreen` below). A renamed live
            // handle carrying the original physical identity is the
            // physical-reconnect proof: core matches identity, never the
            // name, so the same-name fallback is never exercised here.
            assert.ok(!liveScreens.some((s) => s["name"] === spec.name), `screen ${spec.name} absent before return`);
            liveScreens.push({
                name: spec.name,
                geometry: { ...spec.geometry },
                ...(spec.manufacturer !== undefined ? { manufacturer: spec.manufacturer } : {}),
                ...(spec.model !== undefined ? { model: spec.model } : {}),
                ...(spec.serialNumber !== undefined ? { serialNumber: spec.serialNumber } : {}),
            });
            harness.fire("screensChanged");
        },
    };
    const active = wins.find((win) => win.internalId === focusId);
    assert.ok(active !== undefined, `focus window ${focusId} exists`);
    harness.activeRef = active as FakeWindow;
    const surface: Record<string, unknown> = {
        windowList: (): FakeWindow[] => [...harness.wins],
        // Live screen list (tier 3B): the same array the topology helpers
        // splice, so removal/return is visible to the next capture. The
        // fractional-scale DPR key rides only under that opt-in, as before.
        screens: liveScreens,
        desktops: liveDesktops,
        get activeScreen(): { name: string } {
            return harness.activeScreen;
        },
        set activeScreen(ref: unknown) {
            void ref;
        },
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        get activeWindow(): FakeWindow | null {
            return harness.activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined) {
                harness.activeRef = found;
                harness.focused.push(found.internalId);
            }
        },
        get currentDesktop(): FakeDesktop {
            return (currentByOutput.get((harness.activeScreen as { name: string }).name) as FakeDesktop | undefined) ?? (liveDesktops[0] as FakeDesktop);
        },
        set currentDesktop(ref: unknown) {
            // Legacy shared write (KWin 6.6 shape): current everywhere.
            for (const screen of liveScreens) {
                currentByOutput.set(screen["name"] as string, ref as FakeDesktop);
            }
        },
        // Per-output work area (tier 3B): each live screen reports its own
        // origin and width with the same 400-high area the single-output
        // default always carried, so gap arithmetic keeps its shape on
        // every output. The default screen resolves to the exact legacy
        // rect; an unknown screen falls back to it.
        clientArea: (_kind: unknown, outRef: unknown, _deskRef: unknown): Record<string, number> => {
            const name = (outRef as { name?: unknown } | null)?.name;
            const found = liveScreens.find((s) => s["name"] === name);
            const geometry = (found?.["geometry"] ?? { x: 0, y: 0, width: 1920, height: 1080 }) as { x: number; y: number; width: number; height: number };
            return { x: geometry.x, y: geometry.y, width: geometry.width, height: Math.min(400, geometry.height) };
        },
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop => {
            return (currentByOutput.get((screen as { name: string }).name) as FakeDesktop | undefined) ?? (liveDesktops[0] as FakeDesktop);
        },
        // Production per-output switch signature exactly
        // (`setCurrentDesktopForScreen(desktop, output)`, void at pinned
        // KWin 6.7): synchronous current change plus a synchronous
        // `currentDesktopChanged` workspace signal, mirroring the
        // migration-producer fixture. The single-desktop default never
        // fires this path (nothing ever switches away from `ws1`).
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            currentByOutput.set((screen as { name: string }).name, deskRef as FakeDesktop);
            harness.viewWrites.push(`${(screen as { name: string }).name}->${(deskRef as FakeDesktop).id}`);
            harness.fire("currentDesktopChanged");
        },
        // Pinned KWin `sendClientToScreen` echo (`window.cpp`
        // `finishInteractiveOutputChange` at pin ab7df7cc, mirroring the
        // migration-producer fixture): moving a single-desktop window on
        // the old output's current synchronously re-homes it onto the NEW
        // output's current desktop before any explicit target set. The
        // output step emits the window-level `outputChanged`, the echo
        // re-home emits `desktopsChanged` (both subscribed per-window by
        // the entry). The shared single-output default never moves a
        // window across outputs, so this path stays silent there.
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            const from = win.output.name;
            win.output = { name };
            harness.outputWrites.push(`${win.internalId}->${name}`);
            for (const handler of [...win.__signals.outputChanged.handlers]) {
                handler();
            }
            if (win.desktops.length === 1) {
                const home = (win.desktops[0] as FakeDesktop).id;
                const oldCurrent = currentByOutput.get(from)?.id ?? "";
                const newCurrent = currentByOutput.get(name)?.id ?? "";
                if (home === oldCurrent && newCurrent !== oldCurrent && home !== newCurrent) {
                    // The desktops setter below emits the window-level
                    // `desktopsChanged` synchronously (faithful).
                    const target = liveDesktops.find((desk) => desk.id === newCurrent);
                    assert.ok(target !== undefined, `desktop ${newCurrent} exists`);
                    win.desktops = [target as FakeDesktop];
                    harness.echoWrites.push(`${win.internalId}->${newCurrent}`);
                }
            }
        },
        // Opt-in production-faithful native creation (`withNativeCreation`):
        // the default stays a no-op so producers that never exercise
        // displacement behave identically to before. When armed, the
        // pinned void shape applies: a numeric zero-based position in
        // [0, count] plus the logical token as the name, synchronously
        // appending one native desktop and firing `desktopsChanged`
        // synchronously (real KWin signals fire inside the call; the
        // apply loop spends the stable op before calling for exactly
        // this reason). Same-turn before/after enumeration proves the
        // created id causally.
        createDesktop: (position: unknown, name: unknown): void => {
            if (opts?.withNativeCreation !== true) {
                return;
            }
            assert.equal(typeof position, "number", "creation carries a numeric position");
            assert.equal(typeof name, "string", "creation carries a logical token name");
            const id = `ws-${liveDesktops.length + 1}`;
            const desk: FakeDesktop = { id, name: name as string };
            const at = Math.max(0, Math.min(position as number, liveDesktops.length));
            liveDesktops.splice(at, 0, desk);
            harness.fire("desktopsChanged");
        },
        removeDesktop: (): void => undefined,
    };
    for (const win of wins) {
        // Truthful native maximize echo: the real `setMaximize(bool, bool)`
        // Q_INVOKABLE drives `maximizeMode` (both axes -> 3, else 0), so the
        // entry observes back exactly what the adapter sets.
        win.setMaximize = (vertical: boolean, horizontal: boolean): void => {
            harness.nativeStateWrites.push(`${win.internalId}@setMaximize:${vertical},${horizontal}`);
            win.maximizeMode = vertical && horizontal ? 3 : 0;
        };
        let frame = win.frameGeometry;
        let client = truthfulClientFor(frame);
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                client = truthfulClientFor(rect);
                harness.geometryWrites.push(`${win.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            get: () => client,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                client = rect;
                return true;
            },
        });
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                return true;
            },
        });
        // Truthful KWin all-desktops echo (sibling src/window.cpp
        // `Window::setOnAllDesktops`): on(true) empties the desktop list,
        // on(false) restores the output current. The flag itself is
        // derived from list emptiness, so a direct `desktops` assignment
        // flips it implicitly, exactly like `setDesktops` does natively.
        // The entry's sticky executor writes this property; recovery
        // writes `desktops` directly.
        Object.defineProperty(win, "onAllDesktops", {
            configurable: true,
            get: () => desks.length === 0,
            set: (value: boolean) => {
                const want = value === true;
                harness.nativeStateWrites.push(`${win.internalId}@onAllDesktops:${want}`);
                desks = want ? [] : [(currentByOutput.get(win.output.name) as FakeDesktop | undefined) ?? ws];
                return true;
            },
        });
    }
    harness.workspace = surface;
    const startRaw = (): ReturnType<typeof startWorldAdapterEntry> => startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        // Production parity (`kwin/src/entry.ts` sets `floatIntent: true`):
        // the opaque-intent lane is armed only when the test opts into
        // the fake Planner seam, so the default stays bypassed exactly
        // as before.
        floatIntent: opts?.withIntentStore === true,
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "ReadWorldPresentation") {
                harness.reads.push({ method, payload, callback });
                return;
            }
            if (method === "SetPresentation" || method === "HidePresentation" || method === "ClearPresentation") {
                harness.renders.push({ method, payload: typeof payload === "string" ? payload : "" });
                return;
            }
            if (method === "ReadNativePreimage") {
                // Opt-in adapter-side durability seam (`withNativePreimage`):
                // the default drops these exactly as the harness always did
                // (no recording, no answer), so producers that never arm
                // native-state lanes (fifo focus/move/resize) behave
                // identically to before. Maximized/fullscreen/sticky journeys
                // opt in: synchronous acks with v1 exact keys, mirroring the
                // fullscreen-float-toggle fixture and the broker contract in
                // crates/omnitiler/src/native_preimage_store.rs. The
                // retained broker example serves no preimage lane, so the
                // harness answers this adapter-side seam locally; core
                // replies, take lane and native setters stay genuine. Reads
                // serve the snapshot retained across an orderly in-place
                // restart (mirroring server-side persistence); writes
                // replace it. Without a restart the retained snapshot stays
                // empty and every seed reads exactly as before.
                if (opts?.withNativePreimage !== true) {
                    return;
                }
                const seedBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({
                    v: 1, correlation_id: seedBody["correlation_id"],
                    outcome: "ok", records: harness.preimageRetained, stored: harness.preimageRetained.length, returned: harness.preimageRetained.length,
                }));
                return;
            }
            if (method === "WriteNativePreimage") {
                // Opt-in (see above): default drops, preserving the
                // previous behaviour. Synchronous stored ack (v1 exact keys):
                // the sent snapshot landed, so the count echoes the request
                // record length. The snapshot is retained verbatim so a
                // later seed on the same harness (orderly restart) sees
                // the records the first run wrote, as production does.
                if (opts?.withNativePreimage !== true) {
                    return;
                }
                const writeBody = JSON.parse(payload) as Record<string, unknown>;
                const rows = writeBody["records"];
                const kept = Array.isArray(rows) ? rows as Array<Record<string, unknown>> : [];
                harness.preimageRetained.splice(0, harness.preimageRetained.length, ...kept);
                callback(JSON.stringify({
                    v: 1, correlation_id: writeBody["correlation_id"],
                    outcome: "stored", stored: kept.length,
                }));
                return;
            }
            if (method === "ReadFloatIntent" || method === "WriteFloatIntent") {
                // Opt-in opaque-intent seam (`withIntentStore`, mirroring
                // the production `floatIntent: true` in kwin/src/entry.ts):
                // the default drops these exactly as the harness always did
                // (no recording, no answer). Synchronous acks with v2 exact
                // keys per kwin/src/world-intent-store.ts; the retained
                // snapshot mirrors Planner server-side persistence across
                // an orderly restart. Core replies, take lane and native
                // setters stay genuine: only the opaque bytes round-trip.
                if (opts?.withIntentStore !== true) {
                    return;
                }
                const intentBody = JSON.parse(payload) as Record<string, unknown>;
                if (method === "ReadFloatIntent") {
                    const rows = harness.intentRetained;
                    callback(JSON.stringify({
                        v: 2, correlation_id: intentBody["correlation_id"],
                        outcome: "ok", records: rows, stored: rows.length, returned: rows.length,
                    }));
                    return;
                }
                const want = intentBody["records"];
                const kept = Array.isArray(want)
                    ? (want as Array<{ win: string; data: string }>)
                    : [];
                harness.intentRetained.splice(0, harness.intentRetained.length, ...kept);
                callback(JSON.stringify({
                    v: 2, correlation_id: intentBody["correlation_id"],
                    outcome: "stored", stored: kept.length,
                }));
                return;
            }
            if (method !== "DiscoverWorld" && method !== "DescribeWorld" && method !== "QueueWorldCommand" && method !== "TakeCommandResult") {
                return;
            }
            harness.calls.push({ method, payload, callback });
            // Queue/take run through the real broker (ordered process
            // lines, so the broker sees Queue before a later DescribeWorld
            // exactly in call order). Replies arrive asynchronously; the
            // entry already treats delayed queue/take answers as honest
            // on-next-event recovery, and a dead bridge leaves pending
            // filings to the entry's own timeouts.
            if (method === "QueueWorldCommand") {
                harness.bridge.send("QueueWorldCommand", payload).then(
                    (queueReply) => callback(queueReply),
                    () => undefined,
                );
            } else if (method === "TakeCommandResult") {
                harness.bridge.send("TakeCommandResult", payload).then(
                    (takeReply) => {
                        harness.takeReplies.push(takeReply);
                        callback(takeReply);
                    },
                    () => undefined,
                );
            }
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const timer: Timer = { delayMs, callback, cancelled: false, fired: false };
            harness.timers.push(timer);
            return (): void => {
                timer.cancelled = true;
            };
        },
        log: (message: string): void => {
            harness.logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
        highlightCallDbus: (_service: string, _path: string, _iface: string, method: string, ...args: ReadonlyArray<unknown>): void => {
            const [payload] = args;
            harness.renders.push({ method, payload: typeof payload === "string" ? payload : "" });
        },
        ...(opts?.withEffectFacts === true ? {
            effectCallDbus: (_service: string, _path: string, _iface: string, method: string, payload: string, callback: (reply: unknown) => void): void => {
                harness.effectCalls.push({ method, payload, callback });
            },
        } : {}),
    });
    const startEntry = (): NonNullable<ReturnType<typeof startWorldAdapterEntry>> => {
        const handle = startRaw();
        assert.notEqual(handle, null, "world production entry starts on the fake surface");
        return handle as NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    };
    harness.handle = startEntry();
    return harness;
}

export async function discover(h: Harness, from = 0): Promise<string> {
    // Authentic v4 connection pump: the handshake binds the producer
    // adapter lifetime and mints the serving generation; the revisioned
    // settings snapshot (rev 1) precedes the first report on the wire.
    // `from` scopes the pump past earlier lifetimes (orderly in-place
    // restart): the broker resets core on every successful handshake.
    const hsIndex = h.calls.findIndex((call, at) => at >= from && call.method === "DiscoverWorld");
    assert.ok(hsIndex >= 0, "entry discovers on start");
    const hsBody = JSON.parse((h.calls[hsIndex] as CapturedCall).payload) as Record<string, unknown>;
    assert.equal(hsBody["type"], "handshake", "first discovery is the handshake");
    assert.equal(hsBody["adapter"], "test-adapter", "handshake carries the producer adapter");
    const hsReplyText = await h.bridge.send("DiscoverWorld", (h.calls[hsIndex] as CapturedCall).payload);
    const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
    assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
    assert.equal(hsReply["ready"], false, "accepted handshake carries ready:false");
    assert.equal(hsReply["kind"], "", "accepted handshake carries an empty kind");
    assert.equal(hsReply["adapter"], "test-adapter", "handshake retains the producer adapter, never a broker incarnation");
    assert.equal(hsReply["req"], hsBody["req"], "handshake echoes the request correlation");
    assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "", "handshake mints the serving generation");
    const gen = hsReply["gen"] as string;
    // The production broker binds the producer adapter lifetime verbatim
    // (never a broker incarnation) and mints the serving generation it
    // stamps on every later queue/capture/take binding: pass through.
    (h.calls[hsIndex] as CapturedCall).callback(hsReplyText);
    const setIndex = h.calls.findIndex((call, at) => at > hsIndex && call.method === "DiscoverWorld");
    assert.ok(setIndex >= 0, "revisioned settings follow the handshake");
    const setBody = JSON.parse((h.calls[setIndex] as CapturedCall).payload) as Record<string, unknown>;
    assert.equal(setBody["type"], "settings", "second discovery is the revisioned settings snapshot");
    assert.equal(setBody["adapter"], "test-adapter", "settings carry the producer adapter");
    assert.equal(setBody["gen"], gen, "settings bind the serving generation");
    assert.equal(setBody["rev"], "1", "first settings snapshot carries rev 1");
    const setReplyText = await h.bridge.send("DiscoverWorld", (h.calls[setIndex] as CapturedCall).payload);
    const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
    assert.equal(setReply["type"], "settings-reply", "engine answers settings");
    assert.equal(setReply["status"], "accepted", `settings accepted, got ${setReplyText.slice(0, 300)}`);
    assert.equal(setReply["adapter"], "test-adapter", "settings reply retains the producer adapter");
    assert.equal(setReply["gen"], gen, "settings reply binds the serving generation");
    assert.equal(setReply["req"], setBody["req"], "settings reply echoes the request correlation");
    assert.equal(setReply["rev"], "1", "settings reply binds the sent revision");
    (h.calls[setIndex] as CapturedCall).callback(setReplyText);
    assert.deepEqual(
        h.calls.slice(from, setIndex + 1).map((call) => call.method),
        ["DiscoverWorld", "DiscoverWorld"],
        "no report precedes the accepted settings",
    );
    await h.settle();
    return gen;
}

export function winById(h: Harness, id: string): FakeWindow {
    const found = h.wins.find((win) => win.internalId === id);
    assert.ok(found !== undefined, `window ${id} exists on the fake surface`);
    return found as FakeWindow;
}

export function deskById(h: Harness, id: string): FakeDesktop {
    const desks = h.workspace["desktops"] as FakeDesktop[];
    const found = desks.find((desk) => desk.id === id);
    assert.ok(found !== undefined, `desktop ${id} exists on the fake surface`);
    return found;
}

// Fractional-scale oracle input (kwin/src/world-observe.ts `toInt`):
// per-component Math.round, exact halves toward positive infinity. The
// fractional-scale seeds stay nonnegative, so Math.round matches the
// documented rule exactly (no -0 edge). Never a tolerance: the rounded
// native must equal the published integer allocation exactly.
export function normalizedFrameOf(win: FakeWindow): { x: number; y: number; w: number; h: number } {
    const live = frameOf(win);
    return { x: Math.round(live.x), y: Math.round(live.y), w: Math.round(live.w), h: Math.round(live.h) };
}

// Every window row the real Engine plans must match the live native
// frame, output and membership exactly. Membership resolves through the
// core-owned native binding (v2: reply `ws` is the logical id, natives
// are producer-owned and never equal it by construction).
export function appliedMatchesReply(h: Harness, reply: Record<string, unknown>): void {
    appliedMatchesReplyInner(h, reply, false);
}

// Fractional-scale form of the oracle: the live native may carry logical
// doubles (real KWin at a fractional output scale), so each component
// normalizes through the ordinary-fact rule before the exact comparison
// against the published integer allocation. Output and membership checks
// are identical to the integer form.
export function appliedMatchesReplyNormalized(h: Harness, reply: Record<string, unknown>): void {
    appliedMatchesReplyInner(h, reply, true);
}

// One parsed DescribeWorld envelope at `index`: the v4 inner-report bytes
// the real entry sent (screens carry the producer wire scale, windows
// carry normalized integer rects). Lets journeys assert wire
// normalization (scale stays 1 under a DPR-carrying fake; rects equal the
// Math.round'ed native seeds) without hand-writing core replies.
export function describeEnvelopeAt(h: Harness, index: number): Record<string, unknown> {
    const call = h.calls[index] as CapturedCall | undefined;
    assert.ok(call !== undefined && call.method === "DescribeWorld", "expected a DescribeWorld envelope at the index");
    return JSON.parse(call.payload) as Record<string, unknown>;
}

function appliedMatchesReplyInner(h: Harness, reply: Record<string, unknown>, normalize: boolean): void {
    const geometry = reply["windows"] as Array<Record<string, unknown>>;
    assert.ok(Array.isArray(geometry) && geometry.length > 0, "real reply carries desired window rows");
    const rows = reply["workspaces"] as Array<Record<string, unknown>>;
    assert.ok(Array.isArray(rows), "real reply carries workspace rows");
    for (const entry of geometry) {
        const frame = entry["frame"] as [number, number, number, number] | null;
        const win = winById(h, entry["win"] as string);
        if (frame !== null) {
            const live = normalize ? normalizedFrameOf(win) : frameOf(win);
            assert.deepEqual(live, { x: frame[0], y: frame[1], w: frame[2], h: frame[3] }, `native frame of ${entry["win"]} matches the Engine plan`);
        }
        assert.equal(win.output.name, entry["out"], `native output of ${entry["win"]} matches the Engine plan`);
        const planned = rows.find((row) => row["out"] === entry["out"] && row["id"] === entry["ws"]);
        assert.ok(planned !== undefined, `Engine plans a workspace row for ${entry["win"]} at ${entry["out"]}/${entry["ws"]}`);
        if (win.onAllDesktops === true) {
            // KWin all-desktops truth (sibling src/window.cpp
            // `Window::setOnAllDesktops`): a sticky window carries the
            // empty desktop list, never the roster. The Engine homes the
            // row from the output current instead of a member desktop
            // (kwin/src/world-observe.ts sticky homing), so the planned
            // core binding must name the live native current.
            assert.deepEqual(
                win.desktops.map((desk) => desk.id),
                [],
                `sticky native membership of ${entry["win"]} is the empty all-desktops list`,
            );
            const current = (h.workspace["currentDesktopForScreen"] as (screen: { name: string }) => FakeDesktop)({ name: entry["out"] as string });
            assert.deepEqual(
                [planned["bind"]],
                [current.id],
                `sticky home of ${entry["win"]} follows the native output current through the core binding`,
            );
            continue;
        }
        assert.deepEqual(
            win.desktops.map((desk) => desk.id),
            [planned["bind"]],
            `native membership of ${entry["win"]} matches the Engine plan through the core binding`,
        );
    }
}

// Batch-era observation helpers: one fresh capture incorporates native
// once and drains the whole parked FIFO batch, returning one final
// publication; per-command settlements ride the take lane. These helpers
// read actual Engine placements and lane identities instead of assuming
// press-time native order or reusing command sequence 1.
export function describeCount(h: Harness): number {
    return h.calls.filter((call) => call.method === "DescribeWorld").length;
}

export function lastDescribeReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered capture");
    return JSON.parse(h.replies.get(Math.max(...indices) as number) as string) as Record<string, unknown>;
}

export function replyFrames(reply: Record<string, unknown>): Map<string, { x: number; y: number; w: number; h: number }> {
    const out = new Map<string, { x: number; y: number; w: number; h: number }>();
    for (const entry of (reply["windows"] as Array<Record<string, unknown>>) ?? []) {
        const frame = entry["frame"] as [number, number, number, number] | null;
        if (frame !== null) {
            out.set(entry["win"] as string, { x: frame[0], y: frame[1], w: frame[2], h: frame[3] });
        }
    }
    return out;
}

export function takeReadys(h: Harness): Array<Record<string, unknown>> {
    return h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready");
}

export async function flushReads(h: Harness): Promise<string[]> {
    // Forward every pending presentation read to the retained real broker
    // and deliver each genuine reply to its subscriber callback in call
    // order (process lines preserve call order, like production D-Bus).
    const pending = h.reads.splice(0);
    const out: string[] = [];
    for (const read of pending) {
        const reply = await h.bridge.send("ReadWorldPresentation", read.payload);
        h.readReplies.push(reply);
        out.push(reply);
        read.callback(reply);
        h.runTimers();
    }
    return out;
}

export function answerEffectFacts(h: Harness, invalidation: string, changed: boolean): void {
    // Native-seam fixture (the effect service is stubbed here, as in
    // world-entry-freshness.test.ts): answer the single held
    // WaitPresentationFacts waiter with proven facts. Capture bytes stay
    // genuine: only this native seam is scripted.
    assert.equal(h.effectCalls.length, 1, "one held effect-facts waiter");
    const wait = h.effectCalls.splice(0)[0] as CapturedCall;
    wait.callback(JSON.stringify({
        v: 1, instance: "effect-1", rev: "0", invalidation,
        chord: 0, chordKnown: 0, gap: 0, width: 3, extension: -1,
        changed: changed ? 1 : 0, superseded: 0, available: 1,
    }));
}

export async function awaitDescribeFrom(h: Harness, from: number): Promise<number> {
    // Bounded wait for the next unanswered DescribeWorld at or after
    // `from`: the entry dispatches on its debounce after real events.
    for (let round = 0; round < 200; round += 1) {
        const index = h.calls.findIndex((call, at) => at >= from && call.method === "DescribeWorld" && !h.answered.has(at));
        if (index >= 0) {
            return index;
        }
        h.runTimers();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    const tail = h.logs.slice(-12).join("\n");
    assert.ok(false, `expected a dispatched capture through the production broker; recent logs:\n${tail}`);
    throw new Error("unreachable");
}

export async function awaitQueueCount(h: Harness, op: string, expected: number): Promise<void> {
    // Queue filings are async through the real broker (ordered process
    // lines, like production D-Bus): a rapid second press stays parked in
    // the entry until the first filing settles, so yield boundedly for
    // every filing before counting. The count asserts below keep their
    // exact messages; this only settles transport timing.
    for (let round = 0; round < 200; round += 1) {
        if (h.commandPayloads(op).length >= expected) {
            return;
        }
        h.runTimers();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
}

export async function awaitDispatch(h: Harness): Promise<number> {
    // Bounded wait for the parked batch to dispatch its single capture:
    // the dispatch gate fires only after every parked Queue filing settles,
    // so a dispatched capture proves real filing (never a sync assumption).
    for (let round = 0; round < 200; round += 1) {
        const index = h.calls.findIndex((call, at) => call.method === "DescribeWorld" && !h.answered.has(at));
        if (index >= 0) {
            return index;
        }
        h.runTimers();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    assert.ok(false, "expected the parked batch to dispatch one capture through the production broker");
    throw new Error("unreachable");
}

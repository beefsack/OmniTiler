import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";
import { dirname, join, resolve } from "node:path";

import {
    PLAN_DEBOUNCE_MS,
    PlanAdapter,
    type PlanAdapterEnv,
    type PlanObserved,
    type PlanSignal,
} from "../src/plan-adapter";
import { DOMAIN_GAP, OUTER_DOMAIN_GAP } from "../src/domain-gap";
import {
    WORKSPACE_SEND_GET_OWNER_METHOD,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WORKSPACE_SEND_METHOD,
    WORKSPACE_SEND_START_METHOD,
    WORKSPACE_SEND_DBUS_SERVICE,
} from "../src/workspace-send-adapter";
import { startWorkspaceSendAdapterEntry } from "../src/workspace-send-adapter-entry";

// D28 slotless production path (option (a)): NEVER-TILED born-fullscreen
// window through the REAL PlanAdapter initial hold, the SAME real Planner
// instance, a REAL workspace-send entry/adapter transfer, destination hold,
// and D5 first exit. One shared planner_eval bridge serves both adapters.
// Asserts source/target allocations never reflow and native setter counts:
// slotless send runs exactly one membership write with zero geometry and
// zero fullscreen/maximize writes; the mover stays fullscreen and slotless
// until its first exit admits it at the destination. Offline only.

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

    static start(): EngineBridge {
        return new EngineBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                cwd: repoRoot(),
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    send(request: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        return new Promise<string>((resolvePromise, reject) => {
            this.waiters.push({ resolve: resolvePromise, reject });
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

interface FakeSignal {
    connect: (handler: () => void) => void;
    disconnect: (handler: () => void) => void;
}

function makeSignal(): FakeSignal {
    const handlers = new Set<() => void>();
    return {
        connect: (handler) => {
            handlers.add(handler);
        },
        disconnect: (handler) => {
            handlers.delete(handler);
        },
    };
}

interface FakeWindow {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    output: { name: string };
    desktops: Array<object>;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    desktopsChanged: FakeSignal;
}

// Native write counters: reads never count, every property SET counts.
// Lets the production legs assert exact native setter counts during the
// send (exactly one membership write, zero geometry/fullscreen/maximize)
// instead of only counting PlanAdapter-side writes.
interface FakeWindowWrites {
    frameGeometry: number;
    desktops: number;
    fullScreen: number;
    maximizeMode: number;
}

function waitFor(condition: () => boolean, label: string): Promise<void> {
    const start = Date.now();
    const check = (): Promise<void> => {
        if (condition()) {
            return Promise.resolve();
        }
        if (Date.now() - start > 15000) {
            return Promise.reject(new Error(`timeout: ${label}`));
        }
        return new Promise((resolvePromise) => setTimeout(resolvePromise, 5)).then(check);
    };
    return check();
}

function settle(ms: number): Promise<void> {
    return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}

describe("D28 slotless production path (shared Planner)", () => {
    it("follow: holds, sends deferred, holds at destination, then admits on first exit", async () => {
        await runScenario(true);
    });

    it("stay: holds, sends deferred without switching, then admits on first exit", async () => {
        await runScenario(false);
    });

    async function runScenario(follow: boolean): Promise<void> {
        const out = { name: "out-1" };
        const d1 = { id: "ws-1" };
        const d2 = { id: "ws-2" };
        const writes: FakeWindowWrites = { frameGeometry: 0, desktops: 0, fullScreen: 0, maximizeMode: 0 };
        const mkWin = (id: string, desktop: object): FakeWindow => {
            const backing = {
                frameGeometry: { x: 10, y: 10, width: 500, height: 300 },
                desktops: [desktop],
                fullScreen: false,
                maximizeMode: 0,
            };
            const win = {
                normalWindow: true,
                managed: true,
                minimized: false,
                onAllDesktops: false,
                output: out,
                internalId: id,
                desktopsChanged: makeSignal(),
            } as FakeWindow;
            Object.defineProperties(win, {
                frameGeometry: {
                    enumerable: true,
                    configurable: true,
                    get: () => ({ ...backing.frameGeometry }),
                    set: (rect: { x: number; y: number; width: number; height: number }) => {
                        writes.frameGeometry += 1;
                        backing.frameGeometry = { ...rect };
                    },
                },
                desktops: {
                    enumerable: true,
                    configurable: true,
                    get: () => [...backing.desktops],
                    set: (refs: Array<object>) => {
                        writes.desktops += 1;
                        backing.desktops = [...refs];
                    },
                },
                fullScreen: {
                    enumerable: true,
                    configurable: true,
                    get: () => backing.fullScreen,
                    set: (value: boolean) => {
                        writes.fullScreen += 1;
                        backing.fullScreen = value;
                    },
                },
                maximizeMode: {
                    enumerable: true,
                    configurable: true,
                    get: () => backing.maximizeMode,
                    set: (value: number) => {
                        writes.maximizeMode += 1;
                        backing.maximizeMode = value;
                    },
                },
            });
            return win;
        };
        const resetWrites = (): void => {
            writes.frameGeometry = 0;
            writes.desktops = 0;
            writes.fullScreen = 0;
            writes.maximizeMode = 0;
        };
        // Born-fullscreen mover plus a tiled survivor on ws-1 and a tiled
        // target member on ws-2. One shared identity across adapters.
        const winF = mkWin("n-win-f", d1);
        const winS = mkWin("n-win-s", d1);
        const winT = mkWin("n-win-t", d2);
        winF.fullScreen = true;
        winF.frameGeometry = { x: 0, y: 0, width: 1200, height: 800 };
        // Sole tiles fill their domain inset by the outer gap, matching
        // planned allocations so reply readback stays exact.
        winS.frameGeometry = { x: 8, y: 8, width: 1184, height: 784 };
        winT.frameGeometry = { x: 8, y: 8, width: 1184, height: 784 };
        let current: object = d1;
        const sendSwitches: object[] = [];
        const surface: Record<string, unknown> = {
            activeWindow: winF,
            currentDesktop: d1,
            screens: [out],
            desktops: [d1, d2],
            currentDesktopForScreen: (): object => current,
            setCurrentDesktopForScreen: (desktop: object): void => {
                sendSwitches.push(desktop);
                current = desktop;
            },
            clientArea: (): object => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowList: (): Array<object> => [winF, winS, winT],
        };
        const bridge = EngineBridge.start();
        const replies: string[] = [];
        try {
            // ---- Shared pending queues, one bridge (one Planner). ----
            const planPending: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
            const sendPending: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
            const pump = async (): Promise<void> => {
                for (let round = 0; round < 20; round += 1) {
                    const nextPlan = planPending.shift();
                    if (nextPlan !== undefined) {
                        const reply = await bridge.send(nextPlan.payload);
                        replies.push(reply);
                        nextPlan.callback(reply);
                        continue;
                    }
                    const nextSend = sendPending.shift();
                    if (nextSend !== undefined) {
                        const reply = await bridge.send(nextSend.payload);
                        replies.push(reply);
                        nextSend.callback(reply);
                        continue;
                    }
                    return;
                }
                throw new Error("pump did not settle");
            };
            // ---- Real PlanAdapter observing the current desktop. ----
            const planGeometries: Array<{ target: object; rect: object }> = [];
            const planActives: object[] = [];
            const planFullscreenToggles: object[] = [];
            const planMaximizeClears: object[] = [];
            const planLogs: string[] = [];
            const planSubscribes: Array<{ kind: string; handler: (target?: object) => void }> = [];
            const planTimers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
            const observe = (): PlanObserved | null => {
                    const wins = (surface["windowList"] as () => Array<FakeWindow>)().filter(
                        (entry) => entry.desktops.includes(current),
                    );
                    const active = surface["activeWindow"] as FakeWindow | null;
                    if (active === null) {
                        return null;
                    }
                    const entries = wins.map((entry) =>
                        Object.freeze({
                            id: entry.internalId,
                            ref: entry,
                            rect: { x: entry.frameGeometry.x, y: entry.frameGeometry.y, w: entry.frameGeometry.width, h: entry.frameGeometry.height },
                            output: "out-1",
                            workspace: (current as { id: string }).id,
                            fullscreen: entry.fullScreen === true,
                            maximized: entry.maximizeMode !== 0,
                            floating: false,
                            sticky: false,
                            resourceClass: "unknown",
                        }),
                    );
                    const focused = active !== null && wins.includes(active) ? active.internalId : "";
                    // Content-derived fingerprint: identical observations
                    // stay quiet-equal at the reply boundary.
                    const fingerprint = JSON.stringify({
                        ws: (current as { id: string }).id,
                        wins: entries.map((entry) => [
                            entry.id,
                            entry.rect.x,
                            entry.rect.y,
                            entry.rect.w,
                            entry.rect.h,
                            entry.fullscreen,
                            entry.maximized,
                        ]),
                        focused,
                    });
                    return {
                        domainOutput: "out-1",
                        domainWorkspace: (current as { id: string }).id,
                        domainBounds: { x: 0, y: 0, w: 1200, h: 800 },
                        domainGap: DOMAIN_GAP,
                        domainOuterGap: OUTER_DOMAIN_GAP,
                        focusedId: focused,
                        windows: Object.freeze(entries),
                        activeRef: active,
                        fingerprint,
                        revalidate: () => true,
                    };
                };
            const planEnv: PlanAdapterEnv = {
                callDbus: (_service, _path, _iface, method, payload, callback): void => {
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
                    planPending.push({ payload, callback });
                },
                scheduleOnce: (delayMs, callback): (() => void) => {
                    const entry = { delayMs, callback, cancelled: false };
                    planTimers.push(entry);
                    return (): void => {
                        entry.cancelled = true;
                    };
                },
                log: (message): void => {
                    planLogs.push(message);
                },
                observe,
                clearMaximize: (target): "invoked" => {
                    planMaximizeClears.push(target);
                    return "invoked";
                },
                setFullscreen: (target): "invoked" => {
                    planFullscreenToggles.push(target);
                    return "invoked";
                },
                setGeometry: (target, rect): boolean => {
                    planGeometries.push({ target, rect: { ...rect } });
                    const win = target as FakeWindow;
                    win.frameGeometry = { x: rect.x, y: rect.y, width: rect.w, height: rect.h };
                    return true;
                },
                setActive: (target): boolean => {
                    planActives.push(target);
                    return true;
                },
                active: (): object | null => (surface["activeWindow"] as object | null) ?? null,
                subscribe: (kind: PlanSignal, handler: (target?: object) => void): (() => void) => {
                    planSubscribes.push({ kind, handler });
                    return (): void => {};
                },
            };
            const plan = new PlanAdapter(planEnv);
            assert.equal(plan.enable({ owner: "owner-1", generation: "gen-1" }), true);
            const firePlan = (kind: string, target?: object): void => {
                for (const sub of planSubscribes) {
                    if (sub.kind === kind) {
                        sub.handler(target);
                    }
                }
            };
            const runPlanDebounce = (): void => {
                const pending = [...planTimers];
                planTimers.length = 0;
                for (const timer of pending) {
                    if (timer.cancelled) {
                        continue;
                    }
                    if (timer.delayMs === PLAN_DEBOUNCE_MS) {
                        timer.callback();
                    } else {
                        planTimers.push(timer);
                    }
                }
            };
            // ---- Real workspace-send entry sharing the fake surface. ----
            const sendLogs: string[] = [];
            const sendEntry = startWorkspaceSendAdapterEntry({
                workspace: surface,
                callDbus: (service, _path, _iface, method, payload, callback) => {
                    if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                        callback(true);
                        return;
                    }
                    if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_GET_OWNER_METHOD) {
                        callback(":1.7");
                        return;
                    }
                    if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_START_METHOD) {
                        callback(1);
                        return;
                    }
                    if (method === WORKSPACE_SEND_METHOD) {
                        sendPending.push({ payload, callback });
                        return;
                    }
                    callback(undefined);
                },
                scheduleOnce: () => (): void => {},
                log: (message) => {
                    sendLogs.push(message);
                },
                owner: "owner-1",
                generation: "gen-1",
                readInnerGapFn: () => 8,
                readOuterGapFn: () => 8,
            });
            assert.notEqual(sendEntry, null, "send entry starts");
            if (sendEntry === null) {
                throw new Error("send entry failed to start");
            }

            // ---- Leg 1: born-fullscreen hold on ws-1 (real PlanAdapter). ----
            firePlan("added");
            runPlanDebounce();
            assert.equal(planPending.length, 1, "hold dispatches exactly one reconcile");
            const holdPayload = JSON.parse(planPending[0]?.payload ?? "{}") as Record<string, unknown>;
            const holdWindows = holdPayload["windows"] as Array<Record<string, unknown>>;
            const holdMover = holdWindows.find((entry) => entry["window"] === "n-win-f");
            assert.deepEqual(
                { floating: holdMover?.["floating"], fullscreen: holdMover?.["fullscreen"] },
                { floating: true, fullscreen: true },
                "real hold rides synthetic floating, still fullscreen",
            );
            await pump();
            const holdReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(holdReply["outcome"], "planned", "Planner holds the born-fullscreen member");
            const holdGeometry = (holdReply["desired_geometry"] as Array<Record<string, unknown>>).map(
                (entry) => entry["window"],
            );
            assert.ok(!holdGeometry.includes("n-win-f"), "held mover takes no allocation");
            assert.ok(
                planLogs.some((line) => line.includes("initial-fullscreen-held window=n-win-f")),
                planLogs.join("\n"),
            );
            const holdGeom = holdReply["desired_geometry"] as Array<Record<string, unknown>>;
            assert.deepEqual(
                holdGeom.map((entry) => entry["window"]),
                ["n-win-s"],
                "hold covers the survivor alone",
            );
            const survivorBaseline = (holdGeom[0]?.["rect"] as Record<string, unknown>) ?? {};
            assert.deepEqual(survivorBaseline, { x: 8, y: 8, w: 1184, h: 784 }, "survivor fills the domain alone");
            assert.ok(!planGeometries.some((entry) => entry.target === winF), "held mover never actuated");
            assert.deepEqual(planFullscreenToggles, [], "no fullscreen writes on hold");
            assert.deepEqual(planMaximizeClears, [], "no maximize writes on hold");
            assert.deepEqual(planActives, [], "no automatic focus writes over the overlay");
            const planWritesAfterHold = planGeometries.length;

            // ---- Leg 2: explicit send ws-1 -> ws-2 (real entry + adapter). ----
            const moverFrameBefore = { ...winF.frameGeometry };
            resetWrites();
            assert.equal(sendEntry.requestSend("ws-2", follow), true, "slotless send dispatches");
            await waitFor(() => sendPending.length > 0, "send observer request");
            await pump();
            await settle(100);
            const sendReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(sendReply["outcome"], "planned", `Planner plans the slotless send: ${JSON.stringify(sendReply)}`);
            assert.equal((sendReply["operation"] as Record<string, unknown>)["op"], "move-deferred", "shared deferred operation");
            assert.equal((sendReply["operation"] as Record<string, unknown>)["window"], "n-win-f");
            assert.equal((sendReply["operation"] as Record<string, unknown>)["follow"], follow);
            assert.ok(!("leaf" in ((sendReply["operation"] as Record<string, unknown>) ?? {})), "no fabricated leaf");
            assert.deepEqual(winF.desktops, [d2], "mover natively on exact target, absent source");
            assert.equal(winF.fullScreen, true, "no gratuitous fullscreen drop");
            assert.deepEqual({ ...winF.frameGeometry }, moverFrameBefore, "no geometry write to the mover");
            // Exact native setter counts during the send: one membership
            // write, zero geometry/fullscreen/maximize writes.
            assert.equal(writes.desktops, 1, "exactly one native membership write during the send");
            assert.equal(writes.frameGeometry, 0, "zero native geometry writes during the send");
            assert.equal(writes.fullScreen, 0, "zero native fullscreen writes during the send");
            assert.equal(writes.maximizeMode, 0, "zero native maximize writes during the send");
            if (follow) {
                assert.deepEqual(sendSwitches, [d2], "follow switches exactly once");
                assert.equal(surface["activeWindow"], winF, "mover focused after proof");
            } else {
                assert.deepEqual(sendSwitches, [], "stay never switches desktops");
                assert.equal(surface["activeWindow"], winS, "stay focuses the source survivor");
            }
            assert.equal(sendEntry.isInFlight(), false, "send flight released");
            assert.equal(planGeometries.length, planWritesAfterHold, "PlanAdapter writes nothing during the send");

            // ---- Leg 3: destination still holds. ----
            if (!follow) {
                // Stay leaves the foreground on the source; the user
                // switches to inspect the target, natively activating a
                // window there.
                current = d2;
                surface["activeWindow"] = winT;
            }
            assert.equal((current as { id: string }).id, "ws-2", "destination observed");
            // Production entry edge: terminal settlement forces one
            // complete source AND target reconcile on the PlanAdapter.
            plan.notifySendSettled({
                sourceOutput: "out-1",
                sourceWorkspace: "ws-1",
                targetOutput: "out-1",
                targetWorkspace: "ws-2",
            });
            runPlanDebounce();
            assert.equal(planPending.length, 1, "destination refresh dispatches");
            const destPayload = JSON.parse(planPending[0]?.payload ?? "{}") as Record<string, unknown>;
            assert.equal((destPayload["domain"] as Record<string, unknown>)["workspace"], "ws-2");
            const destWindows = destPayload["windows"] as Array<Record<string, unknown>>;
            const destMover = destWindows.find((entry) => entry["window"] === "n-win-f");
            assert.deepEqual(
                { floating: destMover?.["floating"], fullscreen: destMover?.["fullscreen"] },
                { floating: true, fullscreen: true },
                "destination still holds the mover",
            );
            const writesBeforeDest = planGeometries.length;
            await pump();
            const destReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(destReply["outcome"], "planned");
            const destGeometry = (destReply["desired_geometry"] as Array<Record<string, unknown>>).map(
                (entry) => entry["window"],
            );
            assert.ok(!destGeometry.includes("n-win-f"), "destination hold admits nothing");
            assert.ok(
                !planGeometries.slice(writesBeforeDest).some((entry) => entry.target === winF),
                "destination hold never actuates the mover",
            );
            assert.deepEqual(planFullscreenToggles, [], "still no fullscreen writes");
            assert.ok(sendLogs.some((line) => line.includes("stay-confirmed") || line.includes("arrived") || line.includes("follow")), sendLogs.join("\n"));

            // ---- Leg 4: source converges clean without the mover. ----
            current = d1;
            surface["activeWindow"] = winS;
            firePlan("desktops");
            runPlanDebounce();
            assert.equal(planPending.length, 1, "source refresh dispatches");
            await pump();
            const sourceReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(sourceReply["outcome"], "planned");
            const sourceGeometry = sourceReply["desired_geometry"] as Array<Record<string, unknown>>;
            assert.deepEqual(
                sourceGeometry.map((entry) => entry["window"]),
                ["n-win-s"],
                "source converges the survivor alone",
            );
            assert.deepEqual(sourceGeometry[0]?.["rect"], survivorBaseline, "source allocation never reflows");

            // ---- Leg 5: D5 first exit admits at the destination. ----
            current = d2;
            surface["activeWindow"] = winF;
            const exitWritesBefore = { ...writes };
            winF.fullScreen = false;
            winF.frameGeometry = { x: 10, y: 10, width: 500, height: 300 };
            firePlan("fullscreen");
            runPlanDebounce();
            assert.equal(planPending.length, 1, "exit dispatches");
            const exitPayload = JSON.parse(planPending[0]?.payload ?? "{}") as Record<string, unknown>;
            const exitMover = (exitPayload["windows"] as Array<Record<string, unknown>>).find(
                (entry) => entry["window"] === "n-win-f",
            );
            assert.equal(exitMover?.["floating"], undefined, "released member rides as a normal tile");
            assert.ok(
                planLogs.some((line) => line.includes("initial-fullscreen-released window=n-win-f")),
                planLogs.join("\n"),
            );
            const writesBeforeExit = planGeometries.length;
            await pump();
            const exitReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(exitReply["outcome"], "planned");
            const exitWindows = (exitReply["desired_geometry"] as Array<Record<string, unknown>>).map(
                (entry) => `${String(entry["window"])}@${String(entry["workspace"])}`,
            );
            assert.ok(exitWindows.includes("n-win-f@ws-2"), `exit admits at destination: ${exitWindows.join(",")}`);
            assert.ok(!exitWindows.some((entry) => entry.startsWith("n-win-f@ws-1")), "never back on the source");
            assert.ok(
                planGeometries.slice(writesBeforeExit).some((entry) => entry.target === winF),
                "first exit admits the released window at its placement",
            );
            assert.deepEqual(planFullscreenToggles, [], "admission performs no fullscreen writes");
            assert.deepEqual(planMaximizeClears, [], "admission performs no maximize writes");
            // The exit simulation above wrote fullscreen/frameGeometry
            // directly (native truth); the admission itself must add no
            // fullscreen/maximize/membership writes.
            assert.equal(writes.fullScreen, exitWritesBefore.fullScreen + 1, "no adapter fullscreen writes on exit");
            assert.equal(writes.maximizeMode, exitWritesBefore.maximizeMode, "no maximize writes on exit");
            assert.equal(writes.desktops, exitWritesBefore.desktops, "no membership writes on exit");
            sendEntry.stop();
        } finally {
            await bridge.close();
        }
    }
});

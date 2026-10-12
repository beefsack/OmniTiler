import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";
import { dirname, join, resolve } from "node:path";

import {
    PLAN_DEBOUNCE_MS,
    INTENT_READ_METHOD,
    INTENT_WRITE_METHOD,
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

// D16 ordinary intentional-float production path (option (a), no setting):
// a tiled window floated through the REAL PlanAdapter ToggleFloat, then
// sent ws-1 -> ws-2 through the REAL workspace-send entry/adapter and the
// SAME real Planner instance, then (roundtrip) sent back ws-2 -> ws-1.
// Asserts the shared `move-deferred` operation, source/target allocations
// never reflow, and exact native setter counts: each slotless send runs
// exactly one membership write with zero geometry and zero
// fullscreen/maximize writes; the mover keeps its live frame as-is and
// stays floating with its size on arrival. The Engine-owned float intent
// is mirrored into the send entry floating-ids input (the production
// plan entry feeds its observers from the same intent state; the dev
// entry takes it as an explicit input). Offline only.

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

// Minimal local durable-intent store answering the Q3 read/write intents
// (the planner_eval seam speaks DescribePlan only; the planner service
// answers these methods in production, pinned by the float-intent suite).
// Starts empty; writes replace the snapshot; reads prune to live.
function answerIntent(
    store: { members: Set<string>; tile: Set<string> },
    method: string,
    payload: string,
    callback: (reply: unknown) => void,
): boolean {
    if (method !== INTENT_READ_METHOD && method !== INTENT_WRITE_METHOD) {
        return false;
    }
    let request: Record<string, unknown>;
    try {
        request = JSON.parse(payload) as Record<string, unknown>;
    } catch (error) {
        void error;
        callback(JSON.stringify({ v: 1, correlation_id: "-", outcome: "rejected", reason: "invalid-request" }));
        return true;
    }
    const correlation = request["correlation_id"];
    if (method === INTENT_READ_METHOD) {
        const live = Array.isArray(request["live"]) ? new Set(request["live"] as Array<string>) : null;
        const returned = [...store.members].filter((id) => live === null || live.has(id));
        const returnedTile = [...store.tile].filter((id) => live === null || live.has(id));
        callback(
            JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "ok",
                members: returned,
                stored: store.members.size,
                returned: returned.length,
                tile: returnedTile,
                tile_stored: store.tile.size,
                tile_returned: returnedTile.length,
            }),
        );
        return true;
    }
    const members = request["members"];
    const tile = Array.isArray(request["tile"]) ? (request["tile"] as Array<string>) : [];
    if (!Array.isArray(members)) {
        callback(JSON.stringify({ v: 1, correlation_id: correlation, outcome: "rejected", reason: "invalid-members" }));
        return true;
    }
    store.members = new Set(members as Array<string>);
    store.tile = new Set(tile);
    callback(
        JSON.stringify({
            v: 1,
            correlation_id: correlation,
            outcome: "stored",
            stored: (members as Array<string>).length,
            tile_stored: tile.length,
        }),
    );
    return true;
}

describe("D16 float production path (shared Planner)", () => {
    it("follow: floats, sends deferred, arrives floating with frame kept", async () => {
        await runScenario(true);
    });

    it("stay: floats, sends deferred without switching, arrives floating", async () => {
        await runScenario(false);
    });

    it("roundtrip: follow out, stay back, float retained both legs", async () => {
        await runRoundtrip();
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
        // Float mover plus a tiled survivor on ws-1 and a tiled target
        // member on ws-2. One shared identity across adapters.
        const winF = mkWin("n-win-f", d1);
        const winS = mkWin("n-win-s", d1);
        const winT = mkWin("n-win-t", d2);
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
            const planGeometries: Array<{ target: object; rect: object }> = [];
            const planActives: object[] = [];
            const planFullscreenToggles: object[] = [];
            const planMaximizeClears: object[] = [];
            const planLogs: string[] = [];
            const planSubscribes: Array<{ kind: string; handler: (target?: object) => void }> = [];
            const planTimers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
            const hydratedIntents: Array<ReadonlyArray<string>> = [];
            const intentStore = { members: new Set<string>(), tile: new Set<string>() };
            // Settled intent merged into observation, mirroring the real
            // plan entry (which feeds its observers from
            // getIntentMembers): after ToggleFloat commits, refreshes keep
            // floating the mover instead of re-observing it tiled.
            let planRef: PlanAdapter | null = null;
            const planIntentIds = (): Set<string> => {
                try {
                    return new Set<string>([...(planRef?.getIntentMembers() ?? [])]);
                } catch (error) {
                    void error;
                    return new Set<string>();
                }
            };
            const observe = (): PlanObserved | null => {
                const wins = (surface["windowList"] as () => Array<FakeWindow>)().filter(
                    (entry) => entry.desktops.includes(current),
                );
                const active = surface["activeWindow"] as FakeWindow | null;
                if (active === null) {
                    return null;
                }
                const intentIds = planIntentIds();
                const entries = wins.map((entry) =>
                    Object.freeze({
                        id: entry.internalId,
                        ref: entry,
                        rect: { x: entry.frameGeometry.x, y: entry.frameGeometry.y, w: entry.frameGeometry.width, h: entry.frameGeometry.height },
                        output: "out-1",
                        workspace: (current as { id: string }).id,
                        fullscreen: entry.fullScreen === true,
                        maximized: entry.maximizeMode !== 0,
                        floating: intentIds.has(entry.internalId),
                        sticky: false,
                        resourceClass: "unknown",
                    }),
                );
                const focused = active !== null && wins.includes(active) ? active.internalId : "";
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
                    if (answerIntent(intentStore, method, payload, callback)) {
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
                // Float apply raises the mover above: reads report not-above
                // so the single setKeepAbove write runs and reports invoked.
                readKeepAbove: (): boolean => false,
                readKeepBelow: (): boolean => false,
                setKeepAbove: (): "invoked" => "invoked",
                setKeepBelow: (): "invoked" => "invoked",                subscribe: (kind: PlanSignal, handler: (target?: object) => void): (() => void) => {
                    planSubscribes.push({ kind, handler });
                    return (): void => {};
                },
                // Q3 durable float intent: complete live inventory plus the
                // hydration sink, so ToggleFloat commits persist intent and
                // later refreshes keep floating the mover. The real Planner
                // answers the read/write intents through the shared bridge.
                observeCompleteInventory: (): { complete: boolean; ids: string[] } | null => {
                    try {
                        const list = surface["windowList"] as () => Array<FakeWindow>;
                        const ids: string[] = [];
                        for (const entry of list()) {
                            if (typeof entry.internalId !== "string" || entry.internalId.length === 0) {
                                return null;
                            }
                            ids.push(entry.internalId);
                        }
                        return { complete: true, ids };
                    } catch (error) {
                        void error;
                        return null;
                    }
                },
                onIntentHydrated: (members: ReadonlyArray<string>): void => {
                    hydratedIntents.push([...members]);
                },
            };
            const plan = new PlanAdapter(planEnv);
            assert.equal(plan.enable({ owner: "owner-1", generation: "gen-1" }), true);
            planRef = plan;
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
            // Engine-owned float intent mirrored into the send entry input.
            const sendFloatingIds = new Set<string>();
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
                floatingIds: sendFloatingIds,
            });
            assert.notEqual(sendEntry, null, "send entry starts");
            if (sendEntry === null) {
                throw new Error("send entry failed to start");
            }

            // ---- Leg 0: intent bootstrap, then admit all three as tiles
            // on ws-1 (real PlanAdapter). The bootstrap read hydrates
            // (empty store) through the local intent store; first admission
            // follows through the catch-up resync on a second debounce.
            surface["activeWindow"] = winF;
            assert.equal(plan.startIntentBootstrap(), true, "intent bootstrap starts");
            firePlan("added");
            runPlanDebounce();
            await pump();
            assert.deepEqual(hydratedIntents, [[]], "bootstrap hydrates the empty store");
            assert.equal(plan.isIntentReady(), true, "intent ready before first admission");
            runPlanDebounce();
            await pump();
            const admitReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(admitReply["outcome"], "planned", "Planner admits the tiles");
            const planWritesAfterAdmit = planGeometries.length;

            // ---- Leg 1: ToggleFloat win-f through the real PlanAdapter. ----
            resetWrites();
            surface["activeWindow"] = winF;
            plan.requestFloat();
            runPlanDebounce();
            assert.equal(planPending.length, 1, "float dispatches exactly one reconcile");
            const floatBase = replies.length;
            await pump();
            // The toggle commit also persists intent: the toggle reply lands
            // through the bridge while the intent write answers synchronously
            // through the local store.
            const floatReply = JSON.parse(replies[floatBase] ?? "{}") as Record<string, unknown>;
            assert.equal(floatReply["outcome"], "planned", "Planner floats the mover");
            assert.ok(
                planLogs.some((line) => line.includes(":intent-write") && line.includes("outcome=stored")),
                "float intent persists",
            );
            assert.deepEqual([...intentStore.members], ["n-win-f"], "intent store holds the mover");
            sendFloatingIds.add("n-win-f");
            assert.ok(
                planGeometries.slice(planWritesAfterAdmit).some((entry) => entry.target === winF),
                "floating the mover places its float frame",
            );
            assert.deepEqual(planFullscreenToggles, [], "no fullscreen writes on float");
            assert.deepEqual(planMaximizeClears, [], "no maximize writes on float");
            const floatFrame = { ...winF.frameGeometry };
            // Baseline after the float reflow (the surviving tile fills
            // the domain when the mover floats): the send must add zero.
            const planWritesAfterFloat = planGeometries.length;

            // ---- Leg 2: explicit send ws-1 -> ws-2 (real entry + adapter). ----
            resetWrites();
            assert.equal(sendEntry.requestSend("ws-2", follow), true, "float send dispatches");
            await waitFor(() => sendPending.length > 0, "send observer request");
            const sendBody = JSON.parse(sendPending[0]?.payload ?? "{}") as Record<string, unknown>;
            const sendWindows = sendBody["windows"] as Array<Record<string, unknown>>;
            const sendMover = sendWindows.find((entry) => entry["window"] === "n-win-f");
            assert.equal(sendMover?.["floating"], true, "float mover rides still floating");
            assert.equal(sendBody["focused_window"], "n-win-f");
            await pump();
            await settle(100);
            const sendReply = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(sendReply["outcome"], "planned", `Planner plans the float send: ${JSON.stringify(sendReply)}`);
            assert.equal((sendReply["operation"] as Record<string, unknown>)["op"], "move-deferred", "shared deferred operation");
            assert.equal((sendReply["operation"] as Record<string, unknown>)["window"], "n-win-f");
            assert.equal((sendReply["operation"] as Record<string, unknown>)["follow"], follow);
            assert.ok(!("leaf" in ((sendReply["operation"] as Record<string, unknown>) ?? {})), "no fabricated leaf");
            assert.deepEqual(winF.desktops, [d2], "mover natively on exact target, absent source");
            assert.deepEqual({ ...winF.frameGeometry }, floatFrame, "arrival frame kept as-is");
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
            assert.equal(planGeometries.length, planWritesAfterFloat, "PlanAdapter writes nothing during the send");

            // ---- Leg 3: destination still floats the mover. ----
            if (!follow) {
                current = d2;
                surface["activeWindow"] = winT;
            }
            assert.equal((current as { id: string }).id, "ws-2", "destination observed");
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
            assert.equal(destMover?.["floating"], true, "destination still floats the mover");
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
            assert.deepEqual(sourceGeometry[0]?.["rect"], { x: 8, y: 8, w: 1184, h: 784 }, "source allocation never reflows");
            sendEntry.stop();
        } finally {
            await bridge.close();
        }
    }

    async function runRoundtrip(): Promise<void> {
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
        const winF = mkWin("n-win-f", d1);
        const winS = mkWin("n-win-s", d1);
        const winT = mkWin("n-win-t", d2);
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
            const planGeometries: Array<{ target: object; rect: object }> = [];
            const planLogs: string[] = [];
            const planSubscribes: Array<{ kind: string; handler: (target?: object) => void }> = [];
            const planTimers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }> = [];
            // Settled intent merged into observation (same mirror as the
            // follow/stay scenario; no refresh legs run here, so the set
            // stays empty and every observation stays tiled).
            let planRef: PlanAdapter | null = null;
            const planIntentIds = (): Set<string> => {
                try {
                    return new Set<string>([...(planRef?.getIntentMembers() ?? [])]);
                } catch (error) {
                    void error;
                    return new Set<string>();
                }
            };
            const observe = (): PlanObserved | null => {
                const wins = (surface["windowList"] as () => Array<FakeWindow>)().filter(
                    (entry) => entry.desktops.includes(current),
                );
                const active = surface["activeWindow"] as FakeWindow | null;
                if (active === null) {
                    return null;
                }
                const intentIds = planIntentIds();
                const entries = wins.map((entry) =>
                    Object.freeze({
                        id: entry.internalId,
                        ref: entry,
                        rect: { x: entry.frameGeometry.x, y: entry.frameGeometry.y, w: entry.frameGeometry.width, h: entry.frameGeometry.height },
                        output: "out-1",
                        workspace: (current as { id: string }).id,
                        fullscreen: entry.fullScreen === true,
                        maximized: entry.maximizeMode !== 0,
                        floating: intentIds.has(entry.internalId),
                        sticky: false,
                        resourceClass: "unknown",
                    }),
                );
                const focused = active !== null && wins.includes(active) ? active.internalId : "";
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
                clearMaximize: (): "invoked" => "invoked",
                setFullscreen: (): "invoked" => "invoked",
                setGeometry: (target, rect): boolean => {
                    planGeometries.push({ target, rect: { ...rect } });
                    const win = target as FakeWindow;
                    win.frameGeometry = { x: rect.x, y: rect.y, width: rect.w, height: rect.h };
                    return true;
                },
                setActive: (): boolean => true,
                active: (): object | null => (surface["activeWindow"] as object | null) ?? null,
                subscribe: (kind: PlanSignal, handler: (target?: object) => void): (() => void) => {
                    planSubscribes.push({ kind, handler });
                    return (): void => {};
                },
            };
            const plan = new PlanAdapter(planEnv);
            assert.equal(plan.enable({ owner: "owner-1", generation: "gen-1" }), true);
            planRef = plan;
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
            const sendFloatingIds = new Set<string>();
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
                log: () => {},
                owner: "owner-1",
                generation: "gen-1",
                readInnerGapFn: () => 8,
                readOuterGapFn: () => 8,
                floatingIds: sendFloatingIds,
            });
            assert.notEqual(sendEntry, null, "send entry starts");
            if (sendEntry === null) {
                throw new Error("send entry failed to start");
            }

            // Admit, then float the mover through the real PlanAdapter.
            surface["activeWindow"] = winF;
            firePlan("added");
            runPlanDebounce();
            await pump();
            surface["activeWindow"] = winF;
            plan.requestFloat();
            runPlanDebounce();
            assert.equal(planPending.length, 1, "float dispatches");
            await pump();
            assert.equal(
                (JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>)["outcome"],
                "planned",
                "Planner floats the mover",
            );
            sendFloatingIds.add("n-win-f");
            const floatFrame = { ...winF.frameGeometry };

            // Leg one: ws-1 -> ws-2 (follow).
            writes.frameGeometry = 0;
            writes.desktops = 0;
            writes.fullScreen = 0;
            writes.maximizeMode = 0;
            assert.equal(sendEntry.requestSend("ws-2", true), true, "leg one dispatches");
            await waitFor(() => sendPending.length > 0, "leg one observer request");
            await pump();
            await settle(100);
            const legOne = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(legOne["outcome"], "planned", `leg one plans: ${JSON.stringify(legOne)}`);
            assert.equal((legOne["operation"] as Record<string, unknown>)["op"], "move-deferred");
            assert.deepEqual(winF.desktops, [d2], "leg one arrives on ws-2");
            assert.deepEqual({ ...winF.frameGeometry }, floatFrame, "leg one keeps the frame");
            assert.equal(writes.desktops, 1, "leg one writes membership once");
            assert.equal(writes.frameGeometry, 0, "leg one writes no geometry");
            assert.deepEqual(sendSwitches, [d2], "leg one follow switches once");
            assert.equal(surface["activeWindow"], winF, "leg one focuses the mover");
            assert.equal(sendEntry.isInFlight(), false, "leg one flight released");
            assert.ok(planLogs.length >= 0);

            // Leg two: ws-2 -> ws-1 (stay). The follow left the mover
            // active on ws-2 with float intent still mirrored.
            writes.frameGeometry = 0;
            writes.desktops = 0;
            writes.fullScreen = 0;
            writes.maximizeMode = 0;
            const switchesBefore = sendSwitches.length;
            assert.equal(sendEntry.requestSend("ws-1", false), true, "leg two dispatches");
            await waitFor(() => sendPending.length > 0, "leg two observer request");
            const legTwoBody = JSON.parse(sendPending[0]?.payload ?? "{}") as Record<string, unknown>;
            const legTwoWindows = legTwoBody["windows"] as Array<Record<string, unknown>>;
            assert.equal(
                legTwoWindows.find((entry) => entry["window"] === "n-win-f")?.["floating"],
                true,
                "return leg still rides floating",
            );
            await pump();
            await settle(100);
            const legTwo = JSON.parse(replies[replies.length - 1] ?? "{}") as Record<string, unknown>;
            assert.equal(legTwo["outcome"], "planned", `leg two plans: ${JSON.stringify(legTwo)}`);
            assert.equal((legTwo["operation"] as Record<string, unknown>)["op"], "move-deferred", "return leg also carries the deferred transfer");
            assert.deepEqual(winF.desktops, [d1], "return leg arrives home");
            assert.deepEqual({ ...winF.frameGeometry }, floatFrame, "size kept across the roundtrip");
            assert.equal(writes.desktops, 1, "leg two writes membership once");
            assert.equal(writes.frameGeometry, 0, "leg two writes no geometry");
            assert.equal(writes.fullScreen, 0, "leg two writes no fullscreen");
            assert.equal(writes.maximizeMode, 0, "leg two writes no maximize");
            assert.equal(sendSwitches.length, switchesBefore, "return stay never switches");
            assert.equal(surface["activeWindow"], winT, "return stay focuses the ws-2 MRU survivor");
            assert.equal(sendEntry.isInFlight(), false, "leg two flight released");
            sendEntry.stop();
        } finally {
            await bridge.close();
        }
    }
});

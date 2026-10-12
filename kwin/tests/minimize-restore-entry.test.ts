// D23 minimize/restore (R-MNZ-01/02/03) through the production
// `startPlanAdapterEntry` route + real Planner (`planner_eval`).
// No hand-built wire replies for the asserted paths.
//
// MNZ-01: native minimize of middle tile B in H[A,B*,C] is omitted from the
// converged observation (stored old slot, siblings reflow, focus falls to
// MRU C, no OmniTiler focus write).
// MNZ-02: native restore re-observes B and converges back to the stored old
// slot (pre-minimize rect) with no OmniTiler focus write (host owns restore
// focus; the harness keeps C active across the restore).
// MNZ-03: sole-window minimize converges explicitly (retain workspace, focus
// none, stored slot) under both null active and minimized-staying active;
// restore re-admits with no focus write. No release-domain op anywhere.
// Invalidation fallback: an unreadable frame on a minimized member is
// ignored (omission proceeds); an unreadable frame on a tiled member still
// fails the whole domain closed (no dispatch).

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { PLAN_DEBOUNCE_MS } from "../src/plan-adapter";
import { startPlanAdapterEntry } from "../src/plan-adapter-entry";

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
    private readonly waiters: Array<{ resolve: (r: string) => void; reject: (e: Error) => void }> = [];
    private dead: string | null = null;
    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const out = proc.stdout;
        if (out === null || out === undefined) {
            throw new Error("planner_eval stdout unavailable");
        }
        createInterface({ input: out }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.resume();
        proc.on("error", (e) => this.fail(e instanceof Error ? e : new Error(String(e))));
        proc.on("exit", (c) => this.fail(new Error(`planner_eval exited ${String(c)}`)));
    }
    private fail(e: Error): void {
        if (this.dead === null) {
            this.dead = e.message;
        }
        while (this.waiters.length > 0) {
            this.waiters.shift()?.reject(e);
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
    send(req: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        return new Promise<string>((res, rej) => {
            this.waiters.push({ resolve: res, reject: rej });
            try {
                this.proc.stdin?.write(req + "\n");
            } catch (e) {
                rej(e instanceof Error ? e : new Error(String(e)));
            }
        });
    }
    async close(): Promise<void> {
        try {
            this.proc.stdin?.end();
        } catch {
            /* noop */
        }
        await new Promise<void>((done) => {
            const t = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch {
                    /* noop */
                }
                done();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(t);
                done();
            });
        });
    }
}

interface Sig {
    readonly handlers: Set<() => void>;
    readonly signal: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    fire: () => void;
}
function makeSig(): Sig {
    const handlers = new Set<() => void>();
    return {
        handlers,
        signal: {
            connect: (h: () => void): void => {
                handlers.add(h);
            },
            disconnect: (h: () => void): void => {
                handlers.delete(h);
            },
        },
        fire: (): void => {
            for (const h of [...handlers]) {
                h();
            }
        },
    };
}

interface MWin extends Record<string, unknown> {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    output: { name: string; geometry: { x: number; y: number; width: number; height: number } };
    desktops: Array<{ id: string; x11DesktopNumber: number }>;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number } | null;
    resourceClass: string;
}

interface Harness {
    readonly bridge: EngineBridge;
    readonly workspace: Record<string, unknown>;
    readonly wins: MWin[];
    readonly sig: { windowActivated: Sig };
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(r: unknown) => void>;
    readonly replies: Map<number, string>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }>;
    readonly logs: string[];
    readonly entryWrites: object[];
    readonly handle: NonNullable<ReturnType<typeof startPlanAdapterEntry>>;
    setHostActive: (w: MWin | null) => void;
    flushAll: () => Promise<void>;
    fireTimers: (d: number) => number;
    settlePlan: () => Promise<void>;
    stop: () => Promise<void>;
}

function makeWin(
    output: { name: string; geometry: { x: number; y: number; width: number; height: number } },
    desk: { id: string; x11DesktopNumber: number },
    id: string,
    frame: { x: number; y: number; width: number; height: number },
): MWin {
    const s = (): Sig => makeSig();
    const d = s();
    const o = s();
    const f = s();
    const full = s();
    const max = s();
    return {
        normalWindow: true,
        managed: true,
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        output,
        desktops: [desk],
        internalId: id,
        frameGeometry: { ...frame },
        resourceClass: "app",
        desktopsChanged: d.signal,
        outputChanged: o.signal,
        frameGeometryChanged: f.signal,
        fullScreenChanged: full.signal,
        maximizedChanged: max.signal,
    } as unknown as MWin;
}

async function makeHarness(ids: string[], initialActive: number): Promise<Harness> {
    const out = { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
    const ws = { id: "ws-1", x11DesktopNumber: 1 };
    const wins = ids.map((id, index) =>
        makeWin(out, ws, id, { x: 20 + index * 420, y: 20, width: 400, height: 300 }),
    );
    const wsig = {
        windowAdded: makeSig(),
        windowRemoved: makeSig(),
        windowActivated: makeSig(),
        screensChanged: makeSig(),
        currentDesktopChanged: makeSig(),
        desktopsChanged: makeSig(),
    };
    const backing: { active: MWin | null } = { active: wins[initialActive] ?? null };
    const entryWrites: object[] = [];
    let hostWrite = false;
    const workspace: Record<string, unknown> = {
        activeScreen: out,
        screens: [out],
        desktops: [ws],
        currentDesktopForScreen: (): object => ws,
        currentDesktop: ws,
        clientArea: (): object => ({ x: 0, y: 0, w: 1920, h: 1040 }),
        windowList: (): object[] => [...wins],
        windowAdded: wsig.windowAdded.signal,
        windowRemoved: wsig.windowRemoved.signal,
        windowActivated: wsig.windowActivated.signal,
        screensChanged: wsig.screensChanged.signal,
        currentDesktopChanged: wsig.currentDesktopChanged.signal,
        desktopsChanged: wsig.desktopsChanged.signal,
    };
    Object.defineProperty(workspace, "activeWindow", {
        get: () => backing.active,
        set: (v: unknown) => {
            if (!hostWrite && typeof v === "object" && v !== null) {
                entryWrites.push(v as object);
            }
            backing.active = v as MWin | null;
        },
        configurable: true,
        enumerable: true,
    });
    const bridge = EngineBridge.start();
    const dbusCalls: Harness["dbusCalls"] = [];
    const callbacks: Harness["callbacks"] = [];
    const replies = new Map<number, string>();
    const timers: Harness["timers"] = [];
    const logs: Harness["logs"] = [];
    const harness: Harness = {
        bridge,
        workspace,
        wins,
        sig: { windowActivated: wsig.windowActivated },
        dbusCalls,
        callbacks,
        replies,
        timers,
        logs,
        entryWrites,
        handle: null as unknown as NonNullable<ReturnType<typeof startPlanAdapterEntry>>,
        setHostActive: (w) => {
            hostWrite = true;
            try {
                workspace["activeWindow"] = w;
            } finally {
                hostWrite = false;
            }
        },
        flushAll: async () => {
            for (let r = 0; r < 32; r += 1) {
                const next = dbusCalls.findIndex((c, at) => c.method === "DescribePlan" && !replies.has(at));
                if (next < 0) {
                    return;
                }
                const reply = await bridge.send(dbusCalls[next]?.payload as string);
                replies.set(next, reply);
                callbacks[next]?.(reply);
            }
            throw new Error("flushAll did not converge");
        },
        fireTimers: (d) => {
            let n = 0;
            for (const t of timers) {
                if (t.delayMs === d && !t.cancelled && !t.fired) {
                    t.fired = true;
                    n += 1;
                    t.callback();
                }
            }
            return n;
        },
        settlePlan: async () => {
            for (let r = 0; r < 12; r += 1) {
                harness.fireTimers(PLAN_DEBOUNCE_MS);
                const before = dbusCalls.length;
                await harness.flushAll();
                harness.fireTimers(PLAN_DEBOUNCE_MS);
                await harness.flushAll();
                if (dbusCalls.length === before) {
                    return;
                }
            }
            throw new Error("plan did not settle");
        },
        stop: async () => {
            try {
                harness.handle?.stop();
            } catch {
                /* noop */
            }
            await bridge.close();
        },
    };
    const handle = startPlanAdapterEntry({
        workspace,
        owner: "owner-1",
        generation: "gen-1",
        log: (m: string): void => {
            logs.push(m);
        },
        callDbus: (_s, _p, _i, method, payload, cb): void => {
            if (method === "NameHasOwner") {
                cb(true);
                return;
            }
            if (method === "GetNameOwner") {
                cb(":1.7");
                return;
            }
            if (method === "StartServiceByName") {
                cb(1);
                return;
            }
            dbusCalls.push({ method, payload });
            callbacks.push(cb);
        },
        scheduleOnce: (delayMs, cb): (() => void) => {
            const t = { delayMs, callback: cb, cancelled: false, fired: false };
            timers.push(t);
            return (): void => {
                t.cancelled = true;
            };
        },
        registerShortcutFn: () => true,
        readProfileFn: (): string => "cosmic",
        readWorkspaceModeFn: (): unknown => "per-output-local",
        readInnerGapFn: (): number => 8,
        readOuterGapFn: (): number => 8,
        options: { perOutputVirtualDesktops: true },
    });
    assert.notEqual(handle, null);
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

function reconcileBodies(h: Harness, from: number): Array<Record<string, unknown>> {
    const out: Array<Record<string, unknown>> = [];
    for (let index = from; index < h.dbusCalls.length; index += 1) {
        const call = h.dbusCalls[index] as { method: string; payload: string };
        if (call.method !== "DescribePlan") {
            continue;
        }
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        if ((body["command"] as Record<string, unknown>)?.["op"] !== "reconcile") {
            continue;
        }
        out.push(body);
    }
    return out;
}

function windowIds(body: Record<string, unknown>): string[] {
    return (body["windows"] as Array<Record<string, unknown>>).map((e) => e["window"] as string);
}

function frameOf(w: MWin): { x: number; y: number; width: number; height: number } {
    const f = w.frameGeometry;
    assert.notEqual(f, null);
    return { ...(f as { x: number; y: number; width: number; height: number }) };
}

describe("D23 minimize/restore through production entry + real Planner", () => {
    it("MNZ-01 minimize omits the tile; MNZ-02 restore returns to the old slot; no focus writes", async () => {
        const h = await makeHarness(["mnz-a", "mnz-b", "mnz-c"], 1);
        try {
            await h.settlePlan();
            const seedA = frameOf(h.wins[0] as MWin);
            const seedB = frameOf(h.wins[1] as MWin);
            const seedC = frameOf(h.wins[2] as MWin);
            assert.deepEqual(
                h.wins.map((w) => w.internalId).sort(),
                ["mnz-a", "mnz-b", "mnz-c"],
                "seeded triple tiles",
            );

            // MNZ-01: host minimizes B and refocuses MRU C.
            const markMin = h.dbusCalls.length;
            (h.wins[1] as MWin).minimized = true;
            h.setHostActive(h.wins[2] as MWin);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const minBodies = reconcileBodies(h, markMin);
            assert.ok(minBodies.length >= 1, "minimize leg dispatches at least one reconcile");
            for (const body of minBodies) {
                assert.deepEqual(windowIds(body).sort(), ["mnz-a", "mnz-c"], "every minimize-leg reconcile omits B");
                assert.equal(body["focused_window"], "mnz-c", "minimize-leg focus is MRU C");
                assert.equal((body["domain"] as Record<string, unknown>)["workspace"], "ws-1", "domain retained");
            }
            assert.deepEqual(frameOf(h.wins[1] as MWin), seedB, "minimized B keeps its last frame (never written)");
            assert.ok(
                JSON.stringify(frameOf(h.wins[0] as MWin)) !== JSON.stringify(seedA) ||
                    JSON.stringify(frameOf(h.wins[2] as MWin)) !== JSON.stringify(seedC),
                "survivors reflow into the released allocation",
            );
            assert.equal(h.entryWrites.length, 0, "minimize leg performs no OmniTiler focus write");

            // MNZ-02: host restores B without activation (focus stays C).
            const markRestore = h.dbusCalls.length;
            (h.wins[1] as MWin).minimized = false;
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const restoreBodies = reconcileBodies(h, markRestore);
            assert.ok(restoreBodies.length >= 1, "restore leg dispatches at least one reconcile");
            for (const body of restoreBodies) {
                assert.deepEqual(windowIds(body).sort(), ["mnz-a", "mnz-b", "mnz-c"], "every restore-leg reconcile re-observes B");
            }
            assert.deepEqual(frameOf(h.wins[1] as MWin), seedB, "restored B returns to its stored old slot");
            assert.equal(h.workspace["activeWindow"], h.wins[2], "host restore focus (C) is kept");
            assert.equal(h.entryWrites.length, 0, "restore leg performs no OmniTiler focus write");
        } finally {
            await h.stop();
        }
    });

    it("MNZ-03 sole minimize converges empty with focus none; restore re-admits; no release, no focus writes", async () => {
        const h = await makeHarness(["mnz-sole"], 0);
        try {
            await h.settlePlan();
            const sole = h.wins[0] as MWin;
            assert.notEqual(sole.frameGeometry, null);

            // Sole minimize with null native focus.
            const markMin = h.dbusCalls.length;
            sole.minimized = true;
            h.setHostActive(null);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const minBodies = reconcileBodies(h, markMin);
            assert.ok(minBodies.length >= 1, "sole minimize dispatches at least one reconcile (converges, not fail-closed)");
            for (const body of minBodies) {
                assert.deepEqual(windowIds(body), [], "sole-minimize reconcile carries explicit empty evidence");
                assert.equal(body["focused_window"], "", "sole-minimize focus is none");
                assert.equal((body["domain"] as Record<string, unknown>)["workspace"], "ws-1", "workspace retained");
                assert.equal((body["domain"] as Record<string, unknown>)["output"], "out-1", "output retained");
            }
            assert.equal(h.entryWrites.length, 0, "sole minimize performs no OmniTiler focus write");

            // Restore with host activation on the restored window (the
            // realistic taskbar path; a still-unfocused restore stays
            // fail-closed until the host focuses, by design).
            const markRestore = h.dbusCalls.length;
            sole.minimized = false;
            h.setHostActive(sole);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const restoreBodies = reconcileBodies(h, markRestore);
            assert.ok(restoreBodies.length >= 1, "sole restore dispatches at least one reconcile");
            assert.ok(
                restoreBodies.some((b) => windowIds(b).join(",") === "mnz-sole"),
                "sole restore re-admits the window",
            );
            assert.equal(h.entryWrites.length, 0, "sole restore performs no OmniTiler focus write");

            // Sole minimize with the native active staying on the minimized window.
            const markMin2 = h.dbusCalls.length;
            sole.minimized = true;
            h.setHostActive(sole);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const min2Bodies = reconcileBodies(h, markMin2);
            assert.ok(min2Bodies.length >= 1, "minimized-staying active still converges");
            for (const body of min2Bodies) {
                assert.deepEqual(windowIds(body), [], "minimized-staying active omits the tile");
                assert.equal(body["focused_window"], "", "focus is none while sole-minimized");
            }

            // Restore with host activation on the restored window.
            const markRestore2 = h.dbusCalls.length;
            sole.minimized = false;
            h.setHostActive(sole);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const restore2Bodies = reconcileBodies(h, markRestore2);
            assert.ok(
                restore2Bodies.some((b) => windowIds(b).join(",") === "mnz-sole"),
                "host-activated restore re-admits the window",
            );
            assert.equal(h.entryWrites.length, 0, "no OmniTiler focus write on any sole leg");

            for (const call of h.dbusCalls) {
                const body = JSON.parse(call.payload) as Record<string, unknown>;
                assert.notEqual(
                    (body["command"] as Record<string, unknown>)?.["op"],
                    "release-domain",
                    "minimize never releases the domain",
                );
            }
        } finally {
            await h.stop();
        }
    });

    it("invalidation fallback: unreadable minimized frames are ignored, unreadable tiled frames fail closed", async () => {
        const h = await makeHarness(["mnz-a", "mnz-b", "mnz-c"], 1);
        try {
            await h.settlePlan();

            // Minimized B with a stale/unreadable frame still omits (no fail-closed).
            const markMin = h.dbusCalls.length;
            (h.wins[1] as MWin).minimized = true;
            (h.wins[1] as MWin).frameGeometry = null;
            h.setHostActive(h.wins[2] as MWin);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const minBodies = reconcileBodies(h, markMin);
            assert.ok(minBodies.length >= 1, "omission proceeds despite the unreadable minimized frame");
            for (const body of minBodies) {
                assert.deepEqual(windowIds(body).sort(), ["mnz-a", "mnz-c"], "B omitted, survivors converge");
            }
            (h.wins[1] as MWin).minimized = false;
            (h.wins[1] as MWin).frameGeometry = { x: 640, y: 0, width: 640, height: 1040 };
            h.sig.windowActivated.fire();
            await h.settlePlan();

            // Unreadable frame on a tiled member fails the domain closed: no dispatch.
            const liveC = frameOf(h.wins[2] as MWin);
            const markBad = h.dbusCalls.length;
            (h.wins[2] as MWin).frameGeometry = null;
            h.sig.windowActivated.fire();
            h.fireTimers(PLAN_DEBOUNCE_MS);
            await h.flushAll();
            assert.equal(h.dbusCalls.length, markBad, "unreadable tiled frame dispatches nothing (fail closed)");

            // Readable again: the domain converges (minimize still omits B).
            (h.wins[2] as MWin).frameGeometry = { ...liveC };
            (h.wins[1] as MWin).minimized = true;
            h.setHostActive(h.wins[2] as MWin);
            const markRecover = h.dbusCalls.length;
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const recoverBodies = reconcileBodies(h, markRecover);
            assert.ok(recoverBodies.length >= 1, "readable domain converges again");
            for (const body of recoverBodies) {
                assert.deepEqual(windowIds(body).sort(), ["mnz-a", "mnz-c"], "recovered observation omits B");
            }
            assert.equal(h.entryWrites.length, 0, "fallback legs perform no OmniTiler focus write");
        } finally {
            await h.stop();
        }
    });
});

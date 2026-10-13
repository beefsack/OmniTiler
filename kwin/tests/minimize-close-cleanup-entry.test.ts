// D23 close cleanup + best-effort rejoin through the production
// `startPlanAdapterEntry` route + real Planner (`planner_eval`).
// No hand-built wire replies for the asserted paths.
//
// - Minimized close: removing a minimized tile fires `windowRemoved` with the
//   dead ref; the entry resolves its live-time id + owning domain (never
//   reading the dead object) and the next reconcile carries `closed_windows`,
//   so the planner forgets the slot. A later same-id window is a fresh
//   admission (never the old slot), with no OmniTiler focus write.
// - Sole-minimize close: the null observation still delivers one synthesized
//   explicit empty-domain cleanup (`windows: []` + `closed_windows`), planned
//   with no release-domain op and no focus write; the id re-admits after.
// - Intervening change: admitting D while B is minimized does not invalidate
//   the slot; the restore rejoins between its former neighbors (never fresh
//   beside D), with no OmniTiler focus write.

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

type Handler = (payload?: unknown) => void;
interface Sig {
    readonly handlers: Set<Handler>;
    readonly signal: { connect: (h: Handler) => void; disconnect: (h: Handler) => void };
    fire: (payload?: unknown) => void;
}
function makeSig(): Sig {
    const handlers = new Set<Handler>();
    return {
        handlers,
        signal: {
            connect: (h: Handler): void => {
                handlers.add(h);
            },
            disconnect: (h: Handler): void => {
                handlers.delete(h);
            },
        },
        fire: (payload?: unknown): void => {
            for (const h of [...handlers]) {
                h(payload);
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
    readonly sig: { windowActivated: Sig; windowRemoved: Sig; windowAdded: Sig };
    readonly out: { name: string; geometry: { x: number; y: number; width: number; height: number } };
    readonly desk: { id: string; x11DesktopNumber: number };
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
    removeWin: (w: MWin) => void;
    addWin: (id: string, frame: { x: number; y: number; width: number; height: number }) => MWin;
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
        sig: { windowActivated: wsig.windowActivated, windowRemoved: wsig.windowRemoved, windowAdded: wsig.windowAdded },
        out,
        desk: ws,
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
        removeWin: (w) => {
            const at = wins.indexOf(w);
            if (at >= 0) {
                wins.splice(at, 1);
            }
            // KWin delivers the dead client on windowRemoved: the entry must
            // resolve live-time identity without reading it.
            wsig.windowRemoved.fire(w as unknown);
        },
        addWin: (id, frame) => {
            const w = makeWin(out, ws, id, frame);
            wins.push(w);
            wsig.windowAdded.fire(w as unknown);
            return w;
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

function closedIds(body: Record<string, unknown>): string[] {
    const raw = body["closed_windows"];
    if (!Array.isArray(raw)) {
        return [];
    }
    return (raw as Array<unknown>).map((e) => String(e));
}

function frameOf(w: MWin): { x: number; y: number; width: number; height: number } {
    const f = w.frameGeometry;
    assert.notEqual(f, null);
    return { ...(f as { x: number; y: number; width: number; height: number }) };
}

describe("D23 close cleanup + best-effort rejoin through production entry + real Planner", () => {
    it("minimized close reports closed_windows; same-id window is a fresh admission, never the old slot", async () => {
        const h = await makeHarness(["mnz-a", "mnz-b", "mnz-c"], 1);
        try {
            await h.settlePlan();
            const seedB = frameOf(h.wins[1] as MWin);

            // Minimize B with MRU focus on C.
            (h.wins[1] as MWin).minimized = true;
            h.setHostActive(h.wins[2] as MWin);
            h.sig.windowActivated.fire();
            await h.settlePlan();

            // Close minimized B: windowRemoved carries the dead ref while the
            // host keeps C active.
            const deadB = h.wins[1] as MWin;
            const markClose = h.dbusCalls.length;
            h.removeWin(deadB);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const closeBodies = reconcileBodies(h, markClose);
            assert.ok(closeBodies.length >= 1, "close leg dispatches at least one reconcile");
            assert.ok(
                closeBodies.some((b) => closedIds(b).join(",") === "mnz-b"),
                "close leg reports the minimized close as closed_windows",
            );
            for (const body of closeBodies) {
                assert.deepEqual(windowIds(body).sort(), ["mnz-a", "mnz-c"], "survivors converge on the close turn");
                assert.ok(!windowIds(body).includes("mnz-b"), "closed window is never re-observed");
            }

            // A later same-id window is brand-new: fresh ordinary admission
            // beside the focused survivor, never the stored old slot.
            const markRestore = h.dbusCalls.length;
            const freshB = h.addWin("mnz-b", { x: 640, y: 0, width: 640, height: 1040 });
            void freshB;
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const restoreBodies = reconcileBodies(h, markRestore);
            assert.ok(restoreBodies.length >= 1, "re-admission dispatches at least one reconcile");
            assert.ok(
                restoreBodies.some((b) => windowIds(b).includes("mnz-b")),
                "same-id window re-admits",
            );
            for (const body of restoreBodies) {
                assert.deepEqual(closedIds(body), [], "re-admission carries no close");
            }
            const liveB = h.wins.find((w) => w.internalId === "mnz-b") as MWin;
            assert.notDeepEqual(
                { ...frameOf(liveB) },
                { ...seedB },
                "forgotten slot falls back to fresh admission, never the old slot",
            );
            assert.equal(h.workspace["activeWindow"], h.wins[1], "host focus is kept");
            assert.equal(h.entryWrites.length, 0, "close + re-admission perform no OmniTiler focus write");
        } finally {
            await h.stop();
        }
    });

    it("sole-minimize close delivers an explicit empty cleanup; id re-admits; no release, no focus writes", async () => {
        const h = await makeHarness(["mnz-sole"], 0);
        try {
            await h.settlePlan();
            const sole = h.wins[0] as MWin;

            // Sole minimize with null native focus.
            sole.minimized = true;
            h.setHostActive(null);
            h.sig.windowActivated.fire();
            await h.settlePlan();

            // Close the sole minimized window: no live observation remains,
            // yet one explicit empty cleanup must still deliver the close.
            const markClose = h.dbusCalls.length;
            h.removeWin(sole);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const closeBodies = reconcileBodies(h, markClose);
            assert.ok(closeBodies.length >= 1, "sole close dispatches at least one reconcile");
            assert.ok(
                closeBodies.some((b) => windowIds(b).length === 0 && closedIds(b).join(",") === "mnz-sole"),
                "sole close delivers explicit empty evidence with the close",
            );
            for (const body of closeBodies) {
                assert.equal(body["focused_window"], "", "sole-close focus is none");
            }

            // The id re-admits afterwards as a fresh tile.
            const markRestore = h.dbusCalls.length;
            const again = h.addWin("mnz-sole", { x: 20, y: 20, width: 400, height: 300 });
            h.setHostActive(again);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const restoreBodies = reconcileBodies(h, markRestore);
            assert.ok(
                restoreBodies.some((b) => windowIds(b).join(",") === "mnz-sole"),
                "closed sole id re-admits fresh",
            );

            for (const call of h.dbusCalls) {
                const body = JSON.parse(call.payload) as Record<string, unknown>;
                assert.notEqual(
                    (body["command"] as Record<string, unknown>)?.["op"],
                    "release-domain",
                    "close cleanup never releases the domain",
                );
            }
            assert.equal(h.entryWrites.length, 0, "sole close performs no OmniTiler focus write");
        } finally {
            await h.stop();
        }
    });

    it("admitting D while B is minimized keeps the slot; restore rejoins between former neighbors", async () => {
        const h = await makeHarness(["mnz-a", "mnz-b", "mnz-c"], 1);
        try {
            await h.settlePlan();

            // Minimize B with MRU focus on C.
            (h.wins[1] as MWin).minimized = true;
            h.setHostActive(h.wins[2] as MWin);
            h.sig.windowActivated.fire();
            await h.settlePlan();

            // Intervening admission: the host opens and focuses D.
            const markAdmit = h.dbusCalls.length;
            const winD = h.addWin("mnz-d", { x: 1300, y: 20, width: 400, height: 300 });
            h.setHostActive(winD);
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const admitBodies = reconcileBodies(h, markAdmit);
            assert.ok(admitBodies.length >= 1, "admission dispatches at least one reconcile");
            assert.ok(
                admitBodies.some((b) => windowIds(b).sort().join(",") === "mnz-a,mnz-c,mnz-d"),
                "D admits while B stays minimized",
            );

            // Restore B with host focus on D: best-effort rejoin between the
            // former neighbors, never fresh beside D.
            const markRestore = h.dbusCalls.length;
            (h.wins[1] as MWin).minimized = false;
            h.sig.windowActivated.fire();
            await h.settlePlan();
            const restoreBodies = reconcileBodies(h, markRestore);
            assert.ok(restoreBodies.length >= 1, "restore dispatches at least one reconcile");
            assert.ok(
                restoreBodies.some((b) => windowIds(b).sort().join(",") === "mnz-a,mnz-b,mnz-c,mnz-d"),
                "restore re-observes the full set",
            );
            const byId = new Map(h.wins.map((w) => [w.internalId, frameOf(w)]));
            const ax = (byId.get("mnz-a") as { x: number }).x;
            const bx = (byId.get("mnz-b") as { x: number }).x;
            const cx = (byId.get("mnz-c") as { x: number }).x;
            assert.ok(
                ax < bx && bx < cx,
                `B rejoins between former neighbors A and C (ax=${ax} bx=${bx} cx=${cx})`,
            );
            assert.equal(h.workspace["activeWindow"], winD, "host focus (D) is kept");
            assert.equal(h.entryWrites.length, 0, "change + restore perform no OmniTiler focus write");
        } finally {
            await h.stop();
        }
    });
});

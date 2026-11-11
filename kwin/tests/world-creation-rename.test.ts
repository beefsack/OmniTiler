// Created-desktop friendly rename (KDE): the unique logical token rides the
// void createDesktop call for exact-name attribution; only the proven bound
// native is then renamed to the friendly KDE-style `Desktop N` (native
// one-based position). Adopted/unproven/existing desktops are never renamed,
// and the id-based pending proof survives the rename for feedback.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop { id: string; name: string }
const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });

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

function makeTwoOutput(opts: { throwOnce?: boolean; freezeCreated?: boolean } = {}) {
    const wa1 = desk("wa-1", "wa-1");
    const wb1 = desk("wb-1", "wb-1");
    const world = {
        screens: [
            { name: "out-primary", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
            { name: "out-secondary", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } },
        ],
        desktops: [wa1, wb1] as FakeDesktop[],
        wins: [] as Array<Record<string, unknown>>,
        activeScreen: { name: "out-secondary" },
        currentByOut: new Map<string, FakeDesktop>([["out-primary", wa1], ["out-secondary", wb1]]),
        created: [] as string[],
        throwArmed: opts.throwOnce === true,
    };
    const calls: Array<{ method: string; payload: string; callback: (r: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const signals: Record<string, FakeSignal> = {};
    for (const name of [
        "windowAdded",
        "windowRemoved",
        "windowActivated",
        "desktopsChanged",
        "screensChanged",
        "currentDesktopChanged",
        "activeScreenChanged",
    ] as const) {
        signals[name] = makeSignal();
    }
    const surface: Record<string, unknown> = {
        windowList: () => world.wins,
        screens: world.screens,
        get desktops() { return world.desktops; },
        activeScreen: world.activeScreen,
        activeWindow: null,
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (out: { name: string }) => world.currentByOut.get(out.name),
        setCurrentDesktopForScreen: (d: FakeDesktop, out: { name: string }) => { world.currentByOut.set(out.name, d); },
        sendClientToScreen: () => undefined,
        createDesktop: (...args: ReadonlyArray<unknown>) => {
            if (world.throwArmed) {
                world.throwArmed = false;
                throw new Error("Insufficient arguments");
            }
            const name = args[1] as string;
            const id = `native-${world.desktops.length + 1}`;
            const row = desk(id, name);
            if (opts.freezeCreated === true) {
                Object.freeze(row);
            }
            world.desktops.push(row);
            world.created.push(id);
        },
        removeDesktop: () => undefined,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, cb) => { calls.push({ method, payload, callback: cb }); },
        scheduleOnce: (delay, cb) => { timers.push({ delay, cb }); return () => undefined; },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.ok(handle !== null);
    const runTimers = () => { for (let i = 0; i < timers.length; i += 1) { const t = timers[i] as { delay: number; cb: () => void }; if (t.delay <= 120) { timers.splice(i, 1); i -= 1; t.cb(); } } };
    const lastCall = (m: string) => { const f = [...calls].reverse().find((c) => c.method === m); assert.ok(f !== undefined); return f as { method: string; payload: string; callback: (r: unknown) => void }; };
    const fire = (name: string, payload?: unknown): void => {
        for (const handler of [...(signals[name] as FakeSignal).handlers]) {
            handler(payload);
        }
    };
    return { world, calls, handle, runTimers, lastCall, fire };
}

function answerHandshake(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse(found.payload) as { req: string }).req;
    found.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/settings");
    const body = JSON.parse(found.payload) as Record<string, unknown>;
    found.callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

const ACC = (gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string => {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation, the native watermark, and scripted
    // obs/rev advancing together (separate from nativeInvalid); the
    // terminal result stays null (the take lane serves it
    // requester-only).
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null,
        closeFocus: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
        ...extra,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
};

function discover(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
    const auto = h.lastCall("DescribeWorld");
    const b = JSON.parse(auto.payload) as { source: string; nativeInvalid: string };
    auto.callback(ACC(gen, b.source, b.nativeInvalid, "0"));
}

// Core-owned publication oracle: core-published binds incorporate
// retained receipts (they retire with no re-create); the adapter reports
// observed native order plus the current native per output in `native[]`
// and v4 carries no logical views lane (the core derives receipts from
// its own bindings).
function nativeOf(h: ReturnType<typeof makeTwoOutput>): Array<{ out: string; order: string[]; current: string }> {
    return (JSON.parse(h.lastCall("DescribeWorld").payload) as { report: { native: Array<{ out: string; order: string[]; current: string }> } }).report.native;
}
function assertNoViews(h: ReturnType<typeof makeTwoOutput>): void {
    const report = JSON.parse(h.lastCall("DescribeWorld").payload) as { report: Record<string, unknown> };
    assert.ok(!("views" in report.report), "v4 carries no logical views lane");
}

const DESIRE = (op: string, ws: string, out: string): Record<string, unknown> => ({ op, ws, out });

function dispatchObserve(h: ReturnType<typeof makeTwoOutput>): { source: string; nativeInvalid: string } {
    // Signal-driven observation round (production path): a native inventory
    // event invalidates and the debounce dispatches a fresh capture. Press
    // intents queue instead of dispatching, so they cannot drive the round.
    h.fire("desktopsChanged");
    h.runTimers();
    return JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
}

// Show a native desktop on an output (ordinary native view input):
// incorporation is observed through receipt retirement plus native
// order/current evidence, so observing it requires the created desktop to
// be shown.
function showNative(h: ReturnType<typeof makeTwoOutput>, out: string, nativeId: string): void {
    const ref = h.world.desktops.find((d) => d.id === nativeId);
    assert.ok(ref !== undefined, `native ${nativeId} exists`);
    h.world.currentByOut.set(out, ref);
}

const UNBOUND = (logical: string, out: string, order: number): Record<string, unknown> => ({
    id: logical, out, bind: "", scope: { kind: "output", out }, order, mode: "tiled", exist: true, hold: "",
});

describe("created-desktop friendly rename", () => {
    it("renames the proven desktop to Desktop N and keeps the receipt publishable", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 1);
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "only the proven desktop is renamed to its native one-based position");
        assert.equal(h.world.desktops.filter((d) => d.name === "wl-1").length, 0, "token name leaves the inventory after creation");
        // The causal receipt (not the name) is the proof: it survives the
        // rename and rides until the core incorporates it.
        showNative(h, "out-secondary", (h.world.desktops.find((d) => d.id === h.world.created[0]) as FakeDesktop).id);
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1, "rename never re-creates");
        dispatchObserve(h);
        // The core bind incorporates the receipt; the adapter reports native
        // order/current with no logical views lane, never a logical receipt.
        assertNoViews(h);
        const row = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(row !== undefined);
        assert.ok(row.order.includes(native.id), `expected created native in order, got ${JSON.stringify(nativeOf(h))}`);
        assert.equal(row.current, native.id);
        h.handle?.stop();
    });

    it("never renames a desktop the op did not causally prove", () => {
        const h = makeTwoOutput({ throwOnce: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 0, "throw created nothing");
        // A later desktop is never attributed to the spent op: no rename,
        // no second native call, no fabricated binding.
        h.world.desktops.push(desk("native-9", "wl-1"));
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 0, "spent op must not append");
        const stranger = h.world.desktops.find((d) => d.id === "native-9");
        assert.ok(stranger !== undefined);
        assert.equal(stranger.name, "wl-1", "unproven desktop keeps its name, never renamed");
        showNative(h, "out-secondary", "native-9");
        dispatchObserve(h);
        // No binding: the native evidence rides without incorporation and
        // no logical views lane rides.
        assertNoViews(h);
        const shown = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(shown !== undefined && shown.order.includes("native-9"));
        assert.equal(shown.current, "native-9");
        h.handle?.stop();
    });

    it("never renames adopted, unproven, or existing desktops", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        // Ambiguous attempt: the native call is a no-op for this leg, so the
        // token stays unproven while the user appends an unrelated desktop.
        h.world.desktops.push(desk("user-9", "User 9"));
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        // wl-1 either bound (renamed) or stayed ambiguous; in neither case
        // may the stranger or the pre-existing desktops be renamed.
        assert.equal(h.world.desktops.find((d) => d.id === "user-9")?.name, "User 9", "unrelated desktop never renamed");
        assert.equal(h.world.desktops.find((d) => d.id === "wa-1")?.name, "wa-1", "existing desktop never renamed");
        assert.equal(h.world.desktops.find((d) => d.id === "wb-1")?.name, "wb-1", "existing desktop never renamed");
        h.handle?.stop();
    });

    it("gives two same-reply desires distinct friendly names with no shared native", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                UNBOUND("wl-1", "out-secondary", 2),
                UNBOUND("wl-2", "out-primary", 3),
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary"), DESIRE("op-native-2", "wl-2", "out-primary")],
        }));
        assert.equal(h.world.created.length, 2);
        const n1 = h.world.desktops.find((d) => d.id === h.world.created[0]);
        const n2 = h.world.desktops.find((d) => d.id === h.world.created[1]);
        assert.ok(n1 !== undefined && n2 !== undefined);
        assert.notEqual(n1.id, n2.id, "no duplicate native binding");
        assert.equal(n1.name, "Desktop 3");
        assert.equal(n2.name, "Desktop 4", "second token uses its own enumerated position");
        // No core bind yet: no logical views lane rides, but both ids are
        // proven and distinctly renamed with no shared native.
        showNative(h, "out-secondary", n1.id);
        showNative(h, "out-primary", n2.id);
        dispatchObserve(h);
        assertNoViews(h);
        const rows = nativeOf(h);
        assert.ok(rows.find((b) => b.out === "out-secondary")?.order.includes(n1.id));
        assert.ok(rows.find((b) => b.out === "out-primary")?.order.includes(n2.id));
        assert.equal(h.world.created.length, 2, "renames never re-create");
        h.handle?.stop();
    });

    it("keeps the receipt when the rename write fails", () => {
        const h = makeTwoOutput({ freezeCreated: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
            nativeCreations: [DESIRE("op-native-1", "wl-1", "out-secondary")],
        }));
        assert.equal(h.world.created.length, 1);
        // The rename is best-effort: the id-based receipt (not the name) is
        // the proof, so it rides on while the core bind is pending.
        const r2 = dispatchObserve(h);
        const payload = JSON.parse(h.lastCall("DescribeWorld").payload) as { report: { creationAttempts?: Array<Record<string, unknown>> } };
        assert.equal((payload.report.creationAttempts ?? []).length, 1, "failed rename keeps the receipt");
        const created = (h.world.desktops.find((d) => d.id === h.world.created[0]) as FakeDesktop).id;
        showNative(h, "out-secondary", created);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: { kind: "output", out: "out-primary" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: created, scope: { kind: "output", out: "out-secondary" }, order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        dispatchObserve(h);
        // The core bind incorporates the receipt; the adapter reports native
        // order/current with no logical views lane.
        assertNoViews(h);
        const row = nativeOf(h).find((b) => b.out === "out-secondary");
        assert.ok(row !== undefined);
        assert.ok(row.order.includes(created), `expected created native in order, got ${JSON.stringify(nativeOf(h))}`);
        assert.equal(row.current, created, "failed rename still converges on the core bind");
        assert.equal(h.world.created.length, 1, "failed rename never re-creates");
        h.handle?.stop();
    });
});

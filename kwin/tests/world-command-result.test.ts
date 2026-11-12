// Production entry consumption of correlated Rust results: the accepted
// capture plus the requester-only take lane drive the actual entry request
// path. Take-lane terminal results never paint (requester-only, not
// renderer authority): a drag-preview result with no gesture paints
// nothing and a preview result with a live gesture paints nothing through
// the take lane either. Painting belongs to the read-only presentation
// subscriber (Stage 5): the broker ReadWorldPresentation projection paints
// accepted core shapes (border, underlay, drop preview) through the
// native effect, with generation, request, and transport fencing. A
// migration result moves every member and follows the views without
// repeats.
//
// Synthetic strict-v4 receipts; scripted obs/rev are independent of nativeInvalid.
// A valid drag-preview result replaces the retired active-group take shape.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop {
    id: string;
}
interface FakeWindow {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minimized: boolean;
    hidden: boolean;
}

// Current-v4 synthetic 18-key window row (retired shapes refuse).
function tiledRow(win: string, rect: [number, number, number, number] = [4, 4, 792, 592]): Record<string, unknown> {
    return {
        win, out: "out", ws: "ws",
        alloc: [...rect], frame: [...rect],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: false, float_origin: "",
        cmdSeq: null,
        visibility: null, visibility_auth: "untouched", visibility_hold: "",
        state: [],
    };
}

function makeWindow(id: string, out: { name: string }, ws: FakeDesktop): FakeWindow {
    let frame = { x: 0, y: 0, width: 100, height: 100 };
    const win = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        output: out,
        desktops: [ws],
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        hidden: false,
        keepAbove: false,
        keepBelow: false,
        minSize: null,
        maxSize: null,
    } as unknown as FakeWindow;
    Object.defineProperty(win, "frameGeometry", {
        configurable: true,
        get: () => frame,
        set: (rect: { x: number; y: number; width: number; height: number }) => {
            frame = rect;
            return true;
        },
    });
    return win;
}

interface Harness {
    handle: ReturnType<typeof startWorldAdapterEntry>;
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
    timers: Array<{ delay: number; cb: () => void }>;
    renders: Array<{ method: string; args: ReadonlyArray<unknown> }>;
    sentToOutput: string[];
    currents: Map<string, string>;
    runTimers: () => void;
    replyNext: (mutate?: (body: Record<string, unknown>) => void) => void;
}

function setup(wins: FakeWindow[], currents: Map<string, string>, floatIntent = false): Harness {
    const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const renders: Array<{ method: string; args: ReadonlyArray<unknown> }> = [];
    const sentToOutput: string[] = [];
    const screens = [...new Set(wins.map((w) => w.output.name))].map((name) => ({ name }));
    const allDesktops: FakeDesktop[] = [];
    for (const w of wins) {
        for (const d of w.desktops) {
            if (!allDesktops.some((e) => e.id === d.id)) {
                allDesktops.push(d);
            }
        }
    }
    const surface = {
        windowList: () => wins,
        screens,
        desktops: allDesktops,
        activeScreen: { name: wins[0]?.output.name ?? "out-a" },
        get activeWindow() {
            return wins[0] ?? null;
        },
        cursorPos: { x: 2, y: 100 },
        get currentDesktop() {
            return allDesktops[0];
        },
        set currentDesktop(_ref: unknown) {
            void _ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (out: unknown) => {
            const name = (out as { name: string }).name;
            const id = currents.get(name) ?? "";
            return allDesktops.find((d) => d.id === id) ?? null;
        },
        setCurrentDesktopForScreen: (deskRef: unknown, outRef: unknown) => {
            currents.set((outRef as { name: string }).name, (deskRef as { id: string }).id);
        },
        sendClientToScreen: (client: unknown, output: unknown) => {
            sentToOutput.push(`${(client as FakeWindow).internalId}->${(output as { name: string }).name}`);
            (client as FakeWindow).output = output as { name: string };
        },
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            if (method === "ReadFloatIntent") {
                // Opaque schema-2 seed (mirrors the shared float-echo
                // store shape): empty membership, byte-exact records.
                const seedBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({
                    v: 2, correlation_id: seedBody["correlation_id"], outcome: "ok",
                    records: [], stored: 0, returned: 0,
                }));
                return;
            }
            calls.push({ method, payload, callback });
            if (method === "WriteFloatIntent") {
                const writeBody = JSON.parse(payload) as Record<string, unknown>;
                const records = writeBody["records"] as ReadonlyArray<unknown>;
                callback(JSON.stringify({
                    v: 2, correlation_id: writeBody["correlation_id"], outcome: "stored", stored: records.length,
                }));
            } else if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const takeBody = JSON.parse(payload) as Record<string, unknown>;
                const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
                }            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => {
            const arr = (globalThis as Record<string, unknown>)["__logs"] as string[] | undefined;
            if (arr !== undefined) {
                arr.push(m);
            }
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        ...(floatIntent ? { floatIntent: true as const } : {}),
        effectCallDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
        },
        gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "AckGesture") {
                callback(JSON.stringify({
                    v: 1, instance: "gesture-1", serial: "0", revision: "9",
                    phase: "idle", win: "", kind: "",
                    startRect: null, startCursor: null, cursor: null,
                    modalMove: 0, changed: 1, superseded: 0, available: 1,
                }));
            }
        },
        highlightCallDbus: (_service, _path, _iface, method, ...args) => {
            renders.push({ method, args });
        },
    });
    assert.ok(handle !== null);
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= 120) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    const replyNext = (mutate?: (body: Record<string, unknown>) => void): void => {
        // File a benign command so the drained capture collects its
        // terminal result requester-only via TakeCommandResult, then flush
        // the debounce so the queue files and the fresh capture dispatches.
        handle?.requestWorkspacePrevious();
        runTimers();
        const last = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(last !== undefined);
        const envelope = JSON.parse((last as { payload: string }).payload) as Record<string, unknown>;
        takeRev.n += 1;
        const accept: Record<string, unknown> = {
            v: 4, adapter: "test-adapter", gen: "gen-1",
            req: envelope["source"], obs: String(takeRev.n), rev: String(takeRev.n),
            ready: true, consumed: false,
            source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "1",
            gestureSettled: null, gestureAssoc: null,
            focusRequest: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            intentDesired: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws" }, auth: "controlled", reason: "" }],
            workspaces: [],
            outputs: [{ out: "out", view: "ws", mode: "tiled", hold: "" }],
            windows: [tiledRow("win-a"), tiledRow("win-b")],
            transfers: [], effects: [], result: null,
        };
        if (mutate !== undefined) {
            mutate(accept);
        }
        // The terminal result rides the take lane, never the capture.
        // A mutate callback may stage one per reply via `takeResult`;
        // otherwise a previously staged result stands.
        if ("takeResult" in accept) {
            (globalThis as Record<string, unknown>)["__takeResult"] = accept["takeResult"];
            delete accept["takeResult"];
        }
        const text = JSON.stringify(accept);
        const parsed = parseWorldReply(text);
        assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
        (last as { callback: (reply: unknown) => void }).callback(text);
    };
    return { handle, calls, timers, renders, sentToOutput, currents, runTimers, replyNext };
}

function discover(h: Harness): void {
    // Accept settings before the first report.
    const handshake = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(handshake !== undefined, "expected DiscoverWorld/handshake");
    const handshakeReq = (JSON.parse((handshake as { payload: string }).payload) as Record<string, unknown>)["req"];
    (handshake as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req: handshakeReq,
        ready: false, kind: "", gen: "gen-1",
    }));
    const settings = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(settings !== undefined, "expected DiscoverWorld/settings");
    const settingsBody = JSON.parse((settings as { payload: string }).payload) as Record<string, unknown>;
    (settings as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen: "gen-1",
        req: settingsBody["req"], rev: settingsBody["rev"], status: "accepted", diagnostics: [],
    }));
    // Settle the discovery-triggered auto capture so the command filed by
    // replyNext drains on a fresh flight instead of queueing behind it.
    h.runTimers();
    const auto = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined);
    const autoEnv = JSON.parse((auto as { payload: string }).payload) as Record<string, unknown>;
    const autoText = JSON.stringify({
        v: 4, adapter: "test-adapter", gen: "gen-1", req: autoEnv["source"], obs: "1", rev: "1",
        ready: true, consumed: false,
        source: autoEnv["source"], nativeInvalid: autoEnv["nativeInvalid"], clock: "1",
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        intentDesired: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], focusRequest: null, result: null,
    });
    const autoParsed = parseWorldReply(autoText);
    assert.ok(autoParsed !== null && isCoreWorldReply(autoParsed), "authored accept binds strict v4");
    (auto as { callback: (reply: unknown) => void }).callback(autoText);
}

// Effect-facts seam (Stage 5 witness route): answer the held
// WaitPresentationFacts waiter with a snapshot covering the given
// invalidation counter. The entry carries the witness on its captures;
// the subscriber gates paints on it.
function answerEffectFacts(
    h: Harness,
    opts: { instance?: string; rev?: string; invalidation?: string; changed?: boolean } = {},
): void {
    const wait = [...h.calls].reverse().find((c) => c.method === "WaitPresentationFacts");
    assert.ok(wait !== undefined, "held effect facts waiter exists");
    h.calls.splice(h.calls.indexOf(wait), 1);
    (wait as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        v: 1,
        instance: opts.instance ?? "effect-1",
        rev: opts.rev ?? "0",
        invalidation: opts.invalidation ?? "0",
        chord: 0, chordKnown: 0, gap: 0, width: 3, extension: -1,
        changed: opts.changed === true ? 1 : 0, superseded: 0, available: 1,
    }));
}

// Native gesture seam: complete the held WaitGesture waiter with one
// authoritative snapshot. The entry maps it to the core ingress lane and
// exposes the bound association for the subscriber's preview gate.
function answerGesture(h: Harness, body: Record<string, unknown>): void {
    const wait = [...h.calls].reverse().find((c) => c.method === "WaitGesture");
    assert.ok(wait !== undefined, "held WaitGesture exists");
    h.calls.splice(h.calls.indexOf(wait), 1);
    (wait as { callback: (reply: unknown) => void }).callback(JSON.stringify(body));
}

function gestureBody(over: Record<string, unknown>): Record<string, unknown> {
    return {
        v: 1, instance: "gesture-1", serial: "1", revision: "1",
        phase: "started", win: "win-a", kind: "move",
        startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [20, 20],
        modalMove: 1, changed: 1, superseded: 0, available: 1, ...over,
    };
}

// Take-revision counter: every staged accept must advance the revision or
// the freshness gate holds it as stale.
const takeRev = { n: 10 };

const TAKE_RESULTS: Record<string, Record<string, unknown>> = {
    "float": {
        "op": "toggle-float",
        "win": "win-a",
        "rect": [
            162,
            122,
            475,
            355
        ]
    },
    "migrate": {
        "op": "migrate-workspace",
        "direction": "right",
        "sourceOut": "out-a",
        "sourceWs": "ws-1",
        "targetOut": "out-b",
        "targetWs": "ws-1",
        "members": [
            "win-a"
        ],
        "floats": 0
    },
    // The retired active-group take shape has no v4 contract: the
    // take-lane-never-paints invariant stages a valid drag-preview
    // result instead (requester-only, never renderer authority).
    "preview": {
        "op": "drag-preview",
        "rev": "1",
        "domainOut": "out",
        "domainWs": "ws",
        "sourceOut": "out",
        "sourceWs": "ws",
        "rect": [
            4,
            4,
            392,
            592
        ],
        "hover": {
            "domainOut": "out",
            "domainWs": "ws",
            "leaf": "leaf-win-a",
            "win": "win-a",
            "rev": "1",
            "prior": null,
            "priorOut": "out",
            "priorWs": "ws"
        },
        "win": "win-b"
    }
};

function stageTake(name: string): void {
    (globalThis as Record<string, unknown>)["__takeResult"] = TAKE_RESULTS[name];
}

describe("world command result entry", () => {
    it("renders nothing for a preview result with no gesture in flight", () => {
        const out = { name: "out" };
        const ws = { id: "ws" };
        const h = setup([makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)], new Map([["out", "ws"]]));
        discover(h);
        stageTake("preview");
        h.replyNext();
        assert.ok(!h.renders.some((r) => r.method === "SetDragTargetPreview"), "no gesture means no overlay");
        h.handle?.stop();
    });

    it("paints accepted core shapes through the subscriber, never take results", () => {
        (globalThis as Record<string, unknown>)["__logs"] = [];
        const out = { name: "out" };
        const ws = { id: "ws" };
        const h = setup([makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)], new Map([["out", "ws"]]));
        discover(h);
        // Seed the effect facts (witness 0) and open gesture A: the
        // subscriber paints only what covers the latest witness and only
        // the live gesture's own producing preview.
        answerEffectFacts(h);
        answerGesture(h, gestureBody({}));
        stageTake("preview");
        h.replyNext((accept) => {
            accept["gestureAssoc"] = { instance: "gesture-1", serial: "1", coreId: "1" };
        });
        // Answer every presentation read with an accepted projection
        // carrying a found underlay, a valid visible border with unknown
        // witness (passed through exactly; native authority pending), and
        // gesture A's producing drop preview.
        const answerPresentations = (): void => {
            // Drain in a loop: a reply invalidated during flight is
            // skipped, not painted (finding E), and the chained refetch
            // surfaces as a new pending read. Bounded: refetches stop
            // once the projection is current.
            for (let round = 0; round < 10; round += 1) {
                const pending = h.calls.filter((c) => c.method === "ReadWorldPresentation");
                if (pending.length === 0) {
                    break;
                }
                for (const call of pending) {
                const req = (JSON.parse((call as { payload: string }).payload) as Record<string, unknown>)["req"];
                (call as { callback: (reply: unknown) => void }).callback(JSON.stringify({
                    v: 4, outcome: "presentation", gen: "gen-1", req, rev: "12", obs: "12",
                    coveredInstance: "effect-1", coveredInvalid: "0",
                    scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws" }, auth: "controlled", reason: "" }],
                    workspaces: [{ id: "ws", out: "out", bind: "ws", scope: { kind: "workspace", out: "out", ws: "ws" }, order: 0, mode: "tiled", exist: true, hold: "" }],
                    outputs: [{ out: "out", view: "ws", mode: "tiled", hold: "" }],
                    windows: [{
                        win: "win-a", out: "out", ws: "ws",
                        alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592],
                        frame_auth: "controlled", frame_hold: "",
                        member_auth: "controlled", member_hold: "",
                        floating: false, float_origin: "",
                        cmdSeq: null,
                        visibility: null, visibility_auth: "controlled", visibility_hold: "",
                        state: [],
                    }],
                    presentation: {
                        foreground: "win-a",
                        rev: "12",
                        border: { visible: true, reason: "", subject: null, outsets: [8, 8, 304, 304], width: null, radius: null, witness: null },
                        underlay: {
                            status: "found", reason: "", baseRev: "11",
                            homeOut: "out", homeWs: "ws", group: "grp-win-a-r0", leaf: "leaf-win-a", win: "win-a",
                            union: [4, 4, 792, 592], outer: [0, 0, 800, 600],
                            members: [
                                { leaf: "leaf-win-b", win: "win-b", rect: [4, 4, 392, 592] },
                                { leaf: "leaf-win-a", win: "win-a", rect: [404, 4, 392, 592] },
                            ],
                        },
                        pager: [],
                    },
                    preview: {
                        rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
                        producerInstance: "gesture-1", producerSerial: "1",
                    },
                }));
                }
            }
        };
        answerPresentations();
        const sets = h.renders.filter((r) => r.method === "SetPresentation");
        assert.ok(sets.length >= 1, "accepted projection paints");
        const first = sets[sets.length - 1] as { args: ReadonlyArray<unknown> };
        const payload = JSON.parse(first.args[0] as string) as Record<string, unknown>;
        assert.equal(payload["v"], 2);
        assert.equal(payload["gen"], "gen-1");
        assert.equal(payload["rev"], "12");
        assert.equal(payload["foreground"], "win-a");
        assert.deepEqual(payload["border"], {
            visible: true, reason: "", subject: null,
            outsets: [8, 8, 304, 304], width: null, radius: null, witness: null,
        });
        assert.deepEqual(payload["underlay"], {
            status: "found", win: "win-a", outer: [0, 0, 800, 600], members: ["win-b", "win-a"],
        });
        assert.deepEqual(payload["preview"], {
            rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
            producerInstance: "gesture-1", producerSerial: "1",
        });
        // The causal coverage rides the native payload so the effect
        // fences it too.
        assert.equal(payload["coveredInstance"], "effect-1");
        assert.equal(payload["coveredInvalid"], "0");
        // The staged take-lane preview result never paints beside the
        // subscriber: no legacy forwarding methods survive.
        assert.ok(!h.renders.some((r) => r.method === "SetGroupHighlight"), "no legacy group forwarding");
        assert.ok(!h.renders.some((r) => r.method === "ClearGroupHighlight"), "no legacy group clear");
        assert.ok(!h.renders.some((r) => r.method === "SetDragTargetPreview"), "no legacy drag overlay");
        h.handle?.stop();
    });

    it("migrates every member and follows the views without repeats", () => {
        const outA = { name: "out-a" };
        const outB = { name: "out-b" };
        const wa1 = { id: "wa1" };
        const wb1 = { id: "wb1" };
        const currents = new Map([["out-a", "wa1"], ["out-b", "wb1"]]);
        const h = setup([makeWindow("win-a", outA, wa1), makeWindow("win-b", outB, wb1)], currents);
        discover(h);
        stageTake("migrate");
        h.replyNext((accept) => {
            // Stage the creation-flow binding proof for the migrated unit
            // (ws-1 to native wa1 on out-b): the live broker leaves it unbound
            // until the adapter proves the desktop.
            accept["workspaces"] = [
                { id: "ws-1", out: "out-b", bind: "wa1", scope: { kind: "workspace", out: "out-b", ws: "ws-1" }, order: 0, mode: "tiled", exist: true, hold: "" },
            ];
            accept["outputs"] = [
                { out: "out-a", view: "wa1", mode: "tiled", hold: "" },
                { out: "out-b", view: "wa1", mode: "tiled", hold: "" },
            ];
            accept["windows"] = [
                { ...tiledRow("win-a"), out: "out-b", ws: "ws-1", alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592] },
                { ...tiledRow("win-b"), out: "out-b", ws: "wb1", alloc: [404, 4, 392, 592], frame: [404, 4, 392, 592] },
            ];
            accept["scopes"] = [
                { scope: { kind: "workspace", out: "out-b", ws: "ws-1" }, auth: "controlled", reason: "" },
                { scope: { kind: "workspace", out: "out-b", ws: "wb1" }, auth: "controlled", reason: "" },
            ];
            accept["transfers"] = [
                { op: "op-1", kind: "migration", win: "", out: "out-b", ws: "ws-1", follow: false, held: false, cmdSeq: null },
            ];
        });
        assert.ok(h.sentToOutput.some((c) => c === "win-a->out-b"), "member arrives natively");
        assert.equal(currents.get("out-b"), "wa1", "target view follows");
        const sends = h.sentToOutput.length;
        // A repeat dispatch with converged observation issues no new moves.
        h.runTimers();
        const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        if (last !== undefined) {
            const repBody = JSON.parse((last as { payload: string }).payload) as Record<string, unknown>;
            takeRev.n += 1;
            const second: Record<string, unknown> = {
                v: 4, adapter: "test-adapter", gen: "gen-1",
                req: repBody["source"], obs: String(takeRev.n), rev: String(takeRev.n),
                ready: true, consumed: false,
                source: repBody["source"], nativeInvalid: repBody["nativeInvalid"], clock: "1",
                gestureSettled: null, gestureAssoc: null,
                focusRequest: null,
                operationSettled: [], nativeCreations: [],
                warpCursor: null,
                intentDesired: [], switchSettled: [], diagnostics: [],
                presentation: DISABLED_PRESENTATION,
                scopes: [
                    { scope: { kind: "workspace", out: "out-b", ws: "ws-1" }, auth: "controlled", reason: "" },
                    { scope: { kind: "workspace", out: "out-b", ws: "wb1" }, auth: "controlled", reason: "" },
                ],
                workspaces: [
                    { id: "ws-1", out: "out-b", bind: "wa1", scope: { kind: "workspace", out: "out-b", ws: "ws-1" }, order: 0, mode: "tiled", exist: true, hold: "" },
                ],
                outputs: [
                    { out: "out-a", view: "wa1", mode: "tiled", hold: "" },
                    { out: "out-b", view: "wa1", mode: "tiled", hold: "" },
                ],
                windows: [
                    { ...tiledRow("win-a"), out: "out-b", ws: "ws-1", alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592] },
                    { ...tiledRow("win-b"), out: "out-b", ws: "wb1", alloc: [404, 4, 392, 592], frame: [404, 4, 392, 592] },
                ],
                transfers: [
                    { op: "op-1", kind: "migration", win: "", out: "out-b", ws: "ws-1", follow: false, held: false, cmdSeq: null },
                ], effects: [], result: null,
            };
            const secondText = JSON.stringify(second);
            const secondParsed = parseWorldReply(secondText);
            assert.ok(secondParsed !== null && isCoreWorldReply(secondParsed), "authored accept binds strict v4");
            (globalThis as Record<string, unknown>)["__takeResult"] = {
                op: "migrate-workspace", direction: "right",
                sourceOut: "out-a", sourceWs: "ws-1", targetOut: "out-b", targetWs: "ws-1",
                members: ["win-a"], floats: 0,
            };
            (last as { callback: (reply: unknown) => void }).callback(secondText);
            // Re-stage: the repeat dispatch drains observation-only. Take a
            // pending collection path instead of reusing the result.
            delete (globalThis as Record<string, unknown>)["__takeResult"];
        }
        assert.equal(h.sentToOutput.length, sends, "no repeated ambiguous moves");
        h.handle?.stop();
    });

    it("hides on a malformed presentation read instead of painting stale shapes", () => {
        const out = { name: "out" };
        const ws = { id: "ws" };
        const h = setup([makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)], new Map([["out", "ws"]]));
        discover(h);
        h.replyNext();
        // Malformed reads hide display-only (evidence preserved) instead
        // of painting anything stale.
        for (const call of h.calls.filter((c) => c.method === "ReadWorldPresentation")) {
            (call as { callback: (reply: unknown) => void }).callback("not-json");
        }
        const hides = h.renders.filter((r) => r.method === "HidePresentation");
        assert.ok(hides.length >= 1, "malformed read hides the renderer display-only");
        assert.ok(!h.renders.some((r) => r.method === "SetPresentation" || r.method === "SetGroupHighlight"));
        h.handle?.stop();
    });

    it("persists the tile override on unfloat and keeps it settled", () => {
        const out = { name: "out" };
        const ws = { id: "ws" };
        const wins = [makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)];
        const h = setup(wins, new Map([["out", "ws"]]), true);
        discover(h);
        // Float first: the accepted controlled placement writes the live
        // frame; the accepted explicit core intentWrite converges the
        // opaque float membership (genuine core-minted schema-1 bytes,
        // see world-entry.test.ts FLOAT_DATA/TILE_DATA).
        stageTake("float");
        h.replyNext((accept) => {
            accept["windows"] = [
                { ...tiledRow("win-a"), alloc: [162, 122, 475, 355], frame: [162, 122, 475, 355] },
                tiledRow("win-b"),
            ];
            accept["intentDesired"] = [{ win: "win-a", data: "{\"v\":1,\"float\":true,\"stickyPrior\":null}" }];
        });
        const floatWrites = h.calls.filter((c) => c.method === "WriteFloatIntent");
        assert.ok(floatWrites.length >= 1, "float decision persists");
        const floatBody = JSON.parse(
            (floatWrites[floatWrites.length - 1] as { payload: string }).payload,
        ) as Record<string, unknown>;
        assert.deepEqual(floatBody["records"], [{ win: "win-a", data: "{\"v\":1,\"float\":true,\"stickyPrior\":null}" }]);
        // Unfloat through the production shortcut: the command carries the
        // live frame, and the accepted result (no placement rect) records
        // the explicit tile override. Ordinary keyboard windowed intents
        // file win:{currentFocus:true} resolved at core execution time
        // (docs/decisions/commands.md#kde-rapid-input); terminal results
        // and intent writes keep explicit ids.
        h.handle?.requestFloat();
        h.runTimers();
        const sent = [...h.calls].reverse().find((c) => c.method === "QueueWorldCommand");
        assert.ok(sent !== undefined);
        const cmd = (JSON.parse((sent as { payload: string }).payload) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.deepEqual(cmd, { op: "toggle-float", win: { currentFocus: true }, rect: [162, 122, 475, 355] });
        const capture = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(capture !== undefined);
        const envelope = JSON.parse((capture as { payload: string }).payload) as Record<string, unknown>;
        takeRev.n += 1;
        // A real unfloat reply re-admits the window tiled: the float
        // exception is gone, so membership leaves the float set while
        // the tile override records. The successful terminal omits
        // noSpace (never false).
        const unfloat: Record<string, unknown> = {
            v: 4, adapter: "test-adapter", gen: "gen-1",
            req: envelope["source"], obs: String(takeRev.n), rev: String(takeRev.n),
            ready: true, consumed: false,
            source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "1",
            gestureSettled: null, gestureAssoc: null,
            focusRequest: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            intentDesired: [{ win: "win-a", data: "{\"v\":1,\"float\":false,\"stickyPrior\":null}" }],
            switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws" }, auth: "controlled", reason: "" }],
            workspaces: [], outputs: [{ out: "out", view: "ws", mode: "tiled", hold: "" }],
            // Re-admitted tile respects the outer gap.
            windows: [
                { ...tiledRow("win-a"), alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592] },
                tiledRow("win-b"),
            ],
            transfers: [], effects: [], result: null,
        };
        const unfloatText = JSON.stringify(unfloat);
        const unfloatParsed = parseWorldReply(unfloatText);
        assert.ok(unfloatParsed !== null && isCoreWorldReply(unfloatParsed), "authored accept binds strict v4");
        (globalThis as Record<string, unknown>)["__takeResult"] = { op: "toggle-float", win: "win-a", rect: null };
        (capture as unknown as { callback: (reply: unknown) => void }).callback(unfloatText);

        const intentCalls = h.calls.filter((c) => c.method === "WriteFloatIntent");
        assert.ok(intentCalls.length >= 2, "settled changes persist");
        const lastIntent = JSON.parse(
            (intentCalls[intentCalls.length - 1] as { payload: string }).payload,
        ) as Record<string, unknown>;
        assert.deepEqual(lastIntent["records"], [{ win: "win-a", data: "{\"v\":1,\"float\":false,\"stickyPrior\":null}" }], "explicit tile override persists");
        h.handle?.stop();
    });
});

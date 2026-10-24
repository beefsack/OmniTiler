// World drag result consumption: the accepted core preview renders
// through the native effect (identical-rect repeats never re-set), the
// carried hover prior chains into the next preview and the finish drop,
// resize gestures never preview, windows added after activation hook up,
// stale finishes lose to newer Starts, and a drop without finish cursor
// proof refuses (no final-rect-center fallback).

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createWorldDragRoute } from "../src/world-drag";
import type { WorldObservation } from "../src/world-observe";
import type { WorldCommand, WorldReplyResult } from "../src/world-report";

function makeSignal() {
    const handlers = new Set<(payload?: unknown) => void>();
    return {
        signal: {
            connect: (h: (payload?: unknown) => void): void => {
                handlers.add(h);
            },
            disconnect: (h: (payload?: unknown) => void): void => {
                handlers.delete(h);
            },
        },
        emit: (payload?: unknown): void => {
            for (const h of [...handlers]) {
                h(payload);
            }
        },
    };
}

interface FakeWin {
    ref: Record<string, unknown>;
    started: ReturnType<typeof makeSignal>;
    stepped: ReturnType<typeof makeSignal>;
    finished: ReturnType<typeof makeSignal>;
}

function makeWin(
    id: string,
    rect: { x: number; y: number; width: number; height: number },
    out: string,
    ws: string,
    move = true,
): FakeWin {
    const started = makeSignal();
    const stepped = makeSignal();
    const finished = makeSignal();
    const ref: Record<string, unknown> = {
        internalId: id,
        frameGeometry: { ...rect },
        output: { name: out },
        desktops: [{ id: ws }],
        move,
        resize: !move,
        interactiveMoveResizeStarted: started.signal,
        interactiveMoveResizeStepped: stepped.signal,
        interactiveMoveResizeFinished: finished.signal,
    };
    return { ref, started, stepped, finished };
}

function observeFor(wins: Array<{ win: string; ref: object; out: string; ws: string }>): WorldObservation {
    return {
        mode: "per-output-local",
        topology: [],
        bindings: [],
        domains: [{
            id: "out-1", ws: "ws-1",
            bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 8, focused: "", readable: true, hold: "",
            adjacent: {},
            windows: wins.map((w) => ({
                win: w.win, ref: w.ref, out: w.out, ws: w.ws,
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: false, fitExcluded: false, fullscreen: false,
                maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            })),
            closed: [], complete: true,
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        screens: [], windowFacts: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

interface Harness {
    route: ReturnType<typeof createWorldDragRoute>;
    commands: WorldCommand[];
    logs: string[];
    renders: Array<{ method: string; args: ReadonlyArray<unknown> }>;
    surface: Record<string, unknown>;
    oracleCalls: Array<(reply: unknown) => void>;
    fireAdded: (ref: object) => void;
}

function setup(wins: FakeWin[]): Harness {
    const windowAdded = makeSignal();
    const windowRemoved = makeSignal();
    const surface: Record<string, unknown> = {
        windowList: () => wins.map((w) => w.ref),
        cursorPos: { x: 10, y: 10 },
        windowAdded: windowAdded.signal,
        windowRemoved: windowRemoved.signal,
    };
    const commands: WorldCommand[] = [];
    const logs: string[] = [];
    const renders: Array<{ method: string; args: ReadonlyArray<unknown> }> = [];
    const oracleCalls: Array<(reply: unknown) => void> = [];
    const route = createWorldDragRoute({
        surface: surface as object,
        log: (m) => { logs.push(m); },
        token: (v) => String(v),
        readProp: (value, property) => {
            try {
                return Reflect.get(value as object, property);
            } catch (error) {
                void error;
                return undefined;
            }
        },
        observeForCommand: () => observeFor(wins.map((w) => ({
            win: (w.ref["internalId"] as string), ref: w.ref, out: "out-1", ws: "ws-1",
        }))),
        submitCommand: (command) => {
            commands.push(command);
            return true;
        },
        isStopped: () => false,
        trackDetach: () => undefined,
        setPreview: (rect) => {
            renders.push({ method: "SetDragTargetPreview", args: [rect.x, rect.y, rect.w, rect.h] });
            return true;
        },
        clearPreview: () => {
            renders.push({ method: "ClearDragTargetPreview", args: [] });
        },
        oracleCallDbus: (_s, _p, _i, _m, callback) => {
            oracleCalls.push(callback);
        },
    });
    route.attach();
    return { route, commands, logs, renders, surface, oracleCalls, fireAdded: (ref) => windowAdded.emit(ref) };
}

const PREVIEW_RESULT: WorldReplyResult = {
    op: "drag-preview", rev: "1",
    domainOut: "out-1", domainWs: "ws-1", sourceOut: "out-1", sourceWs: "ws-1",
    rect: [0, 0, 396, 600],
    hover: {
        domainOut: "out-1", domainWs: "ws-1", leaf: "leaf-a", win: "win-a",
        rev: "1", prior: null, priorOut: "out-1", priorWs: "ws-1",
    },
};

describe("world drag result", () => {
    it("renders the derived rect and chains hover into the next preview", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const h = setup([win]);
        win.started.emit();
        win.stepped.emit();
        assert.equal(h.commands.length, 1);
        const first = (h.commands[0] as { command: Record<string, unknown> }).command;
        assert.equal(first["hover"], null);
        h.route.notifyReplyAccepted(PREVIEW_RESULT);
        assert.deepEqual(
            h.renders.filter((r) => r.method === "SetDragTargetPreview").map((r) => r.args),
            [[0, 0, 396, 600]],
        );
        // The chained hover rides the next preview verbatim (core-derived,
        // never invented).
        win.stepped.emit();
        assert.equal(h.commands.length, 2);
        const second = (h.commands[1] as { command: Record<string, unknown> }).command;
        assert.deepEqual(second["hover"], {
            domainOut: "out-1", domainWs: "ws-1", leaf: "leaf-a", win: "win-a",
            rev: "1", prior: null, priorOut: "out-1", priorWs: "ws-1",
        });
        // Identical-rect repeats never re-set the native overlay.
        h.route.notifyReplyAccepted(PREVIEW_RESULT);
        assert.equal(h.renders.filter((r) => r.method === "SetDragTargetPreview").length, 1);
        h.route.stop();
    });

    it("clears a source-mismatched preview without rendering", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const h = setup([win]);
        win.started.emit();
        win.stepped.emit();
        assert.equal(h.commands.length, 1);
        h.route.notifyReplyAccepted({ ...PREVIEW_RESULT, sourceOut: "out-9" });
        assert.ok(!h.renders.some((r) => r.method === "SetDragTargetPreview"));
        assert.ok(h.logs.some((l) => l.includes("drag-preview-mismatch")));
        h.route.stop();
    });

    it("never previews resize gestures but still routes their verdict", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1", false);
        const h = setup([win]);
        win.started.emit();
        win.stepped.emit();
        assert.equal(h.commands.length, 0, "resize never previews");
        (h.surface["cursorPos"] as { x: number; y: number }).x = 120;
        (h.surface["cursorPos"] as { x: number; y: number }).y = 10;
        win.finished.emit();
        assert.ok(h.oracleCalls.length >= 1);
        const pull = h.oracleCalls[h.oracleCalls.length - 1] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: -20, y: -10, w: 120, h: 110 },
            windowIdentity: "win-a", correlation: "drag-11", reason: "ok-moved",
        }));
        const resizes = h.commands.filter(
            (c) => (c as { command: Record<string, unknown> }).command["op"] === "pointer-resize",
        );
        assert.equal(resizes.length, 1);
        h.route.stop();
    });

    it("hooks windows added after activation", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const h = setup([win]);
        const late = makeWin("win-b", { x: 200, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        h.fireAdded(late.ref);
        // The late window list now includes the newcomer for Started capture.
        (h.surface["windowList"] as () => unknown[])().push(late.ref);
        // Started capture reads the live cursor for grabbed edges: move it
        // over the newcomer first.
        (h.surface["cursorPos"] as { x: number; y: number }).x = 210;
        (h.surface["cursorPos"] as { x: number; y: number }).y = 10;
        late.started.emit();
        late.stepped.emit();
        const previews = h.commands.filter(
            (c) => (c as { command: Record<string, unknown> }).command["op"] === "drag-preview",
        );
        assert.equal(previews.length, 1);
        assert.equal((previews[0] as { command: Record<string, unknown> }).command["win"], "win-b");
        h.route.stop();
    });

    it("refuses a drop without finish cursor proof (no center fallback)", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const h = setup([win]);
        win.started.emit();
        // No cursor proof at finish: remove the cursor object entirely.
        (h.surface as Record<string, unknown>)["cursorPos"] = null;
        win.finished.emit();
        assert.ok(h.oracleCalls.length >= 1);
        const pull = h.oracleCalls[h.oracleCalls.length - 1] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: 50, y: 60, w: 100, h: 100 },
            windowIdentity: "win-a", correlation: "drag-12", reason: "ok-moved",
        }));
        assert.equal(h.commands.length, 0, "no proof means no drop");
        assert.ok(h.logs.some((l) => l.includes("drag-refused-coords")));
        h.route.stop();
    });

    it("drops a stale finish when a newer Started owns the ref", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const h = setup([win]);
        win.started.emit();
        (h.surface["cursorPos"] as { x: number; y: number }).x = 60;
        (h.surface["cursorPos"] as { x: number; y: number }).y = 70;
        win.finished.emit();
        // A newer Started lands before the async verdict arrives.
        win.started.emit();
        assert.ok(h.oracleCalls.length >= 1);
        const pull = h.oracleCalls[0] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: 50, y: 60, w: 100, h: 100 },
            windowIdentity: "win-a", correlation: "drag-13", reason: "ok-moved",
        }));
        assert.equal(h.commands.length, 0, "stale finish consumes nothing");
        assert.ok(h.logs.some((l) => l.includes("drag-stale-finish")));
        h.route.stop();
    });

    it("carries the finish-snapshotted hover into the drop", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const h = setup([win]);
        win.started.emit();
        win.stepped.emit();
        h.route.notifyReplyAccepted(PREVIEW_RESULT);
        (h.surface["cursorPos"] as { x: number; y: number }).x = 60;
        (h.surface["cursorPos"] as { x: number; y: number }).y = 70;
        win.finished.emit();
        const pull = h.oracleCalls[h.oracleCalls.length - 1] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: 50, y: 60, w: 100, h: 100 },
            windowIdentity: "win-a", correlation: "drag-14", reason: "ok-moved",
        }));
        const drops = h.commands.filter(
            (c) => (c as { command: Record<string, unknown> }).command["op"] === "drag-drop",
        );
        assert.equal(drops.length, 1);
        const drop = (drops[0] as { command: Record<string, unknown> }).command;
        assert.deepEqual((drop["hover"] as Record<string, unknown>)["win"], "win-a");
        assert.deepEqual((drop["hover"] as Record<string, unknown>)["rev"], "1");
        h.route.stop();
    });

    it("drops cross-output with the Started source and pointer coords", () => {
        // Started on the source output with the cursor already across the
        // boundary: the drop names the capture source while the pointer
        // coords name the destination (core resolves the target domain).
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-a", "ws-1");
        const h = setup([win]);
        (h.surface["cursorPos"] as { x: number; y: number }).x = 850;
        (h.surface["cursorPos"] as { x: number; y: number }).y = 50;
        win.started.emit();
        win.stepped.emit();
        assert.equal(h.commands.length, 1);
        const preview = (h.commands[0] as { command: Record<string, unknown> }).command;
        assert.deepEqual(preview["source"], { out: "out-a", ws: "ws-1" });
        const crossResult: WorldReplyResult = {
            op: "drag-preview", rev: "1",
            domainOut: "out-b", domainWs: "ws-1", sourceOut: "out-a", sourceWs: "ws-1",
            rect: [810, 10, 140, 80],
            hover: {
                domainOut: "out-b", domainWs: "ws-1", leaf: "leaf-a", win: "win-a",
                rev: "1", prior: null, priorOut: "out-b", priorWs: "ws-1",
            },
        };
        h.route.notifyReplyAccepted(crossResult);
        win.finished.emit();
        const pull = h.oracleCalls[h.oracleCalls.length - 1] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: 50, y: 60, w: 100, h: 100 },
            windowIdentity: "win-a", correlation: "drag-16", reason: "ok-moved",
        }));
        const drops = h.commands.filter(
            (c) => (c as { command: Record<string, unknown> }).command["op"] === "drag-drop",
        );
        assert.equal(drops.length, 1);
        const drop = (drops[0] as { command: Record<string, unknown> }).command;
        assert.equal(drop["x"], 850);
        assert.equal(drop["y"], 50);
        assert.deepEqual(drop["source"], { out: "out-a", ws: "ws-1" });
        assert.deepEqual((drop["hover"] as Record<string, unknown>)["win"], "win-a");
        h.route.stop();
    });
});

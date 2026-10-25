// World drag continuous latest-wins preview: Stepped ticks coalesce while
// a preview flight is out, resend from the latest pointer after each
// accepted reply (including across gestures), finish routes an exact drop
// with the Started source and null hover, and stale/cancelled clears the
// fence with one bounded line. No hover is ever invented: the oracle
// contract carries no hover-prior source.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createWorldDragRoute } from "../src/world-drag";
import type { WorldObservation } from "../src/world-observe";
import type { WorldCommand } from "../src/world-report";

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

function makeSurfaceExtra() {
    const windowAdded = makeSignal();
    return { windowAdded };
}

function makeWin(id: string, rect: { x: number; y: number; width: number; height: number }, out: string, ws: string): FakeWin {
    const started = makeSignal();
    const stepped = makeSignal();
    const finished = makeSignal();
    const ref: Record<string, unknown> = {
        internalId: id, normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        frameGeometry: { ...rect },
        output: { name: out },
        desktops: [{ id: ws }],
        minimized: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        minSize: null,
        maxSize: null,
        move: true,
        resize: false,
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
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

describe("world drag continuous preview", () => {
    it("coalesces stepped ticks in flight and resends latest-wins after each accepted reply", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const extra = makeSurfaceExtra();
        const surface: Record<string, unknown> = {
            windowList: () => [win.ref],
            cursorPos: { x: 10, y: 10 },
            windowAdded: extra.windowAdded.signal,
        };
        const commands: WorldCommand[] = [];
        const logs: string[] = [];
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
            observeForCommand: () => observeFor([{ win: "win-a", ref: win.ref, out: "out-1", ws: "ws-1" }]),
            submitCommand: (command) => {
                commands.push(command);
                return true;
            },
            isStopped: () => false,
            trackDetach: () => undefined,
            setPreview: () => true,
            clearPreview: () => undefined,
        });
        route.attach();
        win.started.emit();
        win.stepped.emit();
        assert.equal(commands.length, 1);
        const first = (commands[0] as { command: Record<string, unknown> }).command;
        assert.equal(first["op"], "drag-preview");
        assert.equal(first["hover"], null);
        assert.deepEqual(first["source"], { out: "out-1", ws: "ws-1" });
        // Coalesced while the flight is out: no second dispatch.
        (surface["cursorPos"] as { x: number; y: number }).x = 40;
        win.stepped.emit();
        assert.equal(commands.length, 1);
        // Accepted reply flushes the coalesced pending tick immediately
        // from the latest pointer: no later Stepped event is required.
        route.notifyReplyAccepted(null);
        assert.equal(commands.length, 2);
        const second = (commands[1] as { command: Record<string, unknown> }).command;
        assert.equal(second["x"], 40);
        assert.equal(second["hover"], null);
        // The fence is busy again with the flushed flight: the next tick
        // coalesces, and its accept flushes afresh.
        (surface["cursorPos"] as { x: number; y: number }).x = 50;
        win.stepped.emit();
        assert.equal(commands.length, 2);
        route.notifyReplyAccepted(null);
        assert.equal(commands.length, 3);
        const third = (commands[2] as { command: Record<string, unknown> }).command;
        assert.equal(third["x"], 50);
        route.stop();
    });

    it("fences gestures independently and clears with a bounded line on stale", () => {
        const winA = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const winB = makeWin("win-b", { x: 200, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const extra = makeSurfaceExtra();
        const surface: Record<string, unknown> = {
            windowList: () => [winA.ref, winB.ref],
            cursorPos: { x: 5, y: 5 },
            windowAdded: extra.windowAdded.signal,
        };
        const commands: WorldCommand[] = [];
        const logs: string[] = [];
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
            observeForCommand: () => observeFor([
                { win: "win-a", ref: winA.ref, out: "out-1", ws: "ws-1" },
                { win: "win-b", ref: winB.ref, out: "out-1", ws: "ws-1" },
            ]),
            submitCommand: (command) => {
                commands.push(command);
                return true;
            },
            isStopped: () => false,
            trackDetach: () => undefined,
            setPreview: () => true,
            clearPreview: () => undefined,
        });
        route.attach();
        winA.started.emit();
        winB.started.emit();
        winA.stepped.emit();
        winB.stepped.emit();
        assert.equal(commands.length, 2, "independent per-gesture fences");
        winA.stepped.emit();
        assert.equal(commands.length, 2, "first gesture stays fenced");
        route.notifyStaleReply("stale-obs");
        assert.ok(logs.some((l) => l.includes("drag-preview-cleared") && l.includes("stale-obs")));
        winA.stepped.emit();
        assert.equal(commands.length, 3, "stale clears the fence for retry");
        route.stop();
    });

    it("releases the fence when the preview flight is busy", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const extra = makeSurfaceExtra();
        const surface: Record<string, unknown> = {
            windowList: () => [win.ref],
            cursorPos: { x: 10, y: 10 },
            windowAdded: extra.windowAdded.signal,
        };
        const logs: string[] = [];
        let busy = true;
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
            observeForCommand: () => observeFor([{ win: "win-a", ref: win.ref, out: "out-1", ws: "ws-1" }]),
            submitCommand: () => !busy,
            isStopped: () => false,
            trackDetach: () => undefined,
            setPreview: () => true,
            clearPreview: () => undefined,
        });
        route.attach();
        win.started.emit();
        win.stepped.emit();
        assert.ok(logs.some((l) => l.includes("drag-preview-busy")));
        busy = false;
        win.stepped.emit();
        assert.equal(logs.filter((l) => l.includes("drag-preview-busy")).length, 1, "fence released, retry succeeds");
        route.stop();
    });

    it("finishes with an exact drop carrying the started source and null hover", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const extra = makeSurfaceExtra();
        const surface: Record<string, unknown> = {
            windowList: () => [win.ref],
            cursorPos: { x: 2, y: 2 },
            windowAdded: extra.windowAdded.signal,
        };
        const commands: WorldCommand[] = [];
        const logs: string[] = [];
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
            observeForCommand: () => observeFor([{ win: "win-a", ref: win.ref, out: "out-1", ws: "ws-1" }]),
            submitCommand: (command) => {
                commands.push(command);
                return true;
            },
            isStopped: () => false,
            trackDetach: () => undefined,
            setPreview: () => true,
            clearPreview: () => undefined,
            oracleCallDbus: (_s, _p, _i, _m, callback) => {
                oracleCalls.push(callback);
            },
        });
        route.attach();
        win.started.emit();
        (surface["cursorPos"] as { x: number; y: number }).x = 60;
        (surface["cursorPos"] as { x: number; y: number }).y = 70;
        win.finished.emit();
        assert.ok(oracleCalls.length >= 1, "finish pulls LastVerdict");
        const pull = oracleCalls[oracleCalls.length - 1] as (reply: unknown) => void;
        pull(JSON.stringify({
            v: 1, cancelled: false,
            finalRect: { x: 50, y: 60, w: 100, h: 100 },
            windowIdentity: "win-a", correlation: "drag-1", reason: "ok-moved",
        }));
        const drops = commands.filter((c) => (c as { command: Record<string, unknown> }).command["op"] === "drag-drop");
        assert.equal(drops.length, 1);
        const drop = (drops[0] as { command: Record<string, unknown> }).command;
        assert.equal(drop["win"], "win-a");
        assert.equal(drop["x"], 60);
        assert.equal(drop["y"], 70);
        assert.equal(drop["hover"], null);
        assert.deepEqual(drop["source"], { out: "out-1", ws: "ws-1" });
        route.stop();
    });

    it("treats a cancelled verdict as a strict no-op with an independent clear", () => {
        const win = makeWin("win-a", { x: 0, y: 0, width: 100, height: 100 }, "out-1", "ws-1");
        const extra = makeSurfaceExtra();
        const surface: Record<string, unknown> = {
            windowList: () => [win.ref],
            cursorPos: { x: 2, y: 2 },
            windowAdded: extra.windowAdded.signal,
        };
        const commands: WorldCommand[] = [];
        const logs: string[] = [];
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
            observeForCommand: () => observeFor([{ win: "win-a", ref: win.ref, out: "out-1", ws: "ws-1" }]),
            submitCommand: (command) => {
                commands.push(command);
                return true;
            },
            isStopped: () => false,
            trackDetach: () => undefined,
            setPreview: () => true,
            clearPreview: () => undefined,
            oracleCallDbus: (_s, _p, _i, _m, callback) => {
                oracleCalls.push(callback);
            },
        });
        route.attach();
        win.started.emit();
        win.finished.emit();
        assert.ok(oracleCalls.length >= 1);
        const pull = oracleCalls[oracleCalls.length - 1] as (reply: unknown) => void;
        // The cancelled flag dominates: any vocabulary reason parses,
        // the verdict routes nothing, and the preview clears.
        pull(JSON.stringify({
            v: 1, cancelled: true,
            finalRect: { x: 0, y: 0, w: 100, h: 100 },
            windowIdentity: "win-a", correlation: "drag-2", reason: "no-change",
        }));
        assert.equal(commands.length, 0, "cancelled verdict dispatches nothing");
        assert.ok(logs.some((l) => l.includes("drag-preview-cleared") && l.includes("cancelled")));
        route.stop();
    });
});

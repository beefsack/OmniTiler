// Regression: scripting lists are array-likes, not JS arrays.
// List reads (`windowList`, `screens`, `desktops`, per-window `desktops`)
// decode to real items on the scripting host while raw Array.isArray reads
// fail closed, so every raw-Array.isArray liveness read in the world apply
// host failed closed and every accepted world reply applied as all-zero
// no-ops with no dirty flag, no feedback and no redispatch. The mock
// surface mirrors the scripting host: every list read returns a fresh
// array-like ({length, indexed}, never an Array) of fresh wrapper objects,
// so ref identity never survives a call and only stable-id comparison plus
// list decoding can converge.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parseWorldReply, isWorldRejection, isCoreWorldReply } from "../src/world-report";
import { applyWorldReply } from "../src/world-apply";
import type { WorldObservation } from "../src/world-observe";
import { WorldAdapterState } from "../src/world-state";
import { createWorldApplyHost } from "../src/world-native";

// Fully synthetic generic ids. Live wrappers carry the canonical braced
// form; the report/reply carry the normalized bare ids.
const WS_A = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const WS_B = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const WIN_A = "11111111-1111-1111-1111-111111111111";
const WIN_B = "22222222-2222-2222-2222-222222222222";
const OUT = "out-1";
const GEN = "gen-1";
const ADAPTER = "adapter-1";
const REQ = "req-7";
const OBS = "7";
const REV = "7";
const TRANSFER_OP = "op-3";

// Synthetic reply: two controlled frames plus one retained unheld view
// transfer to the non-shown workspace.
const REPLY_TEXT = JSON.stringify({
    v: 3, adapter: "broker-1", gen: GEN, req: REQ, obs: OBS, rev: REV,
    ready: true, consumed: false,
    source: REQ,
    nativeInvalid: "1",
    clock: "0",
    gestureSettled: null, gestureAssoc: null,
    operationSettled: [], nativeCreations: [],
    warpCursor: null,
    presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
    scopes: [
        { scope: `${OUT}/${WS_A}`, auth: "controlled", reason: "" },
        { scope: `${OUT}/${WS_B}`, auth: "controlled", reason: "" },
    ],
    workspaces: [
        { id: WS_A, out: OUT, bind: WS_A, scope: OUT, order: 0, mode: "tiled", exist: true, hold: "" },
        { id: WS_B, out: OUT, bind: WS_B, scope: OUT, order: 1, mode: "tiled", exist: true, hold: "" },
    ],
    outputs: [{ out: OUT, view: WS_B, mode: "tiled", hold: "" }],
    windows: [
        {
            win: WIN_A, out: OUT, ws: WS_A, alloc: [0, 0, 400, 600], frame: [0, 0, 400, 600],
            frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "",
            floating: false, float_origin: "",
        },
        {
            win: WIN_B, out: OUT, ws: WS_B, alloc: [400, 0, 400, 600], frame: [400, 0, 400, 600],
            frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "",
            floating: false, float_origin: "",
        },
    ],
    transfers: [{ op: TRANSFER_OP, kind: "view", win: "", out: OUT, ws: WS_B, follow: false, held: false }],
    effects: [],
    result: null,
});

// Array-like: indexed properties plus length on a plain object.
// Array.isArray is false by construction, exactly like the scripting host.
const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

interface SyntheticWin {
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
    desktops: object;
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
}

describe("world qml-list regression", () => {
    it("synthetic reply parses with its view transfer intact", () => {
        const parsed = parseWorldReply(REPLY_TEXT);
        assert.ok(parsed !== null, "synthetic reply must parse");
        assert.ok(!isWorldRejection(parsed) && isCoreWorldReply(parsed), "synthetic reply must be a core reply");
        if (parsed === null || isWorldRejection(parsed) || !isCoreWorldReply(parsed)) {
            return;
        }
        assert.equal(parsed.req, REQ);
        assert.equal(parsed.rev, REV);
        assert.equal(parsed.obs, OBS);
        assert.equal(parsed.consumed, false);
        assert.equal(parsed.transfers.length, 1);
        const transfer = parsed.transfers[0];
        assert.ok(transfer !== undefined);
        if (transfer === undefined) {
            return;
        }
        assert.equal(transfer.kind, "view");
        assert.equal(transfer.out, OUT);
        assert.equal(transfer.ws, WS_B);
        assert.equal(transfer.held, false);
    });

    it("actuates geometry and the retained view against array-like host lists", () => {
        const parsed = parseWorldReply(REPLY_TEXT);
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (!isCoreWorldReply(parsed)) {
            return;
        }
        // Live inventory behind the synthetic report: the same two windows
        // on their reported workspaces, live at rects that differ from the
        // reply's desired frames, so the frame path runs. Live internalIds
        // use the canonical braced form; the report/reply carry the
        // normalized bare ids.
        const liveWins: SyntheticWin[] = [
            {
                internalId: `{${WIN_A}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 8, y: 8, width: 380, height: 584 },
                output: { name: OUT },
                desktops: qmlLike([{ id: WS_A }]),
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
            },
            {
                internalId: `{${WIN_B}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: 412, y: 8, width: 380, height: 584 },
                output: { name: OUT },
                desktops: qmlLike([{ id: WS_B }]),
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
            },
        ];
        let liveCurrent = WS_A;
        const calls: string[] = [];
        const readProp = (value: object, property: string): unknown => {
            try {
                return Reflect.get(value as Record<string, unknown>, property);
            } catch (error) {
                void error;
                return undefined;
            }
        };
        // Host shape fidelity: windowList is a method returning a fresh
        // array-like of fresh wrappers per call; screens/desktops are
        // array-like properties re-read on every access (getters renew
        // them, so ref identity never survives a read).
        const screensList = [{ name: OUT }];
        const desktopIds = [WS_A, WS_B];
        const surface = {
            windowList: () => qmlLike(liveWins.map((w) => ({ ...w }))),
            get screens() {
                return qmlLike(screensList.map((s) => ({ ...s })));
            },
            get desktops() {
                return qmlLike(desktopIds.map((id) => ({ id })));
            },
            currentDesktopForScreen: (_screen: object) => ({ id: liveCurrent }),
            activeWindow: null,
        };
        const winEntry = (
            win: string, ws: string, rect: { x: number; y: number; w: number; h: number },
        ) => {
            const ref = {
                internalId: win, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                frameGeometry: { x: rect.x, y: rect.y, width: rect.w, height: rect.h },
                output: { name: OUT },
                desktops: qmlLike([{ id: ws }]),
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
            };
            return {
                win, ref, out: OUT, ws,
                rect: { ...rect },
                floating: false, fitExcluded: false, fullscreen: false,
                maximized: false, sticky: false, fixedAuto: false,
                fixedSuppress: false,
                nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            };
        };
        // Observation mirrors the synthetic report. Refs are live-wrapper
        // shaped (internalId plus readable properties), as in production;
        // the host under test re-resolves everything else from the
        // array-like surface on every read.
        const observation: WorldObservation = {
            mode: "per-output-local" as const,
            topology: [{
                out: OUT, order: [WS_A, WS_B], current: WS_A, complete: true,
                rect: { x: 0, y: 0, w: 800, h: 600 },
                primary: true,
            }],
            bindings: [
                { logical: WS_A, native: WS_A, out: OUT, pin: false, disp: false, managed: true },
                { logical: WS_B, native: WS_B, out: OUT, pin: false, disp: false, managed: true },
            ],
            domains: [
                {
                    id: OUT, ws: WS_A,
                    bounds: { x: 0, y: 0, w: 800, h: 600 },
                    gap: 8, outer: 8, focused: WIN_A,
                    readable: true, hold: "", adjacent: {},
                    windows: [winEntry(WIN_A, WS_A, { x: 8, y: 8, w: 380, h: 584 })],
                    closed: [], complete: true,
                },
                {
                    id: OUT, ws: WS_B,
                    bounds: { x: 0, y: 0, w: 800, h: 600 },
                    gap: 8, outer: 8, focused: "",
                    readable: true, hold: "", adjacent: {},
                    windows: [winEntry(WIN_B, WS_B, { x: 412, y: 8, w: 380, h: 584 })],
                    closed: [], complete: true,
                },
            ],
            minimized: [],
            predicate: "both-axes-fixed" as const,
            complete: true,
            gesture: false,
            focusedGlobal: WIN_A,
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
        };
        const state = new WorldAdapterState(ADAPTER, ADAPTER);
        state.onDiscovery(GEN, "broker-1");
        (state as unknown as { currentInvalid: string }).currentInvalid = OBS;
        // Production wiring: every read comes from the host under test;
        // only the two native effect setters are captured.
        const host = createWorldApplyHost(surface as unknown as object, readProp);
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            state,
            observation,
            reply: parsed,
            log: (message: string) => {
                calls.push(`log:${message}`);
            },
            ...host,
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                calls.push(`setGeometry:${rect.x},${rect.y},${rect.w},${rect.h}`);
                // Native write mutates the live wrapper in place; the
                // observation ref models that wrapper, so converge it for
                // the read-back proof, exactly like the host.
                (ref as { frameGeometry: { x: number; y: number; width: number; height: number } }).frameGeometry = { x: rect.x, y: rect.y, width: rect.w, height: rect.h };
                return true;
            },
            switchView: (desktopRef: object, _outRef: object) => {
                const id = (desktopRef as { id: string }).id;
                calls.push(`switchView:${id}`);
                liveCurrent = id;
                return true;
            },
        });
        assert.equal(result.stoppedMidApply, false);
        assert.equal(result.held, 0);
        assert.ok(result.geometryWrites >= 1, `expected frame writes, got ${JSON.stringify(result)}`);
        assert.deepEqual(
            calls.filter((c) => c.startsWith("switchView:")),
            [`switchView:${WS_B}`],
            "retained view transfer must actuate exactly once",
        );
        assert.equal(result.viewWrites, 1);
        assert.equal(liveCurrent, WS_B);
        // The view dispatch records once so the next report carries the raw
        // attempt instead of repeating the native call; the core owns
        // settlement from that attempt plus fresh facts.
        assert.equal(state.isDispatched(TRANSFER_OP), true, "view dispatch recorded once");
        assert.equal(
            state.viewAttemptsForReport().filter((v) => v.op === TRANSFER_OP).length,
            1,
            "exactly the view setter disposition rides the next report",
        );
    });
});

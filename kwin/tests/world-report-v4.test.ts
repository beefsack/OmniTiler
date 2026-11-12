// v4 native-fact report producer (K1b1): exact canonical bytes, nullable
// honesty, broker-namespace omission, and fail-closed encoding. Offline
// only: scripted in-memory workspace fakes with native KWin property
// spellings, no D-Bus, no KWin.
//
// NOTE on string escaping: the producer never relies on the KWin
// JSON.stringify for string bytes. The KWin short escapes (\b \t \n \f
// \r) are LIVE-unproved and would differ from the canonical writer, so
// encodeV4String spells every C0 control as lowercase \u00xx and the
// tests below lock that divergence.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { captureWorldObservation } from "../src/world-observe";
import { buildWorldReportV4InnerJson, deriveLocalFrameWitness, encodeV4String } from "../src/world-report";
import { mapNativePid } from "../src/world-connection";

function makeDesk(id: string) {
    return { id };
}

function makeWin(over: Record<string, unknown>) {
    return {
        internalId: "win-a",
        normalWindow: true,
        dialog: false,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        modal: false,
        wantsInput: true,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        hidden: false,
        frameGeometry: { x: 8.4, y: 8.6, width: 800, height: 600 },
        clientGeometry: { x: 10, y: 30, width: 796, height: 578 },
        minSize: { width: 100, height: 100 },
        // Native unbounded maximum: KWin reports no cap as INT_MAX on
        // every backend (X11 fallback in src/utils/xcbutils.h, Wayland
        // 0-to-INT_MAX map in src/xdgshellwindow.cpp, base
        // Window::maxSize), never (0,0). Rides verbatim; the core
        // sanitizes out-of-bound values to absent at use.
        maxSize: { width: 2147483647, height: 2147483647 },
        output: { name: "out-a" },
        desktops: [makeDesk("native-a")],
        pid: 1234,
        resourceClass: "Example",
        caption: "Window",
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
        ...over,
    };
}

function makeSurface(wins: Array<Record<string, unknown>>) {
    const desktops = [makeDesk("native-a")];
    const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    return {
        windowList: () => wins,
        screens,
        desktops,
        activeScreen: { name: "out-a" },
        activeWindow: wins[0] ?? null,
        cursorPos: { x: 20, y: 30 },
        windowAt: () => (wins.length > 0 ? [wins[0]] : []),
        stackingOrder: [...wins].reverse(),
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => desktops[0],
    };
}

function makeSurfaceWithScreens(
    wins: Array<Record<string, unknown>>,
    screens: Array<Record<string, unknown>>,
) {
    const desktops = [makeDesk("native-a")];
    return {
        windowList: () => wins,
        screens,
        desktops,
        activeScreen: screens[0] ?? { name: "out-a" },
        activeWindow: wins[0] ?? null,
        cursorPos: { x: 20, y: 30 },
        windowAt: () => (wins.length > 0 ? [wins[0]] : []),
        stackingOrder: [...wins].reverse(),
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => desktops[0],
    };
}

function observe(surface: object) {
    const obs = captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-a", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
    assert.ok(obs !== null);
    return obs;
}

describe("v4 screen physical identity", () => {
    function screenIdent(screens: Array<Record<string, unknown>>) {
        const { json, value } = buildInner(makeSurfaceWithScreens([makeWin({})], screens));
        assert.ok(json !== null);
        const rows = value["screens"] as Array<Record<string, unknown>>;
        return { json, rows };
    }

    function deskScreen(over: Record<string, unknown>) {
        return {
            name: "out-a",
            geometry: { x: 0, y: 0, width: 1920, height: 1080 },
            ...over,
        };
    }

    it("rides truthful manufacturer/model/serial with a null connector", () => {
        const { json, rows } = screenIdent([deskScreen({
            manufacturer: "ACME", model: "Ultra 27", serialNumber: "SN-001",
        })]);
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: "ACME", model: "Ultra 27", serial: "SN-001", connector: null,
        });
        assert.ok(json.includes('"identity":{"manufacturer":"ACME","model":"Ultra 27","serial":"SN-001","connector":null}'));
    });

    it("normalizes empty native strings to null, never empty bytes", () => {
        const { json, rows } = screenIdent([deskScreen({
            manufacturer: "", model: "", serialNumber: "",
        })]);
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: null, model: null, serial: null, connector: null,
        });
        assert.ok(!json.includes('"manufacturer":""'));
    });

    it("preserves a literal nonempty unknown as neutral metadata", () => {
        // No arm's native sentinel is the word "unknown" (EDID unknown is
        // "" on all three arms), so a literal value rides verbatim.
        for (const literal of ["unknown", "Unknown", "UNKNOWN"]) {
            const { json, rows } = screenIdent([deskScreen({
                manufacturer: literal, model: literal, serialNumber: literal,
            })]);
            assert.deepEqual(rows[0]?.["identity"], {
                manufacturer: literal, model: literal, serial: literal, connector: null,
            }, `literal ${literal} must ride verbatim`);
            assert.ok(json.includes(`"manufacturer":"${literal}"`));
        }
    });

    it("reads null on missing properties", () => {
        const { rows } = screenIdent([deskScreen({})]);
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: null, model: null, serial: null, connector: null,
        });
    });

    it("reads null on non-string properties without holding the report", () => {
        const { json, rows } = screenIdent([deskScreen({
            manufacturer: 42, model: true, serialNumber: ["SN"],
        })]);
        assert.ok(json !== null, "malformed identity must not hold the report");
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: null, model: null, serial: null, connector: null,
        });
    });

    it("reads null when a native getter throws, keeping the report alive", () => {
        const evil = deskScreen({});
        Object.defineProperty(evil, "manufacturer", {
            enumerable: true, configurable: true,
            get() { throw new Error("native boom"); },
        });
        Object.defineProperty(evil, "serialNumber", {
            enumerable: true, configurable: true,
            get() { throw new Error("native boom"); },
        });
        (evil as Record<string, unknown>)["model"] = "Panel";
        const { json, rows } = screenIdent([evil]);
        assert.ok(json !== null, "throwing getter must not hold the report");
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: null, model: "Panel", serial: null, connector: null,
        });
    });

    it("rides a 256-byte value on a complete screen", () => {
        const exact256 = "m".repeat(256);
        const { json, rows } = screenIdent([deskScreen({
            manufacturer: exact256, model: "ok", serialNumber: "ok",
        })]);
        assert.ok(json !== null, "boundary value must not hold the report");
        assert.equal(rows[0]?.["complete"], true);
        assert.deepEqual(rows[0]?.["full"], [0, 0, 1920, 1080]);
        assert.equal((rows[0]?.["identity"] as Record<string, unknown>)["manufacturer"], exact256);
    });

    it("holds the screen incomplete on overbound text, never a silent null", () => {
        // Any overbound arm holds its screen; the report stays alive.
        for (const prop of ["manufacturer", "model", "serialNumber"] as const) {
            const { json, rows } = screenIdent([deskScreen({
                manufacturer: "ACME", model: "Panel", serialNumber: "SN-1", [prop]: "m".repeat(257),
            })]);
            assert.ok(json !== null, `overbound ${prop} must not drop the report`);
            assert.equal(rows[0]?.["complete"], false, `overbound ${prop} holds the screen`);
            assert.equal(rows[0]?.["full"], null);
            assert.equal(rows[0]?.["work"], null);
            assert.deepEqual(rows[0]?.["identity"], {
                manufacturer: null, model: null, serial: null, connector: null,
            }, `overbound ${prop} nulls every arm`);
        }
    });

    it("bounds multi-byte text in UTF-8 bytes, holding only the overbound screen", () => {
        // 128 e-acutes are exactly 256 bytes: complete. 129 are 258: held.
        const good = screenIdent([deskScreen({
            manufacturer: "\u00e9".repeat(128), model: "ok", serialNumber: "ok",
        })]);
        assert.equal(good.rows[0]?.["complete"], true);
        const screens = [
            deskScreen({ manufacturer: "\u00e9".repeat(129), model: "ok", serialNumber: "ok" }),
            {
                name: "out-b", geometry: { x: 1920, y: 0, width: 1920, height: 1080 },
                manufacturer: "ACME", model: "Panel", serialNumber: "SN-2",
            },
        ];
        const { json, rows } = screenIdent(screens);
        assert.ok(json !== null, "a held screen must not drop the report");
        assert.equal(rows.length, 2);
        assert.equal(rows[0]?.["complete"], false);
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: null, model: null, serial: null, connector: null,
        });
        assert.equal(rows[1]?.["complete"], true);
        assert.deepEqual(rows[1]?.["full"], [1920, 0, 1920, 1080]);
        assert.deepEqual(rows[1]?.["identity"], {
            manufacturer: "ACME", model: "Panel", serial: "SN-2", connector: null,
        });
    });

    it("never substitutes the connector, uuid, name, or ordinal", () => {
        const { rows } = screenIdent([deskScreen({
            manufacturer: "ACME", model: "HDMI-A-1", serialNumber: "1",
            connector: "HDMI-A-1",
            uuid: "some-uuid",
            connectorName: "HDMI-A-1",
        })]);
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: "ACME", model: "HDMI-A-1", serial: "1", connector: null,
        });
    });

    it("keeps duplicate metadata on distinct outputs independent", () => {
        const screens = [
            deskScreen({ manufacturer: "ACME", model: "Panel", serialNumber: "SAME" }),
            { name: "out-b", geometry: { x: 1920, y: 0, width: 1920, height: 1080 }, manufacturer: "ACME", model: "Panel", serialNumber: "SAME" },
        ];
        const { json, rows } = screenIdent(screens);
        assert.ok(json !== null);
        assert.equal(rows.length, 2);
        for (const row of rows) {
            assert.deepEqual(row["identity"], {
                manufacturer: "ACME", model: "Panel", serial: "SAME", connector: null,
            });
        }
    });

    it("keeps metadata under a renamed output handle", () => {
        const { rows } = screenIdent([deskScreen({
            name: "out-renamed", manufacturer: "ACME", model: "Panel", serialNumber: "SN-9",
        })]);
        assert.equal(rows[0]?.["out"], "out-renamed");
        assert.deepEqual(rows[0]?.["identity"], {
            manufacturer: "ACME", model: "Panel", serial: "SN-9", connector: null,
        });
    });

    it("fails closed when hand-built identity text cannot spell canonically", () => {
        const obs = observe(makeSurfaceWithScreens([makeWin({})], [deskScreen({
            manufacturer: "ACME", model: "Panel", serialNumber: "SN-1",
        })]));
        for (const bad of ["", "m".repeat(257), 42 as unknown]) {
            const mutated = {
                ...obs,
                screens: [{ ...obs.screens[0], manufacturer: bad }],
            };
            assert.equal(buildWorldReportV4InnerJson({
                observation: mutated as never,
                creationAttempts: [],
                frameAttempts: [],
                memberAttempts: [],
                viewAttempts: [],
                clearAttempts: [],
                stateAttempts: [],
                intentRecords: [],
            }), null, `bad manufacturer ${String(bad).slice(0, 8)} must fail closed`);
        }
    });
});

function buildInner(surface: object) {
    const obs = observe(surface);
    const json = buildWorldReportV4InnerJson({
        observation: obs,
        creationAttempts: [],
        frameAttempts: [],
        memberAttempts: [],
        viewAttempts: [],
        clearAttempts: [],
        stateAttempts: [],
        intentRecords: [],
    });
    assert.ok(json !== null);
    return { obs, json, value: JSON.parse(json) as Record<string, unknown> };
}

describe("v4 canonical string writer", () => {
    it("escapes quote and backslash short, every C0 control as lowercase \\u00xx", () => {
        assert.equal(encodeV4String('a"b\\c'), '"a\\"b\\\\c"');
        let all = "";
        let want = '"';
        for (let c = 0; c < 32; c += 1) {
            all += String.fromCharCode(c);
            want += `\\u00${c.toString(16).padStart(2, "0")}`;
        }
        want += '"';
        assert.equal(encodeV4String(all), want);
        // Non-ASCII passes through verbatim (no \\uXXXX escaping).
        assert.equal(encodeV4String("h\u00e9llo\u4e2d"), '"h\u00e9llo\u4e2d"');
    });

    it("never uses the JSON.stringify short escapes for \\b \\t \\n \\f \\r", () => {
        const encoded = encodeV4String("\b\t\n\f\r");
        assert.equal(encoded, '"\\u0008\\u0009\\u000a\\u000c\\u000d"');
        assert.ok(!encoded.includes("\\b") && !encoded.includes("\\t") &&
            !encoded.includes("\\n") && !encoded.includes("\\f") && !encoded.includes("\\r"));
    });
});

describe("v4 pid mapping", () => {
    it("maps -1 to the own token, positives to decimal text, others to null", () => {
        assert.equal(mapNativePid(-1), "kwin-internal");
        assert.equal(mapNativePid(1234), "1234");
        assert.equal(mapNativePid(0), null);
        assert.equal(mapNativePid(-2), null);
        assert.equal(mapNativePid(Number.NaN), null);
        assert.equal(mapNativePid("1234"), null);
        assert.equal(mapNativePid(undefined), null);
    });
});

describe("v4 inner report shape", () => {
    it("emits the exact top-level key order without the broker namespace", () => {
        const { value } = buildInner(makeSurface([makeWin({})]));
        assert.deepEqual(Object.keys(value), [
            "v", "screens", "windows", "closed", "native", "focused",
            "foregroundKnown",
            "complete", "gesture", "cursor", "actionForeground",
            "actionCached", "actionOutputs", "presentation",
            "intentRecords", "stateAttempts", "focusFeedback",
            "rendererInput", "nativeGesture", "cursorWindow", "cursorHit",
            "nativeInvalidWitness",
        ]);
        assert.equal(value["v"], 4);
        for (const brokerKey of ["adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
            assert.ok(!(brokerKey in value), `broker-owned ${brokerKey} must be absent`);
        }
        for (const retired of ["owner", "mode", "views", "hostOwned", "equalCount", "birthClear", "predicate",
            "refill", "defaultTiled", "switchTarget", "freshPlacement", "focusFollowsCursor", "cursorWarp",
            "anchorFallback", "noSpaceResult", "bornMaximized", "floatIntent", "tileIntent",
            "primary", "monotonicMs", "gestureIngress",
            "frameAttempts", "memberAttempts", "viewAttempts", "clearAttempts", "creationAttempts"]) {
            assert.ok(!(retired in value), `retired/optional ${retired} must be absent`);
        }
        assert.equal(value["cursorWindow"], "win-a", "raw windowAt fact rides");
        assert.equal(value["cursorHit"], "win-a", "normative hit mirrors the captured windowAt fact");
    });

    it("emits the exact window row order with nativeWs and honest nulls", () => {
        const { value } = buildInner(makeSurface([makeWin({})]));
        const windows = value["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 1);
        const row = windows[0] as Record<string, unknown>;
        assert.deepEqual(Object.keys(row), [
            "win", "out", "rect", "fullscreen", "maximized", "sticky",
            "minimized", "hints", "nativeWs", "hostHidden", "privileged",
            "managed", "acceptsInput", "activationInhibited", "toolWindow",
            "owned", "transient", "modal", "resizable", "above", "below",
            "owner", "parent", "role", "appId", "executable", "processId",
            "class", "title", "type", "activationPolicy", "layer",
            "fullscreenOwner", "fullscreenExitable", "suppressionAvailable",
            "visibility", "lastWrite",
        ]);
        assert.deepEqual(row["rect"], [8, 9, 800, 600]);
        assert.deepEqual(row["hints"], { minW: 104, minH: 122, maxW: 2147483647, maxH: 2147483647 });
        assert.equal(row["nativeWs"], "native-a");
        assert.equal(row["role"], "normal");
        assert.equal(row["appId"], "example.app");
        assert.equal(row["processId"], "1234");
        assert.equal(row["class"], "Example");
        assert.equal(row["title"], "Window");
        assert.equal(row["type"], "normal");
        assert.equal(row["managed"], true);
        assert.equal(row["resizable"], true);
        for (const key of ["privileged", "activationInhibited", "toolWindow", "owned",
            "executable", "activationPolicy", "layer", "fullscreenOwner",
            "fullscreenExitable", "owner", "parent", "lastWrite"]) {
            assert.equal(row[key], null, `${key} must ride null`);
        }
        assert.equal(row["suppressionAvailable"], false);
        assert.deepEqual(row["visibility"], { visible: null, suppressionOwned: false });
    });

    it("omits nativeWs when membership is not exactly one desktop", () => {
        const multi = makeWin({ internalId: "win-a", desktops: [makeDesk("native-a"), makeDesk("native-b")] });
        const { value } = buildInner(makeSurface([multi]));
        const row = (value["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>;
        assert.ok(!("nativeWs" in row), "multi-homed nativeWs must omit, never guess");
    });

    it("maps native minimized/hostHidden to visibility without inventing primary", () => {
        const { value } = buildInner(makeSurface([makeWin({ minimized: true })]));
        const row = (value["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>;
        assert.equal(row["minimized"], true);
        assert.equal(row["hostHidden"], false);
        assert.deepEqual(row["visibility"], { visible: false, suppressionOwned: false });
        assert.ok(!("primary" in (value as Record<string, unknown>)), "active screen is not primary proof");
    });

    it("maps dialog candidates to the dialog type with pid token for own windows", () => {
        const dialog = makeWin({
            internalId: "win-d", normalWindow: false, dialog: true, pid: -1,
            transient: true, modal: true, transientFor: null,
        });
        const { value } = buildInner(makeSurface([dialog]));
        const row = (value["windows"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>;
        assert.equal(row["type"], "dialog");
        assert.equal(row["processId"], "kwin-internal");
        assert.equal(row["owner"], null);
    });

    it("fails closed on a closed id that overlaps a live lifetime", () => {
        const obs = observe(makeSurface([makeWin({})]));
        const json = buildWorldReportV4InnerJson({
            observation: { ...obs, closed: ["win-a"] },
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
        stateAttempts: [],
            intentRecords: [],
        });
        assert.equal(json, null);
    });

    it("spells scale 1 without an exponent and keeps cursor/action facts", () => {
        const { json, value } = buildInner(makeSurface([makeWin({})]));
        assert.ok(json.includes('"scale":1'), "KWin logical scale spells exactly 1");
        const screens = value["screens"] as Array<Record<string, unknown>>;
        assert.deepEqual(Object.keys(screens[0] as Record<string, unknown>),
            ["out", "full", "work", "complete", "scale", "identity"]);
        assert.deepEqual(screens[0]?.["identity"],
            { manufacturer: null, model: null, serial: null, connector: null });
        assert.deepEqual(value["cursor"], [20, 30]);
        assert.equal(value["focused"], "win-a");
        assert.equal(value["actionForeground"], "out-a");
        assert.equal(value["actionCached"], null);
        assert.deepEqual(value["actionOutputs"], ["out-a"]);
        const presentation = value["presentation"] as Record<string, unknown>;
        assert.deepEqual(Object.keys(presentation), [
            "foreground", "minimized", "maximized", "fullscreen",
            "hostHidden", "hiddenWorkspace", "frame", "chordHeld",
            "moveWindow", "pagerOrder", "pagerFrames", "witness", "foregroundOut",
        ]);
        assert.equal(presentation["witness"], null);
    });
});

describe("v4 frame attempt lane", () => {
    function buildWithFrames(frames: Array<Record<string, unknown>>) {
        const obs = observe(makeSurface([makeWin({})]));
        const json = buildWorldReportV4InnerJson({
            observation: obs,
            creationAttempts: [],
            frameAttempts: frames as never,
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
        stateAttempts: [],
            intentRecords: [],
        });
        return { json, value: json === null ? null : JSON.parse(json) as Record<string, unknown> };
    }

    function ordinary(over: Record<string, unknown> = {}) {
        return {
            win: "win-a",
            target: [0, 0, 200, 200],
            setter: "accepted",
            readback: [0, 0, 200, 200],
            dpiBefore: null,
            dpiAfter: null,
            attemptMs: "7",
            op: "",
            pre: null,
            ...over,
        };
    }

    it("encodes an ordinary accepted write with empty op and present readback", () => {
        const { json, value } = buildWithFrames([ordinary()]);
        assert.ok(json !== null, "ordinary accepted row must encode");
        const rows = (value as Record<string, unknown>)["frameAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 1);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>), [
            "win", "target", "setter", "readback",
            "dpiBefore", "dpiAfter", "attemptMs", "op", "pre",
        ]);
        assert.equal(rows[0]?.["op"], "");
        assert.deepEqual(rows[0]?.["readback"], [0, 0, 200, 200]);
        assert.equal(rows[0]?.["pre"], null);
    });

    it("encodes an ordinary refused write with null readback and null pre", () => {
        const { json, value } = buildWithFrames([ordinary({ setter: "refused", readback: null })]);
        assert.ok(json !== null, "ordinary refused row with unreadable readback must encode");
        const rows = (value as Record<string, unknown>)["frameAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows[0]?.["readback"], null);
        assert.equal(rows[0]?.["op"], "");
        assert.equal(rows[0]?.["pre"], null);
    });

    it("encodes an explicit float placement under its retained operation id", () => {
        const { json, value } = buildWithFrames([ordinary({ op: "op-1", pre: [3, 3, 11, 11] })]);
        assert.ok(json !== null, "explicit placement row must encode");
        const rows = (value as Record<string, unknown>)["frameAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows[0]?.["op"], "op-1");
        assert.deepEqual(rows[0]?.["pre"], [3, 3, 11, 11]);
    });

    it("fails closed on a bad nonempty operation id", () => {
        assert.equal(buildWithFrames([ordinary({ op: "not an id!" })]).json, null);
    });

    it("fails closed on a bad window id", () => {
        assert.equal(buildWithFrames([ordinary({ win: "" })]).json, null);
    });

    it("fails closed on an invalid pre rect", () => {
        assert.equal(buildWithFrames([ordinary({ op: "op-1", pre: [0, 0, 1.5, 2] })]).json, null);
    });

    it("fails closed on a pre-write frame without an operation", () => {
        assert.equal(buildWithFrames([ordinary({ pre: [3, 3, 11, 11] })]).json, null);
    });

    it("fails closed on an invalid target rect", () => {
        assert.equal(buildWithFrames([ordinary({ target: [0, 0, Number.NaN, 2] })]).json, null);
    });
});

describe("v4 member/view attempt lanes", () => {
    function buildWithAttempts(
        members: Array<Record<string, unknown>>,
        views: Array<Record<string, unknown>>,
    ) {
        const obs = observe(makeSurface([makeWin({})]));
        const json = buildWorldReportV4InnerJson({
            observation: obs,
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: members as never,
            viewAttempts: views as never,
            clearAttempts: [],
        stateAttempts: [],
            intentRecords: [],
        });
        return { json, value: json === null ? null : JSON.parse(json) as Record<string, unknown> };
    }

    function member(over: Record<string, unknown> = {}) {
        return {
            op: "op-9",
            win: "win-a",
            out: "out-2",
            ws: "w2",
            setter: "accepted",
            pre: { kind: "native", out: "out-1", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            attemptMs: "7",
            ...over,
        };
    }

    function view(over: Record<string, unknown> = {}) {
        return {
            op: "op-9",
            out: "out-2",
            ws: "w2",
            setter: "accepted",
            pre: { kind: "native", out: "out-2", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            base: { kind: "native", out: "out-2", ws: "d1" },
            attemptMs: "7",
            ...over,
        };
    }

    it("encodes native member evidence in canonical key order without sidecars", () => {
        const { json, value } = buildWithAttempts([member()], []);
        assert.ok(json !== null, "native member row must encode");
        const rows = (value as Record<string, unknown>)["memberAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 1);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>), [
            "op", "win", "out", "ws", "setter", "pre", "post", "attemptMs",
        ]);
        // The cross-output preimage keeps its source output, never the target.
        assert.ok(json.includes('"pre":{"kind":"native","out":"out-1","ws":"d1"}'), "pre ref spells kind/out/ws");
        assert.ok(json.includes('"post":{"kind":"native","out":"out-2","ws":"d2"}'), "post ref spells kind/out/ws");
        for (const sidecar of ["nativePre", "nativePost", "nativeBase"]) {
            assert.ok(!json.includes(`"${sidecar}"`), `retired ${sidecar} must be absent`);
        }
        assert.ok(json.includes('"attemptMs":"7"'), "original write stamp preserved");
    });

    it("keeps logical member evidence tagged logical", () => {
        const { json } = buildWithAttempts([member({
            pre: { kind: "logical", out: "out-1", ws: "w9" },
            post: { kind: "logical", out: "out-1", ws: "w1" },
        })], []);
        assert.ok(json !== null, "logical member row must encode");
        assert.ok(json.includes('"pre":{"kind":"logical","out":"out-1","ws":"w9"}'), "logical pre stays tagged");
        assert.ok(json.includes('"post":{"kind":"logical","out":"out-1","ws":"w1"}'), "logical post stays tagged");
    });

    it("encodes a native view row with pre/post/base and no sidecars", () => {
        const { json, value } = buildWithAttempts([], [view()]);
        assert.ok(json !== null, "native view row must encode");
        const rows = (value as Record<string, unknown>)["viewAttempts"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 1);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>), [
            "op", "out", "ws", "setter", "pre", "post", "base", "attemptMs",
        ]);
        assert.ok(json.includes('"base":{"kind":"native","out":"out-2","ws":"d1"}'), "baseline spells kind/out/ws");
        for (const sidecar of ["nativePre", "nativePost", "nativeBase"]) {
            assert.ok(!json.includes(`"${sidecar}"`), `retired ${sidecar} must be absent`);
        }
    });

    it("keeps unreadable evidence null on both lanes", () => {
        const { json, value } = buildWithAttempts(
            [member({ pre: null, post: null })],
            [view({ pre: null, post: null, base: null })],
        );
        assert.ok(json !== null, "null-evidence rows must encode");
        const members = (value as Record<string, unknown>)["memberAttempts"] as Array<Record<string, unknown>>;
        const views = (value as Record<string, unknown>)["viewAttempts"] as Array<Record<string, unknown>>;
        assert.equal(members[0]?.["pre"], null);
        assert.equal(members[0]?.["post"], null);
        assert.equal(views[0]?.["pre"], null);
        assert.equal(views[0]?.["post"], null);
        assert.equal(views[0]?.["base"], null);
        assert.ok(json.includes('"pre":null'), "null evidence stays null");
    });

    it("fails closed on a malformed ref id on either lane", () => {
        assert.equal(buildWithAttempts([member({ pre: { kind: "native", out: "out-1", ws: "bad id" } })], []).json, null);
        assert.equal(buildWithAttempts([member({ post: { kind: "native", out: "", ws: "d2" } })], []).json, null);
        assert.equal(buildWithAttempts([], [view({ base: { kind: "native", out: "out-2", ws: "" } })]).json, null);
    });

    it("fails closed on a logical view ref, which has no wire spelling", () => {
        assert.equal(buildWithAttempts([], [view({ pre: { kind: "logical", out: "out-2", ws: "w2" } })]).json, null);
    });

    it("fails closed on a bad operation id on either lane", () => {
        assert.equal(buildWithAttempts([member({ op: "not an id!" })], []).json, null);
        assert.equal(buildWithAttempts([], [view({ op: "" })]).json, null);
    });
});

describe("v4 local frame witness (adapter-local watermark)", () => {
    function observeWith(
        surface: object,
        presentationFacts: { instance: string; rev: string; invalidation: string; chord: boolean; borderGap: number; borderWidth: number; underlayExtension: number } | null,
        watermark: string | null,
    ) {
        const obs = captureWorldObservation({
            workspace: surface,
            readMode: () => "per-output-local",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-a", pin: false, disp: false, managed: true }),
            isFloating: () => false,
            ...(presentationFacts === null ? {} : { presentationFacts }),
        });
        assert.ok(obs !== null);
        (obs as { localInvalidWatermark?: string | null }).localInvalidWatermark = watermark;
        (obs as { nativeInvalidWitness?: { instance: string; counter: string } | null }).nativeInvalidWitness =
            presentationFacts === null ? null : { instance: presentationFacts.instance, counter: presentationFacts.invalidation };
        return obs;
    }

    function facts() {
        return { instance: "effect-1", rev: "r-1", invalidation: "7", chord: false, borderGap: 0, borderWidth: 3, underlayExtension: -1 };
    }

    function innerWitness(obs: object) {
        const json = buildWorldReportV4InnerJson({
            observation: obs as never,
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
        stateAttempts: [],
            intentRecords: [],
        });
        assert.ok(json !== null);
        const value = JSON.parse(json) as Record<string, unknown>;
        return (value["presentation"] as Record<string, unknown>)["witness"] as Record<string, unknown> | null;
    }

    it("mints the exact known witness with local watermark, diverging native and broker counters", () => {
        const obs = observeWith(makeSurface([makeWin({})]), facts(), "5");
        const witness = deriveLocalFrameWitness(obs);
        assert.ok(witness !== null);
        assert.equal(witness.win, "win-a");
        assert.equal(witness.foreground, "win-a");
        assert.equal(witness.obs, "5");
        assert.deepEqual(witness.frame, [8, 9, 800, 600]);
        const wire = innerWitness(obs);
        assert.deepEqual(Object.keys(wire as object).sort(), ["foreground", "frame", "obs", "win"]);
        assert.equal((wire as Record<string, unknown>)["win"], "win-a");
        assert.equal((wire as Record<string, unknown>)["obs"], "5");
        assert.deepEqual((wire as Record<string, unknown>)["frame"], [8, 9, 800, 600]);
        assert.equal((wire as Record<string, unknown>)["foreground"], "win-a");
    });

    it("uses the local watermark, never the native effect counter", () => {
        const native = facts();
        native.invalidation = "1";
        const obs = observeWith(makeSurface([makeWin({})]), native, "9");
        const witness = deriveLocalFrameWitness(obs);
        assert.ok(witness !== null);
        assert.equal(witness.obs, "9");
    });

    it("nulls on missing watermark (reentrant invalidation fence)", () => {
        const obs = observeWith(makeSurface([makeWin({})]), facts(), null);
        assert.equal(deriveLocalFrameWitness(obs), null);
        assert.equal(innerWitness(obs), null);
    });

    it("nulls on malformed watermark", () => {
        const obs = observeWith(makeSurface([makeWin({})]), facts(), "07");
        assert.equal(deriveLocalFrameWitness(obs), null);
    });

    it("nulls on unknown or malformed effect coverage", () => {
        const unknown = observeWith(makeSurface([makeWin({})]), null, "5");
        assert.equal(deriveLocalFrameWitness(unknown), null);
        const malformed = observeWith(makeSurface([makeWin({})]), facts(), "5");
        (malformed as { nativeInvalidWitness?: unknown }).nativeInvalidWitness = { instance: "", counter: "7" };
        assert.equal(deriveLocalFrameWitness(malformed), null);
        const badCounter = observeWith(makeSurface([makeWin({})]), facts(), "5");
        (badCounter as { nativeInvalidWitness?: unknown }).nativeInvalidWitness = { instance: "effect-1", counter: "07" };
        assert.equal(deriveLocalFrameWitness(badCounter), null);
    });

    it("nulls on mismatched foreground vs focusedGlobal", () => {
        const obs = observeWith(makeSurface([makeWin({})]), facts(), "5");
        (obs.presentation as { foreground: string }).foreground = "win-b";
        assert.equal(deriveLocalFrameWitness(obs), null);
    });

    it("nulls on removed foreground lifetime and closed lifetime", () => {
        const surface = makeSurface([]);
        const obs = observeWith(surface, facts(), "5");
        assert.equal(deriveLocalFrameWitness(obs), null);
        const live = observeWith(makeSurface([makeWin({})]), facts(), "5");
        (live as { closed: string[] }).closed = ["win-a"];
        assert.equal(deriveLocalFrameWitness(live), null);
    });

    it("nulls on minimized or unreadable frame", () => {
        const minimized = observeWith(makeSurface([makeWin({ minimized: true })]), facts(), "5");
        assert.equal(deriveLocalFrameWitness(minimized), null);
        const live = observeWith(makeSurface([makeWin({})]), facts(), "5");
        (live.presentation as { frame: { x: number; y: number; w: number; h: number } | null }).frame = null;
        assert.equal(deriveLocalFrameWitness(live), null);
    });

    it("nulls on incomplete inventory", () => {
        const obs = observeWith(makeSurface([makeWin({})]), facts(), "5");
        (obs as { complete: boolean }).complete = false;
        assert.equal(deriveLocalFrameWitness(obs), null);
    });

    it("fails closed at the exhausted local watermark", () => {
        const obs = observeWith(makeSurface([makeWin({})]), facts(), "18446744073709551615");
        assert.equal(deriveLocalFrameWitness(obs), null);
        assert.equal(innerWitness(obs), null);
    });

    it("consecutive no-change captures may share the local watermark", () => {
        const first = observeWith(makeSurface([makeWin({})]), facts(), "5");
        const second = observeWith(makeSurface([makeWin({})]), facts(), "5");
        assert.ok(deriveLocalFrameWitness(first) !== null);
        assert.ok(deriveLocalFrameWitness(second) !== null);
        assert.equal(deriveLocalFrameWitness(first)?.obs, deriveLocalFrameWitness(second)?.obs);
    });
});

describe("v4 foregroundKnown provenance bit", () => {
    function innerFor(obs: ReturnType<typeof observe>) {
        const json = buildWorldReportV4InnerJson({
            observation: obs,
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
            stateAttempts: [],
            intentRecords: [],
        });
        return json === null ? null : JSON.parse(json) as Record<string, unknown>;
    }

    it("true with bound focused proves foreground", () => {
        const obs = observe(makeSurface([makeWin({})]));
        assert.equal(obs.focusedGlobal, "win-a");
        assert.equal(obs.foregroundKnown, true);
        const value = innerFor(obs);
        assert.ok(value !== null);
        assert.equal(value["focused"], "win-a");
        assert.equal(value["foregroundKnown"], true);
    });

    it("true with empty focused proves no client", () => {
        const obs = observe(makeSurface([]));
        assert.equal(obs.focusedGlobal, "");
        assert.equal(obs.foregroundKnown, true);
        const value = innerFor(obs);
        assert.ok(value !== null);
        assert.equal(value["focused"], "");
        assert.equal(value["foregroundKnown"], true);
    });

    it("false with empty focused means unknown", () => {
        const obs = observe(makeSurface([makeWin({})]));
        const unknown = { ...obs, focusedGlobal: "", foregroundKnown: false };
        const value = innerFor(unknown);
        assert.ok(value !== null);
        assert.equal(value["focused"], "");
        assert.equal(value["foregroundKnown"], false);
    });

    it("false with nonempty focused fails the report closed", () => {
        const obs = observe(makeSurface([makeWin({})]));
        assert.ok(obs.focusedGlobal !== "");
        const invalid = { ...obs, foregroundKnown: false };
        assert.equal(innerFor(invalid), null);
    });
});
